//! Rollback / Undo for Agent Actions
//!
//! Provides filesystem snapshot and restore capabilities so that agent actions
//! can be undone. Each snapshot captures the state of an agent's working
//! directory before a potentially destructive operation.
//!
//! Snapshots are lightweight: they record file checksums and content deltas.
//! A full restore replays the inverse of recorded changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info};

use agnos_common::AgentId;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single file's state at snapshot time.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    /// Relative path from the agent's working directory.
    pub relative_path: PathBuf,
    /// SHA-256 checksum of the file at snapshot time.
    pub checksum: String,
    /// Full content (for files under the size limit).
    pub content: Option<Vec<u8>>,
    /// File existed at snapshot time.
    pub existed: bool,
}

/// A point-in-time snapshot of an agent's working directory.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Unique snapshot ID.
    pub id: String,
    /// Agent that owns this snapshot.
    pub agent_id: AgentId,
    /// Human-readable label (e.g., "before file deletion").
    pub label: String,
    /// When the snapshot was taken.
    pub created_at: DateTime<Utc>,
    /// File states at snapshot time.
    pub files: Vec<FileSnapshot>,
    /// Working directory that was snapshotted.
    pub working_dir: PathBuf,
}

/// Result of a rollback operation.
#[derive(Debug, Clone)]
pub struct RollbackResult {
    /// Snapshot that was restored.
    pub snapshot_id: String,
    /// Files that were restored.
    pub restored: Vec<PathBuf>,
    /// Files that were removed (created after snapshot).
    pub removed: Vec<PathBuf>,
    /// Files that could not be restored.
    pub errors: Vec<(PathBuf, String)>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Maximum file size to include full content in snapshot (1 MB).
const MAX_SNAPSHOT_FILE_SIZE: u64 = 1_048_576;

/// Maximum total snapshot size (100 MB) — prevents runaway memory usage.
const MAX_SNAPSHOT_TOTAL_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum snapshots to retain per agent.
const MAX_SNAPSHOTS_PER_AGENT: usize = 20;

/// Manages rollback snapshots for agents.
pub struct RollbackManager {
    /// Per-agent snapshot stacks (most recent last).
    snapshots: RwLock<HashMap<AgentId, Vec<Snapshot>>>,
    /// Maximum file size for content capture.
    max_file_size: u64,
    /// Maximum snapshots per agent.
    max_snapshots: usize,
}

impl RollbackManager {
    pub fn new() -> Self {
        Self {
            snapshots: RwLock::new(HashMap::new()),
            max_file_size: MAX_SNAPSHOT_FILE_SIZE,
            max_snapshots: MAX_SNAPSHOTS_PER_AGENT,
        }
    }

    /// Create a snapshot of the given directory for an agent.
    ///
    /// Only captures regular files (not symlinks or special files).
    /// Files larger than `max_file_size` record only checksums, not content.
    pub async fn create_snapshot(
        &self,
        agent_id: AgentId,
        working_dir: &Path,
        label: &str,
    ) -> Result<String> {
        let snapshot_id = uuid::Uuid::new_v4().to_string();

        let files = Self::scan_directory(working_dir, working_dir, self.max_file_size)
            .context("Failed to scan directory for snapshot")?;

        let snapshot = Snapshot {
            id: snapshot_id.clone(),
            agent_id,
            label: label.to_string(),
            created_at: Utc::now(),
            files,
            working_dir: working_dir.to_path_buf(),
        };

        let file_count = snapshot.files.len();

        let mut snapshots = self.snapshots.write().await;
        let agent_snaps = snapshots.entry(agent_id).or_default();

        // Evict oldest if at capacity
        if agent_snaps.len() >= self.max_snapshots {
            let removed = agent_snaps.remove(0);
            debug!(
                "Evicted oldest snapshot '{}' for agent {}",
                removed.id, agent_id
            );
        }

        agent_snaps.push(snapshot);
        info!(
            "Created snapshot '{}' for agent {} ({} files): {}",
            snapshot_id, agent_id, file_count, label
        );

        Ok(snapshot_id)
    }

    /// Restore a snapshot, reverting the working directory to its state at snapshot time.
    ///
    /// Files that existed at snapshot time are restored. Files created after
    /// the snapshot are removed. Files that were only checksummed (too large
    /// for content capture) are skipped with an error entry.
    pub async fn rollback(
        &self,
        agent_id: AgentId,
        snapshot_id: &str,
    ) -> Result<RollbackResult> {
        let snapshots = self.snapshots.read().await;
        let agent_snaps = snapshots
            .get(&agent_id)
            .context("No snapshots for agent")?;

        let snapshot = agent_snaps
            .iter()
            .find(|s| s.id == snapshot_id)
            .context("Snapshot not found")?
            .clone();

        drop(snapshots);

        let mut result = RollbackResult {
            snapshot_id: snapshot_id.to_string(),
            restored: Vec::new(),
            removed: Vec::new(),
            errors: Vec::new(),
        };

        // Build a set of files that existed at snapshot time
        let snapshot_files: HashMap<&Path, &FileSnapshot> = snapshot
            .files
            .iter()
            .map(|f| (f.relative_path.as_path(), f))
            .collect();

        // 1. Restore files that existed at snapshot time
        for file_snap in &snapshot.files {
            if !file_snap.existed {
                continue;
            }

            let full_path = snapshot.working_dir.join(&file_snap.relative_path);

            match &file_snap.content {
                Some(content) => {
                    // Ensure parent directory exists
                    if let Some(parent) = full_path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            result.errors.push((
                                file_snap.relative_path.clone(),
                                format!("Failed to create parent dir: {}", e),
                            ));
                            continue;
                        }
                    }

                    match std::fs::write(&full_path, content) {
                        Ok(()) => {
                            result.restored.push(file_snap.relative_path.clone());
                        }
                        Err(e) => {
                            result.errors.push((
                                file_snap.relative_path.clone(),
                                format!("Failed to write: {}", e),
                            ));
                        }
                    }
                }
                None => {
                    // Content was too large to snapshot — can only verify checksum
                    result.errors.push((
                        file_snap.relative_path.clone(),
                        "Content not captured (file too large); cannot restore".to_string(),
                    ));
                }
            }
        }

        // 2. Remove files that were created after the snapshot
        if snapshot.working_dir.exists() {
            let current_files =
                Self::list_files_relative(&snapshot.working_dir, &snapshot.working_dir)?;

            for rel_path in current_files {
                if !snapshot_files.contains_key(rel_path.as_path()) {
                    let full_path = snapshot.working_dir.join(&rel_path);
                    match std::fs::remove_file(&full_path) {
                        Ok(()) => {
                            result.removed.push(rel_path);
                        }
                        Err(e) => {
                            result
                                .errors
                                .push((rel_path, format!("Failed to remove: {}", e)));
                        }
                    }
                }
            }
        }

        info!(
            "Rollback '{}' for agent {}: {} restored, {} removed, {} errors",
            snapshot_id,
            agent_id,
            result.restored.len(),
            result.removed.len(),
            result.errors.len()
        );

        Ok(result)
    }

    /// List snapshots for an agent (most recent first).
    pub async fn list_snapshots(&self, agent_id: AgentId) -> Vec<SnapshotInfo> {
        let snapshots = self.snapshots.read().await;
        match snapshots.get(&agent_id) {
            Some(snaps) => snaps
                .iter()
                .rev()
                .map(|s| SnapshotInfo {
                    id: s.id.clone(),
                    label: s.label.clone(),
                    created_at: s.created_at,
                    file_count: s.files.len(),
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Delete a specific snapshot.
    pub async fn delete_snapshot(
        &self,
        agent_id: AgentId,
        snapshot_id: &str,
    ) -> Result<()> {
        let mut snapshots = self.snapshots.write().await;
        let agent_snaps = snapshots
            .entry(agent_id)
            .or_default();

        let before = agent_snaps.len();
        agent_snaps.retain(|s| s.id != snapshot_id);

        if agent_snaps.len() == before {
            anyhow::bail!("Snapshot '{}' not found", snapshot_id);
        }

        debug!("Deleted snapshot '{}' for agent {}", snapshot_id, agent_id);
        Ok(())
    }

    /// Delete all snapshots for an agent.
    pub async fn clear_agent_snapshots(&self, agent_id: AgentId) {
        self.snapshots.write().await.remove(&agent_id);
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Scan a directory tree and capture file states.
    /// Enforces a cumulative size limit to prevent unbounded memory usage.
    fn scan_directory(
        base: &Path,
        dir: &Path,
        max_file_size: u64,
    ) -> Result<Vec<FileSnapshot>> {
        let mut total_bytes: u64 = 0;
        Self::scan_directory_inner(base, dir, max_file_size, &mut total_bytes)
    }

    fn scan_directory_inner(
        base: &Path,
        dir: &Path,
        max_file_size: u64,
        total_bytes: &mut u64,
    ) -> Result<Vec<FileSnapshot>> {
        let mut files = Vec::new();

        if !dir.exists() {
            return Ok(files);
        }

        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                let mut sub_files = Self::scan_directory_inner(base, &path, max_file_size, total_bytes)?;
                files.append(&mut sub_files);
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_path_buf();

                let metadata = std::fs::metadata(&path)?;
                let size = metadata.len();

                // Check if capturing this file's content would exceed total limit
                let content = if size <= max_file_size {
                    if *total_bytes + size > MAX_SNAPSHOT_TOTAL_BYTES {
                        // Total budget exceeded — record checksum only
                        None
                    } else {
                        *total_bytes += size;
                        Some(std::fs::read(&path)?)
                    }
                } else {
                    None
                };

                let checksum = match &content {
                    Some(data) => Self::sha256_hex(data),
                    None => {
                        // For large files, read in chunks for checksum
                        Self::sha256_file(&path)?
                    }
                };

                files.push(FileSnapshot {
                    relative_path: relative,
                    checksum,
                    content,
                    existed: true,
                });
            }
            // Skip symlinks and special files
        }

        Ok(files)
    }

    /// List all regular files relative to a base directory.
    fn list_files_relative(base: &Path, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if !dir.exists() {
            return Ok(files);
        }

        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                let mut sub = Self::list_files_relative(base, &path)?;
                files.append(&mut sub);
            } else if file_type.is_file() {
                let relative = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
                files.push(relative);
            }
        }

        Ok(files)
    }

    /// Compute a SHA-256 hex digest of a byte slice.
    fn sha256_hex(data: &[u8]) -> String {
        sha256_hex_digest(data)
    }

    /// Compute a SHA-256 hex digest of a file.
    fn sha256_file(path: &Path) -> Result<String> {
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read file for checksum: {}", path.display()))?;
        Ok(sha256_hex_digest(&data))
    }
}

impl Default for RollbackManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight snapshot info (without file contents).
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub file_count: usize,
}

/// Compute a SHA-256 hex digest of data.
fn sha256_hex_digest(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file1.txt"), "hello world").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "goodbye world").unwrap();
        std::fs::create_dir_all(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/nested.txt"), "nested content").unwrap();
        dir
    }

    #[tokio::test]
    async fn test_create_snapshot() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "test snapshot")
            .await
            .unwrap();

        assert!(!snap_id.is_empty());

        let snaps = mgr.list_snapshots(id).await;
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].label, "test snapshot");
        assert_eq!(snaps[0].file_count, 3);
    }

    #[tokio::test]
    async fn test_rollback_restores_modified_file() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "before edit")
            .await
            .unwrap();

        // Modify a file
        std::fs::write(dir.path().join("file1.txt"), "MODIFIED").unwrap();

        let result = mgr.rollback(id, &snap_id).await.unwrap();
        assert!(!result.restored.is_empty());
        assert!(result.errors.is_empty());

        let content = std::fs::read_to_string(dir.path().join("file1.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_rollback_removes_new_files() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "before add")
            .await
            .unwrap();

        // Create a new file
        std::fs::write(dir.path().join("new_file.txt"), "new content").unwrap();
        assert!(dir.path().join("new_file.txt").exists());

        let result = mgr.rollback(id, &snap_id).await.unwrap();
        assert!(result.removed.contains(&PathBuf::from("new_file.txt")));
        assert!(!dir.path().join("new_file.txt").exists());
    }

    #[tokio::test]
    async fn test_rollback_restores_deleted_file() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "before delete")
            .await
            .unwrap();

        // Delete a file
        std::fs::remove_file(dir.path().join("file2.txt")).unwrap();
        assert!(!dir.path().join("file2.txt").exists());

        let _result = mgr.rollback(id, &snap_id).await.unwrap();
        assert!(dir.path().join("file2.txt").exists());
        let content = std::fs::read_to_string(dir.path().join("file2.txt")).unwrap();
        assert_eq!(content, "goodbye world");
    }

    #[tokio::test]
    async fn test_snapshot_eviction() {
        let mut mgr = RollbackManager::new();
        mgr.max_snapshots = 3;
        let id = AgentId::new();
        let dir = setup_test_dir();

        for i in 0..5 {
            mgr.create_snapshot(id, dir.path(), &format!("snap {}", i))
                .await
                .unwrap();
        }

        let snaps = mgr.list_snapshots(id).await;
        assert_eq!(snaps.len(), 3);
        // Most recent should be last created
        assert_eq!(snaps[0].label, "snap 4");
    }

    #[tokio::test]
    async fn test_delete_snapshot() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "to delete")
            .await
            .unwrap();

        mgr.delete_snapshot(id, &snap_id).await.unwrap();
        let snaps = mgr.list_snapshots(id).await;
        assert!(snaps.is_empty());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_snapshot() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();

        let result = mgr.delete_snapshot(id, "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clear_agent_snapshots() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        mgr.create_snapshot(id, dir.path(), "a").await.unwrap();
        mgr.create_snapshot(id, dir.path(), "b").await.unwrap();

        mgr.clear_agent_snapshots(id).await;
        let snaps = mgr.list_snapshots(id).await;
        assert!(snaps.is_empty());
    }

    #[tokio::test]
    async fn test_rollback_nonexistent_agent() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();

        let result = mgr.rollback(id, "some-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rollback_nonexistent_snapshot() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        mgr.create_snapshot(id, dir.path(), "exists").await.unwrap();

        let result = mgr.rollback(id, "wrong-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_directory_snapshot() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = TempDir::new().unwrap();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "empty")
            .await
            .unwrap();

        let snaps = mgr.list_snapshots(id).await;
        assert_eq!(snaps[0].file_count, 0);

        // Adding a file then rolling back should remove it
        std::fs::write(dir.path().join("new.txt"), "content").unwrap();
        let result = mgr.rollback(id, &snap_id).await.unwrap();
        assert!(result.removed.contains(&PathBuf::from("new.txt")));
    }

    #[tokio::test]
    async fn test_nested_directory_snapshot() {
        let mgr = RollbackManager::new();
        let id = AgentId::new();
        let dir = setup_test_dir();

        let snap_id = mgr
            .create_snapshot(id, dir.path(), "with nested")
            .await
            .unwrap();

        // Modify nested file
        std::fs::write(dir.path().join("subdir/nested.txt"), "CHANGED").unwrap();

        let result = mgr.rollback(id, &snap_id).await.unwrap();
        let content =
            std::fs::read_to_string(dir.path().join("subdir/nested.txt")).unwrap();
        assert_eq!(content, "nested content");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_sha256_deterministic() {
        let hash1 = RollbackManager::sha256_hex(b"hello");
        let hash2 = RollbackManager::sha256_hex(b"hello");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_different_inputs() {
        let hash1 = RollbackManager::sha256_hex(b"hello");
        let hash2 = RollbackManager::sha256_hex(b"world");
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_multiple_agents_independent() {
        let mgr = RollbackManager::new();
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        let dir1 = setup_test_dir();
        let dir2 = TempDir::new().unwrap();

        mgr.create_snapshot(id1, dir1.path(), "agent1")
            .await
            .unwrap();
        mgr.create_snapshot(id2, dir2.path(), "agent2")
            .await
            .unwrap();

        let snaps1 = mgr.list_snapshots(id1).await;
        let snaps2 = mgr.list_snapshots(id2).await;
        assert_eq!(snaps1.len(), 1);
        assert_eq!(snaps2.len(), 1);

        mgr.clear_agent_snapshots(id1).await;
        let snaps1 = mgr.list_snapshots(id1).await;
        let snaps2 = mgr.list_snapshots(id2).await;
        assert!(snaps1.is_empty());
        assert_eq!(snaps2.len(), 1);
    }
}
