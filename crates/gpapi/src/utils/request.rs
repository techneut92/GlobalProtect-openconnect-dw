use std::{borrow::Cow, fs};

use anyhow::bail;
use log::warn;
use openssl::pkey::PKey;
use pem::parse_many;
use reqwest::Identity;

#[derive(Debug, thiserror::Error)]
pub enum RequestIdentityError {
  #[error("Failed to find the private key")]
  NoKey,
  #[error("No passphrase provided")]
  NoPassphrase(&'static str),
  #[error("Failed to decrypt private key")]
  DecryptError(&'static str),
}

/// Create an identity object from a certificate and key
/// The file is expected to be the PKCS#8 PEM or PKCS#12 format
/// When using a PKCS#12 file, the key is NOT required, but a passphrase is required
pub fn create_identity(cert: &str, key: Option<&str>, passphrase: Option<&str>) -> anyhow::Result<Identity> {
  // PKCS#11 (smart-card / YubiKey PIV) client certs cannot be expressed as a
  // reqwest native-tls Identity (no key material to read). The openconnect tunnel
  // side accepts `pkcs11:` URIs natively, but the portal/gateway *prelogin* runs
  // through reqwest+native-tls here.
  // TODO(pkcs11): present the pkcs11 client cert for prelogin via a rustls
  // ClientConfig with a cryptoki-backed signing key, wired in through
  // `reqwest::ClientBuilder::use_preconfigured_tls` in gp_params.rs.
  if cert.starts_with("pkcs11:") {
    anyhow::bail!(
      "PKCS#11 client certificates are not yet supported for the portal/gateway prelogin \
       (only the openconnect tunnel accepts pkcs11 URIs). See TODO(pkcs11) in gpapi/src/utils/request.rs."
    );
  }

  if cert.ends_with(".p12") || cert.ends_with(".pfx") {
    create_identity_from_pkcs12(cert, passphrase)
  } else {
    create_identity_from_pem(cert, key, passphrase)
  }
}

fn create_identity_from_pem(cert: &str, key: Option<&str>, passphrase: Option<&str>) -> anyhow::Result<Identity> {
  let cert_pem = fs::read(cert).map_err(|err| anyhow::anyhow!("Failed to read certificate file: {}", err))?;

  // Use the certificate as the key if no key is provided
  let key_pem_file = match key {
    Some(key) => Cow::Owned(fs::read(key).map_err(|err| anyhow::anyhow!("Failed to read key file: {}", err))?),
    None => Cow::Borrowed(&cert_pem),
  };

  // Find the private key in the pem file
  let key_pem = parse_many(key_pem_file.as_ref())?
    .into_iter()
    .find(|pem| pem.tag().ends_with("PRIVATE KEY"))
    .ok_or(RequestIdentityError::NoKey)?;

  // The key pem could be encrypted, so we need to decrypt it
  let decrypted_key_pem = if key_pem.tag().ends_with("ENCRYPTED PRIVATE KEY") {
    let passphrase = passphrase.ok_or_else(|| {
      warn!("Key is encrypted but no passphrase provided");
      RequestIdentityError::NoPassphrase("PEM")
    })?;
    let pem_content = pem::encode(&key_pem);
    let key = PKey::private_key_from_pem_passphrase(pem_content.as_bytes(), passphrase.as_bytes()).map_err(|err| {
      warn!("Failed to decrypt key: {}", err);
      RequestIdentityError::DecryptError("PEM")
    })?;

    key.private_key_to_pem_pkcs8()?
  } else {
    pem::encode(&key_pem).into()
  };

  let identity = Identity::from_pkcs8_pem(&cert_pem, &decrypted_key_pem)?;
  Ok(identity)
}

fn create_identity_from_pkcs12(pkcs12: &str, passphrase: Option<&str>) -> anyhow::Result<Identity> {
  let pkcs12 = fs::read(pkcs12)?;

  let Some(passphrase) = passphrase else {
    bail!(RequestIdentityError::NoPassphrase("PKCS#12"));
  };

  let identity = Identity::from_pkcs12_der(&pkcs12, passphrase)?;
  Ok(identity)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn create_identity_from_pem_requires_passphrase() {
    let cert = "tests/files/badssl.com-client.pem";
    let identity = create_identity_from_pem(cert, None, None);

    assert!(identity.is_err());
    assert!(identity.unwrap_err().to_string().contains("No passphrase provided"));
  }

  #[test]
  fn create_identity_from_pem_with_passphrase() {
    let cert = "tests/files/badssl.com-client.pem";
    let passphrase = "badssl.com";

    let identity = create_identity_from_pem(cert, None, Some(passphrase));

    assert!(identity.is_ok());
  }

  #[test]
  fn create_identity_from_pem_unencrypted_key() {
    let cert = "tests/files/badssl.com-client-unencrypted.pem";
    let identity = create_identity_from_pem(cert, None, None);
    println!("{:?}", identity);

    assert!(identity.is_ok());
  }

  #[test]
  fn create_identity_from_pem_cert_and_encrypted_key() {
    let cert = "tests/files/badssl.com-client.pem";
    let key = "tests/files/badssl.com-client.pem";
    let passphrase = "badssl.com";

    let identity = create_identity_from_pem(cert, Some(key), Some(passphrase));

    assert!(identity.is_ok());
  }

  #[test]
  fn create_identity_from_pem_cert_and_encrypted_key_no_passphrase() {
    let cert = "tests/files/badssl.com-client.pem";
    let key = "tests/files/badssl.com-client.pem";

    let identity = create_identity_from_pem(cert, Some(key), None);

    assert!(identity.is_err());
    assert!(identity.unwrap_err().to_string().contains("No passphrase provided"));
  }

  #[test]
  fn create_identity_from_pem_cert_and_unencrypted_key() {
    let cert = "tests/files/badssl.com-client.pem";
    let key = "tests/files/badssl.com-client-unencrypted.pem";

    let identity = create_identity_from_pem(cert, Some(key), None);

    assert!(identity.is_ok());
  }
}
