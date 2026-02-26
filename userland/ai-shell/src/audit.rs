//! Audit logging for shell actions

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Audit entry for shell actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub user: String,
    pub mode: String,
    pub input: String,
    pub action: String,
    pub approved: bool,
    pub result: String,
}

/// Audit logger
pub struct AuditLogger {
    file: PathBuf,
}

impl AuditLogger {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }
    
    /// Log an action
    pub async fn log(&self, entry: AuditEntry) -> Result<()> {
        let line = serde_json::to_string(&entry)?;
        
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .await?;
        
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        
        Ok(())
    }
}

/// Create audit entry for command execution
pub fn create_entry(
    user: &str,
    mode: &str,
    input: &str,
    action: &str,
    approved: bool,
    result: &str,
) -> AuditEntry {
    AuditEntry {
        timestamp: Utc::now().to_rfc3339(),
        user: user.to_string(),
        mode: mode.to_string(),
        input: input.to_string(),
        action: action.to_string(),
        approved,
        result: result.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_entry() {
        let entry = create_entry(
            "testuser",
            "AiAssisted",
            "ls -la",
            "execute",
            true,
            "success",
        );
        
        assert_eq!(entry.user, "testuser");
        assert_eq!(entry.mode, "AiAssisted");
        assert_eq!(entry.input, "ls -la");
        assert_eq!(entry.action, "execute");
        assert!(entry.approved);
        assert_eq!(entry.result, "success");
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn test_audit_entry_serialization() {
        let entry = create_entry("user", "mode", "input", "action", false, "denied");
        let json = serde_json::to_string(&entry).unwrap();
        
        assert!(json.contains("user"));
        assert!(json.contains("mode"));
        assert!(json.contains("input"));
        assert!(json.contains("action"));
        assert!(json.contains("approved"));
        assert!(json.contains("result"));
    }

    #[test]
    fn test_audit_entry_deserialization() {
        let json = r#"{
            "timestamp": "2024-01-01T00:00:00Z",
            "user": "testuser",
            "mode": "Human",
            "input": "cd /home",
            "action": "builtin",
            "approved": true,
            "result": "success"
        }"#;
        
        let entry: AuditEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.user, "testuser");
        assert_eq!(entry.mode, "Human");
        assert!(entry.approved);
    }

    #[test]
    fn test_audit_entry_clone() {
        let entry = create_entry("user", "mode", "cmd", "action", true, "ok");
        let cloned = entry.clone();
        
        assert_eq!(entry.user, cloned.user);
        assert_eq!(entry.timestamp, cloned.timestamp);
    }
}
