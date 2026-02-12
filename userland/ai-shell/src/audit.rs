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
