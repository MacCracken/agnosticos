//! File Watcher
//!
//! Polling-based file change detection for auto-reindexing. Tracks file
//! modification times and emits [`WatchEvent`]s when changes are detected.
//!
//! Uses stat-based polling rather than inotify to avoid adding new
//! dependencies. The polling interval and glob patterns are configurable
//! via [`FileWatcherConfig`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Identifier for a watched path registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatchId(pub u64);

impl std::fmt::Display for WatchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WatchId({})", self.0)
    }
}

/// An event emitted when a watched file changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// A new file was detected.
    Created(PathBuf),
    /// An existing file's mtime changed.
    Modified(PathBuf),
    /// A previously seen file no longer exists.
    Deleted(PathBuf),
}

/// Configuration for the file watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatcherConfig {
    /// Polling interval in milliseconds.
    pub poll_interval_ms: u64,
    /// Whether to watch directories recursively by default.
    pub recursive: bool,
    /// Glob-style filename patterns to include (e.g., `["*.txt", "*.md"]`).
    /// An empty list means "accept all files".
    pub patterns: Vec<String>,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1000,
            recursive: true,
            patterns: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal state per watched registration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct WatchRegistration {
    id: WatchId,
    root: PathBuf,
    recursive: bool,
}

// ---------------------------------------------------------------------------
// FileWatcher
// ---------------------------------------------------------------------------

/// Polling-based file watcher.
///
/// Call [`FileWatcher::watch`] to register paths, then periodically call
/// [`FileWatcher::poll_events`] to retrieve change events.
#[derive(Debug)]
pub struct FileWatcher {
    config: FileWatcherConfig,
    next_id: u64,
    registrations: Vec<WatchRegistration>,
    /// Last-known mtime per absolute file path.
    known_files: HashMap<PathBuf, SystemTime>,
}

impl FileWatcher {
    /// Create a new file watcher with default configuration.
    pub fn new() -> Self {
        Self::with_config(FileWatcherConfig::default())
    }

    /// Create a new file watcher with the given configuration.
    pub fn with_config(config: FileWatcherConfig) -> Self {
        Self {
            config,
            next_id: 1,
            registrations: Vec::new(),
            known_files: HashMap::new(),
        }
    }

    /// Register a path for watching. Returns a [`WatchId`] that can be used
    /// to unwatch later.
    pub fn watch(&mut self, path: &Path, recursive: bool) -> Result<WatchId> {
        if !path.exists() {
            bail!("path does not exist: {}", path.display());
        }

        let id = WatchId(self.next_id);
        self.next_id += 1;

        let root = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize: {}", path.display()))?;

        self.registrations.push(WatchRegistration {
            id,
            root: root.clone(),
            recursive,
        });

        // Seed the known-files map with the current state so the first poll
        // doesn't report everything as "Created".
        self.seed_known_files(&root, recursive);

        debug!(id = %id, path = %root.display(), recursive, "file_watcher: watching");
        Ok(id)
    }

    /// Stop watching a previously registered path.
    pub fn unwatch(&mut self, id: WatchId) -> Result<()> {
        let before = self.registrations.len();
        self.registrations.retain(|r| r.id != id);
        if self.registrations.len() == before {
            bail!("unknown watch id: {}", id);
        }
        debug!(id = %id, "file_watcher: unwatched");
        Ok(())
    }

    /// Non-blocking poll for file change events since the last poll.
    pub fn poll_events(&mut self) -> Vec<WatchEvent> {
        let mut events = Vec::new();

        // Collect current file states across all registrations.
        let mut current_files: HashMap<PathBuf, SystemTime> = HashMap::new();
        for reg in &self.registrations {
            self.collect_files(&reg.root, reg.recursive, &mut current_files);
        }

        // Detect created and modified files.
        for (path, mtime) in &current_files {
            if !self.matches_patterns(path) {
                continue;
            }
            match self.known_files.get(path) {
                None => {
                    events.push(WatchEvent::Created(path.clone()));
                }
                Some(old_mtime) if old_mtime != mtime => {
                    events.push(WatchEvent::Modified(path.clone()));
                }
                _ => {}
            }
        }

        // Detect deleted files (were known but no longer present).
        for path in self.known_files.keys() {
            if !current_files.contains_key(path) && self.matches_patterns(path) {
                events.push(WatchEvent::Deleted(path.clone()));
            }
        }

        // Update known state.
        self.known_files = current_files;

        events
    }

    /// List all currently watched paths and their ids.
    pub fn watched_paths(&self) -> Vec<(WatchId, PathBuf)> {
        self.registrations
            .iter()
            .map(|r| (r.id, r.root.clone()))
            .collect()
    }

    /// Number of active watch registrations.
    pub fn watch_count(&self) -> usize {
        self.registrations.len()
    }

    /// Access the current config.
    pub fn config(&self) -> &FileWatcherConfig {
        &self.config
    }

    // -- internal helpers --

    /// Seed known files from the filesystem so the first poll doesn't produce
    /// spurious Created events.
    fn seed_known_files(&mut self, root: &Path, recursive: bool) {
        let mut files = HashMap::new();
        self.collect_files(root, recursive, &mut files);
        for (path, mtime) in files {
            self.known_files.entry(path).or_insert(mtime);
        }
    }

    /// Collect all files (and their mtimes) under `root`.
    fn collect_files(&self, root: &Path, recursive: bool, out: &mut HashMap<PathBuf, SystemTime>) {
        if root.is_file() {
            if let Ok(meta) = root.metadata() {
                if let Ok(mtime) = meta.modified() {
                    out.insert(root.to_path_buf(), mtime);
                }
            }
            return;
        }

        let read_dir = match std::fs::read_dir(root) {
            Ok(rd) => rd,
            Err(err) => {
                warn!(error = %err, path = %root.display(), "file_watcher: failed to read dir");
                return;
            }
        };

        for entry_result in read_dir {
            let entry = match entry_result {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if ft.is_file() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        out.insert(path, mtime);
                    }
                }
            } else if ft.is_dir() && recursive {
                self.collect_files(&path, true, out);
            }
        }
    }

    /// Check if a path matches the configured glob patterns.
    /// Empty patterns list means "accept everything".
    fn matches_patterns(&self, path: &Path) -> bool {
        if self.config.patterns.is_empty() {
            return true;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => return false,
        };

        self.config
            .patterns
            .iter()
            .any(|pattern| simple_glob_match(pattern, file_name))
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Very basic glob matching supporting `*` as a wildcard sequence.
///
/// Supports patterns like `*.txt`, `config.*`, `*test*`. Does not support
/// `?`, `[…]`, or `**`.
fn simple_glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 1 {
        // No wildcard — exact match.
        return pattern == name;
    }

    let mut pos = 0usize;

    // First part must match at the start (unless pattern starts with *).
    if let Some(first) = parts.first() {
        if !first.is_empty() {
            if !name.starts_with(first) {
                return false;
            }
            pos = first.len();
        }
    }

    // Last part must match at the end (unless pattern ends with *).
    if let Some(last) = parts.last() {
        if !last.is_empty() && !name[pos..].ends_with(last) {
            return false;
        }
    }

    // Interior parts must appear in order.
    for part in &parts[1..parts.len().saturating_sub(1)] {
        if part.is_empty() {
            continue;
        }
        match name[pos..].find(part) {
            Some(idx) => pos += idx + part.len(),
            None => return false,
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    // -- WatchId --

    #[test]
    fn test_watch_id_display() {
        assert_eq!(WatchId(42).to_string(), "WatchId(42)");
    }

    // -- simple_glob_match --

    #[test]
    fn test_glob_star_extension() {
        assert!(simple_glob_match("*.txt", "readme.txt"));
        assert!(!simple_glob_match("*.txt", "readme.md"));
    }

    #[test]
    fn test_glob_prefix_star() {
        assert!(simple_glob_match("config.*", "config.toml"));
        assert!(!simple_glob_match("config.*", "settings.toml"));
    }

    #[test]
    fn test_glob_star_middle() {
        assert!(simple_glob_match("*test*", "my_test_file"));
        assert!(!simple_glob_match("*test*", "production"));
    }

    #[test]
    fn test_glob_exact() {
        assert!(simple_glob_match("Makefile", "Makefile"));
        assert!(!simple_glob_match("Makefile", "makefile"));
    }

    #[test]
    fn test_glob_star_only() {
        assert!(simple_glob_match("*", "anything"));
    }

    // -- FileWatcher new / default --

    #[test]
    fn test_new_watcher() {
        let fw = FileWatcher::new();
        assert_eq!(fw.watch_count(), 0);
        assert!(fw.watched_paths().is_empty());
    }

    // -- watch / unwatch --

    #[test]
    fn test_watch_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello").unwrap();

        let mut fw = FileWatcher::new();
        let id = fw.watch(&file, false).unwrap();
        assert_eq!(fw.watch_count(), 1);
        assert_eq!(fw.watched_paths().len(), 1);
        assert_eq!(fw.watched_paths()[0].0, id);
    }

    #[test]
    fn test_watch_nonexistent() {
        let mut fw = FileWatcher::new();
        let res = fw.watch(Path::new("/tmp/does_not_exist_agnos_fw_test"), false);
        assert!(res.is_err());
    }

    #[test]
    fn test_unwatch() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();

        let mut fw = FileWatcher::new();
        let id = fw.watch(dir.path(), false).unwrap();
        fw.unwatch(id).unwrap();
        assert_eq!(fw.watch_count(), 0);
    }

    #[test]
    fn test_unwatch_unknown_id() {
        let mut fw = FileWatcher::new();
        assert!(fw.unwatch(WatchId(999)).is_err());
    }

    // -- poll_events --

    #[test]
    fn test_poll_no_changes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("stable.txt"), "data").unwrap();

        let mut fw = FileWatcher::new();
        fw.watch(dir.path(), false).unwrap();

        let events = fw.poll_events();
        assert!(
            events.is_empty(),
            "no changes expected on first poll after seed"
        );
    }

    #[test]
    fn test_poll_detect_created() {
        let dir = tempfile::tempdir().unwrap();

        let mut fw = FileWatcher::new();
        fw.watch(dir.path(), true).unwrap();

        // Create a new file after watching.
        fs::write(dir.path().join("new_file.txt"), "created").unwrap();

        let events = fw.poll_events();
        let created: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, WatchEvent::Created(_)))
            .collect();
        assert!(!created.is_empty(), "should detect created file");
    }

    #[test]
    fn test_poll_detect_modified() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("modify_me.txt");
        fs::write(&file, "original").unwrap();

        let mut fw = FileWatcher::new();
        fw.watch(dir.path(), false).unwrap();

        // Wait briefly so mtime differs, then modify.
        thread::sleep(Duration::from_millis(50));
        fs::write(&file, "modified content").unwrap();

        let events = fw.poll_events();
        let modified: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, WatchEvent::Modified(_)))
            .collect();
        assert!(!modified.is_empty(), "should detect modified file");
    }

    #[test]
    fn test_poll_detect_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("delete_me.txt");
        fs::write(&file, "soon gone").unwrap();

        let mut fw = FileWatcher::new();
        fw.watch(dir.path(), false).unwrap();

        // Delete the file.
        fs::remove_file(&file).unwrap();

        let events = fw.poll_events();
        let deleted: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, WatchEvent::Deleted(_)))
            .collect();
        assert!(!deleted.is_empty(), "should detect deleted file");
    }

    // -- patterns filtering --

    #[test]
    fn test_patterns_filter() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("include.txt"), "yes").unwrap();

        let config = FileWatcherConfig {
            patterns: vec!["*.md".into()],
            ..Default::default()
        };
        let mut fw = FileWatcher::with_config(config);
        fw.watch(dir.path(), false).unwrap();

        // Create a .txt file (should NOT be reported) and a .md file (should be).
        fs::write(dir.path().join("new.txt"), "no").unwrap();
        fs::write(dir.path().join("new.md"), "yes").unwrap();

        let events = fw.poll_events();
        // Only the .md file should produce a Created event.
        for event in &events {
            match event {
                WatchEvent::Created(p) | WatchEvent::Modified(p) => {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    assert_eq!(ext, "md", "only .md files should be reported");
                }
                _ => {}
            }
        }
    }

    // -- config --

    #[test]
    fn test_config_defaults() {
        let cfg = FileWatcherConfig::default();
        assert_eq!(cfg.poll_interval_ms, 1000);
        assert!(cfg.recursive);
        assert!(cfg.patterns.is_empty());
    }

    #[test]
    fn test_watcher_config_access() {
        let cfg = FileWatcherConfig {
            poll_interval_ms: 500,
            recursive: false,
            patterns: vec!["*.rs".into()],
        };
        let fw = FileWatcher::with_config(cfg.clone());
        assert_eq!(fw.config().poll_interval_ms, 500);
        assert!(!fw.config().recursive);
    }
}
