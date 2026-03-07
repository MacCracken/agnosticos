//! Audit logging for shell actions

use anyhow::Result;
use chrono::{DateTime, Utc};
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

/// Filter criteria for audit log queries
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Filter by agent name or ID substring
    pub agent: Option<String>,
    /// Filter by action type
    pub action: Option<String>,
    /// Filter by approval status
    pub approved: Option<bool>,
    /// Maximum entries to return
    pub limit: Option<usize>,
    /// Time window (entries newer than this many seconds ago)
    pub since_seconds: Option<u64>,
}

/// Viewer for structured audit log queries
pub struct AuditViewer {
    file: PathBuf,
}

impl AuditViewer {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// Read and filter audit entries from the log file
    pub async fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>> {
        if !self.file.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&self.file).await?;
        let now = Utc::now();

        let mut entries: Vec<AuditEntry> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .filter(|entry: &AuditEntry| {
                // Agent filter
                if let Some(ref agent) = filter.agent {
                    if !entry.user.contains(agent) && !entry.input.contains(agent) {
                        return false;
                    }
                }
                // Action filter
                if let Some(ref action) = filter.action {
                    if entry.action != *action {
                        return false;
                    }
                }
                // Approval filter
                if let Some(approved) = filter.approved {
                    if entry.approved != approved {
                        return false;
                    }
                }
                // Time filter
                if let Some(since_secs) = filter.since_seconds {
                    if let Ok(ts) = entry.timestamp.parse::<DateTime<Utc>>() {
                        let age = (now - ts).num_seconds();
                        if age < 0 || age as u64 > since_secs {
                            return false;
                        }
                    }
                }
                true
            })
            .collect();

        // Reverse to show most recent first
        entries.reverse();

        // Apply limit
        if let Some(limit) = filter.limit {
            entries.truncate(limit);
        }

        Ok(entries)
    }

    /// Format entries as a readable table
    pub fn format_table(entries: &[AuditEntry]) -> String {
        if entries.is_empty() {
            return "No audit entries found.".to_string();
        }

        let mut lines = Vec::new();
        lines.push(format!(
            "{:<24} {:<12} {:<10} {:<8} {:<30}",
            "TIMESTAMP", "USER", "ACTION", "OK?", "INPUT"
        ));
        lines.push("\u{2500}".repeat(90));

        for entry in entries {
            let ts = if entry.timestamp.len() > 24 {
                &entry.timestamp[..24]
            } else {
                &entry.timestamp
            };
            let input_truncated = if entry.input.len() > 30 {
                format!("{}...", &entry.input[..27])
            } else {
                entry.input.clone()
            };
            let approved_str = if entry.approved { "yes" } else { "NO" };

            lines.push(format!(
                "{:<24} {:<12} {:<10} {:<8} {:<30}",
                ts, entry.user, entry.action, approved_str, input_truncated
            ));
        }

        lines.join("\n")
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

    #[test]
    fn test_audit_logger_new() {
        let logger = AuditLogger::new(PathBuf::from("/tmp/test_audit.log"));
        assert_eq!(logger.file, PathBuf::from("/tmp/test_audit.log"));
    }

    #[tokio::test]
    async fn test_audit_logger_log() {
        let dir = std::env::temp_dir().join("agnos_audit_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit.log");
        let _ = std::fs::remove_file(&path);

        let logger = AuditLogger::new(path.clone());
        let entry = create_entry("test", "Human", "ls", "execute", true, "ok");
        logger.log(entry).await.unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test"));
        assert!(content.contains("Human"));
        assert!(content.ends_with('\n'));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_audit_logger_appends() {
        let dir = std::env::temp_dir().join("agnos_audit_append_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit_append.log");
        let _ = std::fs::remove_file(&path);

        let logger = AuditLogger::new(path.clone());
        let e1 = create_entry("user1", "mode1", "cmd1", "a1", true, "ok");
        let e2 = create_entry("user2", "mode2", "cmd2", "a2", false, "denied");
        logger.log(e1).await.unwrap();
        logger.log(e2).await.unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("user1"));
        assert!(lines[1].contains("user2"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_create_entry_denied() {
        let entry = create_entry("admin", "Strict", "rm -rf /", "delete", false, "denied");
        assert!(!entry.approved);
        assert_eq!(entry.result, "denied");
    }

    #[test]
    fn test_audit_entry_debug() {
        let entry = create_entry("u", "m", "i", "a", true, "r");
        let dbg = format!("{:?}", entry);
        assert!(dbg.contains("AuditEntry"));
    }

    // --- AuditViewer / AuditFilter tests ---

    #[test]
    fn test_filter_default() {
        let filter = AuditFilter::default();
        assert!(filter.agent.is_none());
        assert!(filter.action.is_none());
        assert!(filter.approved.is_none());
        assert!(filter.limit.is_none());
        assert!(filter.since_seconds.is_none());
    }

    #[tokio::test]
    async fn test_query_nonexistent_file() {
        let viewer = AuditViewer::new(PathBuf::from("/tmp/agnos_nonexistent_audit_test_12345.log"));
        let result = viewer.query(&AuditFilter::default()).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_query_empty_file() {
        let dir = std::env::temp_dir().join("agnos_viewer_empty_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("empty_audit.log");
        std::fs::write(&path, "").unwrap();

        let viewer = AuditViewer::new(path.clone());
        let result = viewer.query(&AuditFilter::default()).await.unwrap();
        assert!(result.is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_query_with_agent_filter() {
        let dir = std::env::temp_dir().join("agnos_viewer_agent_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("agent_filter.log");
        let _ = std::fs::remove_file(&path);

        let logger = AuditLogger::new(path.clone());
        logger
            .log(create_entry("alice", "m", "cmd1", "exec", true, "ok"))
            .await
            .unwrap();
        logger
            .log(create_entry("bob", "m", "cmd2", "exec", true, "ok"))
            .await
            .unwrap();

        let viewer = AuditViewer::new(path.clone());
        let filter = AuditFilter {
            agent: Some("alice".to_string()),
            ..Default::default()
        };
        let result = viewer.query(&filter).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].user, "alice");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_query_with_action_filter() {
        let dir = std::env::temp_dir().join("agnos_viewer_action_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("action_filter.log");
        let _ = std::fs::remove_file(&path);

        let logger = AuditLogger::new(path.clone());
        logger
            .log(create_entry("u", "m", "cmd", "execute", true, "ok"))
            .await
            .unwrap();
        logger
            .log(create_entry("u", "m", "cmd", "delete", false, "denied"))
            .await
            .unwrap();

        let viewer = AuditViewer::new(path.clone());
        let filter = AuditFilter {
            action: Some("delete".to_string()),
            ..Default::default()
        };
        let result = viewer.query(&filter).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, "delete");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_query_with_approval_filter() {
        let dir = std::env::temp_dir().join("agnos_viewer_approval_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("approval_filter.log");
        let _ = std::fs::remove_file(&path);

        let logger = AuditLogger::new(path.clone());
        logger
            .log(create_entry("u", "m", "cmd1", "a", true, "ok"))
            .await
            .unwrap();
        logger
            .log(create_entry("u", "m", "cmd2", "a", false, "denied"))
            .await
            .unwrap();
        logger
            .log(create_entry("u", "m", "cmd3", "a", true, "ok"))
            .await
            .unwrap();

        let viewer = AuditViewer::new(path.clone());
        let filter = AuditFilter {
            approved: Some(false),
            ..Default::default()
        };
        let result = viewer.query(&filter).await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].approved);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_query_with_limit() {
        let dir = std::env::temp_dir().join("agnos_viewer_limit_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("limit_filter.log");
        let _ = std::fs::remove_file(&path);

        let logger = AuditLogger::new(path.clone());
        for i in 0..5 {
            logger
                .log(create_entry(
                    &format!("u{}", i),
                    "m",
                    "cmd",
                    "a",
                    true,
                    "ok",
                ))
                .await
                .unwrap();
        }

        let viewer = AuditViewer::new(path.clone());
        let filter = AuditFilter {
            limit: Some(2),
            ..Default::default()
        };
        let result = viewer.query(&filter).await.unwrap();
        assert_eq!(result.len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_format_table_empty() {
        let table = AuditViewer::format_table(&[]);
        assert_eq!(table, "No audit entries found.");
    }

    #[test]
    fn test_format_table_with_entries() {
        let entries = vec![
            create_entry("alice", "Human", "ls -la", "execute", true, "ok"),
            create_entry("bob", "Strict", "rm /tmp/x", "delete", false, "denied"),
        ];
        let table = AuditViewer::format_table(&entries);
        assert!(table.contains("TIMESTAMP"));
        assert!(table.contains("USER"));
        assert!(table.contains("ACTION"));
        assert!(table.contains("alice"));
        assert!(table.contains("bob"));
        assert!(table.contains("yes"));
        assert!(table.contains("NO"));
    }

    #[test]
    fn test_format_table_truncation() {
        let long_input = "a".repeat(50);
        let entries = vec![create_entry("u", "m", &long_input, "a", true, "r")];
        let table = AuditViewer::format_table(&entries);
        // Input should be truncated to 30 chars (27 + "...")
        assert!(table.contains("..."));
        assert!(!table.contains(&"a".repeat(50)));
    }

    #[test]
    fn test_audit_viewer_new() {
        let viewer = AuditViewer::new(PathBuf::from("/tmp/test.log"));
        assert_eq!(viewer.file, PathBuf::from("/tmp/test.log"));
    }
}
