//! Encrypted, persistent store for session bundles.
//!
//! Bundles are serialized to JSON, encrypted with AES-256-GCM and written to
//! `<data_dir>/sessions/<session_id>.json.enc`. The store survives container
//! restarts as long as `<data_dir>` is a persistent volume (`/app/data`).
//! Cookie values are NEVER written in cleartext and NEVER logged.

pub mod crypto;
pub mod restore;
pub mod types;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use tokio::sync::Mutex;
use uuid::Uuid;

pub use types::{Bundle, CookieSpec, FingerprintSpec, ScreenSize, SessionMeta, StorageEntry};

use crypto::SessionCipher;

const ENC_SUFFIX: &str = ".json.enc";

/// Thread-safe handle to the encrypted session store.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<Inner>,
}

struct Inner {
    dir: PathBuf,
    cipher: SessionCipher,
    /// Serializes filesystem writes so concurrent imports don't race.
    write_lock: Mutex<()>,
}

impl SessionStore {
    /// Opens (or initializes) the store under `<data_dir>/sessions/`.
    ///
    /// `data_dir` should be the persistent volume root (`/app/data`); when a
    /// profile path is configured its parent is a reasonable alternative.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("sessions");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating sessions dir {}", dir.display()))?;
        let cipher = SessionCipher::load_or_init(&dir)?;
        Ok(Self {
            inner: Arc::new(Inner {
                dir,
                cipher,
                write_lock: Mutex::new(()),
            }),
        })
    }

    /// Derives the data dir from the configured profile path (its parent) or
    /// falls back to `/app/data` (the Docker volume).
    pub fn open_from_profile(profile_path: Option<&Path>) -> Result<Self> {
        let data_dir = profile_path
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("/app/data"));
        Self::open(&data_dir)
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.inner.dir.join(format!("{}{}", id, ENC_SUFFIX))
    }

    /// Encrypts and persists a bundle, returning the generated session id.
    pub async fn save(&self, bundle: &Bundle) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.save_with_id(&id, bundle).await?;
        Ok(id)
    }

    /// Encrypts and persists a bundle under an explicit id.
    pub async fn save_with_id(&self, id: &str, bundle: &Bundle) -> Result<()> {
        validate_id(id)?;
        let plaintext = serde_json::to_vec(bundle).context("serializing bundle")?;
        let sealed = self.inner.cipher.seal(&plaintext)?;
        let path = self.path_for(id);
        let _guard = self.inner.write_lock.lock().await;
        // Write to a temp file then rename for atomicity.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &sealed)
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("renaming into {}", path.display()))?;
        Ok(())
    }

    /// Loads and decrypts a bundle by id. Returns `Ok(None)` when missing.
    pub async fn load(&self, id: &str) -> Result<Option<Bundle>> {
        validate_id(id)?;
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(None);
        }
        let sealed = std::fs::read(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let plaintext = self.inner.cipher.open(&sealed)?;
        let bundle: Bundle = serde_json::from_slice(&plaintext).context("deserializing bundle")?;
        Ok(Some(bundle))
    }

    /// Deletes a stored session. Returns `true` if a file was removed.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        validate_id(id)?;
        let path = self.path_for(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("removing {}", path.display()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Lists metadata for all stored sessions (no cookie values).
    pub async fn list(&self) -> Result<Vec<SessionMeta>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.inner.dir) {
            Ok(e) => e,
            Err(_) => return Ok(out),
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(id) = name.strip_suffix(ENC_SUFFIX) else { continue };
            if validate_id(id).is_err() {
                continue;
            }
            if let Ok(Some(bundle)) = self.load(id).await {
                out.push(SessionMeta {
                    id: id.to_string(),
                    origin: bundle.origin.clone(),
                    created_at: bundle.created_at.clone(),
                    cookie_count: bundle.cookies.len(),
                    storage_origins: bundle.storage.len(),
                });
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(out)
    }
}

impl std::fmt::Debug for SessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionStore")
            .field("dir", &self.inner.dir)
            .finish()
    }
}

/// Rejects ids that could escape the sessions directory (path traversal).
fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow!("invalid session id"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::types::*;
    use super::*;

    fn sample_bundle() -> Bundle {
        Bundle {
            version: BUNDLE_VERSION,
            created_at: Some("2026-06-11T10:00:00Z".to_string()),
            origin: "https://service.example.com".to_string(),
            cookies: vec![CookieSpec {
                name: "sid".to_string(),
                value: "secret-value".to_string(),
                domain: ".example.com".to_string(),
                path: "/".to_string(),
                secure: true,
                http_only: true,
                same_site: Some("Lax".to_string()),
                expires: Some(1_900_000_000.0),
            }],
            storage: vec![StorageEntry {
                origin: "https://service.example.com".to_string(),
                local: [("token".to_string(), "abc".to_string())].into_iter().collect(),
                session: Default::default(),
            }],
            fingerprint: Some(FingerprintSpec {
                user_agent: Some("Mozilla/5.0 ... Chrome/126.0.0.0 Safari/537.36".to_string()),
                platform: Some("Linux x86_64".to_string()),
                languages: Some(vec!["de-DE".to_string(), "de".to_string()]),
                hardware_concurrency: Some(8),
                device_memory: Some(8),
                screen: Some(ScreenSize { width: 1920, height: 1080 }),
                webgl_vendor: Some("Google Inc. (NVIDIA)".to_string()),
                webgl_renderer: Some("ANGLE (NVIDIA)".to_string()),
                timezone: Some("Europe/Berlin".to_string()),
            }),
        }
    }

    #[test]
    fn test_bundle_serde_roundtrip() {
        let bundle = sample_bundle();
        let json = serde_json::to_string(&bundle).expect("serialize");
        let back: Bundle = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.origin, bundle.origin);
        assert_eq!(back.cookies.len(), 1);
        assert_eq!(back.cookies[0].http_only, true);
        assert_eq!(back.cookies[0].same_site.as_deref(), Some("Lax"));
        assert_eq!(back.storage[0].local.get("token").map(String::as_str), Some("abc"));
    }

    #[test]
    fn test_bundle_deserializes_extension_json() {
        // Shape exactly as the browser extension emits it.
        let raw = r#"{
            "version": 1,
            "created_at": "2026-06-11T00:00:00Z",
            "origin": "https://x.test",
            "cookies": [
                {"name":"a","value":"b","domain":".x.test","path":"/","secure":true,"httpOnly":false,"sameSite":"None","expires":1700000000}
            ],
            "storage": [ {"origin":"https://x.test","local":{"k":"v"},"session":{}} ],
            "fingerprint": {"user_agent":"UA","platform":"Win32","languages":["en-US","en"],
                "hardware_concurrency":4,"device_memory":8,"screen":{"width":1280,"height":720},
                "webgl_vendor":"V","webgl_renderer":"R","timezone":"UTC"}
        }"#;
        let bundle: Bundle = serde_json::from_str(raw).expect("extension json");
        assert_eq!(bundle.cookies[0].same_site.as_deref(), Some("None"));
        let fp = bundle.fingerprint.expect("fingerprint");
        assert!(fp.to_identity_spec().is_some());
    }

    #[test]
    fn test_fingerprint_to_identity_spec_maps_fields() {
        let fp = sample_bundle().fingerprint.unwrap();
        let spec = fp.to_identity_spec().expect("spec");
        match spec {
            crate::api::identity::IdentitySpec::Explicit(o) => {
                assert_eq!(o.platform.as_deref(), Some("Linux x86_64"));
                assert_eq!(o.hardware_concurrency, Some(8));
                assert_eq!(o.screen.as_ref().map(|s| s.width), Some(1920));
                assert_eq!(o.timezone.as_deref(), Some("Europe/Berlin"));
            }
            _ => panic!("expected explicit identity"),
        }
    }

    #[test]
    fn test_empty_fingerprint_is_none() {
        let fp = FingerprintSpec::default();
        assert!(fp.is_empty());
        assert!(fp.to_identity_spec().is_none());
    }

    #[tokio::test]
    async fn test_store_save_load_list_delete_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("ki-sess-test-{}", Uuid::new_v4()));
        let store = SessionStore::open(&tmp).expect("open store");
        let bundle = sample_bundle();

        let id = store.save(&bundle).await.expect("save");
        // Encrypted file must NOT contain the cookie value in cleartext.
        let path = store.path_for(&id);
        let raw = std::fs::read(&path).expect("read enc");
        assert!(
            !raw.windows(b"secret-value".len()).any(|w| w == b"secret-value"),
            "cookie value leaked in cleartext"
        );

        let loaded = store.load(&id).await.expect("load").expect("present");
        assert_eq!(loaded.cookies[0].value, "secret-value");

        let list = store.list().await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].cookie_count, 1);
        assert_eq!(list[0].origin, "https://service.example.com");

        assert!(store.delete(&id).await.expect("delete"));
        assert!(store.load(&id).await.expect("load2").is_none());
        assert!(!store.delete(&id).await.expect("delete2"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_validate_id_rejects_traversal() {
        assert!(validate_id("../etc/passwd").is_err());
        assert!(validate_id("a/b").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_id("valid-id_123").is_ok());
    }
}
