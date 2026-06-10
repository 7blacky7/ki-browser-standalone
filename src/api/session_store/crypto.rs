//! At-rest encryption for session bundles (AES-256-GCM).
//!
//! The key is taken from the env var `KI_BROWSER_SESSION_KEY` when present
//! (base64 or raw, must decode to 32 bytes), otherwise a random 32-byte key is
//! generated once and persisted to `<sessions_dir>/.key` with `0600` perms.
//! Encrypted files are `nonce(12) || ciphertext` and never contain cookie
//! values in cleartext.

use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use rand::RngCore;

const KEY_ENV: &str = "KI_BROWSER_SESSION_KEY";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Holds the symmetric key used to seal/open bundles.
#[derive(Clone)]
pub struct SessionCipher {
    key: [u8; KEY_LEN],
}

impl SessionCipher {
    /// Loads the key from env or the persisted key file, generating and
    /// persisting a fresh key when neither is available.
    pub fn load_or_init(sessions_dir: &Path) -> Result<Self> {
        if let Ok(raw) = std::env::var(KEY_ENV) {
            let key = decode_key(raw.trim())
                .with_context(|| format!("{} must decode to {} bytes", KEY_ENV, KEY_LEN))?;
            return Ok(Self { key });
        }

        let key_path = key_file_path(sessions_dir);
        if key_path.exists() {
            let bytes = std::fs::read(&key_path)
                .with_context(|| format!("reading key file {}", key_path.display()))?;
            let key = to_key_array(&bytes)
                .ok_or_else(|| anyhow!("key file {} is not {} bytes", key_path.display(), KEY_LEN))?;
            return Ok(Self { key });
        }

        // Generate and persist a new key.
        let mut key = [0u8; KEY_LEN];
        rand::thread_rng().fill_bytes(&mut key);
        std::fs::create_dir_all(sessions_dir)
            .with_context(|| format!("creating {}", sessions_dir.display()))?;
        write_key_file(&key_path, &key)?;
        Ok(Self { key })
    }

    /// Seals plaintext into `nonce || ciphertext`.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("session encryption failed: {}", e))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Opens `nonce || ciphertext` back into plaintext.
    pub fn open(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < NONCE_LEN {
            return Err(anyhow!("ciphertext too short"));
        }
        let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key));
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow!("session decryption failed: {}", e))
    }
}

fn key_file_path(sessions_dir: &Path) -> PathBuf {
    sessions_dir.join(".key")
}

fn decode_key(raw: &str) -> Option<[u8; KEY_LEN]> {
    if let Some(k) = to_key_array(raw.as_bytes()) {
        return Some(k);
    }
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(raw) {
        return to_key_array(&decoded);
    }
    if let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw) {
        return to_key_array(&decoded);
    }
    None
}

fn to_key_array(bytes: &[u8]) -> Option<[u8; KEY_LEN]> {
    if bytes.len() == KEY_LEN {
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(bytes);
        Some(key)
    } else {
        None
    }
}

fn write_key_file(path: &Path, key: &[u8; KEY_LEN]) -> Result<()> {
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("creating key file {}", path.display()))?;
    file.write_all(key)?;
    file.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_open_roundtrip() {
        let cipher = SessionCipher { key: [7u8; KEY_LEN] };
        let plaintext = b"top-secret-session-cookie";
        let sealed = cipher.seal(plaintext).expect("seal");
        assert_ne!(&sealed[NONCE_LEN..], plaintext, "ciphertext must differ from plaintext");
        let opened = cipher.open(&sealed).expect("open");
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn test_open_rejects_short_input() {
        let cipher = SessionCipher { key: [1u8; KEY_LEN] };
        assert!(cipher.open(&[0u8; 4]).is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let a = SessionCipher { key: [1u8; KEY_LEN] };
        let b = SessionCipher { key: [2u8; KEY_LEN] };
        let sealed = a.seal(b"data").expect("seal");
        assert!(b.open(&sealed).is_err());
    }

    #[test]
    fn test_decode_key_base64() {
        let raw = [9u8; KEY_LEN];
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        assert_eq!(decode_key(&b64), Some(raw));
        // A 32-char ASCII string is itself a valid raw key.
        let raw32 = "abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(decode_key(raw32).map(|k| k.to_vec()), Some(raw32.as_bytes().to_vec()));
        // Too short -> None.
        assert_eq!(decode_key("short"), None);
    }
}
