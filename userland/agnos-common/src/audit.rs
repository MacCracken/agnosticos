//! Audit logging types

use crate::{AgentId, UserId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub sequence: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: AuditEventType,
    pub agent_id: Option<AgentId>,
    pub user_id: UserId,
    pub action: String,
    pub resource: String,
    pub result: AuditResult,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    AgentCreated,
    AgentTerminated,
    AgentAction,
    FileAccess,
    NetworkAccess,
    LlmInference,
    PermissionChange,
    ConfigChange,
    SecurityEvent,
    SystemEvent,
    ExternalAudit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure,
    Denied,
}

/// Audit log entry with cryptographic chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub event: AuditEvent,
    pub previous_hash: String,
    pub entry_hash: String,
    pub signature: String,
}

/// Audit log configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_file: String,
    pub max_file_size: u64,
    pub max_files: u32,
    pub encrypt: bool,
    pub sign_entries: bool,
}

/// Verify the integrity of an audit hash chain.
///
/// Each entry's `previous_hash` must match the preceding entry's `entry_hash`.
/// Returns `Ok(())` if the chain is valid, or an error describing the first break.
pub fn verify_chain(entries: &[AuditEntry]) -> std::result::Result<(), AuditChainError> {
    if entries.is_empty() {
        return Ok(());
    }

    for i in 1..entries.len() {
        let prev = &entries[i - 1];
        let curr = &entries[i];
        if curr.previous_hash != prev.entry_hash {
            return Err(AuditChainError {
                position: i,
                expected_hash: prev.entry_hash.clone(),
                found_hash: curr.previous_hash.clone(),
            });
        }
    }

    Ok(())
}

/// Error returned when the audit hash chain is broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditChainError {
    /// Index of the entry where the chain broke.
    pub position: usize,
    /// The expected `previous_hash` (from the prior entry's `entry_hash`).
    pub expected_hash: String,
    /// The actual `previous_hash` found on the broken entry.
    pub found_hash: String,
}

impl std::fmt::Display for AuditChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "audit chain broken at entry {}: expected previous_hash '{}', found '{}'",
            self.position, self.expected_hash, self.found_hash
        )
    }
}

impl std::error::Error for AuditChainError {}

/// Create an [`AuditEntry`] from an event and the previous entry's hash.
///
/// Computes a SHA-256 hash over `sequence || timestamp || action || resource || previous_hash`
/// and returns an `AuditEntry` linked into the chain. The `signature` field is set to
/// `"unsigned"` (real cryptographic signing is out of scope).
pub fn create_audit_entry(event: AuditEvent, previous_hash: &str) -> AuditEntry {
    let mut hasher = Sha256::new();
    hasher.update(event.sequence.to_le_bytes());
    hasher.update(event.timestamp.to_rfc3339().as_bytes());
    hasher.update(event.action.as_bytes());
    hasher.update(event.resource.as_bytes());
    hasher.update(previous_hash.as_bytes());
    let hash_bytes = hasher.finalize();
    let entry_hash = format!("{:x}", hash_bytes);

    AuditEntry {
        event,
        previous_hash: previous_hash.to_string(),
        entry_hash,
        signature: "unsigned".to_string(),
    }
}

/// In-memory audit chain with cryptographic hash linking.
///
/// Each appended event is assigned an auto-incrementing sequence number and
/// hash-chained to the previous entry via [`create_audit_entry`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditChain {
    entries: Vec<AuditEntry>,
    next_sequence: u64,
}

impl AuditChain {
    /// Create an empty audit chain.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_sequence: 0,
        }
    }

    /// Append an event to the chain, returning a reference to the new entry.
    ///
    /// The event's `sequence` field is overwritten with the chain's internal
    /// auto-incrementing counter to guarantee monotonicity.
    pub fn append(&mut self, mut event: AuditEvent) -> &AuditEntry {
        event.sequence = self.next_sequence;
        let prev_hash = self
            .entries
            .last()
            .map(|e| e.entry_hash.as_str())
            .unwrap_or("genesis");
        let entry = create_audit_entry(event, prev_hash);
        self.entries.push(entry);
        self.next_sequence += 1;
        self.entries.last().unwrap()
    }

    /// Number of entries in the chain.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the chain contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Verify the full chain integrity.
    pub fn verify(&self) -> std::result::Result<(), AuditChainError> {
        verify_chain(&self.entries)
    }

    /// Return a slice over all entries.
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Return the hash of the last entry, or `None` if the chain is empty.
    pub fn last_hash(&self) -> Option<&str> {
        self.entries.last().map(|e| e.entry_hash.as_str())
    }
}

impl Default for AuditChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_file: "/var/log/agnos/audit.log".into(),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_files: 10,
            encrypt: true,
            sign_entries: true,
        }
    }
}

/// File-based audit log writer with size-based rotation.
///
/// Writes JSON-line audit entries to a file, rotating when `max_file_size` is
/// reached and keeping at most `max_files` rotated copies.
pub struct AuditLogWriter {
    config: AuditConfig,
    current_size: u64,
}

impl AuditLogWriter {
    /// Create a new writer. Creates the log directory if needed.
    pub fn new(config: AuditConfig) -> std::io::Result<Self> {
        let path = std::path::Path::new(&config.log_file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let current_size = std::fs::metadata(&config.log_file)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(Self {
            config,
            current_size,
        })
    }

    /// Append an audit entry to the log file, rotating if necessary.
    pub fn write_entry(&mut self, entry: &AuditEntry) -> std::io::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let line = serde_json::to_string(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let line_bytes = line.len() as u64 + 1; // +1 for newline

        // Rotate if adding this entry would exceed max_file_size
        if self.current_size + line_bytes > self.config.max_file_size {
            self.rotate()?;
        }

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.log_file)?;
        writeln!(file, "{}", line)?;
        self.current_size += line_bytes;

        Ok(())
    }

    /// Rotate log files: audit.log → audit.log.1 → audit.log.2 → ...
    /// Deletes the oldest file if max_files is exceeded.
    fn rotate(&mut self) -> std::io::Result<()> {
        let base = &self.config.log_file;

        // Remove the oldest if it would exceed max_files
        let oldest = format!("{}.{}", base, self.config.max_files);
        if std::path::Path::new(&oldest).exists() {
            std::fs::remove_file(&oldest)?;
        }

        // Shift existing rotated files: .N → .N+1
        for i in (1..self.config.max_files).rev() {
            let from = format!("{}.{}", base, i);
            let to = format!("{}.{}", base, i + 1);
            if std::path::Path::new(&from).exists() {
                std::fs::rename(&from, &to)?;
            }
        }

        // Rotate current file to .1
        if std::path::Path::new(base).exists() {
            std::fs::rename(base, format!("{}.1", base))?;
        }

        self.current_size = 0;
        Ok(())
    }

    /// Get current log file size in bytes.
    pub fn current_size(&self) -> u64 {
        self.current_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_config_default() {
        let config = AuditConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_files, 10);
        assert!(config.encrypt);
        assert!(config.sign_entries);
    }

    #[test]
    fn test_audit_event_type_variants() {
        use crate::audit::AuditEventType;
        assert_eq!(AuditEventType::AgentCreated, AuditEventType::AgentCreated);
        assert_ne!(
            AuditEventType::AgentCreated,
            AuditEventType::AgentTerminated
        );
    }

    #[test]
    fn test_audit_result_variants() {
        use crate::audit::AuditResult;
        assert_eq!(AuditResult::Success, AuditResult::Success);
        assert_ne!(AuditResult::Success, AuditResult::Denied);
    }

    #[test]
    fn test_audit_entry() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::AgentCreated,
            agent_id: None,
            user_id: UserId::new(),
            action: "create".to_string(),
            resource: "agent".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"name": "test"}),
        };

        let entry = AuditEntry {
            event,
            previous_hash: "abc123".to_string(),
            entry_hash: "def456".to_string(),
            signature: "sig789".to_string(),
        };

        assert_eq!(entry.previous_hash, "abc123");
        assert_eq!(entry.entry_hash, "def456");
    }

    // --- New coverage tests (batch 2) ---

    #[test]
    fn test_audit_event_all_event_types() {
        let types = [
            AuditEventType::AgentCreated,
            AuditEventType::AgentTerminated,
            AuditEventType::AgentAction,
            AuditEventType::FileAccess,
            AuditEventType::NetworkAccess,
            AuditEventType::LlmInference,
            AuditEventType::PermissionChange,
            AuditEventType::ConfigChange,
            AuditEventType::SecurityEvent,
            AuditEventType::SystemEvent,
            AuditEventType::ExternalAudit,
        ];
        // Ensure all 11 variants are distinct
        for (i, a) in types.iter().enumerate() {
            for (j, b) in types.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_audit_result_all_variants() {
        let results = [AuditResult::Success, AuditResult::Failure, AuditResult::Denied];
        for (i, a) in results.iter().enumerate() {
            for (j, b) in results.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_audit_event_serialization_roundtrip() {
        let event = AuditEvent {
            sequence: 42,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::FileAccess,
            agent_id: Some(AgentId::new()),
            user_id: UserId::new(),
            action: "read".to_string(),
            resource: "/etc/passwd".to_string(),
            result: AuditResult::Denied,
            details: serde_json::json!({"reason": "sandbox violation"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sequence, 42);
        assert_eq!(deserialized.event_type, AuditEventType::FileAccess);
        assert_eq!(deserialized.result, AuditResult::Denied);
        assert_eq!(deserialized.action, "read");
        assert_eq!(deserialized.resource, "/etc/passwd");
    }

    #[test]
    fn test_audit_event_with_agent_id() {
        let agent_id = AgentId::new();
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::AgentAction,
            agent_id: Some(agent_id),
            user_id: UserId::new(),
            action: "execute".to_string(),
            resource: "task-1".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        assert!(event.agent_id.is_some());
    }

    #[test]
    fn test_audit_event_without_agent_id() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "boot".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({}),
        };
        assert!(event.agent_id.is_none());
    }

    #[test]
    fn test_audit_event_details_complex_json() {
        let details = serde_json::json!({
            "paths": ["/tmp/a", "/tmp/b"],
            "permissions": {"read": true, "write": false},
            "count": 42
        });
        let event = AuditEvent {
            sequence: 100,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::PermissionChange,
            agent_id: None,
            user_id: UserId::new(),
            action: "chmod".to_string(),
            resource: "/tmp/a".to_string(),
            result: AuditResult::Success,
            details: details.clone(),
        };
        assert_eq!(event.details["count"], 42);
        assert_eq!(event.details["paths"][0], "/tmp/a");
    }

    #[test]
    fn test_audit_entry_serialization_roundtrip() {
        let event = AuditEvent {
            sequence: 5,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::LlmInference,
            agent_id: None,
            user_id: UserId::new(),
            action: "infer".to_string(),
            resource: "gpt-4".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"tokens": 1024}),
        };
        let entry = AuditEntry {
            event,
            previous_hash: "aaa".to_string(),
            entry_hash: "bbb".to_string(),
            signature: "ccc".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.previous_hash, "aaa");
        assert_eq!(deserialized.entry_hash, "bbb");
        assert_eq!(deserialized.signature, "ccc");
        assert_eq!(deserialized.event.sequence, 5);
    }

    #[test]
    fn test_audit_config_custom_values() {
        let config = AuditConfig {
            enabled: false,
            log_file: "/custom/audit.log".into(),
            max_file_size: 50 * 1024 * 1024,
            max_files: 5,
            encrypt: false,
            sign_entries: false,
        };
        assert!(!config.enabled);
        assert_eq!(config.log_file, "/custom/audit.log");
        assert_eq!(config.max_file_size, 50 * 1024 * 1024);
        assert_eq!(config.max_files, 5);
        assert!(!config.encrypt);
        assert!(!config.sign_entries);
    }

    #[test]
    fn test_audit_config_default_log_file() {
        let config = AuditConfig::default();
        assert_eq!(config.log_file, "/var/log/agnos/audit.log");
    }

    #[test]
    fn test_audit_config_default_max_file_size() {
        let config = AuditConfig::default();
        assert_eq!(config.max_file_size, 100 * 1024 * 1024);
    }

    #[test]
    fn test_audit_config_serialization_roundtrip() {
        let config = AuditConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AuditConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.enabled, config.enabled);
        assert_eq!(deserialized.log_file, config.log_file);
        assert_eq!(deserialized.max_file_size, config.max_file_size);
        assert_eq!(deserialized.max_files, config.max_files);
    }

    #[test]
    fn test_audit_config_clone() {
        let config = AuditConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.log_file, config.log_file);
        assert_eq!(cloned.max_files, config.max_files);
    }

    #[test]
    fn test_audit_config_debug() {
        let config = AuditConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("AuditConfig"));
        assert!(debug.contains("enabled"));
    }

    #[test]
    fn test_audit_event_type_serialization_roundtrip() {
        let types = [
            AuditEventType::AgentCreated,
            AuditEventType::AgentTerminated,
            AuditEventType::AgentAction,
            AuditEventType::FileAccess,
            AuditEventType::NetworkAccess,
            AuditEventType::LlmInference,
            AuditEventType::PermissionChange,
            AuditEventType::ConfigChange,
            AuditEventType::SecurityEvent,
            AuditEventType::SystemEvent,
            AuditEventType::ExternalAudit,
        ];
        for et in &types {
            let json = serde_json::to_string(et).unwrap();
            let deserialized: AuditEventType = serde_json::from_str(&json).unwrap();
            assert_eq!(*et, deserialized);
        }
    }

    #[test]
    fn test_audit_result_serialization_roundtrip() {
        let results = [AuditResult::Success, AuditResult::Failure, AuditResult::Denied];
        for r in &results {
            let json = serde_json::to_string(r).unwrap();
            let deserialized: AuditResult = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, deserialized);
        }
    }

    #[test]
    fn test_audit_event_type_copy() {
        let a = AuditEventType::SecurityEvent;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn test_audit_result_copy() {
        let a = AuditResult::Failure;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn test_audit_event_clone() {
        let event = AuditEvent {
            sequence: 99,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::ConfigChange,
            agent_id: Some(AgentId::new()),
            user_id: UserId::new(),
            action: "update".to_string(),
            resource: "config.toml".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"key": "value"}),
        };
        let cloned = event.clone();
        assert_eq!(cloned.sequence, 99);
        assert_eq!(cloned.event_type, AuditEventType::ConfigChange);
        assert_eq!(cloned.action, "update");
    }

    #[test]
    fn test_audit_entry_clone() {
        let entry = AuditEntry {
            event: AuditEvent {
                sequence: 1,
                timestamp: chrono::Utc::now(),
                event_type: AuditEventType::AgentCreated,
                agent_id: None,
                user_id: UserId::new(),
                action: "create".to_string(),
                resource: "agent".to_string(),
                result: AuditResult::Success,
                details: serde_json::json!(null),
            },
            previous_hash: "prev".to_string(),
            entry_hash: "curr".to_string(),
            signature: "sig".to_string(),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.previous_hash, "prev");
        assert_eq!(cloned.entry_hash, "curr");
    }

    #[test]
    fn test_audit_event_sequence_zero() {
        let event = AuditEvent {
            sequence: 0,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "init".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        assert_eq!(event.sequence, 0);
    }

    #[test]
    fn test_audit_event_max_sequence() {
        let event = AuditEvent {
            sequence: u64::MAX,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "test".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        assert_eq!(event.sequence, u64::MAX);
    }

    #[test]
    fn test_audit_event_empty_details() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::NetworkAccess,
            agent_id: None,
            user_id: UserId::new(),
            action: "connect".to_string(),
            resource: "tcp://1.2.3.4:443".to_string(),
            result: AuditResult::Failure,
            details: serde_json::json!({}),
        };
        assert!(event.details.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_audit_event_debug() {
        let event = AuditEvent {
            sequence: 1,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::AgentCreated,
            agent_id: None,
            user_id: UserId::new(),
            action: "create".to_string(),
            resource: "agent".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("AuditEvent"));
        assert!(debug.contains("AgentCreated"));
    }

    #[test]
    fn test_audit_event_type_debug() {
        let et = AuditEventType::LlmInference;
        let debug = format!("{:?}", et);
        assert_eq!(debug, "LlmInference");
    }

    #[test]
    fn test_audit_result_debug() {
        assert_eq!(format!("{:?}", AuditResult::Success), "Success");
        assert_eq!(format!("{:?}", AuditResult::Failure), "Failure");
        assert_eq!(format!("{:?}", AuditResult::Denied), "Denied");
    }

    fn make_event(seq: u64) -> AuditEvent {
        AuditEvent {
            sequence: seq,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: UserId::new(),
            action: "test".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        }
    }

    fn make_chain(n: usize) -> Vec<AuditEntry> {
        let mut entries = Vec::new();
        let mut prev_hash = "genesis".to_string();
        for i in 0..n {
            let hash = format!("hash_{}", i);
            entries.push(AuditEntry {
                event: make_event(i as u64),
                previous_hash: prev_hash.clone(),
                entry_hash: hash.clone(),
                signature: "sig".to_string(),
            });
            prev_hash = hash;
        }
        entries
    }

    #[test]
    fn test_verify_chain_empty() {
        assert!(verify_chain(&[]).is_ok());
    }

    #[test]
    fn test_verify_chain_single_entry() {
        let chain = make_chain(1);
        assert!(verify_chain(&chain).is_ok());
    }

    #[test]
    fn test_verify_chain_valid() {
        let chain = make_chain(5);
        assert!(verify_chain(&chain).is_ok());
    }

    #[test]
    fn test_verify_chain_broken() {
        let mut chain = make_chain(5);
        // Break the chain at position 3
        chain[3].previous_hash = "tampered".to_string();
        let err = verify_chain(&chain).unwrap_err();
        assert_eq!(err.position, 3);
        assert_eq!(err.expected_hash, "hash_2");
        assert_eq!(err.found_hash, "tampered");
    }

    #[test]
    fn test_verify_chain_error_display() {
        let err = AuditChainError {
            position: 7,
            expected_hash: "abc".to_string(),
            found_hash: "xyz".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("entry 7"));
        assert!(msg.contains("abc"));
        assert!(msg.contains("xyz"));
    }

    fn make_test_entry(seq: u64) -> AuditEntry {
        AuditEntry {
            event: make_event(seq),
            previous_hash: format!("prev_{}", seq),
            entry_hash: format!("hash_{}", seq),
            signature: "sig".to_string(),
        }
    }

    #[test]
    fn test_audit_log_writer_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("audit.log");
        let config = AuditConfig {
            enabled: true,
            log_file: log_path.to_str().unwrap().to_string(),
            max_file_size: 1024 * 1024,
            max_files: 3,
            encrypt: false,
            sign_entries: false,
        };
        let mut writer = AuditLogWriter::new(config).unwrap();
        writer.write_entry(&make_test_entry(1)).unwrap();
        assert!(log_path.exists());
        assert!(writer.current_size() > 0);
    }

    #[test]
    fn test_audit_log_writer_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("audit.log");
        let config = AuditConfig {
            enabled: false,
            log_file: log_path.to_str().unwrap().to_string(),
            ..AuditConfig::default()
        };
        let mut writer = AuditLogWriter::new(config).unwrap();
        writer.write_entry(&make_test_entry(1)).unwrap();
        assert!(!log_path.exists());
    }

    #[test]
    fn test_audit_log_writer_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("audit.log");
        let config = AuditConfig {
            enabled: true,
            log_file: log_path.to_str().unwrap().to_string(),
            max_file_size: 200, // Very small to trigger rotation
            max_files: 3,
            encrypt: false,
            sign_entries: false,
        };
        let mut writer = AuditLogWriter::new(config).unwrap();

        // Write enough entries to trigger rotation
        for i in 0..10 {
            writer.write_entry(&make_test_entry(i)).unwrap();
        }

        // Should have rotated files
        let rotated_1 = dir.path().join("audit.log.1");
        assert!(rotated_1.exists(), "audit.log.1 should exist after rotation");
    }

    #[test]
    fn test_audit_log_writer_max_files_respected() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("audit.log");
        let config = AuditConfig {
            enabled: true,
            log_file: log_path.to_str().unwrap().to_string(),
            max_file_size: 100, // Very small
            max_files: 2,
            encrypt: false,
            sign_entries: false,
        };
        let mut writer = AuditLogWriter::new(config).unwrap();

        for i in 0..30 {
            writer.write_entry(&make_test_entry(i)).unwrap();
        }

        // Should NOT have more than max_files rotated copies
        let too_many = dir.path().join("audit.log.3");
        assert!(!too_many.exists(), "audit.log.3 should not exist with max_files=2");
    }

    // --- AuditChain and create_audit_entry tests ---

    #[test]
    fn test_create_audit_entry_computes_valid_hash() {
        let event = make_event(0);
        let entry = create_audit_entry(event, "genesis");
        assert_eq!(entry.previous_hash, "genesis");
        assert_eq!(entry.signature, "unsigned");
        // SHA-256 hex is 64 chars
        assert_eq!(entry.entry_hash.len(), 64);
        // Hash should be deterministic
        let event2 = AuditEvent {
            sequence: 0,
            timestamp: entry.event.timestamp,
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: entry.event.user_id,
            action: "test".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        let entry2 = create_audit_entry(event2, "genesis");
        assert_eq!(entry.entry_hash, entry2.entry_hash);
    }

    #[test]
    fn test_create_audit_entry_different_previous_hash_changes_output() {
        let event1 = make_event(0);
        let ts = event1.timestamp;
        let uid = event1.user_id;
        let entry1 = create_audit_entry(event1, "aaa");
        let event2 = AuditEvent {
            sequence: 0,
            timestamp: ts,
            event_type: AuditEventType::SystemEvent,
            agent_id: None,
            user_id: uid,
            action: "test".to_string(),
            resource: "system".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!(null),
        };
        let entry2 = create_audit_entry(event2, "bbb");
        assert_ne!(entry1.entry_hash, entry2.entry_hash);
    }

    #[test]
    fn test_audit_chain_append_increments_sequence() {
        let mut chain = AuditChain::new();
        assert_eq!(chain.len(), 0);
        assert!(chain.is_empty());

        chain.append(make_event(999)); // sequence should be overridden to 0
        assert_eq!(chain.len(), 1);
        assert_eq!(chain.entries()[0].event.sequence, 0);

        chain.append(make_event(999));
        assert_eq!(chain.len(), 2);
        assert_eq!(chain.entries()[1].event.sequence, 1);
    }

    #[test]
    fn test_audit_chain_verify_valid() {
        let mut chain = AuditChain::new();
        for _ in 0..5 {
            chain.append(make_event(0));
        }
        assert!(chain.verify().is_ok());
    }

    #[test]
    fn test_audit_chain_verify_tampered() {
        let mut chain = AuditChain::new();
        for _ in 0..5 {
            chain.append(make_event(0));
        }
        // Tamper with entry 2's previous_hash
        chain.entries[2].previous_hash = "tampered".to_string();
        let err = chain.verify().unwrap_err();
        assert_eq!(err.position, 2);
    }

    #[test]
    fn test_audit_chain_last_hash() {
        let mut chain = AuditChain::new();
        assert!(chain.last_hash().is_none());
        chain.append(make_event(0));
        let h = chain.last_hash().unwrap().to_string();
        assert_eq!(h.len(), 64);
        chain.append(make_event(0));
        assert_ne!(chain.last_hash().unwrap(), h);
    }

    #[test]
    fn test_audit_chain_with_external_audit_events() {
        let mut chain = AuditChain::new();
        let event = AuditEvent {
            sequence: 0,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::ExternalAudit,
            agent_id: None,
            user_id: UserId::new(),
            action: "external.ingest".to_string(),
            resource: "siem-feed".to_string(),
            result: AuditResult::Success,
            details: serde_json::json!({"source": "splunk"}),
        };
        chain.append(event);
        assert_eq!(chain.entries()[0].event.event_type, AuditEventType::ExternalAudit);
        assert!(chain.verify().is_ok());
    }

    #[test]
    fn test_audit_chain_default() {
        let chain = AuditChain::default();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn test_audit_chain_entries_hash_linked() {
        let mut chain = AuditChain::new();
        chain.append(make_event(0));
        chain.append(make_event(0));
        chain.append(make_event(0));
        // Each entry's previous_hash must match the prior entry's entry_hash
        let entries = chain.entries();
        assert_eq!(entries[0].previous_hash, "genesis");
        assert_eq!(entries[1].previous_hash, entries[0].entry_hash);
        assert_eq!(entries[2].previous_hash, entries[1].entry_hash);
    }

    #[test]
    fn test_audit_log_writer_entries_are_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("audit.log");
        let config = AuditConfig {
            enabled: true,
            log_file: log_path.to_str().unwrap().to_string(),
            max_file_size: 1024 * 1024,
            max_files: 3,
            encrypt: false,
            sign_entries: false,
        };
        let mut writer = AuditLogWriter::new(config).unwrap();
        writer.write_entry(&make_test_entry(1)).unwrap();
        writer.write_entry(&make_test_entry(2)).unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        for line in content.lines() {
            let parsed: AuditEntry = serde_json::from_str(line).unwrap();
            assert!(!parsed.entry_hash.is_empty());
        }
    }
}
