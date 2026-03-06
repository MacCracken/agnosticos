//! Secrets Management — Vault-style secret injection for agents
//!
//! Provides multiple backends for secret storage and a `SecretInjector` that
//! populates agent environments before spawn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::{AgnosError, Result};

/// A secret value with best-effort zeroing on drop.
///
/// `Clone` is deliberately not derived — secrets should be passed by reference
/// to avoid uncontrolled copies in memory. Use `SecretValue::duplicate()` if
/// an explicit copy is genuinely needed (e.g. injecting into a child process).
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretValue {
    /// The raw secret bytes, base64-encoded for serialisation safety.
    pub data: String,
    /// Optional metadata (e.g. rotation date, owner).
    pub metadata: HashMap<String, String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SecretValue {
    /// Explicit, auditable copy for cases that genuinely need an owned value.
    pub fn duplicate(&self) -> Self {
        Self {
            data: self.data.clone(),
            metadata: self.metadata.clone(),
            created_at: self.created_at,
        }
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        // Best-effort zeroing of secret data.
        // SAFETY: We overwrite the String's bytes in place before it is freed.
        // The compiler may optimise this away; for production use, integrate
        // the `zeroize` crate.
        unsafe {
            let bytes = self.data.as_bytes_mut();
            std::ptr::write_bytes(bytes.as_mut_ptr(), 0, bytes.len());
        }
    }
}

/// Trait for pluggable secret backends.
#[async_trait]
pub trait SecretBackend: Send + Sync {
    /// Retrieve a secret by key.
    async fn get_secret(&self, key: &str) -> Result<Option<SecretValue>>;
    /// Store or update a secret.
    async fn set_secret(&self, key: &str, value: SecretValue) -> Result<()>;
    /// Delete a secret.
    async fn delete_secret(&self, key: &str) -> Result<bool>;
    /// List all secret keys (never returns values).
    async fn list_secrets(&self) -> Result<Vec<String>>;
}

// ---------------------------------------------------------------------------
// Environment Variable Backend
// ---------------------------------------------------------------------------

/// Reads secrets from environment variables.  Useful for dev/CI.
///
/// Keys are upper-cased and prefixed with `AGNOS_SECRET_`.
pub struct EnvSecretBackend {
    prefix: String,
}

impl EnvSecretBackend {
    pub fn new() -> Self {
        Self {
            prefix: "AGNOS_SECRET_".to_string(),
        }
    }

    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    fn env_key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key.to_uppercase().replace('-', "_"))
    }
}

impl Default for EnvSecretBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretBackend for EnvSecretBackend {
    async fn get_secret(&self, key: &str) -> Result<Option<SecretValue>> {
        let env_key = self.env_key(key);
        match std::env::var(&env_key) {
            Ok(val) => {
                debug!("Secret '{}' read from env var '{}'", key, env_key);
                Ok(Some(SecretValue {
                    data: val,
                    metadata: HashMap::new(),
                    created_at: chrono::Utc::now(),
                }))
            }
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(e) => Err(AgnosError::Unknown(format!(
                "Failed to read env var '{}': {}",
                env_key, e
            ))),
        }
    }

    async fn set_secret(&self, key: &str, value: SecretValue) -> Result<()> {
        let env_key = self.env_key(key);
        // Safety: setting env vars is inherently process-global.
        // This is acceptable for dev/test backends.
        unsafe {
            std::env::set_var(&env_key, &value.data);
        }
        debug!("Secret '{}' written to env var '{}'", key, env_key);
        Ok(())
    }

    async fn delete_secret(&self, key: &str) -> Result<bool> {
        let env_key = self.env_key(key);
        let existed = std::env::var(&env_key).is_ok();
        if existed {
            unsafe {
                std::env::remove_var(&env_key);
            }
        }
        Ok(existed)
    }

    async fn list_secrets(&self) -> Result<Vec<String>> {
        let keys: Vec<String> = std::env::vars()
            .filter_map(|(k, _)| {
                k.strip_prefix(&self.prefix)
                    .map(|rest| rest.to_lowercase().replace('_', "-"))
            })
            .collect();
        Ok(keys)
    }
}

// ---------------------------------------------------------------------------
// Encrypted File Backend (AES-256-GCM)
// ---------------------------------------------------------------------------

/// AES-256-GCM encrypted file store.
///
/// Each secret is stored as a separate file under `base_dir`, with the
/// filename derived from the key.  The encryption key must be 32 bytes.
pub struct FileSecretBackend {
    base_dir: PathBuf,
    /// 32-byte encryption key.
    encryption_key: [u8; 32],
}

impl FileSecretBackend {
    /// Create a new file backend.
    ///
    /// `encryption_key` must be exactly 32 bytes (AES-256).
    pub fn new(base_dir: &Path, encryption_key: [u8; 32]) -> Result<Self> {
        Ok(Self {
            base_dir: base_dir.to_path_buf(),
            encryption_key,
        })
    }

    fn secret_path(&self, key: &str) -> PathBuf {
        // Sanitise key to prevent path traversal
        let safe_key: String = key
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.base_dir.join(format!("{}.secret", safe_key))
    }

    /// Encrypt plaintext with AES-256-GCM.
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| AgnosError::Unknown(format!("AES key error: {}", e)))?;

        // Generate random 12-byte nonce
        let mut nonce_bytes = [0u8; 12];
        getrandom::getrandom(&mut nonce_bytes)
            .map_err(|e| AgnosError::Unknown(format!("RNG error: {}", e)))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AgnosError::Unknown(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut output = nonce_bytes.to_vec();
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Decrypt ciphertext with AES-256-GCM.
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        if data.len() < 12 {
            return Err(AgnosError::Unknown("Ciphertext too short".to_string()));
        }

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| AgnosError::Unknown(format!("AES key error: {}", e)))?;

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AgnosError::Unknown(format!("Decryption failed: {}", e)))
    }
}

#[async_trait]
impl SecretBackend for FileSecretBackend {
    async fn get_secret(&self, key: &str) -> Result<Option<SecretValue>> {
        let path = self.secret_path(key);
        if !path.exists() {
            return Ok(None);
        }

        let encrypted = tokio::fs::read(&path).await?;
        let plaintext = self.decrypt(&encrypted)?;
        let value: SecretValue = serde_json::from_slice(&plaintext)?;

        debug!("Secret '{}' read from file", key);
        Ok(Some(value))
    }

    async fn set_secret(&self, key: &str, value: SecretValue) -> Result<()> {
        // Ensure directory exists
        if !self.base_dir.exists() {
            tokio::fs::create_dir_all(&self.base_dir).await?;
            // Restrict directory permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                tokio::fs::set_permissions(&self.base_dir, perms).await?;
            }
        }

        let plaintext = serde_json::to_vec(&value)?;
        let encrypted = self.encrypt(&plaintext)?;

        let path = self.secret_path(key);
        tokio::fs::write(&path, &encrypted).await?;

        // Restrict file permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&path, perms).await?;
        }

        debug!("Secret '{}' written to file", key);
        Ok(())
    }

    async fn delete_secret(&self, key: &str) -> Result<bool> {
        let path = self.secret_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
            debug!("Secret '{}' deleted from file", key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_secrets(&self) -> Result<Vec<String>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(key) = name.strip_suffix(".secret") {
                keys.push(key.to_string());
            }
        }
        Ok(keys)
    }
}

// ---------------------------------------------------------------------------
// HashiCorp Vault Backend
// ---------------------------------------------------------------------------

/// HTTP client for HashiCorp Vault's KV v2 API.
pub struct VaultSecretBackend {
    client: reqwest::Client,
    addr: String,
    token: String,
    mount: String,
}

impl VaultSecretBackend {
    /// Create a new Vault backend.
    ///
    /// - `addr`: e.g. `http://127.0.0.1:8200`
    /// - `token`: Vault auth token
    /// - `mount`: KV v2 mount path (default `secret`)
    pub fn new(addr: &str, token: &str, mount: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            addr: addr.trim_end_matches('/').to_string(),
            token: token.to_string(),
            mount: mount.to_string(),
        }
    }

    fn data_url(&self, key: &str) -> String {
        format!("{}/v1/{}/data/{}", self.addr, self.mount, key)
    }

    fn metadata_url(&self, key: &str) -> String {
        format!("{}/v1/{}/metadata/{}", self.addr, self.mount, key)
    }
}

#[async_trait]
impl SecretBackend for VaultSecretBackend {
    async fn get_secret(&self, key: &str) -> Result<Option<SecretValue>> {
        let resp = self
            .client
            .get(self.data_url(key))
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .map_err(|e| AgnosError::Unknown(format!("Vault request failed: {}", e)))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(AgnosError::Unknown(format!(
                "Vault returned status {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AgnosError::Unknown(format!("Vault response parse error: {}", e)))?;

        let data = &body["data"]["data"];
        let secret_data = data["value"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let mut metadata = HashMap::new();
        if let Some(obj) = data.as_object() {
            for (k, v) in obj {
                if k != "value" {
                    metadata.insert(k.clone(), v.as_str().unwrap_or_default().to_string());
                }
            }
        }

        debug!("Secret '{}' read from Vault", key);
        Ok(Some(SecretValue {
            data: secret_data,
            metadata,
            created_at: chrono::Utc::now(),
        }))
    }

    async fn set_secret(&self, key: &str, value: SecretValue) -> Result<()> {
        let mut payload = serde_json::Map::new();
        payload.insert("value".to_string(), serde_json::Value::String(value.data.clone()));
        for (k, v) in &value.metadata {
            payload.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        let body = serde_json::json!({ "data": payload });

        let resp = self
            .client
            .post(self.data_url(key))
            .header("X-Vault-Token", &self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgnosError::Unknown(format!("Vault request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(AgnosError::Unknown(format!(
                "Vault write failed with status {}",
                resp.status()
            )));
        }

        info!("Secret '{}' written to Vault", key);
        Ok(())
    }

    async fn delete_secret(&self, key: &str) -> Result<bool> {
        let resp = self
            .client
            .delete(self.metadata_url(key))
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .map_err(|e| AgnosError::Unknown(format!("Vault request failed: {}", e)))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !resp.status().is_success() {
            return Err(AgnosError::Unknown(format!(
                "Vault delete failed with status {}",
                resp.status()
            )));
        }

        info!("Secret '{}' deleted from Vault", key);
        Ok(true)
    }

    async fn list_secrets(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/{}/metadata/?list=true", self.addr, self.mount);
        let resp = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .map_err(|e| AgnosError::Unknown(format!("Vault request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AgnosError::Unknown(format!("Vault response parse error: {}", e)))?;

        let keys = body["data"]["keys"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(keys)
    }
}

// ---------------------------------------------------------------------------
// Secret Injector
// ---------------------------------------------------------------------------

/// Injects secrets into an agent's environment variables before spawn.
pub struct SecretInjector {
    backend: Box<dyn SecretBackend>,
}

impl SecretInjector {
    pub fn new(backend: Box<dyn SecretBackend>) -> Self {
        Self { backend }
    }

    /// Resolve a list of secret keys and return them as environment variable
    /// mappings (`ENV_VAR_NAME` → `value`).
    ///
    /// `mappings` maps secret key → env var name.
    pub async fn resolve(
        &self,
        mappings: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>> {
        let mut env = HashMap::new();

        for (secret_key, env_var) in mappings {
            match self.backend.get_secret(secret_key).await? {
                Some(secret) => {
                    env.insert(env_var.clone(), secret.data.clone());
                    debug!("Injected secret '{}' as env '{}'", secret_key, env_var);
                }
                None => {
                    warn!("Secret '{}' not found — skipping", secret_key);
                }
            }
        }

        info!(
            "Resolved {}/{} secrets for injection",
            env.len(),
            mappings.len()
        );
        Ok(env)
    }

    /// Convenience: inject all resolved secrets into the current process
    /// environment (for testing / simple single-process agents).
    pub async fn inject_into_env(&self, mappings: &HashMap<String, String>) -> Result<()> {
        let resolved = self.resolve(mappings).await?;
        for (k, v) in &resolved {
            // Safety: setting env vars is inherently process-global.
            unsafe {
                std::env::set_var(k, v);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_env_backend_set_get_delete() {
        let backend = EnvSecretBackend::with_prefix("TEST_SEC_");

        // Set
        let val = SecretValue {
            data: "my-secret-value".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("db-password", val).await.unwrap();

        // Get
        let retrieved = backend.get_secret("db-password").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().data, "my-secret-value");

        // Delete
        let deleted = backend.delete_secret("db-password").await.unwrap();
        assert!(deleted);

        // Verify gone
        let gone = backend.get_secret("db-password").await.unwrap();
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn test_env_backend_list() {
        let backend = EnvSecretBackend::with_prefix("LISTTEST_");

        let val = SecretValue {
            data: "v".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("alpha", val.duplicate()).await.unwrap();
        backend.set_secret("beta", val).await.unwrap();

        let keys = backend.list_secrets().await.unwrap();
        assert!(keys.contains(&"alpha".to_string()));
        assert!(keys.contains(&"beta".to_string()));

        // Cleanup
        backend.delete_secret("alpha").await.unwrap();
        backend.delete_secret("beta").await.unwrap();
    }

    #[tokio::test]
    async fn test_env_backend_missing_key() {
        let backend = EnvSecretBackend::with_prefix("MISS_");
        let result = backend.get_secret("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_file_backend_roundtrip() {
        let dir = std::env::temp_dir().join("agnos_secret_test");
        let _ = std::fs::remove_dir_all(&dir);

        let key = [0x42u8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();

        let val = SecretValue {
            data: "file-secret".to_string(),
            metadata: HashMap::from([("owner".to_string(), "test".to_string())]),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("api-key", val).await.unwrap();

        let retrieved = backend.get_secret("api-key").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.data, "file-secret");
        assert_eq!(retrieved.metadata.get("owner").unwrap(), "test");

        // List
        let keys = backend.list_secrets().await.unwrap();
        assert!(keys.contains(&"api-key".to_string()));

        // Delete
        assert!(backend.delete_secret("api-key").await.unwrap());
        assert!(!backend.delete_secret("api-key").await.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_backend_decrypt_tampered() {
        let dir = std::env::temp_dir().join("agnos_secret_tamper_test");
        let _ = std::fs::remove_dir_all(&dir);

        let key = [0x55u8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();

        let val = SecretValue {
            data: "sensitive".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("tamper", val).await.unwrap();

        // Tamper with the encrypted file
        let path = backend.secret_path("tamper");
        let mut data = std::fs::read(&path).unwrap();
        if let Some(last) = data.last_mut() {
            *last ^= 0xFF;
        }
        std::fs::write(&path, &data).unwrap();

        // Decryption should fail
        let result = backend.get_secret("tamper").await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_secret_injector_resolve() {
        let backend = EnvSecretBackend::with_prefix("INJ_");

        // Pre-populate a secret
        let val = SecretValue {
            data: "injected-value".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("db-pass", val).await.unwrap();

        let injector = SecretInjector::new(Box::new(EnvSecretBackend::with_prefix("INJ_")));

        let mut mappings = HashMap::new();
        mappings.insert("db-pass".to_string(), "DATABASE_PASSWORD".to_string());
        mappings.insert("missing-key".to_string(), "SHOULD_SKIP".to_string());

        let resolved = injector.resolve(&mappings).await.unwrap();
        assert_eq!(resolved.get("DATABASE_PASSWORD").unwrap(), "injected-value");
        assert!(!resolved.contains_key("SHOULD_SKIP"));

        // Cleanup
        let backend = EnvSecretBackend::with_prefix("INJ_");
        backend.delete_secret("db-pass").await.unwrap();
    }

    #[tokio::test]
    async fn test_secret_value_serialization() {
        let val = SecretValue {
            data: "test".to_string(),
            metadata: HashMap::from([("k".to_string(), "v".to_string())]),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: SecretValue = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.data, "test");
        assert_eq!(deserialized.metadata.get("k").unwrap(), "v");
    }

    #[test]
    fn test_file_backend_path_sanitization() {
        let key = [0u8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp/secrets"), key).unwrap();

        // Path traversal attempt should be sanitised
        let path = backend.secret_path("../../../etc/passwd");
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_vault_backend_urls() {
        let backend = VaultSecretBackend::new("http://127.0.0.1:8200", "token", "secret");
        assert_eq!(
            backend.data_url("my-key"),
            "http://127.0.0.1:8200/v1/secret/data/my-key"
        );
        assert_eq!(
            backend.metadata_url("my-key"),
            "http://127.0.0.1:8200/v1/secret/metadata/my-key"
        );
    }

    #[test]
    fn test_vault_backend_trailing_slash() {
        let backend = VaultSecretBackend::new("http://127.0.0.1:8200/", "token", "kv");
        assert_eq!(
            backend.data_url("key"),
            "http://127.0.0.1:8200/v1/kv/data/key"
        );
    }

    // --- Additional secrets.rs coverage tests ---

    #[test]
    fn test_env_backend_new_default_prefix() {
        let backend = EnvSecretBackend::new();
        assert_eq!(backend.prefix, "AGNOS_SECRET_");
    }

    #[test]
    fn test_env_backend_default_trait() {
        let backend = EnvSecretBackend::default();
        assert_eq!(backend.prefix, "AGNOS_SECRET_");
    }

    #[test]
    fn test_env_backend_custom_prefix() {
        let backend = EnvSecretBackend::with_prefix("CUSTOM_");
        assert_eq!(backend.prefix, "CUSTOM_");
    }

    #[test]
    fn test_env_backend_env_key_formatting() {
        let backend = EnvSecretBackend::with_prefix("PFX_");
        // Dashes become underscores, lowercase becomes uppercase
        assert_eq!(backend.env_key("my-secret"), "PFX_MY_SECRET");
        assert_eq!(backend.env_key("ALREADY_UPPER"), "PFX_ALREADY_UPPER");
        assert_eq!(backend.env_key("mix-Case_key"), "PFX_MIX_CASE_KEY");
    }

    #[tokio::test]
    async fn test_env_backend_delete_nonexistent() {
        let backend = EnvSecretBackend::with_prefix("DEL_MISS_");
        let deleted = backend.delete_secret("no-such-key").await.unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_file_backend_new_construction() {
        let key = [0xAAu8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp/test_secrets"), key);
        assert!(backend.is_ok());
        let backend = backend.unwrap();
        assert_eq!(backend.base_dir, PathBuf::from("/tmp/test_secrets"));
        assert_eq!(backend.encryption_key, [0xAAu8; 32]);
    }

    #[test]
    fn test_file_backend_secret_path_various_keys() {
        let key = [0u8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp/secrets"), key).unwrap();

        // Normal key
        let path = backend.secret_path("api-key_1");
        assert_eq!(path, PathBuf::from("/tmp/secrets/api-key_1.secret"));

        // Special characters get sanitised to underscores
        let path = backend.secret_path("key with spaces");
        assert!(path.to_string_lossy().contains("key_with_spaces.secret"));

        // Dots get sanitised
        let path = backend.secret_path("key.with.dots");
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_file_backend_decrypt_too_short() {
        let key = [0x11u8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        // Data shorter than 12 bytes (nonce length) should fail
        let result = backend.decrypt(&[1, 2, 3]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn test_file_backend_encrypt_decrypt_roundtrip() {
        let key = [0x22u8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        let plaintext = b"hello world secret data";
        let encrypted = backend.encrypt(plaintext).unwrap();
        // Encrypted data should be nonce(12) + ciphertext (longer than plaintext due to tag)
        assert!(encrypted.len() > 12);
        let decrypted = backend.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_file_backend_decrypt_wrong_key() {
        let key1 = [0x33u8; 32];
        let key2 = [0x44u8; 32];
        let backend1 = FileSecretBackend::new(Path::new("/tmp"), key1).unwrap();
        let backend2 = FileSecretBackend::new(Path::new("/tmp"), key2).unwrap();
        let encrypted = backend1.encrypt(b"secret").unwrap();
        let result = backend2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_backend_get_secret_nonexistent() {
        let dir = std::env::temp_dir().join("agnos_secret_nofile_test");
        let _ = std::fs::remove_dir_all(&dir);
        let key = [0x55u8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();
        let result = backend.get_secret("does-not-exist").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_file_backend_list_secrets_empty_dir() {
        let dir = std::env::temp_dir().join("agnos_secret_emptylist_test");
        let _ = std::fs::remove_dir_all(&dir);
        let key = [0x66u8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();
        // Dir doesn't exist yet — should return empty list
        let keys = backend.list_secrets().await.unwrap();
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn test_file_backend_list_secrets_filters_non_secret_files() {
        let dir = std::env::temp_dir().join("agnos_secret_filter_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Create a non-.secret file that should be ignored
        std::fs::write(dir.join("readme.txt"), b"not a secret").unwrap();
        // Create a .secret file
        std::fs::write(dir.join("api-key.secret"), b"fake").unwrap();

        let key = [0x77u8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();
        let keys = backend.list_secrets().await.unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"api-key".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_vault_backend_construction() {
        let backend = VaultSecretBackend::new("http://vault:8200", "s.mytoken", "kv2");
        assert_eq!(backend.addr, "http://vault:8200");
        assert_eq!(backend.token, "s.mytoken");
        assert_eq!(backend.mount, "kv2");
    }

    #[test]
    fn test_vault_backend_data_url_nested_key() {
        let backend = VaultSecretBackend::new("http://127.0.0.1:8200", "token", "secret");
        assert_eq!(
            backend.data_url("path/to/key"),
            "http://127.0.0.1:8200/v1/secret/data/path/to/key"
        );
    }

    #[test]
    fn test_vault_backend_metadata_url_nested_key() {
        let backend = VaultSecretBackend::new("http://127.0.0.1:8200", "token", "secret");
        assert_eq!(
            backend.metadata_url("path/to/key"),
            "http://127.0.0.1:8200/v1/secret/metadata/path/to/key"
        );
    }

    #[tokio::test]
    async fn test_secret_injector_new() {
        let backend = EnvSecretBackend::with_prefix("INJNEW_");
        let injector = SecretInjector::new(Box::new(backend));
        // Just verify construction works — injector wraps the backend
        let empty_mappings = HashMap::new();
        let resolved = injector.resolve(&empty_mappings).await.unwrap();
        assert!(resolved.is_empty());
    }

    #[tokio::test]
    async fn test_secret_injector_resolve_all_missing() {
        let backend = EnvSecretBackend::with_prefix("INJMISS_");
        let injector = SecretInjector::new(Box::new(backend));
        let mut mappings = HashMap::new();
        mappings.insert("nonexistent1".to_string(), "VAR1".to_string());
        mappings.insert("nonexistent2".to_string(), "VAR2".to_string());
        let resolved = injector.resolve(&mappings).await.unwrap();
        // All missing — should be empty
        assert!(resolved.is_empty());
    }

    #[tokio::test]
    async fn test_secret_injector_inject_into_env() {
        let backend = EnvSecretBackend::with_prefix("INJENV_");
        let val = SecretValue {
            data: "injected-env-val".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("inject-test", val).await.unwrap();

        let injector = SecretInjector::new(Box::new(EnvSecretBackend::with_prefix("INJENV_")));
        let mut mappings = HashMap::new();
        mappings.insert("inject-test".to_string(), "MY_INJECTED_VAR".to_string());
        injector.inject_into_env(&mappings).await.unwrap();

        assert_eq!(std::env::var("MY_INJECTED_VAR").unwrap(), "injected-env-val");

        // Cleanup
        unsafe {
            std::env::remove_var("MY_INJECTED_VAR");
        }
        let cleanup = EnvSecretBackend::with_prefix("INJENV_");
        cleanup.delete_secret("inject-test").await.unwrap();
    }

    #[test]
    fn test_secret_value_metadata_access() {
        let mut metadata = HashMap::new();
        metadata.insert("owner".to_string(), "agent-1".to_string());
        metadata.insert("rotation".to_string(), "30d".to_string());
        let val = SecretValue {
            data: "secret-data".to_string(),
            metadata,
            created_at: chrono::Utc::now(),
        };
        assert_eq!(val.metadata.len(), 2);
        assert_eq!(val.metadata.get("owner").unwrap(), "agent-1");
        assert_eq!(val.metadata.get("rotation").unwrap(), "30d");
    }

    #[test]
    fn test_secret_value_duplicate() {
        let val = SecretValue {
            data: "clone-test".to_string(),
            metadata: HashMap::from([("k".to_string(), "v".to_string())]),
            created_at: chrono::Utc::now(),
        };
        let cloned = val.duplicate();
        assert_eq!(cloned.data, val.data);
        assert_eq!(cloned.metadata, val.metadata);
    }

    #[test]
    fn test_secret_value_debug() {
        let val = SecretValue {
            data: "debug-test".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        let debug = format!("{:?}", val);
        assert!(debug.contains("debug-test"));
    }

    #[tokio::test]
    async fn test_injector_partial_resolve() {
        // One key exists, one doesn't — only the existing one should resolve
        let backend = EnvSecretBackend::with_prefix("PARTIAL_");
        let val = SecretValue {
            data: "found-value".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("exists", val).await.unwrap();

        let injector = SecretInjector::new(Box::new(EnvSecretBackend::with_prefix("PARTIAL_")));
        let mut mappings = HashMap::new();
        mappings.insert("exists".to_string(), "FOUND_VAR".to_string());
        mappings.insert("missing".to_string(), "MISSING_VAR".to_string());

        let resolved = injector.resolve(&mappings).await.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved.get("FOUND_VAR").unwrap(), "found-value");
        assert!(!resolved.contains_key("MISSING_VAR"));

        // Cleanup
        let cleanup = EnvSecretBackend::with_prefix("PARTIAL_");
        cleanup.delete_secret("exists").await.unwrap();
    }

    // --- New coverage tests (batch 2) ---

    #[test]
    fn test_env_key_empty_key() {
        let backend = EnvSecretBackend::with_prefix("EMPTYKEY_");
        assert_eq!(backend.env_key(""), "EMPTYKEY_");
    }

    #[test]
    fn test_env_key_special_characters() {
        let backend = EnvSecretBackend::with_prefix("SPEC_");
        // Dashes become underscores, everything uppercased
        assert_eq!(backend.env_key("a-b-c"), "SPEC_A_B_C");
        // Other chars preserved (only dashes mapped)
        assert_eq!(backend.env_key("key.with.dots"), "SPEC_KEY.WITH.DOTS");
    }

    #[test]
    fn test_env_key_all_dashes() {
        let backend = EnvSecretBackend::with_prefix("DASH_");
        assert_eq!(backend.env_key("---"), "DASH____");
    }

    #[tokio::test]
    async fn test_env_backend_overwrite_secret() {
        let backend = EnvSecretBackend::with_prefix("OVERWR_");
        let val1 = SecretValue {
            data: "original".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("key", val1).await.unwrap();

        let val2 = SecretValue {
            data: "updated".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("key", val2).await.unwrap();

        let retrieved = backend.get_secret("key").await.unwrap().unwrap();
        assert_eq!(retrieved.data, "updated");

        backend.delete_secret("key").await.unwrap();
    }

    #[test]
    fn test_file_backend_secret_path_empty_key() {
        let key = [0u8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp/secrets"), key).unwrap();
        let path = backend.secret_path("");
        assert_eq!(path, PathBuf::from("/tmp/secrets/.secret"));
    }

    #[test]
    fn test_file_backend_secret_path_unicode() {
        let key = [0u8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp/secrets"), key).unwrap();
        // 'é' is alphanumeric in Rust, so it passes the filter
        let path = backend.secret_path("café");
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with(".secret"));
        // Characters like '!' should be sanitized to '_'
        let path2 = backend.secret_path("key!@#$");
        let name2 = path2.file_name().unwrap().to_string_lossy();
        assert!(!name2.contains('!'));
        assert!(!name2.contains('@'));
        assert!(!name2.contains('#'));
        assert!(!name2.contains('$'));
        assert!(name2.ends_with(".secret"));
    }

    #[test]
    fn test_file_backend_encrypt_produces_different_ciphertext_each_time() {
        let key = [0xABu8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        let plaintext = b"same plaintext";
        let enc1 = backend.encrypt(plaintext).unwrap();
        let enc2 = backend.encrypt(plaintext).unwrap();
        // Random nonce means different ciphertext each time
        assert_ne!(enc1, enc2);
        // But both decrypt to same plaintext
        assert_eq!(backend.decrypt(&enc1).unwrap(), plaintext);
        assert_eq!(backend.decrypt(&enc2).unwrap(), plaintext);
    }

    #[test]
    fn test_file_backend_decrypt_exactly_12_bytes() {
        let key = [0xCCu8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        // Exactly 12 bytes = nonce only, no ciphertext — should fail (auth tag missing)
        let result = backend.decrypt(&[0u8; 12]);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_backend_decrypt_13_bytes_invalid() {
        let key = [0xDDu8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        // 13 bytes = nonce + 1 byte garbage — should fail decryption
        let result = backend.decrypt(&[0u8; 13]);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_backend_encrypt_empty_plaintext() {
        let key = [0xEEu8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        let encrypted = backend.encrypt(b"").unwrap();
        // Even empty plaintext produces nonce + auth tag
        assert!(encrypted.len() > 12);
        let decrypted = backend.decrypt(&encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_file_backend_encrypt_large_plaintext() {
        let key = [0xFFu8; 32];
        let backend = FileSecretBackend::new(Path::new("/tmp"), key).unwrap();
        let plaintext = vec![0x42u8; 10_000];
        let encrypted = backend.encrypt(&plaintext).unwrap();
        let decrypted = backend.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_file_backend_multiple_secrets() {
        let dir = std::env::temp_dir().join("agnos_secret_multi_test");
        let _ = std::fs::remove_dir_all(&dir);

        let key = [0x99u8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();

        for i in 0..5 {
            let val = SecretValue {
                data: format!("secret-{}", i),
                metadata: HashMap::new(),
                created_at: chrono::Utc::now(),
            };
            backend.set_secret(&format!("key-{}", i), val).await.unwrap();
        }

        let keys = backend.list_secrets().await.unwrap();
        assert_eq!(keys.len(), 5);

        for i in 0..5 {
            let retrieved = backend.get_secret(&format!("key-{}", i)).await.unwrap().unwrap();
            assert_eq!(retrieved.data, format!("secret-{}", i));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_vault_backend_custom_mount() {
        let backend = VaultSecretBackend::new("http://vault:8200", "tok", "custom-mount");
        assert_eq!(
            backend.data_url("k"),
            "http://vault:8200/v1/custom-mount/data/k"
        );
        assert_eq!(
            backend.metadata_url("k"),
            "http://vault:8200/v1/custom-mount/metadata/k"
        );
    }

    #[test]
    fn test_vault_backend_multiple_trailing_slashes() {
        let backend = VaultSecretBackend::new("http://vault:8200///", "tok", "secret");
        // Only last slash is trimmed by trim_end_matches
        assert_eq!(backend.addr, "http://vault:8200");
    }

    #[test]
    fn test_vault_backend_empty_key() {
        let backend = VaultSecretBackend::new("http://vault:8200", "tok", "secret");
        assert_eq!(
            backend.data_url(""),
            "http://vault:8200/v1/secret/data/"
        );
    }

    #[test]
    fn test_secret_value_empty_data() {
        let val = SecretValue {
            data: String::new(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: SecretValue = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.data, "");
        assert!(deserialized.metadata.is_empty());
    }

    #[test]
    fn test_secret_value_large_metadata() {
        let mut metadata = HashMap::new();
        for i in 0..100 {
            metadata.insert(format!("key-{}", i), format!("value-{}", i));
        }
        let val = SecretValue {
            data: "d".to_string(),
            metadata,
            created_at: chrono::Utc::now(),
        };
        assert_eq!(val.metadata.len(), 100);
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: SecretValue = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.metadata.len(), 100);
    }

    #[test]
    fn test_secret_value_timestamp_preserved() {
        let now = chrono::Utc::now();
        let val = SecretValue {
            data: "ts-test".to_string(),
            metadata: HashMap::new(),
            created_at: now,
        };
        let json = serde_json::to_string(&val).unwrap();
        let deserialized: SecretValue = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.created_at, now);
    }

    #[tokio::test]
    async fn test_env_backend_list_with_custom_prefix_no_match() {
        let backend = EnvSecretBackend::with_prefix("NOMATCH_UNIQUE_PFX_");
        let keys = backend.list_secrets().await.unwrap();
        // Should return empty or at least not contain random env vars
        for key in &keys {
            // All returned keys should have been from our prefix
            assert!(!key.is_empty());
        }
    }

    #[tokio::test]
    async fn test_injector_empty_mappings() {
        let backend = EnvSecretBackend::with_prefix("EMPTY_MAP_");
        let injector = SecretInjector::new(Box::new(backend));
        let mappings = HashMap::new();
        let resolved = injector.resolve(&mappings).await.unwrap();
        assert!(resolved.is_empty());
    }

    #[tokio::test]
    async fn test_injector_inject_into_env_empty() {
        let backend = EnvSecretBackend::with_prefix("INJEMPTY_");
        let injector = SecretInjector::new(Box::new(backend));
        let mappings = HashMap::new();
        let result = injector.inject_into_env(&mappings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_backend_overwrite_existing_secret() {
        let dir = std::env::temp_dir().join("agnos_secret_overwrite_test");
        let _ = std::fs::remove_dir_all(&dir);

        let key = [0xBBu8; 32];
        let backend = FileSecretBackend::new(&dir, key).unwrap();

        let val1 = SecretValue {
            data: "first".to_string(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("overwrite-key", val1).await.unwrap();

        let val2 = SecretValue {
            data: "second".to_string(),
            metadata: HashMap::from([("ver".to_string(), "2".to_string())]),
            created_at: chrono::Utc::now(),
        };
        backend.set_secret("overwrite-key", val2).await.unwrap();

        let retrieved = backend.get_secret("overwrite-key").await.unwrap().unwrap();
        assert_eq!(retrieved.data, "second");
        assert_eq!(retrieved.metadata.get("ver").unwrap(), "2");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
