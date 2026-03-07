//! Marketplace Transparency Log
//!
//! Append-only log of package publish events. Each entry is hash-chained
//! to the previous, preventing silent modification. Similar in concept to
//! Go's sum database and certificate transparency logs.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single entry in the transparency log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Sequence number (0-indexed).
    pub sequence: u64,
    /// When this entry was created.
    pub timestamp: DateTime<Utc>,
    /// Fully-qualified package name (publisher/name).
    pub package: String,
    /// Version published.
    pub version: String,
    /// Publisher key ID that signed the package.
    pub publisher_key_id: String,
    /// SHA-256 hash of the package content.
    pub content_hash: String,
    /// SHA-256 hash of the Ed25519 signature.
    pub signature_hash: String,
    /// Hash of the previous log entry (empty for entry 0).
    pub previous_hash: String,
    /// Hash of this entry (computed over all fields above).
    pub entry_hash: String,
}

impl LogEntry {
    /// Compute the canonical hash for this entry (excludes `entry_hash` itself).
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.sequence.to_be_bytes());
        hasher.update(self.timestamp.to_rfc3339().as_bytes());
        hasher.update(self.package.as_bytes());
        hasher.update(self.version.as_bytes());
        hasher.update(self.publisher_key_id.as_bytes());
        hasher.update(self.content_hash.as_bytes());
        hasher.update(self.signature_hash.as_bytes());
        hasher.update(self.previous_hash.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Verify that `entry_hash` matches the computed hash.
    pub fn verify_self(&self) -> bool {
        self.entry_hash == self.compute_hash()
    }
}

// ---------------------------------------------------------------------------
// Transparency log
// ---------------------------------------------------------------------------

/// Append-only transparency log for package publish events.
pub struct TransparencyLog {
    entries: Vec<LogEntry>,
}

impl TransparencyLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Append a new entry to the log.
    pub fn append(
        &mut self,
        package: String,
        version: String,
        publisher_key_id: String,
        content_hash: String,
        signature_hash: String,
    ) -> &LogEntry {
        let sequence = self.entries.len() as u64;
        let previous_hash = self
            .entries
            .last()
            .map(|e| e.entry_hash.clone())
            .unwrap_or_default();

        let mut entry = LogEntry {
            sequence,
            timestamp: Utc::now(),
            package,
            version,
            publisher_key_id,
            content_hash,
            signature_hash,
            previous_hash,
            entry_hash: String::new(),
        };
        entry.entry_hash = entry.compute_hash();
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// Verify the entire log chain integrity.
    pub fn verify_chain(&self) -> Result<()> {
        for (i, entry) in self.entries.iter().enumerate() {
            // Verify self-hash
            if !entry.verify_self() {
                return Err(anyhow::anyhow!(
                    "Log entry {} has invalid hash (expected {}, got {})",
                    i,
                    entry.compute_hash(),
                    entry.entry_hash
                ));
            }

            // Verify chain link
            if i == 0 {
                if !entry.previous_hash.is_empty() {
                    return Err(anyhow::anyhow!(
                        "First log entry should have empty previous_hash"
                    ));
                }
            } else {
                let prev = &self.entries[i - 1];
                if entry.previous_hash != prev.entry_hash {
                    return Err(anyhow::anyhow!(
                        "Log entry {} chain broken: previous_hash {} != entry {} hash {}",
                        i,
                        entry.previous_hash,
                        i - 1,
                        prev.entry_hash
                    ));
                }
            }

            // Verify sequence
            if entry.sequence != i as u64 {
                return Err(anyhow::anyhow!(
                    "Log entry {} has wrong sequence number {}",
                    i,
                    entry.sequence
                ));
            }
        }
        Ok(())
    }

    /// Look up a specific package+version in the log.
    pub fn find(&self, package: &str, version: &str) -> Option<&LogEntry> {
        self.entries
            .iter()
            .find(|e| e.package == package && e.version == version)
    }

    /// Get all entries for a package.
    pub fn entries_for_package(&self, package: &str) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.package == package)
            .collect()
    }

    /// Get the latest entry.
    pub fn latest(&self) -> Option<&LogEntry> {
        self.entries.last()
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Export the log as JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.entries).context("Failed to serialize transparency log")
    }

    /// Import log entries from JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        let entries: Vec<LogEntry> =
            serde_json::from_str(json).context("Failed to deserialize transparency log")?;
        let log = Self { entries };
        log.verify_chain()?;
        Ok(log)
    }
}

impl Default for TransparencyLog {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_log() {
        let log = TransparencyLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        assert!(log.latest().is_none());
        assert!(log.verify_chain().is_ok());
    }

    #[test]
    fn test_append_single_entry() {
        let mut log = TransparencyLog::new();
        let entry = log.append(
            "acme/scanner".to_string(),
            "1.0.0".to_string(),
            "key123".to_string(),
            "contenthash".to_string(),
            "sighash".to_string(),
        );
        assert_eq!(entry.sequence, 0);
        assert!(entry.previous_hash.is_empty());
        assert!(!entry.entry_hash.is_empty());
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn test_append_chain() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        log.append("a/b".into(), "1.1.0".into(), "k1".into(), "h2".into(), "s2".into());
        log.append("c/d".into(), "0.1.0".into(), "k2".into(), "h3".into(), "s3".into());

        assert_eq!(log.len(), 3);
        assert!(log.verify_chain().is_ok());

        // Second entry links to first
        assert_eq!(
            log.entries[1].previous_hash,
            log.entries[0].entry_hash
        );
        // Third links to second
        assert_eq!(
            log.entries[2].previous_hash,
            log.entries[1].entry_hash
        );
    }

    #[test]
    fn test_verify_chain_tampered_hash() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        log.append("a/b".into(), "1.1.0".into(), "k1".into(), "h2".into(), "s2".into());

        // Tamper with first entry's hash
        log.entries[0].entry_hash = "tampered".to_string();
        assert!(log.verify_chain().is_err());
    }

    #[test]
    fn test_verify_chain_broken_link() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        log.append("a/b".into(), "1.1.0".into(), "k1".into(), "h2".into(), "s2".into());

        // Break chain link (keep self-hash valid but wrong previous)
        log.entries[1].previous_hash = "wrong".to_string();
        // This will also invalidate self-hash
        assert!(log.verify_chain().is_err());
    }

    #[test]
    fn test_verify_chain_wrong_sequence() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());

        log.entries[0].sequence = 5;
        // Self-hash will be invalid
        assert!(log.verify_chain().is_err());
    }

    #[test]
    fn test_find_entry() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        log.append("c/d".into(), "2.0.0".into(), "k2".into(), "h2".into(), "s2".into());

        assert!(log.find("a/b", "1.0.0").is_some());
        assert!(log.find("c/d", "2.0.0").is_some());
        assert!(log.find("a/b", "9.9.9").is_none());
        assert!(log.find("x/y", "1.0.0").is_none());
    }

    #[test]
    fn test_entries_for_package() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        log.append("a/b".into(), "1.1.0".into(), "k1".into(), "h2".into(), "s2".into());
        log.append("c/d".into(), "0.1.0".into(), "k2".into(), "h3".into(), "s3".into());

        assert_eq!(log.entries_for_package("a/b").len(), 2);
        assert_eq!(log.entries_for_package("c/d").len(), 1);
        assert_eq!(log.entries_for_package("x/y").len(), 0);
    }

    #[test]
    fn test_latest() {
        let mut log = TransparencyLog::new();
        assert!(log.latest().is_none());

        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        assert_eq!(log.latest().unwrap().version, "1.0.0");

        log.append("a/b".into(), "2.0.0".into(), "k1".into(), "h2".into(), "s2".into());
        assert_eq!(log.latest().unwrap().version, "2.0.0");
    }

    #[test]
    fn test_entry_verify_self() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        assert!(log.entries[0].verify_self());
    }

    #[test]
    fn test_json_roundtrip() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        log.append("c/d".into(), "2.0.0".into(), "k2".into(), "h2".into(), "s2".into());

        let json = log.to_json().unwrap();
        let recovered = TransparencyLog::from_json(&json).unwrap();
        assert_eq!(recovered.len(), 2);
        assert!(recovered.verify_chain().is_ok());
    }

    #[test]
    fn test_json_import_tampered() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());

        let mut json = log.to_json().unwrap();
        // Tamper with content
        json = json.replace("h1", "tampered");
        assert!(TransparencyLog::from_json(&json).is_err());
    }

    #[test]
    fn test_json_import_invalid() {
        assert!(TransparencyLog::from_json("not json").is_err());
    }

    #[test]
    fn test_entry_hash_deterministic() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        let hash1 = log.entries[0].compute_hash();
        let hash2 = log.entries[0].compute_hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_first_entry_empty_previous() {
        let mut log = TransparencyLog::new();
        log.append("a/b".into(), "1.0.0".into(), "k1".into(), "h1".into(), "s1".into());
        assert!(log.entries[0].previous_hash.is_empty());

        // Verify passes with empty previous_hash for first entry
        assert!(log.verify_chain().is_ok());
    }
}
