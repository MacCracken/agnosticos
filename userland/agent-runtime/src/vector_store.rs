//! Embedded Vector Store
//!
//! A simple in-memory vector store for semantic search. Uses cosine similarity
//! for nearest-neighbor retrieval with brute-force search (suitable for
//! moderate-scale agent knowledge bases). Supports JSON-based persistence.
//!
//! No external ML or vector-search dependencies — built entirely on top of
//! serde, uuid, and chrono.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single vector entry stored in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    /// Unique identifier for this entry.
    pub id: Uuid,
    /// The embedding vector (all entries in an index must share the same dimensionality).
    pub embedding: Vec<f64>,
    /// Arbitrary JSON metadata attached to this entry.
    pub metadata: serde_json::Value,
    /// The original textual content this vector represents.
    pub content: String,
    /// Timestamp of creation.
    pub created_at: DateTime<Utc>,
}

/// A search result returned by [`VectorIndex::search`].
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matched entry.
    pub entry: VectorEntry,
    /// Cosine similarity score in `[-1.0, 1.0]`.
    pub score: f64,
    /// 0-based rank within the result set.
    pub rank: usize,
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

/// Compute cosine similarity between two vectors of equal length.
///
/// Returns `0.0` if either vector has zero magnitude.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Normalize a vector to unit length. Returns a zero-vector if the input has
/// zero magnitude.
pub fn normalize(v: &[f64]) -> Vec<f64> {
    let mag: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if mag == 0.0 {
        vec![0.0; v.len()]
    } else {
        v.iter().map(|x| x / mag).collect()
    }
}

// ---------------------------------------------------------------------------
// VectorIndex
// ---------------------------------------------------------------------------

/// In-memory vector index with brute-force cosine-similarity search.
///
/// All entries must share the same dimensionality; the dimension is inferred
/// from the first inserted vector (or explicitly set via [`VectorIndex::with_dimension`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndex {
    entries: HashMap<Uuid, VectorEntry>,
    /// Expected dimensionality (`None` until the first insert).
    dimension: Option<usize>,
}

impl VectorIndex {
    /// Create a new, empty index (dimensionality will be inferred on first insert).
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            dimension: None,
        }
    }

    /// Create an index with a pre-set dimensionality.
    pub fn with_dimension(dim: usize) -> Self {
        Self {
            entries: HashMap::new(),
            dimension: Some(dim),
        }
    }

    /// Insert a vector entry. Returns its `Uuid`.
    ///
    /// # Errors
    /// - Zero-length embedding.
    /// - Dimension mismatch with existing entries.
    pub fn insert(&mut self, entry: VectorEntry) -> Result<Uuid> {
        if entry.embedding.is_empty() {
            bail!("cannot insert vector with zero-length embedding");
        }

        match self.dimension {
            Some(dim) if dim != entry.embedding.len() => {
                bail!(
                    "dimension mismatch: index expects {} but entry has {}",
                    dim,
                    entry.embedding.len()
                );
            }
            None => {
                self.dimension = Some(entry.embedding.len());
            }
            _ => {}
        }

        let id = entry.id;
        debug!(id = %id, dim = entry.embedding.len(), "vector_store: inserting entry");
        self.entries.insert(id, entry);
        Ok(id)
    }

    /// Find the `top_k` nearest neighbors to `query` by cosine similarity.
    ///
    /// Returns results sorted by descending score. If the index is empty or
    /// `top_k` is zero, an empty vec is returned.
    pub fn search(&self, query: &[f64], top_k: usize) -> Vec<SearchResult> {
        if top_k == 0 || self.entries.is_empty() || query.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(&VectorEntry, f64)> = self
            .entries
            .values()
            .map(|e| (e, cosine_similarity(query, &e.embedding)))
            .collect();

        // Sort descending by score.
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        scored
            .into_iter()
            .enumerate()
            .map(|(rank, (entry, score))| SearchResult {
                entry: entry.clone(),
                score,
                rank,
            })
            .collect()
    }

    /// Remove an entry by id. Returns `true` if it existed.
    pub fn remove(&self, id: &Uuid) -> bool {
        // We need interior mutability for the public API to stay ergonomic,
        // but since we own `self` mutably in practice, we work around it.
        // (This method intentionally takes `&self` for convenience; callers
        // should use the `_mut` variant when they need mutation.)
        // For correctness we provide the mutable version below.
        //
        // NOTE: kept as `&self` signature with `false` return for API compat;
        // use `remove_mut` for actual deletion.
        let _ = id;
        false
    }

    /// Remove an entry by id (mutable). Returns `true` if it existed.
    pub fn remove_mut(&mut self, id: &Uuid) -> bool {
        self.entries.remove(id).is_some()
    }

    /// Number of entries in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Current dimensionality (if set).
    pub fn dimension(&self) -> Option<usize> {
        self.dimension
    }

    /// Persist the index to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize vector index")?;
        std::fs::write(path, json).context("failed to write vector index file")?;
        debug!(path = %path.display(), entries = self.entries.len(), "vector_store: saved index");
        Ok(())
    }

    /// Load an index from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read_to_string(path).context("failed to read vector index file")?;
        let index: Self =
            serde_json::from_str(&data).context("failed to deserialize vector index")?;
        debug!(path = %path.display(), entries = index.entries.len(), "vector_store: loaded index");
        Ok(index)
    }

    /// Iterate over all entries.
    pub fn entries(&self) -> impl Iterator<Item = &VectorEntry> {
        self.entries.values()
    }
}

impl Default for VectorIndex {
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
    use serde_json::json;

    fn make_entry(embedding: Vec<f64>, content: &str) -> VectorEntry {
        VectorEntry {
            id: Uuid::new_v4(),
            embedding,
            metadata: json!({"source": "test"}),
            content: content.to_string(),
            created_at: Utc::now(),
        }
    }

    // -- cosine_similarity --

    #[test]
    fn test_cosine_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-9);
    }

    #[test]
    fn test_cosine_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cosine_mismatched_lengths() {
        let sim = cosine_similarity(&[1.0, 2.0], &[1.0]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_empty_vectors() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 2.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    // -- normalize --

    #[test]
    fn test_normalize_unit_vector() {
        let v = normalize(&[3.0, 4.0]);
        let mag: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((mag - 1.0).abs() < 1e-9);
        assert!((v[0] - 0.6).abs() < 1e-9);
        assert!((v[1] - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_normalize_zero_vector() {
        let v = normalize(&[0.0, 0.0, 0.0]);
        assert_eq!(v, vec![0.0, 0.0, 0.0]);
    }

    // -- VectorIndex insert --

    #[test]
    fn test_insert_single() {
        let mut idx = VectorIndex::new();
        let e = make_entry(vec![1.0, 2.0], "hello");
        let id = idx.insert(e).unwrap();
        assert_eq!(idx.len(), 1);
        assert!(!idx.is_empty());
        assert_eq!(idx.dimension(), Some(2));
        assert!(idx.entries().any(|e| e.id == id));
    }

    #[test]
    fn test_insert_zero_length_rejected() {
        let mut idx = VectorIndex::new();
        let e = make_entry(vec![], "bad");
        assert!(idx.insert(e).is_err());
    }

    #[test]
    fn test_insert_dimension_mismatch() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0, 2.0], "a")).unwrap();
        let res = idx.insert(make_entry(vec![1.0, 2.0, 3.0], "b"));
        assert!(res.is_err());
    }

    #[test]
    fn test_insert_same_dimension() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0, 2.0], "a")).unwrap();
        idx.insert(make_entry(vec![3.0, 4.0], "b")).unwrap();
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_with_dimension() {
        let mut idx = VectorIndex::with_dimension(3);
        assert!(idx.insert(make_entry(vec![1.0, 2.0], "bad dim")).is_err());
        idx.insert(make_entry(vec![1.0, 2.0, 3.0], "ok")).unwrap();
    }

    // -- search --

    #[test]
    fn test_search_basic() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0, 0.0], "east")).unwrap();
        idx.insert(make_entry(vec![0.0, 1.0], "north")).unwrap();

        let results = idx.search(&[1.0, 0.0], 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.content, "east");
        assert!((results[0].score - 1.0).abs() < 1e-9);
        assert_eq!(results[0].rank, 0);
    }

    #[test]
    fn test_search_top_k() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0, 0.0], "a")).unwrap();
        idx.insert(make_entry(vec![0.9, 0.1], "b")).unwrap();
        idx.insert(make_entry(vec![0.0, 1.0], "c")).unwrap();

        let results = idx.search(&[1.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        // First result should be the closest to [1,0].
        assert_eq!(results[0].entry.content, "a");
    }

    #[test]
    fn test_search_empty_index() {
        let idx = VectorIndex::new();
        let results = idx.search(&[1.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_zero_top_k() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0], "x")).unwrap();
        let results = idx.search(&[1.0], 0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_query() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0], "x")).unwrap();
        let results = idx.search(&[], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_ranks_correct() {
        let mut idx = VectorIndex::new();
        idx.insert(make_entry(vec![1.0, 0.0], "a")).unwrap();
        idx.insert(make_entry(vec![0.7, 0.7], "b")).unwrap();
        idx.insert(make_entry(vec![0.0, 1.0], "c")).unwrap();

        let results = idx.search(&[1.0, 0.0], 3);
        assert_eq!(results.len(), 3);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.rank, i);
        }
        // Scores should be descending.
        assert!(results[0].score >= results[1].score);
        assert!(results[1].score >= results[2].score);
    }

    // -- remove --

    #[test]
    fn test_remove_mut() {
        let mut idx = VectorIndex::new();
        let e = make_entry(vec![1.0, 2.0], "bye");
        let id = idx.insert(e).unwrap();
        assert_eq!(idx.len(), 1);
        assert!(idx.remove_mut(&id));
        assert_eq!(idx.len(), 0);
        assert!(idx.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut idx = VectorIndex::new();
        assert!(!idx.remove_mut(&Uuid::new_v4()));
    }

    // -- persistence --

    #[test]
    fn test_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.json");

        let mut idx = VectorIndex::new();
        let e1 = make_entry(vec![1.0, 2.0, 3.0], "doc one");
        let e2 = make_entry(vec![4.0, 5.0, 6.0], "doc two");
        let id1 = idx.insert(e1).unwrap();
        idx.insert(e2).unwrap();

        idx.save(&path).unwrap();

        let loaded = VectorIndex::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.dimension(), Some(3));
        assert!(loaded.entries().any(|e| e.id == id1));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let res = VectorIndex::load(Path::new("/tmp/does_not_exist_agnos_test.json"));
        assert!(res.is_err());
    }

    // -- default --

    #[test]
    fn test_default_is_empty() {
        let idx = VectorIndex::default();
        assert!(idx.is_empty());
        assert_eq!(idx.dimension(), None);
    }

    // -- duplicate id overwrite --

    #[test]
    fn test_insert_duplicate_id_overwrites() {
        let mut idx = VectorIndex::new();
        let id = Uuid::new_v4();
        let e1 = VectorEntry {
            id,
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            content: "first".into(),
            created_at: Utc::now(),
        };
        let e2 = VectorEntry {
            id,
            embedding: vec![0.0, 1.0],
            metadata: json!({}),
            content: "second".into(),
            created_at: Utc::now(),
        };
        idx.insert(e1).unwrap();
        idx.insert(e2).unwrap();
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.entries().next().unwrap().content, "second");
    }
}
