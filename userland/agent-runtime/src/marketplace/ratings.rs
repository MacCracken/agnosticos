//! Agent Rating & Review System
//!
//! Allows agents and users to rate and review marketplace packages.
//! Supports deduplication (one rating per agent per package, latest wins),
//! aggregate statistics, filtering, and JSON persistence.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum length for review text.
pub const MAX_REVIEW_LENGTH: usize = 2000;

/// Minimum star rating.
pub const MIN_SCORE: u8 = 1;

/// Maximum star rating.
pub const MAX_SCORE: u8 = 5;

// ---------------------------------------------------------------------------
// Rating
// ---------------------------------------------------------------------------

/// A single rating/review for a marketplace package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rating {
    /// Agent or user ID of the reviewer.
    pub agent_id: String,
    /// Name of the rated package.
    pub package_name: String,
    /// Star score (1-5).
    pub score: u8,
    /// Optional free-text review (max 2000 characters).
    #[serde(default)]
    pub review: Option<String>,
    /// When the rating was submitted.
    pub created_at: DateTime<Utc>,
    /// Version of the package that was reviewed.
    pub version_reviewed: String,
}

// ---------------------------------------------------------------------------
// RatingStats
// ---------------------------------------------------------------------------

/// Aggregate statistics for a single package's ratings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RatingStats {
    /// Package name.
    pub package_name: String,
    /// Average score across all ratings.
    pub average_score: f64,
    /// Total number of ratings.
    pub total_ratings: usize,
    /// Distribution of scores (score -> count).
    pub distribution: HashMap<u8, usize>,
    /// Timestamp of the most recent review.
    pub latest_review: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// RatingFilter
// ---------------------------------------------------------------------------

/// Filter criteria for querying ratings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RatingFilter {
    /// Only ratings with score >= this value.
    pub min_score: Option<u8>,
    /// Only ratings for this package.
    pub package_name: Option<String>,
    /// Only ratings by this agent.
    pub agent_id: Option<String>,
    /// Only ratings on or after this time.
    pub from: Option<DateTime<Utc>>,
    /// Only ratings on or before this time.
    pub until: Option<DateTime<Utc>>,
}

impl RatingFilter {
    /// Returns `true` if `rating` matches every set criterion.
    fn matches(&self, rating: &Rating) -> bool {
        if let Some(min) = self.min_score {
            if rating.score < min {
                return false;
            }
        }
        if let Some(ref pkg) = self.package_name {
            if rating.package_name != *pkg {
                return false;
            }
        }
        if let Some(ref aid) = self.agent_id {
            if rating.agent_id != *aid {
                return false;
            }
        }
        if let Some(from) = self.from {
            if rating.created_at < from {
                return false;
            }
        }
        if let Some(until) = self.until {
            if rating.created_at > until {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// RatingStore
// ---------------------------------------------------------------------------

/// In-memory store of all ratings, keyed by `(package_name, agent_id)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingStore {
    /// All ratings, keyed by package name, then by agent_id for dedup.
    ratings: HashMap<String, HashMap<String, Rating>>,
}

impl RatingStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            ratings: HashMap::new(),
        }
    }

    // -- mutations --

    /// Add or update a rating.
    ///
    /// Validates input and enforces deduplication: if the same agent already
    /// rated the same package, the previous rating is replaced.
    pub fn add_rating(
        &mut self,
        agent_id: String,
        package_name: String,
        score: u8,
        review: Option<String>,
        version_reviewed: String,
    ) -> Result<Rating> {
        // Validate inputs
        if agent_id.is_empty() {
            bail!("agent_id must not be empty");
        }
        if package_name.is_empty() {
            bail!("package_name must not be empty");
        }
        if score < MIN_SCORE || score > MAX_SCORE {
            bail!(
                "score must be between {} and {}, got {}",
                MIN_SCORE,
                MAX_SCORE,
                score
            );
        }
        if let Some(ref text) = review {
            if text.len() > MAX_REVIEW_LENGTH {
                bail!(
                    "review text exceeds maximum length of {} characters (got {})",
                    MAX_REVIEW_LENGTH,
                    text.len()
                );
            }
        }

        let rating = Rating {
            agent_id: agent_id.clone(),
            package_name: package_name.clone(),
            score,
            review,
            created_at: Utc::now(),
            version_reviewed,
        };

        let pkg_ratings = self.ratings.entry(package_name.clone()).or_default();

        if pkg_ratings.contains_key(&agent_id) {
            debug!(
                package = %package_name,
                agent = %agent_id,
                "Updating existing rating (dedup)"
            );
        } else {
            debug!(
                package = %package_name,
                agent = %agent_id,
                score = score,
                "New rating added"
            );
        }

        pkg_ratings.insert(agent_id, rating.clone());
        Ok(rating)
    }

    // -- queries --

    /// Return all ratings that match the given filter.
    pub fn get_ratings(&self, filter: &RatingFilter) -> Vec<Rating> {
        let mut results: Vec<Rating> = self
            .ratings
            .values()
            .flat_map(|agent_map| agent_map.values())
            .filter(|r| filter.matches(r))
            .cloned()
            .collect();

        // Sort by created_at descending (newest first).
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results
    }

    /// Compute aggregate statistics for a package.
    ///
    /// Returns `None` if the package has no ratings.
    pub fn get_stats(&self, package_name: &str) -> Option<RatingStats> {
        let agent_map = self.ratings.get(package_name)?;
        if agent_map.is_empty() {
            return None;
        }

        let mut distribution: HashMap<u8, usize> = HashMap::new();
        let mut total: f64 = 0.0;
        let mut latest: Option<DateTime<Utc>> = None;

        for rating in agent_map.values() {
            *distribution.entry(rating.score).or_insert(0) += 1;
            total += rating.score as f64;
            match latest {
                None => latest = Some(rating.created_at),
                Some(prev) if rating.created_at > prev => latest = Some(rating.created_at),
                _ => {}
            }
        }

        let count = agent_map.len();
        Some(RatingStats {
            package_name: package_name.to_string(),
            average_score: total / count as f64,
            total_ratings: count,
            distribution,
            latest_review: latest,
        })
    }

    /// Return packages sorted by average rating (descending).
    ///
    /// If `min_ratings` is set, packages with fewer ratings are excluded.
    pub fn top_rated(&self, min_ratings: Option<usize>) -> Vec<RatingStats> {
        let threshold = min_ratings.unwrap_or(0);

        let mut stats: Vec<RatingStats> = self
            .ratings
            .keys()
            .filter_map(|name| self.get_stats(name))
            .filter(|s| s.total_ratings >= threshold)
            .collect();

        // Sort by average_score desc, then by total_ratings desc for ties.
        stats.sort_by(|a, b| {
            b.average_score
                .partial_cmp(&a.average_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.total_ratings.cmp(&a.total_ratings))
        });

        stats
    }

    /// Total number of unique ratings across all packages.
    pub fn total_count(&self) -> usize {
        self.ratings.values().map(|m| m.len()).sum()
    }

    /// Number of rated packages.
    pub fn package_count(&self) -> usize {
        self.ratings.len()
    }

    // -- persistence --

    /// Save the store to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("Failed to serialize rating store")?;
        std::fs::write(path, json).with_context(|| {
            format!("Failed to write rating store to {}", path.display())
        })?;
        debug!(path = %path.display(), "Rating store saved");
        Ok(())
    }

    /// Load a store from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            warn!(
                path = %path.display(),
                "Rating store file not found, returning empty store"
            );
            return Ok(Self::new());
        }
        let data = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read rating store from {}", path.display())
        })?;
        let store: Self =
            serde_json::from_str(&data).context("Failed to deserialize rating store")?;
        debug!(
            path = %path.display(),
            packages = store.package_count(),
            ratings = store.total_count(),
            "Rating store loaded"
        );
        Ok(store)
    }
}

impl Default for RatingStore {
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
    use chrono::Duration;

    // -- helpers --

    fn make_store_with_ratings() -> RatingStore {
        let mut store = RatingStore::new();
        store
            .add_rating(
                "agent-1".into(),
                "pkg-a".into(),
                5,
                Some("Great!".into()),
                "1.0.0".into(),
            )
            .unwrap();
        store
            .add_rating(
                "agent-2".into(),
                "pkg-a".into(),
                3,
                None,
                "1.0.0".into(),
            )
            .unwrap();
        store
            .add_rating(
                "agent-1".into(),
                "pkg-b".into(),
                4,
                Some("Good".into()),
                "2.0.0".into(),
            )
            .unwrap();
        store
    }

    // -- add_rating --

    #[test]
    fn test_add_rating_basic() {
        let mut store = RatingStore::new();
        let r = store
            .add_rating(
                "agent-1".into(),
                "my-package".into(),
                4,
                Some("Nice package".into()),
                "1.0.0".into(),
            )
            .unwrap();
        assert_eq!(r.agent_id, "agent-1");
        assert_eq!(r.package_name, "my-package");
        assert_eq!(r.score, 4);
        assert_eq!(r.review.as_deref(), Some("Nice package"));
        assert_eq!(r.version_reviewed, "1.0.0");
        assert_eq!(store.total_count(), 1);
    }

    #[test]
    fn test_add_rating_no_review() {
        let mut store = RatingStore::new();
        let r = store
            .add_rating(
                "agent-1".into(),
                "pkg".into(),
                3,
                None,
                "0.1.0".into(),
            )
            .unwrap();
        assert!(r.review.is_none());
    }

    #[test]
    fn test_add_rating_min_score() {
        let mut store = RatingStore::new();
        let r = store
            .add_rating("a".into(), "p".into(), 1, None, "1.0.0".into())
            .unwrap();
        assert_eq!(r.score, 1);
    }

    #[test]
    fn test_add_rating_max_score() {
        let mut store = RatingStore::new();
        let r = store
            .add_rating("a".into(), "p".into(), 5, None, "1.0.0".into())
            .unwrap();
        assert_eq!(r.score, 5);
    }

    // -- validation errors --

    #[test]
    fn test_add_rating_score_zero() {
        let mut store = RatingStore::new();
        let err = store
            .add_rating("a".into(), "p".into(), 0, None, "1.0.0".into())
            .unwrap_err();
        assert!(err.to_string().contains("score must be between"));
    }

    #[test]
    fn test_add_rating_score_too_high() {
        let mut store = RatingStore::new();
        let err = store
            .add_rating("a".into(), "p".into(), 6, None, "1.0.0".into())
            .unwrap_err();
        assert!(err.to_string().contains("score must be between"));
    }

    #[test]
    fn test_add_rating_score_255() {
        let mut store = RatingStore::new();
        assert!(store
            .add_rating("a".into(), "p".into(), 255, None, "1.0.0".into())
            .is_err());
    }

    #[test]
    fn test_add_rating_empty_agent_id() {
        let mut store = RatingStore::new();
        let err = store
            .add_rating("".into(), "p".into(), 3, None, "1.0.0".into())
            .unwrap_err();
        assert!(err.to_string().contains("agent_id must not be empty"));
    }

    #[test]
    fn test_add_rating_empty_package_name() {
        let mut store = RatingStore::new();
        let err = store
            .add_rating("a".into(), "".into(), 3, None, "1.0.0".into())
            .unwrap_err();
        assert!(err.to_string().contains("package_name must not be empty"));
    }

    #[test]
    fn test_add_rating_review_too_long() {
        let mut store = RatingStore::new();
        let long_review = "x".repeat(2001);
        let err = store
            .add_rating(
                "a".into(),
                "p".into(),
                4,
                Some(long_review),
                "1.0.0".into(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("exceeds maximum length"));
    }

    #[test]
    fn test_add_rating_review_exactly_max_length() {
        let mut store = RatingStore::new();
        let review = "y".repeat(MAX_REVIEW_LENGTH);
        let r = store
            .add_rating(
                "a".into(),
                "p".into(),
                5,
                Some(review.clone()),
                "1.0.0".into(),
            )
            .unwrap();
        assert_eq!(r.review.unwrap().len(), MAX_REVIEW_LENGTH);
    }

    // -- deduplication --

    #[test]
    fn test_dedup_replaces_old_rating() {
        let mut store = RatingStore::new();
        store
            .add_rating(
                "agent-1".into(),
                "pkg".into(),
                2,
                Some("Meh".into()),
                "1.0.0".into(),
            )
            .unwrap();
        store
            .add_rating(
                "agent-1".into(),
                "pkg".into(),
                5,
                Some("Much better now!".into()),
                "1.1.0".into(),
            )
            .unwrap();
        assert_eq!(store.total_count(), 1);
        let stats = store.get_stats("pkg").unwrap();
        assert_eq!(stats.total_ratings, 1);
        assert!((stats.average_score - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dedup_same_agent_different_packages() {
        let mut store = RatingStore::new();
        store
            .add_rating("agent-1".into(), "pkg-a".into(), 5, None, "1.0.0".into())
            .unwrap();
        store
            .add_rating("agent-1".into(), "pkg-b".into(), 3, None, "1.0.0".into())
            .unwrap();
        assert_eq!(store.total_count(), 2);
    }

    #[test]
    fn test_dedup_different_agents_same_package() {
        let mut store = RatingStore::new();
        store
            .add_rating("agent-1".into(), "pkg".into(), 5, None, "1.0.0".into())
            .unwrap();
        store
            .add_rating("agent-2".into(), "pkg".into(), 3, None, "1.0.0".into())
            .unwrap();
        assert_eq!(store.total_count(), 2);
        let stats = store.get_stats("pkg").unwrap();
        assert_eq!(stats.total_ratings, 2);
    }

    // -- get_stats --

    #[test]
    fn test_stats_basic() {
        let store = make_store_with_ratings();
        let stats = store.get_stats("pkg-a").unwrap();
        assert_eq!(stats.package_name, "pkg-a");
        assert_eq!(stats.total_ratings, 2);
        assert!((stats.average_score - 4.0).abs() < f64::EPSILON);
        assert_eq!(*stats.distribution.get(&5).unwrap_or(&0), 1);
        assert_eq!(*stats.distribution.get(&3).unwrap_or(&0), 1);
        assert!(stats.latest_review.is_some());
    }

    #[test]
    fn test_stats_single_rating() {
        let store = make_store_with_ratings();
        let stats = store.get_stats("pkg-b").unwrap();
        assert_eq!(stats.total_ratings, 1);
        assert!((stats.average_score - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_stats_nonexistent_package() {
        let store = RatingStore::new();
        assert!(store.get_stats("nonexistent").is_none());
    }

    #[test]
    fn test_stats_distribution() {
        let mut store = RatingStore::new();
        for i in 1..=5 {
            store
                .add_rating(
                    format!("agent-{}", i),
                    "pkg".into(),
                    i as u8,
                    None,
                    "1.0.0".into(),
                )
                .unwrap();
        }
        let stats = store.get_stats("pkg").unwrap();
        assert_eq!(stats.total_ratings, 5);
        assert!((stats.average_score - 3.0).abs() < f64::EPSILON);
        for score in 1..=5u8 {
            assert_eq!(*stats.distribution.get(&score).unwrap(), 1);
        }
    }

    #[test]
    fn test_stats_all_same_score() {
        let mut store = RatingStore::new();
        for i in 0..10 {
            store
                .add_rating(
                    format!("agent-{}", i),
                    "pkg".into(),
                    4,
                    None,
                    "1.0.0".into(),
                )
                .unwrap();
        }
        let stats = store.get_stats("pkg").unwrap();
        assert_eq!(stats.total_ratings, 10);
        assert!((stats.average_score - 4.0).abs() < f64::EPSILON);
        assert_eq!(*stats.distribution.get(&4).unwrap(), 10);
    }

    // -- get_ratings (filtering) --

    #[test]
    fn test_filter_no_criteria() {
        let store = make_store_with_ratings();
        let all = store.get_ratings(&RatingFilter::default());
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_filter_by_package() {
        let store = make_store_with_ratings();
        let filter = RatingFilter {
            package_name: Some("pkg-a".into()),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.package_name == "pkg-a"));
    }

    #[test]
    fn test_filter_by_agent() {
        let store = make_store_with_ratings();
        let filter = RatingFilter {
            agent_id: Some("agent-1".into()),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.agent_id == "agent-1"));
    }

    #[test]
    fn test_filter_by_min_score() {
        let store = make_store_with_ratings();
        let filter = RatingFilter {
            min_score: Some(4),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.score >= 4));
    }

    #[test]
    fn test_filter_by_date_range() {
        let store = make_store_with_ratings();
        let now = Utc::now();
        let filter = RatingFilter {
            from: Some(now - Duration::hours(1)),
            until: Some(now + Duration::hours(1)),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_filter_combined() {
        let store = make_store_with_ratings();
        let filter = RatingFilter {
            package_name: Some("pkg-a".into()),
            min_score: Some(4),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 5);
    }

    #[test]
    fn test_filter_no_match() {
        let store = make_store_with_ratings();
        let filter = RatingFilter {
            package_name: Some("nonexistent".into()),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert!(results.is_empty());
    }

    #[test]
    fn test_filter_future_from_excludes_all() {
        let store = make_store_with_ratings();
        let filter = RatingFilter {
            from: Some(Utc::now() + Duration::days(1)),
            ..Default::default()
        };
        let results = store.get_ratings(&filter);
        assert!(results.is_empty());
    }

    // -- top_rated --

    #[test]
    fn test_top_rated_ordering() {
        let store = make_store_with_ratings();
        let top = store.top_rated(None);
        assert_eq!(top.len(), 2);
        // pkg-a has avg 4.0, pkg-b has avg 4.0 — tiebreak by total_ratings desc
        assert!(top[0].average_score >= top[1].average_score);
        if (top[0].average_score - top[1].average_score).abs() < f64::EPSILON {
            assert!(top[0].total_ratings >= top[1].total_ratings);
        }
    }

    #[test]
    fn test_top_rated_min_ratings_threshold() {
        let store = make_store_with_ratings();
        let top = store.top_rated(Some(2));
        // Only pkg-a has 2 ratings
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].package_name, "pkg-a");
    }

    #[test]
    fn test_top_rated_high_threshold_empty() {
        let store = make_store_with_ratings();
        let top = store.top_rated(Some(100));
        assert!(top.is_empty());
    }

    #[test]
    fn test_top_rated_empty_store() {
        let store = RatingStore::new();
        assert!(store.top_rated(None).is_empty());
    }

    #[test]
    fn test_top_rated_score_ordering() {
        let mut store = RatingStore::new();
        store
            .add_rating("a1".into(), "low".into(), 1, None, "1.0.0".into())
            .unwrap();
        store
            .add_rating("a1".into(), "high".into(), 5, None, "1.0.0".into())
            .unwrap();
        store
            .add_rating("a1".into(), "mid".into(), 3, None, "1.0.0".into())
            .unwrap();
        let top = store.top_rated(None);
        assert_eq!(top[0].package_name, "high");
        assert_eq!(top[1].package_name, "mid");
        assert_eq!(top[2].package_name, "low");
    }

    // -- persistence --

    #[test]
    fn test_save_and_load_roundtrip() {
        let store = make_store_with_ratings();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ratings.json");

        store.save(&path).unwrap();
        assert!(path.exists());

        let loaded = RatingStore::load(&path).unwrap();
        assert_eq!(loaded.total_count(), store.total_count());
        assert_eq!(loaded.package_count(), store.package_count());

        let stats_orig = store.get_stats("pkg-a").unwrap();
        let stats_loaded = loaded.get_stats("pkg-a").unwrap();
        assert_eq!(stats_orig.total_ratings, stats_loaded.total_ratings);
        assert!((stats_orig.average_score - stats_loaded.average_score).abs() < f64::EPSILON);
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let store = RatingStore::load(&path).unwrap();
        assert_eq!(store.total_count(), 0);
    }

    #[test]
    fn test_save_empty_store() {
        let store = RatingStore::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        store.save(&path).unwrap();
        let loaded = RatingStore::load(&path).unwrap();
        assert_eq!(loaded.total_count(), 0);
    }

    #[test]
    fn test_load_corrupted_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json!!!").unwrap();
        assert!(RatingStore::load(&path).is_err());
    }

    // -- edge cases --

    #[test]
    fn test_empty_store_counts() {
        let store = RatingStore::new();
        assert_eq!(store.total_count(), 0);
        assert_eq!(store.package_count(), 0);
    }

    #[test]
    fn test_default_trait() {
        let store = RatingStore::default();
        assert_eq!(store.total_count(), 0);
    }

    #[test]
    fn test_rating_serialization() {
        let rating = Rating {
            agent_id: "agent-1".into(),
            package_name: "pkg".into(),
            score: 4,
            review: Some("Good".into()),
            created_at: Utc::now(),
            version_reviewed: "1.0.0".into(),
        };
        let json = serde_json::to_string(&rating).unwrap();
        let parsed: Rating = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, rating.agent_id);
        assert_eq!(parsed.score, rating.score);
        assert_eq!(parsed.review, rating.review);
    }

    #[test]
    fn test_rating_stats_serialization() {
        let mut store = RatingStore::new();
        store
            .add_rating("a".into(), "p".into(), 4, None, "1.0.0".into())
            .unwrap();
        let stats = store.get_stats("p").unwrap();
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: RatingStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.package_name, "p");
        assert_eq!(parsed.total_ratings, 1);
    }

    #[test]
    fn test_filter_serialization() {
        let filter = RatingFilter {
            min_score: Some(3),
            package_name: Some("pkg".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&filter).unwrap();
        let parsed: RatingFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.min_score, Some(3));
        assert_eq!(parsed.package_name.as_deref(), Some("pkg"));
    }

    #[test]
    fn test_get_ratings_sorted_newest_first() {
        let mut store = RatingStore::new();
        // Add ratings in sequence — each gets Utc::now() so they're ordered
        store
            .add_rating("a1".into(), "pkg".into(), 3, None, "1.0.0".into())
            .unwrap();
        store
            .add_rating("a2".into(), "pkg".into(), 5, None, "1.0.0".into())
            .unwrap();
        let results = store.get_ratings(&RatingFilter::default());
        // Most recent (a2) should be first
        assert!(results[0].created_at >= results[1].created_at);
    }

    #[test]
    fn test_multiple_packages_independence() {
        let mut store = RatingStore::new();
        store
            .add_rating("a1".into(), "pkg-x".into(), 1, None, "1.0.0".into())
            .unwrap();
        store
            .add_rating("a1".into(), "pkg-y".into(), 5, None, "1.0.0".into())
            .unwrap();
        let sx = store.get_stats("pkg-x").unwrap();
        let sy = store.get_stats("pkg-y").unwrap();
        assert!((sx.average_score - 1.0).abs() < f64::EPSILON);
        assert!((sy.average_score - 5.0).abs() < f64::EPSILON);
    }
}
