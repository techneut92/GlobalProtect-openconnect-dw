//! Windows-side client-cert signer for prelogin mTLS.
//!
//! Instead of reaching the smart card from inside WSL (USB passthrough), this
//! shells out to `powershell.exe` via WSL↔Windows interop and signs with a
//! certificate in the Windows cert store (e.g. a YubiKey PIV cert via CNG).
//! The PIN/touch prompt happens on the Windows side, where the card already works.
//!
//! URI form: `winsign:<thumbprint>` — cert located in `Cert:\CurrentUser\My`.
//!
//! NOTE: assumes an RSA key (YubiKey PIV default). ECDSA is wired but .NET emits
//! IEEE-P1363 signatures which would need P1363→DER conversion for TLS (TODO).

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use log::info;
use rustls::{
  pki_types::CertificateDer,
  sign::{CertifiedKey, Signer, SigningKey},
  ClientConfig, SignatureAlgorithm, SignatureScheme,
};

/// Locate powershell.exe: env override, then the standard Windows path (works
/// even when the Windows PATH isn't propagated into WSL), then bare PATH.
fn powershell_bin() -> String {
  if let Ok(p) = std::env::var("GP_POWERSHELL") {
    return p;
  }
  let full = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";
  if std::path::Path::new(full).exists() {
    return full.to_string();
  }
  "powershell.exe".to_string()
}

pub fn is_winsign_uri(s: &str) -> bool {
  s.trim_start().starts_with("winsign:")
}

fn thumbprint_from_uri(uri: &str) -> Result<String> {
  let tp = uri
    .trim_start()
    .strip_prefix("winsign:")
    .ok_or_else(|| anyhow!("not a winsign URI"))?
    .trim()
    .replace([' ', ':'], "");
  if tp.is_empty() {
    bail!("winsign URI needs a cert thumbprint: winsign:<thumbprint>");
  }
  Ok(tp)
}

fn run_powershell(script: &str, stdin_data: Option<&str>) -> Result<String> {
  // NB: no -NonInteractive — a PIN/consent dialog must be allowed to appear for
  // smart-card / YubiKey-backed keys, otherwise the sign is auto-cancelled.
  let mut child = Command::new(powershell_bin())
    .args(["-NoProfile", "-Command", script])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .context("failed to spawn powershell.exe (is WSL↔Windows interop enabled?)")?;
  if let Some(data) = stdin_data {
    child.stdin.as_mut().unwrap().write_all(data.as_bytes())?;
  }
  let out = child.wait_with_output()?;
  if !out.status.success() {
    bail!("powershell signer failed: {}", String::from_utf8_lossy(&out.stderr).trim());
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Fetch the certificate (DER) from the Windows store by thumbprint.
fn fetch_cert_der(thumbprint: &str) -> Result<Vec<u8>> {
  let script =
    format!("$c=Get-Item 'Cert:\\CurrentUser\\My\\{thumbprint}' -ErrorAction Stop; [Convert]::ToBase64String($c.RawData)");
  let b64 = run_powershell(&script, None)?;
  B64.decode(b64.trim()).context("failed to decode cert from Windows")
}

#[derive(Debug)]
struct WindowsKey {
  thumbprint: String,
  algorithm: SignatureAlgorithm,
}

#[derive(Debug)]
struct WindowsSigner {
  thumbprint: String,
  scheme: SignatureScheme,
}

impl SigningKey for WindowsKey {
  fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
    let prefs = match self.algorithm {
      SignatureAlgorithm::RSA => vec![SignatureScheme::RSA_PSS_SHA256, SignatureScheme::RSA_PKCS1_SHA256],
      SignatureAlgorithm::ECDSA => vec![SignatureScheme::ECDSA_NISTP256_SHA256],
      _ => vec![],
    };
    let scheme = prefs.into_iter().find(|s| offered.contains(s))?;
    Some(Box::new(WindowsSigner {
      thumbprint: self.thumbprint.clone(),
      scheme,
    }))
  }

  fn algorithm(&self) -> SignatureAlgorithm {
    self.algorithm
  }
}

impl Signer for WindowsSigner {
  fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
    let pad = match self.scheme {
      SignatureScheme::RSA_PSS_SHA256 => "Pss",
      SignatureScheme::RSA_PKCS1_SHA256 => "Pkcs1",
      SignatureScheme::ECDSA_NISTP256_SHA256 => "Ecdsa",
      other => return Err(rustls::Error::General(format!("unsupported scheme {other:?}"))),
    };
    let b64msg = B64.encode(message);
    let script = format!(
      "$tp='{tp}';$pad='{pad}';\
       $msg=[Convert]::FromBase64String([Console]::In.ReadToEnd());\
       $c=Get-Item \"Cert:\\CurrentUser\\My\\$tp\";\
       if($pad -eq 'Ecdsa'){{\
         $k=[System.Security.Cryptography.X509Certificates.ECDsaCertificateExtensions]::GetECDsaPrivateKey($c);\
         $sig=$k.SignData($msg,[Security.Cryptography.HashAlgorithmName]::SHA256)\
       }}else{{\
         $k=[System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($c);\
         $p=if($pad -eq 'Pss'){{[Security.Cryptography.RSASignaturePadding]::Pss}}else{{[Security.Cryptography.RSASignaturePadding]::Pkcs1}};\
         $sig=$k.SignData($msg,[Security.Cryptography.HashAlgorithmName]::SHA256,$p)\
       }};[Convert]::ToBase64String($sig)",
      tp = self.thumbprint,
    );
    let out = run_powershell(&script, Some(&b64msg)).map_err(|e| rustls::Error::General(format!("windows sign: {e}")))?;
    B64.decode(out.trim()).map_err(|e| rustls::Error::General(format!("bad signature b64: {e}")))
  }

  fn scheme(&self) -> SignatureScheme {
    self.scheme
  }
}

/// Build a rustls `ClientConfig` that signs the client cert via Windows/powershell.
pub fn create_winsign_client_config(uri: &str, ignore_tls_errors: bool) -> Result<ClientConfig> {
  let thumbprint = thumbprint_from_uri(uri)?;
  info!("Using Windows (powershell.exe) client-cert signer, thumbprint {thumbprint}");
  let cert_der = fetch_cert_der(&thumbprint)?;
  // Assume RSA (YubiKey PIV default). ECDSA path exists but needs P1363→DER.
  let key: Arc<dyn SigningKey> = Arc::new(WindowsKey {
    thumbprint,
    algorithm: SignatureAlgorithm::RSA,
  });
  let certified = Arc::new(CertifiedKey::new(vec![CertificateDer::from(cert_der)], key));
  super::pkcs11::build_client_config(certified, ignore_tls_errors)
}
