//! Agent Persistent Memory Store
//!
//! Per-agent key-value storage that survives restarts. Each agent gets
//! an isolated namespace under `/var/lib/agnos/agent-memory/`. Values are
//! JSON-serializable and stored as individual files for simplicity and
//! crash-safety (atomic write via rename).

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use agnos_common::AgentId;

/// Default base directory for agent memory storage
const DEFAULT_MEMORY_DIR: &str = "/var/lib/agnos/agent-memory";

/// Maximum key length (bytes)
const MAX_KEY_LENGTH: usize = 256;

/// Maximum value size (bytes) — 1 MB
const MAX_VALUE_SIZE: usize = 1_048_576;

/// A single memory entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Per-agent memory store
pub struct AgentMemoryStore {
    base_dir: PathBuf,
}

impl AgentMemoryStore {
    /// Create a new memory store with the default base directory
    pub fn new() -> Self {
        Self {
            base_dir: PathBuf::from(DEFAULT_MEMORY_DIR),
        }
    }

    /// Create with a custom base directory (useful for testing)
    pub fn with_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get the directory for an agent's memory
    fn agent_dir(&self, agent_id: AgentId) -> PathBuf {
        self.base_dir.join(agent_id.to_string())
    }

    /// Get the file path for a key
    fn key_path(&self, agent_id: AgentId, key: &str) -> PathBuf {
        // Sanitize key to prevent path traversal
        let safe_key = sanitize_key(key);
        self.agent_dir(agent_id).join(format!("{}.json", safe_key))
    }

    /// Store a value for an agent
    pub async fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
        tags: Vec<String>,
    ) -> Result<()> {
        validate_key(key)?;
        let serialized = serde_json::to_vec_pretty(&value)?;
        if serialized.len() > MAX_VALUE_SIZE {
            anyhow::bail!("Value exceeds maximum size of {} bytes", MAX_VALUE_SIZE);
        }

        let dir = self.agent_dir(agent_id);
        tokio::fs::create_dir_all(&dir)
            .await
            .context("Failed to create agent memory directory")?;

        let now = chrono::Utc::now().to_rfc3339();
        let path = self.key_path(agent_id, key);

        // Check if entry exists for created_at
        let created_at = if path.exists() {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => serde_json::from_str::<MemoryEntry>(&content)
                    .map(|e| e.created_at)
                    .unwrap_or_else(|_| now.clone()),
                Err(_) => now.clone(),
            }
        } else {
            now.clone()
        };

        let entry = MemoryEntry {
            key: key.to_string(),
            value,
            created_at,
            updated_at: now,
            tags,
        };

        let content = serde_json::to_string_pretty(&entry)?;

        // Atomic write: write to temp file, then rename
        let tmp_path = path.with_extension("tmp");
        tokio::fs::write(&tmp_path, &content)
            .await
            .context("Failed to write memory entry")?;
        tokio::fs::rename(&tmp_path, &path)
            .await
            .context("Failed to finalize memory entry")?;

        debug!("Agent {} stored key '{}'", agent_id, key);
        Ok(())
    }

    /// Get a value for an agent
    pub async fn get(&self, agent_id: AgentId, key: &str) -> Result<Option<MemoryEntry>> {
        let path = self.key_path(agent_id, key);
        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .context("Failed to read memory entry")?;
        let entry: MemoryEntry =
            serde_json::from_str(&content).context("Failed to parse memory entry")?;
        Ok(Some(entry))
    }

    /// Delete a key for an agent
    pub async fn delete(&self, agent_id: AgentId, key: &str) -> Result<bool> {
        let path = self.key_path(agent_id, key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
            debug!("Agent {} deleted key '{}'", agent_id, key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all keys for an agent
    pub async fn list_keys(&self, agent_id: AgentId) -> Result<Vec<String>> {
        let dir = self.agent_dir(agent_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::new();
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    keys.push(stem.to_string());
                }
            }
        }
        keys.sort();
        Ok(keys)
    }

    /// List keys filtered by tag
    pub async fn list_by_tag(&self, agent_id: AgentId, tag: &str) -> Result<Vec<String>> {
        let dir = self.agent_dir(agent_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut matching = Vec::new();
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    if let Ok(mem_entry) = serde_json::from_str::<MemoryEntry>(&content) {
                        if mem_entry.tags.iter().any(|t| t == tag) {
                            matching.push(mem_entry.key);
                        }
                    }
                }
            }
        }
        matching.sort();
        Ok(matching)
    }

    /// Clear all memory for an agent
    pub async fn clear(&self, agent_id: AgentId) -> Result<u64> {
        let dir = self.agent_dir(agent_id);
        if !dir.exists() {
            return Ok(0);
        }

        let mut count = 0u64;
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                tokio::fs::remove_file(entry.path()).await?;
                count += 1;
            }
        }
        debug!("Agent {} cleared {} memory entries", agent_id, count);
        Ok(count)
    }

    /// Get total memory usage in bytes for an agent
    pub async fn usage_bytes(&self, agent_id: AgentId) -> Result<u64> {
        let dir = self.agent_dir(agent_id);
        if !dir.exists() {
            return Ok(0);
        }

        let mut total = 0u64;
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if let Ok(meta) = entry.metadata().await {
                total += meta.len();
            }
        }
        Ok(total)
    }
}

impl Default for AgentMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Sanitize a key for use as a filename
fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Validate a key
fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        anyhow::bail!("Key cannot be empty");
    }
    if key.len() > MAX_KEY_LENGTH {
        anyhow::bail!("Key exceeds maximum length of {} bytes", MAX_KEY_LENGTH);
    }
    if key.contains("..") || key.contains('/') || key.contains('\\') {
        anyhow::bail!("Key contains invalid characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn agent(n: u8) -> AgentId {
        AgentId(Uuid::from_bytes([n; 16]))
    }

    #[tokio::test]
    async fn test_set_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(id, "greeting", serde_json::json!("hello"), vec![])
            .await
            .unwrap();

        let entry = store.get(id, "greeting").await.unwrap().unwrap();
        assert_eq!(entry.key, "greeting");
        assert_eq!(entry.value, serde_json::json!("hello"));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let result = store.get(agent(1), "nope").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(id, "tmp", serde_json::json!(42), vec![])
            .await
            .unwrap();
        let deleted = store.delete(id, "tmp").await.unwrap();
        assert!(deleted);
        assert!(store.get(id, "tmp").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let deleted = store.delete(agent(1), "ghost").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_list_keys_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let keys = store.list_keys(agent(1)).await.unwrap();
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn test_list_keys_populated() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(id, "alpha", serde_json::json!(1), vec![])
            .await
            .unwrap();
        store
            .set(id, "beta", serde_json::json!(2), vec![])
            .await
            .unwrap();
        store
            .set(id, "gamma", serde_json::json!(3), vec![])
            .await
            .unwrap();

        let keys = store.list_keys(id).await.unwrap();
        assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
    }

    #[tokio::test]
    async fn test_clear() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(id, "a", serde_json::json!(1), vec![])
            .await
            .unwrap();
        store
            .set(id, "b", serde_json::json!(2), vec![])
            .await
            .unwrap();

        let count = store.clear(id).await.unwrap();
        assert_eq!(count, 2);
        assert!(store.list_keys(id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_usage_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        // No directory yet
        assert_eq!(store.usage_bytes(id).await.unwrap(), 0);

        store
            .set(id, "data", serde_json::json!("some content"), vec![])
            .await
            .unwrap();

        let bytes = store.usage_bytes(id).await.unwrap();
        assert!(bytes > 0);
    }

    #[test]
    fn test_validate_key_valid() {
        assert!(validate_key("my-key").is_ok());
        assert!(validate_key("key_123").is_ok());
        assert!(validate_key("a").is_ok());
    }

    #[test]
    fn test_validate_key_empty() {
        let err = validate_key("").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_validate_key_too_long() {
        let long_key = "x".repeat(MAX_KEY_LENGTH + 1);
        let err = validate_key(&long_key).unwrap_err();
        assert!(err.to_string().contains("maximum length"));
    }

    #[test]
    fn test_validate_key_path_traversal() {
        assert!(validate_key("../etc/passwd").is_err());
        assert!(validate_key("foo/bar").is_err());
        assert!(validate_key("foo\\bar").is_err());
        assert!(validate_key("..").is_err());
    }

    #[test]
    fn test_sanitize_key() {
        assert_eq!(sanitize_key("hello-world"), "hello-world");
        assert_eq!(sanitize_key("key_123"), "key_123");
        assert_eq!(sanitize_key("has spaces"), "has_spaces");
        assert_eq!(sanitize_key("path/traversal"), "path_traversal");
        assert_eq!(sanitize_key("special!@#chars"), "special___chars");
    }

    #[tokio::test]
    async fn test_set_with_tags() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(
                id,
                "config",
                serde_json::json!({"level": "debug"}),
                vec!["settings".into(), "debug".into()],
            )
            .await
            .unwrap();

        let entry = store.get(id, "config").await.unwrap().unwrap();
        assert_eq!(entry.tags, vec!["settings", "debug"]);
    }

    #[tokio::test]
    async fn test_list_by_tag() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(id, "a", serde_json::json!(1), vec!["x".into()])
            .await
            .unwrap();
        store
            .set(id, "b", serde_json::json!(2), vec!["y".into()])
            .await
            .unwrap();
        store
            .set(id, "c", serde_json::json!(3), vec!["x".into(), "y".into()])
            .await
            .unwrap();

        let x_keys = store.list_by_tag(id, "x").await.unwrap();
        assert_eq!(x_keys, vec!["a", "c"]);

        let y_keys = store.list_by_tag(id, "y").await.unwrap();
        assert_eq!(y_keys, vec!["b", "c"]);

        let z_keys = store.list_by_tag(id, "z").await.unwrap();
        assert!(z_keys.is_empty());
    }

    #[tokio::test]
    async fn test_value_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        // Create a value larger than MAX_VALUE_SIZE
        let big_string = "x".repeat(MAX_VALUE_SIZE + 100);
        let result = store
            .set(id, "big", serde_json::json!(big_string), vec![])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("maximum size"));
    }

    #[tokio::test]
    async fn test_key_path_isolation_between_agents() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id1 = agent(1);
        let id2 = agent(2);

        store
            .set(id1, "shared-key", serde_json::json!("agent1-value"), vec![])
            .await
            .unwrap();
        store
            .set(id2, "shared-key", serde_json::json!("agent2-value"), vec![])
            .await
            .unwrap();

        let entry1 = store.get(id1, "shared-key").await.unwrap().unwrap();
        let entry2 = store.get(id2, "shared-key").await.unwrap().unwrap();

        assert_eq!(entry1.value, serde_json::json!("agent1-value"));
        assert_eq!(entry2.value, serde_json::json!("agent2-value"));
    }

    #[tokio::test]
    async fn test_overwrite_preserves_created_at() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let id = agent(1);

        store
            .set(id, "evolving", serde_json::json!("v1"), vec![])
            .await
            .unwrap();
        let first = store.get(id, "evolving").await.unwrap().unwrap();
        let original_created = first.created_at.clone();

        // Small delay to ensure timestamp differs
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        store
            .set(id, "evolving", serde_json::json!("v2"), vec![])
            .await
            .unwrap();
        let second = store.get(id, "evolving").await.unwrap().unwrap();

        assert_eq!(second.created_at, original_created);
        assert_eq!(second.value, serde_json::json!("v2"));
        assert_ne!(second.updated_at, second.created_at);
    }

    #[test]
    fn test_memory_entry_serialization() {
        let entry = MemoryEntry {
            key: "test".to_string(),
            value: serde_json::json!({"nested": true}),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            tags: vec!["tag1".into()],
        };

        let json = serde_json::to_string(&entry).unwrap();
        let restored: MemoryEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.key, "test");
        assert_eq!(restored.value, serde_json::json!({"nested": true}));
        assert_eq!(restored.tags, vec!["tag1"]);
    }

    #[test]
    fn test_memory_entry_deserialization_without_tags() {
        // tags has #[serde(default)], so missing tags should work
        let json = r#"{"key":"k","value":1,"created_at":"t","updated_at":"t"}"#;
        let entry: MemoryEntry = serde_json::from_str(json).unwrap();
        assert!(entry.tags.is_empty());
    }

    #[test]
    fn test_default_construction() {
        let store = AgentMemoryStore::default();
        // Default should use the standard system path
        assert_eq!(store.base_dir, PathBuf::from("/var/lib/agnos/agent-memory"));
    }

    #[tokio::test]
    async fn test_clear_empty_agent() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let count = store.clear(agent(99)).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_list_by_tag_no_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentMemoryStore::with_dir(dir.path().to_path_buf());
        let keys = store.list_by_tag(agent(1), "anything").await.unwrap();
        assert!(keys.is_empty());
    }
}
