//! PKCS#11 (smart-card / YubiKey PIV) client-certificate support for the
//! portal/gateway prelogin mTLS.
//!
//! reqwest's native-tls `Identity` can't carry a PKCS#11 key (the private key is
//! non-extractable). So for `pkcs11:` client certs we build a rustls
//! `ClientConfig` whose client-cert resolver signs on the token via `cryptoki`,
//! and feed it to reqwest via `ClientBuilder::use_preconfigured_tls`.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use cryptoki::{
  context::{CInitializeArgs, Pkcs11},
  mechanism::{
    rsa::{PkcsMgfType, PkcsPssParams},
    Mechanism, MechanismType,
  },
  object::{Attribute, AttributeType, KeyType, ObjectClass, ObjectHandle},
  session::{Session, UserType},
  types::AuthPin,
};
use log::{info, warn};
use rustls::{
  client::{
    danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    ResolvesClientCert,
  },
  pki_types::{CertificateDer, ServerName, UnixTime},
  sign::{CertifiedKey, Signer, SigningKey},
  ClientConfig, DigitallySignedStruct, RootCertStore, SignatureAlgorithm, SignatureScheme,
};

/// Library directories searched for a PKCS#11 module, across common distros
/// (Fedora/openSUSE multilib, Debian/Ubuntu/Arch multiarch, Alpine, /usr/local).
const MODULE_DIRS: &[&str] = &[
  "/usr/lib64/pkcs11",
  "/usr/lib/pkcs11",
  "/usr/lib/x86_64-linux-gnu/pkcs11",
  "/usr/lib/aarch64-linux-gnu/pkcs11",
  "/usr/lib64",
  "/usr/lib",
  "/usr/lib/x86_64-linux-gnu",
  "/usr/lib/aarch64-linux-gnu",
  "/usr/local/lib",
];

/// Module filenames in preference order. Concrete token modules come first —
/// each exposes exactly one real, loginable token. p11-kit-proxy is the last
/// resort: it aggregates *every* registered token including the system trust
/// store, whose tokens can't satisfy a client-cert login and would derail slot
/// selection when the URI carries no `token=`. Overridable via `GP_PKCS11_MODULE`.
const MODULE_NAMES: &[&str] = &[
  "opensc-pkcs11.so", // PIV smart cards incl. YubiKey
  "libykcs11.so.2",   // YubiKey native PIV
  "libykcs11.so",
  "libsofthsm2.so",   // SoftHSM test tokens
  "p11-kit-proxy.so", // aggregate, last resort
];

pub fn is_pkcs11_uri(s: &str) -> bool {
  s.trim_start().starts_with("pkcs11:")
}

#[derive(Default, Debug)]
struct Pkcs11Uri {
  token: Option<String>,
  object: Option<String>,
  id: Option<Vec<u8>>,
  pin: Option<String>,
}

fn pct_decode_str(s: &str) -> String {
  String::from_utf8_lossy(&pct_decode_bytes(s)).into_owned()
}

fn pct_decode_bytes(s: &str) -> Vec<u8> {
  let bytes = s.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' && i + 2 < bytes.len() {
      if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
        out.push(b);
        i += 3;
        continue;
      }
    }
    out.push(bytes[i]);
    i += 1;
  }
  out
}

fn parse_pkcs11_uri(uri: &str) -> Result<Pkcs11Uri> {
  let body = uri
    .trim_start()
    .strip_prefix("pkcs11:")
    .ok_or_else(|| anyhow!("not a pkcs11 URI: {uri}"))?;
  let (path_part, query_part) = match body.split_once('?') {
    Some((p, q)) => (p, Some(q)),
    None => (body, None),
  };
  let mut out = Pkcs11Uri::default();
  for kv in path_part.split(';') {
    if let Some((k, v)) = kv.split_once('=') {
      match k {
        "token" => out.token = Some(pct_decode_str(v)),
        "object" => out.object = Some(pct_decode_str(v)),
        "id" => out.id = Some(pct_decode_bytes(v)),
        _ => {}
      }
    }
  }
  if let Some(q) = query_part {
    for kv in q.split('&') {
      if let Some((k, v)) = kv.split_once('=') {
        if k == "pin-value" {
          out.pin = Some(pct_decode_str(v));
        }
      }
    }
  }
  Ok(out)
}

/// Locate a `name` (or any of `names`) under the known module directories.
fn find_module_file(names: &[&str]) -> Option<String> {
  for name in names {
    for dir in MODULE_DIRS {
      let p = std::path::Path::new(dir).join(name);
      if p.exists() {
        return Some(p.to_string_lossy().into_owned());
      }
    }
  }
  None
}

/// Resolve the PKCS#11 module: explicit `GP_PKCS11_MODULE` wins (an absolute
/// path is used as-is, a bare filename is resolved against the search dirs),
/// otherwise auto-detect a known module.
fn module_path() -> Result<String> {
  if let Ok(m) = std::env::var("GP_PKCS11_MODULE") {
    if m.is_empty() {
      // fall through to auto-detection
    } else if m.contains('/') {
      return Ok(m);
    } else if let Some(p) = find_module_file(&[m.as_str()]) {
      return Ok(p);
    } else {
      // Not found under the search dirs; hand the bare name to the loader so a
      // library on the linker search path still works.
      return Ok(m);
    }
  }
  find_module_file(MODULE_NAMES).ok_or_else(|| {
    anyhow!(
      "no PKCS#11 module found; install opensc (provides opensc-pkcs11.so) or set \
       GP_PKCS11_MODULE to your module path (e.g. libsofthsm2.so)"
    )
  })
}

/// rustls signing key backed by a PKCS#11 private key handle.
struct Pkcs11Key {
  session: Arc<Mutex<Session>>,
  handle: ObjectHandle,
  algorithm: SignatureAlgorithm,
}

impl std::fmt::Debug for Pkcs11Key {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Pkcs11Key({:?})", self.algorithm)
  }
}

#[derive(Debug)]
struct Pkcs11Signer {
  session: Arc<Mutex<Session>>,
  handle: ObjectHandle,
  scheme: SignatureScheme,
}

impl SigningKey for Pkcs11Key {
  fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
    let preferred = match self.algorithm {
      SignatureAlgorithm::RSA => vec![
        SignatureScheme::RSA_PSS_SHA256,
        SignatureScheme::RSA_PKCS1_SHA256,
      ],
      SignatureAlgorithm::ECDSA => vec![SignatureScheme::ECDSA_NISTP256_SHA256],
      _ => vec![],
    };
    let scheme = preferred.into_iter().find(|s| offered.contains(s))?;
    Some(Box::new(Pkcs11Signer {
      session: Arc::clone(&self.session),
      handle: self.handle,
      scheme,
    }))
  }

  fn algorithm(&self) -> SignatureAlgorithm {
    self.algorithm
  }
}

impl Signer for Pkcs11Signer {
  fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
    let mechanism = match self.scheme {
      SignatureScheme::RSA_PKCS1_SHA256 => Mechanism::Sha256RsaPkcs,
      SignatureScheme::RSA_PSS_SHA256 => Mechanism::Sha256RsaPkcsPss(PkcsPssParams {
        hash_alg: MechanismType::SHA256,
        mgf: PkcsMgfType::MGF1_SHA256,
        s_len: 32.into(),
      }),
      SignatureScheme::ECDSA_NISTP256_SHA256 => Mechanism::EcdsaSha256,
      other => {
        return Err(rustls::Error::General(format!("unsupported signature scheme {other:?}")));
      }
    };
    let session = self
      .session
      .lock()
      .map_err(|_| rustls::Error::General("pkcs11 session lock poisoned".into()))?;
    session
      .sign(&mechanism, self.handle, message)
      .map_err(|e| rustls::Error::General(format!("pkcs11 sign failed: {e}")))
  }

  fn scheme(&self) -> SignatureScheme {
    self.scheme
  }
}

#[derive(Debug)]
struct StaticClientCertResolver {
  certified: Arc<CertifiedKey>,
}

impl ResolvesClientCert for StaticClientCertResolver {
  fn resolve(&self, _root_hint_subjects: &[&[u8]], _sigschemes: &[SignatureScheme]) -> Option<Arc<CertifiedKey>> {
    Some(Arc::clone(&self.certified))
  }

  fn has_certs(&self) -> bool {
    true
  }
}

/// Server cert verifier that accepts everything (used when `ignore_tls_errors`).
#[derive(Debug)]
struct NoVerifier(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for NoVerifier {
  fn verify_server_cert(
    &self,
    _end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    _now: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    Ok(ServerCertVerified::assertion())
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.0.signature_verification_algorithms.supported_schemes()
  }
}

/// Build a rustls `ClientConfig` that authenticates with a PKCS#11 client cert.
/// `cert_uri` / `key_uri` are `pkcs11:` URIs; `pin` overrides any `pin-value`.
pub fn create_pkcs11_client_config(
  cert_uri: &str,
  key_uri: Option<&str>,
  pin: Option<&str>,
  ignore_tls_errors: bool,
) -> Result<ClientConfig> {
  let cert_u = parse_pkcs11_uri(cert_uri)?;
  let key_u = match key_uri {
    Some(k) => parse_pkcs11_uri(k)?,
    None => parse_pkcs11_uri(cert_uri)?,
  };
  let pin = pin
    .map(|p| p.to_string())
    .or(cert_u.pin.clone())
    .or(key_u.pin.clone())
    .ok_or_else(|| anyhow!("a PIN is required for the PKCS#11 token (use --cert-pin or pin-value=… in the URI)"))?;

  let module = module_path()?;
  info!("Loading PKCS#11 module: {module}");
  let pkcs11 = Pkcs11::new(&module).context("failed to load PKCS#11 module")?;
  // gpservice is long-lived and now runs prelogin repeatedly (once per connect),
  // and the openconnect tunnel also initialises the same module via GnuTLS —
  // so C_Initialize is process-global and may already have run. Treat
  // "already initialized" as success (the module is usable) instead of failing
  // the second connect with "PKCS#11 initialize failed".
  match pkcs11.initialize(CInitializeArgs::OsThreads) {
    Ok(()) => {}
    Err(cryptoki::error::Error::Pkcs11(cryptoki::error::RvError::CryptokiAlreadyInitialized, _)) => {
      info!("PKCS#11 module already initialised in this process; reusing it");
    }
    Err(e) => return Err(e).context("PKCS#11 initialize failed"),
  }

  // Pick the slot whose token label matches the URI (or the first with a token).
  let slots = pkcs11.get_slots_with_token().context("no PKCS#11 token slots")?;
  let want_token = cert_u.token.as_deref().or(key_u.token.as_deref());
  let slot = slots
    .into_iter()
    .find(|s| match (want_token, pkcs11.get_token_info(*s)) {
      (Some(label), Ok(info)) => info.label().trim() == label,
      (None, Ok(_)) => true,
      _ => false,
    })
    .ok_or_else(|| anyhow!("no PKCS#11 token matching {:?}", want_token))?;

  let session = pkcs11.open_ro_session(slot).context("failed to open PKCS#11 session")?;
  session
    .login(UserType::User, Some(&AuthPin::new(pin)))
    .context("PKCS#11 login failed (wrong PIN?)")?;

  // Find the client certificate and read its DER value.
  let mut cert_template = vec![Attribute::Class(ObjectClass::CERTIFICATE)];
  if let Some(id) = &cert_u.id {
    cert_template.push(Attribute::Id(id.clone()));
  }
  if let Some(obj) = &cert_u.object {
    cert_template.push(Attribute::Label(obj.as_bytes().to_vec()));
  }
  let cert_handle = *session
    .find_objects(&cert_template)
    .context("failed to search for certificate")?
    .first()
    .ok_or_else(|| anyhow!("no certificate found on token for {cert_uri}"))?;
  let cert_der = session
    .get_attributes(cert_handle, &[AttributeType::Value])?
    .into_iter()
    .find_map(|a| match a {
      Attribute::Value(v) => Some(v),
      _ => None,
    })
    .ok_or_else(|| anyhow!("certificate object has no value"))?;

  // Find the private key + its key type.
  let mut key_template = vec![Attribute::Class(ObjectClass::PRIVATE_KEY)];
  if let Some(id) = &key_u.id {
    key_template.push(Attribute::Id(id.clone()));
  }
  if let Some(obj) = &key_u.object {
    key_template.push(Attribute::Label(obj.as_bytes().to_vec()));
  }
  let key_handle = *session
    .find_objects(&key_template)
    .context("failed to search for private key")?
    .first()
    .ok_or_else(|| anyhow!("no private key found on token"))?;
  let key_type = session
    .get_attributes(key_handle, &[AttributeType::KeyType])?
    .into_iter()
    .find_map(|a| match a {
      Attribute::KeyType(kt) => Some(kt),
      _ => None,
    })
    .ok_or_else(|| anyhow!("private key has no key type"))?;
  let algorithm = match key_type {
    KeyType::RSA => SignatureAlgorithm::RSA,
    KeyType::EC => SignatureAlgorithm::ECDSA,
    other => bail!("unsupported PKCS#11 key type: {other:?}"),
  };

  let session = Arc::new(Mutex::new(session));
  let signing_key: Arc<dyn SigningKey> = Arc::new(Pkcs11Key {
    session,
    handle: key_handle,
    algorithm,
  });
  let certified = Arc::new(CertifiedKey::new(vec![CertificateDer::from(cert_der)], signing_key));
  build_client_config(certified, ignore_tls_errors)
}

/// Build a rustls `ClientConfig` presenting `certified` as the client cert.
/// Shared by the PKCS#11 and the Windows-exec signers.
pub(crate) fn build_client_config(certified: Arc<CertifiedKey>, ignore_tls_errors: bool) -> Result<ClientConfig> {
  let resolver = Arc::new(StaticClientCertResolver { certified });

  let provider = Arc::new(rustls::crypto::ring::default_provider());
  let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
    .with_safe_default_protocol_versions()
    .context("rustls protocol versions")?;

  let config = if ignore_tls_errors {
    warn!("Ignoring TLS errors for the mTLS prelogin connection");
    builder
      .dangerous()
      .with_custom_certificate_verifier(Arc::new(NoVerifier(provider)))
      .with_client_cert_resolver(resolver)
  } else {
    let mut roots = RootCertStore::empty();
    for c in rustls_native_certs::load_native_certs().certs {
      let _ = roots.add(c);
    }
    builder.with_root_certificates(roots).with_client_cert_resolver(resolver)
  };

  Ok(config)
}
