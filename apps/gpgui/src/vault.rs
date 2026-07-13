//! Encrypted identity vault.
//!
//! Identities (including secrets — PIN, password, key passphrase) are stored in
//! `~/.config/gpgui-ng/identities.enc`, encrypted with a key derived from a
//! **master PIN** via Argon2id. The plaintext identities live in memory only
//! while the vault is unlocked.
//!
//! File layout: `[16-byte salt][12-byte nonce][ChaCha20-Poly1305 ciphertext]`,
//! plaintext = JSON `Vec<Identity>`.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::{Aead, OsRng};
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, Key, KeyInit};
use serde::{Deserialize, Serialize};

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

/// A saved identity (connection profile). Held in plaintext only while unlocked.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
  pub name: String,
  pub portal: String,
  /// 0 smart card, 1 cert file, 2 SAML, 3 username/password.
  pub auth_method: i32,
  pub as_gateway: bool,
  pub module_path: String,
  pub username: String,
  pub password: String,
  pub pin: String,
  pub cert_id: String,
  pub cert_manufacturer: String,
  pub cert_file: String,
  pub key_file: String,
  pub key_password: String,
}

pub struct Vault {
  path: PathBuf,
  salt: [u8; SALT_LEN],
  key: Option<[u8; 32]>,
  identities: Vec<Identity>,
  /// True if the encrypted file exists on disk.
  pub exists: bool,
  pub unlocked: bool,
}

/// Current key derivation: Argon2id, stronger than the default (19 MiB / t=2) to
/// slow offline brute-force of a short master PIN if identities.enc is stolen.
fn derive_key(pin: &str, salt: &[u8]) -> Result<[u8; 32]> {
  let mut key = [0u8; 32];
  let params = argon2::Params::new(64 * 1024, 3, 1, Some(32)).map_err(|e| anyhow!("argon2 params: {e}"))?;
  argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
    .hash_password_into(pin.as_bytes(), salt, &mut key)
    .map_err(|e| anyhow!("key derivation failed: {e}"))?;
  Ok(key)
}

/// Legacy key derivation (Argon2 default) for vaults created before the params
/// bump. Only an unlock fallback — such vaults are transparently re-encrypted
/// with the current params on unlock, so no one loses their identities.
fn derive_key_legacy(pin: &str, salt: &[u8]) -> Result<[u8; 32]> {
  let mut key = [0u8; 32];
  argon2::Argon2::default()
    .hash_password_into(pin.as_bytes(), salt, &mut key)
    .map_err(|e| anyhow!("key derivation failed: {e}"))?;
  Ok(key)
}

impl Vault {
  /// Load vault metadata. Reads the salt if the file exists; stays locked.
  pub fn load(path: PathBuf) -> Self {
    let data = std::fs::read(&path).ok();
    match data {
      Some(bytes) if bytes.len() > SALT_LEN + NONCE_LEN => {
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[..SALT_LEN]);
        Vault { path, salt, key: None, identities: Vec::new(), exists: true, unlocked: false }
      }
      _ => Vault { path, salt: [0u8; SALT_LEN], key: None, identities: Vec::new(), exists: false, unlocked: false },
    }
  }

  pub fn identities(&self) -> &[Identity] {
    &self.identities
  }

  /// First-time setup: pick a salt, derive the key, start an empty unlocked vault.
  pub fn set_master_pin(&mut self, pin: &str) -> Result<()> {
    if pin.is_empty() {
      bail!("master PIN cannot be empty");
    }
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    self.salt = salt;
    self.key = Some(derive_key(pin, &salt)?);
    self.identities = Vec::new();
    self.unlocked = true;
    self.exists = true;
    self.save()
  }

  /// Decrypt the vault with the master PIN.
  pub fn unlock(&mut self, pin: &str) -> Result<()> {
    let bytes = std::fs::read(&self.path)?;
    if bytes.len() <= SALT_LEN + NONCE_LEN {
      bail!("vault file is corrupt");
    }
    let nonce = &bytes[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &bytes[SALT_LEN + NONCE_LEN..];
    let decrypt = |key: &[u8; 32]| {
      ChaCha20Poly1305::new(Key::from_slice(key)).decrypt(nonce.into(), ciphertext).ok()
    };

    // Current params first, then the legacy Argon2 default so vaults created
    // before the params bump still open.
    let key = derive_key(pin, &self.salt)?;
    if let Some(plaintext) = decrypt(&key) {
      self.identities = serde_json::from_slice(&plaintext).unwrap_or_default();
      self.key = Some(key);
      self.unlocked = true;
      return Ok(());
    }

    let legacy = derive_key_legacy(pin, &self.salt)?;
    let Some(plaintext) = decrypt(&legacy) else {
      bail!("wrong master PIN");
    };
    // Same PIN, legacy vault: load the identities and transparently re-encrypt
    // with the current (stronger) params so the legacy KDF is never needed again.
    // The re-save is best-effort — unlock still succeeds if it fails (it'll retry
    // next time), so a write hiccup never looks like a wrong PIN.
    self.identities = serde_json::from_slice(&plaintext).unwrap_or_default();
    self.key = Some(key);
    self.unlocked = true;
    let _ = self.save();
    Ok(())
  }

  pub fn lock(&mut self) {
    self.key = None;
    self.identities.clear();
    self.unlocked = false;
  }

  /// Forgotten-PIN reset: delete the encrypted vault and return to the no-vault
  /// state so the user can set a new master PIN. Saved identities are gone — they
  /// were encrypted with the forgotten PIN and cannot be recovered.
  pub fn reset(&mut self) {
    let _ = std::fs::remove_file(&self.path);
    self.salt = [0u8; SALT_LEN];
    self.key = None;
    self.identities.clear();
    self.exists = false;
    self.unlocked = false;
  }

  pub fn upsert(&mut self, identity: Identity) -> Result<()> {
    if let Some(e) = self.identities.iter_mut().find(|i| i.name == identity.name) {
      *e = identity;
    } else {
      self.identities.push(identity);
    }
    self.save()
  }

  pub fn remove(&mut self, name: &str) -> Result<()> {
    self.identities.retain(|i| i.name != name);
    self.save()
  }

  fn save(&self) -> Result<()> {
    let key = self.key.ok_or_else(|| anyhow!("vault is locked"))?;
    if let Some(dir) = self.path.parent() {
      std::fs::create_dir_all(dir)?;
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let plaintext = serde_json::to_vec(&self.identities)?;
    let ciphertext = cipher
      .encrypt(&nonce, plaintext.as_ref())
      .map_err(|e| anyhow!("encrypt failed: {e}"))?;
    let mut out = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&self.salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    // Atomic + 0600 (temp in the same dir, then rename) so a crash mid-write can't
    // corrupt the vault and lose every saved identity.
    crate::config::write_secret_file(&self.path, &out).map_err(|e| anyhow!("writing vault: {e}"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A vault written with the legacy Argon2 default params must still unlock with
  /// the current code (recovering every identity) and be transparently
  /// re-encrypted with the current params — nobody loses their identities.
  #[test]
  fn legacy_vault_migrates_on_unlock() {
    let dir = std::env::temp_dir().join(format!("gpgui-vault-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("identities.enc");
    let pin = "1234";

    // Write a legacy (Argon2-default) vault by hand.
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let ids = vec![Identity { name: "work".into(), portal: "gp.example.com".into(), ..Default::default() }];
    let legacy_key = derive_key_legacy(pin, &salt).unwrap();
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = ChaCha20Poly1305::new(Key::from_slice(&legacy_key))
      .encrypt(&nonce, serde_json::to_vec(&ids).unwrap().as_ref())
      .unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    std::fs::write(&path, &out).unwrap();

    // Unlock with the current code — must recover the identity via the fallback.
    let mut v = Vault::load(path.clone());
    v.unlock(pin).unwrap();
    assert_eq!(v.identities.len(), 1);
    assert_eq!(v.identities[0].name, "work");
    assert_eq!(v.identities[0].portal, "gp.example.com");

    // The file was re-encrypted with the current params: it now decrypts directly
    // with the current KDF (no legacy fallback needed).
    let bytes = std::fs::read(&path).unwrap();
    let cur = derive_key(pin, &bytes[..SALT_LEN]).unwrap();
    let n = &bytes[SALT_LEN..SALT_LEN + NONCE_LEN];
    let dec = ChaCha20Poly1305::new(Key::from_slice(&cur)).decrypt(n.into(), &bytes[SALT_LEN + NONCE_LEN..]);
    assert!(dec.is_ok(), "vault should decrypt with current params after migration");

    std::fs::remove_dir_all(&dir).ok();
  }
}
