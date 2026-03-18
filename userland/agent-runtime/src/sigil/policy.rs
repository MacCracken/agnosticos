//! Policy enforcement — RevocationList management.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Revocation
// ---------------------------------------------------------------------------

/// A single revocation entry — either a key or a specific artifact hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationEntry {
    /// Revoke all artifacts signed by this key.
    pub key_id: Option<String>,
    /// Revoke a specific artifact by its content hash.
    pub content_hash: Option<String>,
    /// Reason for revocation.
    pub reason: String,
    /// When the revocation was created.
    pub revoked_at: DateTime<Utc>,
    /// Identity of the revoker.
    pub revoked_by: String,
}

/// A list of revoked keys and artifact hashes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevocationList {
    pub(super) entries: Vec<RevocationEntry>,
}

impl RevocationList {
    /// Create an empty revocation list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a revocation entry.
    ///
    /// At least one of `key_id` or `content_hash` must be `Some`,
    /// otherwise an error is returned.
    pub fn add(&mut self, entry: RevocationEntry) -> Result<()> {
        if entry.key_id.is_none() && entry.content_hash.is_none() {
            anyhow::bail!("RevocationEntry must have at least one of key_id or content_hash set");
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Check whether a key ID has been revoked.
    pub fn is_key_revoked(&self, key_id: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.key_id.as_deref() == Some(key_id))
    }

    /// Check whether an artifact content hash has been revoked.
    pub fn is_artifact_revoked(&self, content_hash: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.content_hash.as_deref() == Some(content_hash))
    }

    /// Number of entries in the revocation list.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the revocation list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String> {
        use anyhow::Context;
        serde_json::to_string_pretty(&self.entries).context("Failed to serialize revocation list")
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        use anyhow::Context;
        let entries: Vec<RevocationEntry> =
            serde_json::from_str(json).context("Failed to deserialize revocation list")?;
        Ok(Self { entries })
    }
}
