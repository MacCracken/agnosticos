//! Marketplace Trust & Signing
//!
//! Ed25519 package signing and verification. Publishers sign their bundles
//! offline; the registry and local installer verify signatures before use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Key management
// ---------------------------------------------------------------------------

/// A versioned publisher key with validity window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVersion {
    /// Short hex identifier (first 8 bytes of public key).
    pub key_id: String,
    /// When this key became valid.
    pub valid_from: DateTime<Utc>,
    /// When this key expires (None = no expiry).
    pub valid_until: Option<DateTime<Utc>>,
    /// Ed25519 public key bytes (32 bytes, hex-encoded for serialization).
    pub public_key_hex: String,
}

impl KeyVersion {
    /// Check if the key is valid at the given time.
    pub fn is_valid_at(&self, when: DateTime<Utc>) -> bool {
        if when < self.valid_from {
            return false;
        }
        if let Some(until) = self.valid_until {
            if when > until {
                return false;
            }
        }
        true
    }

    /// Decode the public key from hex.
    pub fn verifying_key(&self) -> Result<VerifyingKey> {
        let bytes = hex::decode(&self.public_key_hex)
            .context("Invalid hex in public key")?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Public key must be 32 bytes"))?;
        VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key: {}", e))
    }
}

/// Keyring storing trusted publisher public keys.
pub struct PublisherKeyring {
    /// Keys indexed by key_id.
    keys: HashMap<String, Vec<KeyVersion>>,
    /// Directory where key files are stored.
    keys_dir: PathBuf,
}

impl PublisherKeyring {
    /// Create a new keyring backed by the given directory.
    pub fn new(keys_dir: &Path) -> Self {
        Self {
            keys: HashMap::new(),
            keys_dir: keys_dir.to_path_buf(),
        }
    }

    /// Load all key files from the keys directory.
    pub fn load(&mut self) -> Result<usize> {
        self.keys.clear();
        let mut count = 0;

        if !self.keys_dir.exists() {
            return Ok(0);
        }

        let entries = std::fs::read_dir(&self.keys_dir)
            .with_context(|| format!("Failed to read keys directory: {}", self.keys_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        match serde_json::from_str::<Vec<KeyVersion>>(&content) {
                            Ok(versions) => {
                                for kv in &versions {
                                    self.keys
                                        .entry(kv.key_id.clone())
                                        .or_default()
                                        .push(kv.clone());
                                    count += 1;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Skipping malformed key file {}: {}", path.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read key file {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(count)
    }

    /// Add a key version to the keyring (in-memory only).
    pub fn add_key(&mut self, key: KeyVersion) {
        self.keys.entry(key.key_id.clone()).or_default().push(key);
    }

    /// Look up the current valid key for a given key_id.
    pub fn get_current_key(&self, key_id: &str) -> Option<&KeyVersion> {
        let now = Utc::now();
        self.keys
            .get(key_id)?
            .iter()
            .find(|k| k.is_valid_at(now))
    }

    /// Get all key versions for a key_id (including expired).
    pub fn get_all_versions(&self, key_id: &str) -> Vec<&KeyVersion> {
        self.keys
            .get(key_id)
            .map(|versions| versions.iter().collect())
            .unwrap_or_default()
    }

    /// Number of distinct key IDs.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

/// Compute SHA-256 hash of arbitrary data.
pub fn hash_data(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Sign data with an Ed25519 signing key. Returns the signature bytes.
pub fn sign_data(data: &[u8], signing_key: &SigningKey) -> Vec<u8> {
    let signature = signing_key.sign(data);
    signature.to_bytes().to_vec()
}

/// Verify a signature against data and a verifying key.
pub fn verify_signature(data: &[u8], signature_bytes: &[u8], verifying_key: &VerifyingKey) -> Result<()> {
    let sig_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Signature must be 64 bytes"))?;
    let signature = Signature::from_bytes(&sig_bytes);
    verifying_key
        .verify(data, &signature)
        .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))
}

/// Generate a new Ed25519 keypair. Returns (signing_key, verifying_key, key_id).
pub fn generate_keypair() -> (SigningKey, VerifyingKey, String) {
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    let key_id = hex::encode(&verifying_key.to_bytes()[..8]);
    (signing_key, verifying_key, key_id)
}

/// Derive key_id from a verifying key (hex of first 8 bytes).
pub fn key_id_from_verifying_key(vk: &VerifyingKey) -> String {
    hex::encode(&vk.to_bytes()[..8])
}

// We need hex encoding — use a minimal inline implementation to avoid adding
// the `hex` crate. (sha2 already gives us hex via format!, but we need decode too.)
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, anyhow::Error> {
        if s.len() % 2 != 0 {
            return Err(anyhow::anyhow!("Hex string has odd length"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| anyhow::anyhow!("Invalid hex: {}", e))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let (sk, vk, key_id) = generate_keypair();
        assert_eq!(key_id.len(), 16); // 8 bytes = 16 hex chars
        assert_eq!(key_id, key_id_from_verifying_key(&vk));
        // Verify signing key matches verifying key
        assert_eq!(sk.verifying_key(), vk);
    }

    #[test]
    fn test_sign_and_verify() {
        let (sk, vk, _) = generate_keypair();
        let data = b"hello marketplace";
        let sig = sign_data(data, &sk);
        assert_eq!(sig.len(), 64);
        assert!(verify_signature(data, &sig, &vk).is_ok());
    }

    #[test]
    fn test_verify_wrong_data() {
        let (sk, vk, _) = generate_keypair();
        let sig = sign_data(b"original", &sk);
        assert!(verify_signature(b"tampered", &sig, &vk).is_err());
    }

    #[test]
    fn test_verify_wrong_key() {
        let (sk, _, _) = generate_keypair();
        let (_, vk2, _) = generate_keypair();
        let sig = sign_data(b"data", &sk);
        assert!(verify_signature(b"data", &sig, &vk2).is_err());
    }

    #[test]
    fn test_verify_invalid_signature_length() {
        let (_, vk, _) = generate_keypair();
        assert!(verify_signature(b"data", &[0u8; 32], &vk).is_err());
    }

    #[test]
    fn test_hash_data() {
        let hash = hash_data(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_hash_data_empty() {
        let hash = hash_data(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_key_version_validity() {
        let now = Utc::now();
        let kv = KeyVersion {
            key_id: "test".to_string(),
            valid_from: now - chrono::Duration::hours(1),
            valid_until: Some(now + chrono::Duration::hours(1)),
            public_key_hex: "00".repeat(32),
        };
        assert!(kv.is_valid_at(now));
        assert!(!kv.is_valid_at(now - chrono::Duration::hours(2)));
        assert!(!kv.is_valid_at(now + chrono::Duration::hours(2)));
    }

    #[test]
    fn test_key_version_no_expiry() {
        let now = Utc::now();
        let kv = KeyVersion {
            key_id: "test".to_string(),
            valid_from: now - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: "00".repeat(32),
        };
        assert!(kv.is_valid_at(now));
        assert!(kv.is_valid_at(now + chrono::Duration::days(365)));
    }

    #[test]
    fn test_key_version_not_yet_valid() {
        let now = Utc::now();
        let kv = KeyVersion {
            key_id: "test".to_string(),
            valid_from: now + chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: "00".repeat(32),
        };
        assert!(!kv.is_valid_at(now));
    }

    #[test]
    fn test_key_version_verifying_key() {
        let (_, vk, _) = generate_keypair();
        let kv = KeyVersion {
            key_id: "test".to_string(),
            valid_from: Utc::now(),
            valid_until: None,
            public_key_hex: hex::encode(&vk.to_bytes()),
        };
        let recovered = kv.verifying_key().unwrap();
        assert_eq!(recovered, vk);
    }

    #[test]
    fn test_key_version_invalid_hex() {
        let kv = KeyVersion {
            key_id: "test".to_string(),
            valid_from: Utc::now(),
            valid_until: None,
            public_key_hex: "not-hex".to_string(),
        };
        assert!(kv.verifying_key().is_err());
    }

    #[test]
    fn test_key_version_wrong_length() {
        let kv = KeyVersion {
            key_id: "test".to_string(),
            valid_from: Utc::now(),
            valid_until: None,
            public_key_hex: "aabb".to_string(),
        };
        assert!(kv.verifying_key().is_err());
    }

    #[test]
    fn test_keyring_empty() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = PublisherKeyring::new(dir.path());
        assert!(keyring.is_empty());
        assert_eq!(keyring.len(), 0);
    }

    #[test]
    fn test_keyring_add_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let mut keyring = PublisherKeyring::new(dir.path());
        let (_, vk, key_id) = generate_keypair();

        keyring.add_key(KeyVersion {
            key_id: key_id.clone(),
            valid_from: Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: hex::encode(&vk.to_bytes()),
        });

        assert_eq!(keyring.len(), 1);
        let found = keyring.get_current_key(&key_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().key_id, key_id);
    }

    #[test]
    fn test_keyring_expired_key() {
        let dir = tempfile::tempdir().unwrap();
        let mut keyring = PublisherKeyring::new(dir.path());
        let now = Utc::now();

        keyring.add_key(KeyVersion {
            key_id: "expired".to_string(),
            valid_from: now - chrono::Duration::hours(2),
            valid_until: Some(now - chrono::Duration::hours(1)),
            public_key_hex: "00".repeat(32),
        });

        assert!(keyring.get_current_key("expired").is_none());
        assert_eq!(keyring.get_all_versions("expired").len(), 1);
    }

    #[test]
    fn test_keyring_load_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        let (_, vk, key_id) = generate_keypair();

        let keys = vec![KeyVersion {
            key_id: key_id.clone(),
            valid_from: Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: hex::encode(&vk.to_bytes()),
        }];

        let key_file = dir.path().join("publisher.json");
        std::fs::write(&key_file, serde_json::to_string(&keys).unwrap()).unwrap();

        let mut keyring = PublisherKeyring::new(dir.path());
        let count = keyring.load().unwrap();
        assert_eq!(count, 1);
        assert!(keyring.get_current_key(&key_id).is_some());
    }

    #[test]
    fn test_keyring_load_nonexistent_dir() {
        let mut keyring = PublisherKeyring::new(Path::new("/tmp/nonexistent_keyring_dir_12345"));
        let count = keyring.load().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_keyring_load_malformed_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.json"), "not valid json").unwrap();
        let mut keyring = PublisherKeyring::new(dir.path());
        let count = keyring.load().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_keyring_get_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = PublisherKeyring::new(dir.path());
        assert!(keyring.get_current_key("nope").is_none());
        assert!(keyring.get_all_versions("nope").is_empty());
    }

    #[test]
    fn test_hex_roundtrip() {
        let data = vec![0u8, 1, 15, 16, 255];
        let encoded = hex::encode(&data);
        assert_eq!(encoded, "00010f10ff");
        let decoded = hex::decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_decode_error() {
        assert!(hex::decode("zz").is_err());
        assert!(hex::decode("abc").is_err()); // odd length
    }

    #[test]
    fn test_sign_verify_roundtrip_with_keyring() {
        let (sk, vk, key_id) = generate_keypair();
        let dir = tempfile::tempdir().unwrap();
        let mut keyring = PublisherKeyring::new(dir.path());

        keyring.add_key(KeyVersion {
            key_id: key_id.clone(),
            valid_from: Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: hex::encode(&vk.to_bytes()),
        });

        let data = b"package content hash";
        let sig = sign_data(data, &sk);

        let kv = keyring.get_current_key(&key_id).unwrap();
        let recovered_vk = kv.verifying_key().unwrap();
        assert!(verify_signature(data, &sig, &recovered_vk).is_ok());
    }
}
