//! System Knowledge Base
//!
//! Auto-indexes system documentation, agent manifests, audit logs, and
//! configuration files. Provides keyword-based search over indexed content
//! with source-type filtering and statistics.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The type of knowledge source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KnowledgeSource {
    /// System manual pages.
    ManPage,
    /// Agent manifest / registration files.
    AgentManifest,
    /// Audit log entries.
    AuditLog,
    /// Configuration files.
    ConfigFile,
    /// User-defined source type.
    Custom(String),
}

impl std::fmt::Display for KnowledgeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManPage => write!(f, "ManPage"),
            Self::AgentManifest => write!(f, "AgentManifest"),
            Self::AuditLog => write!(f, "AuditLog"),
            Self::ConfigFile => write!(f, "ConfigFile"),
            Self::Custom(s) => write!(f, "Custom({})", s),
        }
    }
}

/// A single indexed knowledge entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    /// Unique identifier.
    pub id: Uuid,
    /// Source category.
    pub source: KnowledgeSource,
    /// File path this entry was loaded from.
    pub path: PathBuf,
    /// Full textual content.
    pub content: String,
    /// Timestamp when this entry was indexed.
    pub indexed_at: DateTime<Utc>,
    /// Free-form tags for additional classification.
    pub tags: Vec<String>,
}

/// A search result with relevance scoring.
#[derive(Debug, Clone)]
pub struct KnowledgeResult {
    /// The matched entry.
    pub entry: KnowledgeEntry,
    /// Relevance score (higher is better).
    pub relevance_score: f64,
}

/// Aggregate statistics about the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeStats {
    /// Total number of indexed entries.
    pub total_entries: usize,
    /// Sum of content byte lengths.
    pub total_bytes: usize,
    /// Count of entries per source type.
    pub entries_by_source: HashMap<String, usize>,
}

// ---------------------------------------------------------------------------
// KnowledgeBase
// ---------------------------------------------------------------------------

/// In-memory knowledge base that indexes text content from various sources
/// and supports keyword-frequency search.
#[derive(Debug, Clone)]
pub struct KnowledgeBase {
    entries: Vec<KnowledgeEntry>,
}

impl KnowledgeBase {
    /// Create a new, empty knowledge base.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Recursively index all text files under `path`.
    ///
    /// Returns the number of successfully indexed files. Files that cannot be
    /// read as UTF-8 text are silently skipped.
    pub fn index_directory(&mut self, path: &Path, source: KnowledgeSource) -> Result<usize> {
        if !path.is_dir() {
            anyhow::bail!("path is not a directory: {}", path.display());
        }

        let mut count = 0usize;
        self.walk_dir(path, &source, &mut count)?;
        debug!(path = %path.display(), source = %source, indexed = count, "knowledge_base: indexed directory");
        Ok(count)
    }

    /// Recursively walk a directory, indexing text files.
    fn walk_dir(
        &mut self,
        dir: &Path,
        source: &KnowledgeSource,
        count: &mut usize,
    ) -> Result<()> {
        let read_dir = std::fs::read_dir(dir)
            .with_context(|| format!("failed to read directory: {}", dir.display()))?;

        for entry_result in read_dir {
            let entry = match entry_result {
                Ok(e) => e,
                Err(err) => {
                    warn!(error = %err, "knowledge_base: skipping unreadable dir entry");
                    continue;
                }
            };

            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            let path = entry.path();

            if ft.is_dir() {
                // Recurse, but don't fail the whole operation on one bad subdir.
                if let Err(err) = self.walk_dir(&path, source, count) {
                    warn!(error = %err, path = %path.display(), "knowledge_base: skipping subdirectory");
                }
            } else if ft.is_file() {
                match std::fs::read_to_string(&path) {
                    Ok(content) if !content.is_empty() => {
                        let entry = KnowledgeEntry {
                            id: Uuid::new_v4(),
                            source: source.clone(),
                            path: path.clone(),
                            content,
                            indexed_at: Utc::now(),
                            tags: Vec::new(),
                        };
                        self.entries.push(entry);
                        *count += 1;
                    }
                    Ok(_) => {
                        // Empty file — skip.
                    }
                    Err(_) => {
                        // Non-UTF-8 or unreadable — skip.
                    }
                }
            }
        }

        Ok(())
    }

    /// Index a single document.
    pub fn index_text(
        &mut self,
        content: &str,
        source: KnowledgeSource,
        path: &Path,
    ) -> Result<Uuid> {
        let entry = KnowledgeEntry {
            id: Uuid::new_v4(),
            source,
            path: path.to_path_buf(),
            content: content.to_string(),
            indexed_at: Utc::now(),
            tags: Vec::new(),
        };
        let id = entry.id;
        self.entries.push(entry);
        debug!(id = %id, path = %path.display(), "knowledge_base: indexed text");
        Ok(id)
    }

    /// Keyword search across all entries.
    ///
    /// Scores entries by how many query words appear in the content (case-insensitive),
    /// weighted by frequency. Returns up to `limit` results sorted by descending
    /// relevance.
    pub fn search(&self, query: &str, limit: usize) -> Vec<KnowledgeResult> {
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }

        let query_words: Vec<String> = query
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        if query_words.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<KnowledgeResult> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let lower = entry.content.to_lowercase();
                let mut score = 0.0_f64;

                for qw in &query_words {
                    let matches = lower.matches(qw.as_str()).count();
                    if matches > 0 {
                        // TF-like: log(1 + count) to dampen very frequent terms.
                        score += (1.0 + matches as f64).ln();
                    }
                }

                if score > 0.0 {
                    Some(KnowledgeResult {
                        entry: entry.clone(),
                        relevance_score: score,
                    })
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        scored
    }

    /// Return entries matching a specific source type, up to `limit`.
    pub fn search_by_source(
        &self,
        source: &KnowledgeSource,
        limit: usize,
    ) -> Vec<KnowledgeEntry> {
        self.entries
            .iter()
            .filter(|e| &e.source == source)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Aggregate statistics about the knowledge base.
    pub fn stats(&self) -> KnowledgeStats {
        let mut entries_by_source: HashMap<String, usize> = HashMap::new();
        let mut total_bytes = 0usize;

        for entry in &self.entries {
            *entries_by_source
                .entry(entry.source.to_string())
                .or_default() += 1;
            total_bytes += entry.content.len();
        }

        KnowledgeStats {
            total_entries: self.entries.len(),
            total_bytes,
            entries_by_source,
        }
    }

    /// Remove all entries for a given source type. Returns the number removed.
    pub fn remove_source(&mut self, source: &KnowledgeSource) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| &e.source != source);
        let removed = before - self.entries.len();
        debug!(source = %source, removed = removed, "knowledge_base: removed source");
        removed
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the knowledge base is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for KnowledgeBase {
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
    use std::fs;

    // -- KnowledgeSource display --

    #[test]
    fn test_source_display() {
        assert_eq!(KnowledgeSource::ManPage.to_string(), "ManPage");
        assert_eq!(
            KnowledgeSource::Custom("notes".into()).to_string(),
            "Custom(notes)"
        );
    }

    // -- new --

    #[test]
    fn test_new_is_empty() {
        let kb = KnowledgeBase::new();
        assert!(kb.is_empty());
        assert_eq!(kb.len(), 0);
    }

    // -- index_text --

    #[test]
    fn test_index_text() {
        let mut kb = KnowledgeBase::new();
        let id = kb
            .index_text("hello world", KnowledgeSource::ManPage, Path::new("/tmp/test"))
            .unwrap();
        assert_eq!(kb.len(), 1);
        let entry = &kb.entries[0];
        assert_eq!(entry.id, id);
        assert_eq!(entry.content, "hello world");
        assert_eq!(entry.source, KnowledgeSource::ManPage);
    }

    // -- search --

    #[test]
    fn test_search_basic() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("rust programming language", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();
        kb.index_text("python scripting", KnowledgeSource::ManPage, Path::new("/b"))
            .unwrap();

        let results = kb.search("rust", 10);
        assert_eq!(results.len(), 1);
        assert!(results[0].entry.content.contains("rust"));
        assert!(results[0].relevance_score > 0.0);
    }

    #[test]
    fn test_search_multiple_matches() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("cat cat cat", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();
        kb.index_text("cat dog", KnowledgeSource::ManPage, Path::new("/b"))
            .unwrap();

        let results = kb.search("cat", 10);
        assert_eq!(results.len(), 2);
        // First result should have higher score (more occurrences).
        assert!(results[0].relevance_score >= results[1].relevance_score);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("Hello World", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();

        let results = kb.search("hello", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_no_match() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("alpha beta", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();

        let results = kb.search("gamma", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_query() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("test", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();
        assert!(kb.search("", 10).is_empty());
    }

    #[test]
    fn test_search_limit() {
        let mut kb = KnowledgeBase::new();
        for i in 0..10 {
            kb.index_text(
                &format!("document number {i} about rust"),
                KnowledgeSource::ManPage,
                Path::new("/a"),
            )
            .unwrap();
        }

        let results = kb.search("rust", 3);
        assert_eq!(results.len(), 3);
    }

    // -- search_by_source --

    #[test]
    fn test_search_by_source() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("a", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();
        kb.index_text("b", KnowledgeSource::AuditLog, Path::new("/b"))
            .unwrap();
        kb.index_text("c", KnowledgeSource::ManPage, Path::new("/c"))
            .unwrap();

        let results = kb.search_by_source(&KnowledgeSource::ManPage, 10);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.source == KnowledgeSource::ManPage));
    }

    #[test]
    fn test_search_by_source_limit() {
        let mut kb = KnowledgeBase::new();
        for _ in 0..5 {
            kb.index_text("x", KnowledgeSource::ConfigFile, Path::new("/x"))
                .unwrap();
        }

        let results = kb.search_by_source(&KnowledgeSource::ConfigFile, 2);
        assert_eq!(results.len(), 2);
    }

    // -- stats --

    #[test]
    fn test_stats() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("hello", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();
        kb.index_text("world!", KnowledgeSource::AuditLog, Path::new("/b"))
            .unwrap();

        let stats = kb.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.total_bytes, 5 + 6); // "hello" + "world!"
        assert_eq!(stats.entries_by_source["ManPage"], 1);
        assert_eq!(stats.entries_by_source["AuditLog"], 1);
    }

    #[test]
    fn test_stats_empty() {
        let kb = KnowledgeBase::new();
        let stats = kb.stats();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.total_bytes, 0);
        assert!(stats.entries_by_source.is_empty());
    }

    // -- remove_source --

    #[test]
    fn test_remove_source() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("a", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();
        kb.index_text("b", KnowledgeSource::AuditLog, Path::new("/b"))
            .unwrap();
        kb.index_text("c", KnowledgeSource::ManPage, Path::new("/c"))
            .unwrap();

        let removed = kb.remove_source(&KnowledgeSource::ManPage);
        assert_eq!(removed, 2);
        assert_eq!(kb.len(), 1);
        assert_eq!(kb.entries[0].source, KnowledgeSource::AuditLog);
    }

    #[test]
    fn test_remove_source_none_matching() {
        let mut kb = KnowledgeBase::new();
        kb.index_text("a", KnowledgeSource::ManPage, Path::new("/a"))
            .unwrap();

        let removed = kb.remove_source(&KnowledgeSource::AuditLog);
        assert_eq!(removed, 0);
        assert_eq!(kb.len(), 1);
    }

    // -- index_directory --

    #[test]
    fn test_index_directory() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(dir.path().join("doc1.txt"), "first document").unwrap();
        fs::write(dir.path().join("doc2.txt"), "second document").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/nested.txt"), "nested document").unwrap();
        // Empty file — should be skipped.
        fs::write(dir.path().join("empty.txt"), "").unwrap();

        let mut kb = KnowledgeBase::new();
        let count = kb
            .index_directory(dir.path(), KnowledgeSource::ConfigFile)
            .unwrap();

        assert_eq!(count, 3);
        assert_eq!(kb.len(), 3);
    }

    #[test]
    fn test_index_directory_not_a_dir() {
        let mut kb = KnowledgeBase::new();
        let res = kb.index_directory(Path::new("/tmp/nonexistent_agnos_test"), KnowledgeSource::ManPage);
        assert!(res.is_err());
    }

    // -- default --

    #[test]
    fn test_default() {
        let kb = KnowledgeBase::default();
        assert!(kb.is_empty());
    }
}
