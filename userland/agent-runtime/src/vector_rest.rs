//! Shared Vector Store REST API
//!
//! Exposes the AGNOS embedded vector store as a queryable REST service,
//! including federated cross-node search. External services can insert,
//! search, and manage vector collections via HTTP.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// REST API Request/Response Types
// ---------------------------------------------------------------------------

/// Request to create a vector collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCollectionRequest {
    /// Collection name.
    pub name: String,
    /// Vector dimension.
    pub dimension: usize,
    /// Distance metric.
    #[serde(default)]
    pub metric: DistanceMetric,
    /// Whether to replicate across federated nodes.
    #[serde(default)]
    pub federated: bool,
    /// Replication factor (for federated collections).
    #[serde(default)]
    pub replication_factor: Option<u32>,
    /// Optional metadata schema description.
    #[serde(default)]
    pub metadata_schema: HashMap<String, String>,
}

/// Distance metric for vector similarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

impl Default for DistanceMetric {
    fn default() -> Self {
        Self::Cosine
    }
}

/// Request to insert vectors into a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertVectorsRequest {
    /// Collection name.
    pub collection: String,
    /// Vectors to insert.
    pub vectors: Vec<VectorInput>,
    /// Whether to sync to federated replicas.
    #[serde(default = "default_true")]
    pub sync_replicas: bool,
}

fn default_true() -> bool {
    true
}

/// A single vector to insert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorInput {
    /// Optional client-provided ID (generated if omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The embedding values.
    pub values: Vec<f64>,
    /// Content text associated with the vector.
    #[serde(default)]
    pub content: String,
    /// Arbitrary metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Response after inserting vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertVectorsResponse {
    /// Number of vectors inserted.
    pub inserted: usize,
    /// IDs of inserted vectors.
    pub ids: Vec<String>,
    /// Number of federated replicas synced.
    pub replicas_synced: usize,
}

/// Request to search vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchVectorsRequest {
    /// Collection name.
    pub collection: String,
    /// Query embedding.
    pub query: Vec<f64>,
    /// Number of results to return.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Metadata filter (key-value equality).
    #[serde(default)]
    pub filter: HashMap<String, serde_json::Value>,
    /// Whether to include content in results.
    #[serde(default = "default_true")]
    pub include_content: bool,
    /// Whether to search federated replicas.
    #[serde(default = "default_true")]
    pub include_federated: bool,
    /// Minimum similarity score threshold.
    #[serde(default)]
    pub min_score: Option<f64>,
}

fn default_top_k() -> usize {
    10
}

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    /// Vector ID.
    pub id: String,
    /// Similarity score.
    pub score: f64,
    /// Content (if requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Node that returned this result (for federated searches).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node: Option<String>,
}

/// Response from a vector search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchVectorsResponse {
    /// Results sorted by descending score.
    pub results: Vec<VectorSearchResult>,
    /// Total results before top_k truncation.
    pub total_candidates: usize,
    /// Number of nodes searched.
    pub nodes_searched: usize,
    /// Search latency in milliseconds.
    pub latency_ms: u64,
}

/// Request to delete vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteVectorsRequest {
    /// Collection name.
    pub collection: String,
    /// Vector IDs to delete.
    pub ids: Vec<String>,
    /// Whether to delete from federated replicas.
    #[serde(default = "default_true")]
    pub sync_replicas: bool,
}

/// Response from vector deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteVectorsResponse {
    /// Number of vectors deleted.
    pub deleted: usize,
}

/// Collection info returned from listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    /// Collection name.
    pub name: String,
    /// Vector dimension.
    pub dimension: usize,
    /// Number of vectors.
    pub vector_count: usize,
    /// Distance metric.
    pub metric: DistanceMetric,
    /// Whether federated.
    pub federated: bool,
    /// Number of federated replicas.
    pub replica_count: usize,
    /// Creation timestamp (Unix seconds).
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Vector REST Service
// ---------------------------------------------------------------------------

/// Service that manages vector collections and exposes them via REST-like operations.
#[derive(Debug, Clone)]
pub struct VectorRestService {
    /// Collection name → collection metadata.
    collections: HashMap<String, CollectionMetadata>,
    /// Local node ID for federated operations.
    local_node_id: String,
    /// Maximum vectors per collection.
    max_vectors_per_collection: usize,
    /// Maximum collections.
    max_collections: usize,
}

/// Internal collection metadata.
#[derive(Debug, Clone)]
struct CollectionMetadata {
    dimension: usize,
    metric: DistanceMetric,
    federated: bool,
    replication_factor: Option<u32>,
    vector_count: usize,
    created_at: u64,
}

impl VectorRestService {
    /// Create a new vector REST service.
    pub fn new(local_node_id: String) -> Self {
        info!(node = %local_node_id, "Vector REST service initialised");
        Self {
            collections: HashMap::new(),
            local_node_id,
            max_vectors_per_collection: 1_000_000,
            max_collections: 100,
        }
    }

    /// Create a new collection.
    pub fn create_collection(
        &mut self,
        req: &CreateCollectionRequest,
    ) -> Result<CollectionInfo, VectorRestError> {
        if req.name.is_empty() {
            return Err(VectorRestError::InvalidRequest(
                "collection name required".to_string(),
            ));
        }
        if req.dimension == 0 {
            return Err(VectorRestError::InvalidRequest(
                "dimension must be > 0".to_string(),
            ));
        }
        if self.collections.contains_key(&req.name) {
            return Err(VectorRestError::CollectionExists(req.name.clone()));
        }
        if self.collections.len() >= self.max_collections {
            return Err(VectorRestError::LimitExceeded(format!(
                "max {} collections",
                self.max_collections
            )));
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = CollectionMetadata {
            dimension: req.dimension,
            metric: req.metric,
            federated: req.federated,
            replication_factor: req.replication_factor,
            vector_count: 0,
            created_at: now,
        };

        self.collections.insert(req.name.clone(), meta);

        info!(
            collection = %req.name,
            dimension = req.dimension,
            federated = req.federated,
            "Created vector collection"
        );

        Ok(CollectionInfo {
            name: req.name.clone(),
            dimension: req.dimension,
            vector_count: 0,
            metric: req.metric,
            federated: req.federated,
            replica_count: if req.federated { 1 } else { 0 },
            created_at: now,
        })
    }

    /// Delete a collection.
    pub fn delete_collection(&mut self, name: &str) -> Result<(), VectorRestError> {
        if self.collections.remove(name).is_none() {
            return Err(VectorRestError::CollectionNotFound(name.to_string()));
        }
        info!(collection = %name, "Deleted vector collection");
        Ok(())
    }

    /// Get collection info.
    pub fn get_collection(&self, name: &str) -> Result<CollectionInfo, VectorRestError> {
        let meta = self
            .collections
            .get(name)
            .ok_or_else(|| VectorRestError::CollectionNotFound(name.to_string()))?;

        Ok(CollectionInfo {
            name: name.to_string(),
            dimension: meta.dimension,
            vector_count: meta.vector_count,
            metric: meta.metric,
            federated: meta.federated,
            replica_count: if meta.federated {
                meta.replication_factor.unwrap_or(1) as usize
            } else {
                0
            },
            created_at: meta.created_at,
        })
    }

    /// List all collections.
    pub fn list_collections(&self) -> Vec<CollectionInfo> {
        let mut infos: Vec<_> = self
            .collections
            .iter()
            .map(|(name, meta)| CollectionInfo {
                name: name.clone(),
                dimension: meta.dimension,
                vector_count: meta.vector_count,
                metric: meta.metric,
                federated: meta.federated,
                replica_count: if meta.federated {
                    meta.replication_factor.unwrap_or(1) as usize
                } else {
                    0
                },
                created_at: meta.created_at,
            })
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Record an insert operation (updates vector count).
    pub fn record_insert(
        &mut self,
        collection: &str,
        count: usize,
    ) -> Result<(), VectorRestError> {
        let meta = self
            .collections
            .get_mut(collection)
            .ok_or_else(|| VectorRestError::CollectionNotFound(collection.to_string()))?;

        if meta.vector_count + count > self.max_vectors_per_collection {
            return Err(VectorRestError::LimitExceeded(format!(
                "collection would exceed {} vectors",
                self.max_vectors_per_collection
            )));
        }

        meta.vector_count += count;
        debug!(collection, count, total = meta.vector_count, "Recorded vector insert");
        Ok(())
    }

    /// Record a delete operation (updates vector count).
    pub fn record_delete(
        &mut self,
        collection: &str,
        count: usize,
    ) -> Result<(), VectorRestError> {
        let meta = self
            .collections
            .get_mut(collection)
            .ok_or_else(|| VectorRestError::CollectionNotFound(collection.to_string()))?;

        meta.vector_count = meta.vector_count.saturating_sub(count);
        debug!(collection, count, total = meta.vector_count, "Recorded vector delete");
        Ok(())
    }

    /// Validate an insert request against collection constraints.
    pub fn validate_insert(
        &self,
        req: &InsertVectorsRequest,
    ) -> Result<(), VectorRestError> {
        let meta = self
            .collections
            .get(&req.collection)
            .ok_or_else(|| VectorRestError::CollectionNotFound(req.collection.clone()))?;

        for (i, v) in req.vectors.iter().enumerate() {
            if v.values.len() != meta.dimension {
                return Err(VectorRestError::DimensionMismatch {
                    expected: meta.dimension,
                    got: v.values.len(),
                    index: i,
                });
            }
        }

        Ok(())
    }

    /// Validate a search request against collection constraints.
    pub fn validate_search(
        &self,
        req: &SearchVectorsRequest,
    ) -> Result<(), VectorRestError> {
        let meta = self
            .collections
            .get(&req.collection)
            .ok_or_else(|| VectorRestError::CollectionNotFound(req.collection.clone()))?;

        if req.query.len() != meta.dimension {
            return Err(VectorRestError::DimensionMismatch {
                expected: meta.dimension,
                got: req.query.len(),
                index: 0,
            });
        }

        Ok(())
    }

    /// Number of collections.
    pub fn collection_count(&self) -> usize {
        self.collections.len()
    }

    /// Total vectors across all collections.
    pub fn total_vectors(&self) -> usize {
        self.collections.values().map(|m| m.vector_count).sum()
    }

    /// Get service stats.
    pub fn stats(&self) -> VectorServiceStats {
        let federated = self.collections.values().filter(|m| m.federated).count();
        VectorServiceStats {
            collections: self.collections.len(),
            total_vectors: self.total_vectors(),
            federated_collections: federated,
            local_node_id: self.local_node_id.clone(),
            max_collections: self.max_collections,
            max_vectors_per_collection: self.max_vectors_per_collection,
        }
    }
}

/// Vector service statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorServiceStats {
    pub collections: usize,
    pub total_vectors: usize,
    pub federated_collections: usize,
    pub local_node_id: String,
    pub max_collections: usize,
    pub max_vectors_per_collection: usize,
}

/// Vector REST API errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorRestError {
    CollectionNotFound(String),
    CollectionExists(String),
    DimensionMismatch {
        expected: usize,
        got: usize,
        index: usize,
    },
    InvalidRequest(String),
    LimitExceeded(String),
}

impl std::fmt::Display for VectorRestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CollectionNotFound(n) => write!(f, "collection not found: {}", n),
            Self::CollectionExists(n) => write!(f, "collection already exists: {}", n),
            Self::DimensionMismatch {
                expected,
                got,
                index,
            } => write!(
                f,
                "dimension mismatch at index {}: expected {}, got {}",
                index, expected, got
            ),
            Self::InvalidRequest(msg) => write!(f, "invalid request: {}", msg),
            Self::LimitExceeded(msg) => write!(f, "limit exceeded: {}", msg),
        }
    }
}

impl std::error::Error for VectorRestError {}

// ---------------------------------------------------------------------------
// REST Endpoint Definitions
// ---------------------------------------------------------------------------

/// Describes the vector store REST API endpoints.
pub fn vector_api_endpoints() -> Vec<VectorEndpoint> {
    vec![
        VectorEndpoint {
            method: "GET".to_string(),
            path: "/v1/vectors/collections".to_string(),
            description: "List all vector collections".to_string(),
            auth_scope: "vectors:read".to_string(),
        },
        VectorEndpoint {
            method: "POST".to_string(),
            path: "/v1/vectors/collections".to_string(),
            description: "Create a new vector collection".to_string(),
            auth_scope: "vectors:write".to_string(),
        },
        VectorEndpoint {
            method: "GET".to_string(),
            path: "/v1/vectors/collections/:name".to_string(),
            description: "Get collection info".to_string(),
            auth_scope: "vectors:read".to_string(),
        },
        VectorEndpoint {
            method: "DELETE".to_string(),
            path: "/v1/vectors/collections/:name".to_string(),
            description: "Delete a collection".to_string(),
            auth_scope: "vectors:write".to_string(),
        },
        VectorEndpoint {
            method: "POST".to_string(),
            path: "/v1/vectors/insert".to_string(),
            description: "Insert vectors into a collection".to_string(),
            auth_scope: "vectors:write".to_string(),
        },
        VectorEndpoint {
            method: "POST".to_string(),
            path: "/v1/vectors/search".to_string(),
            description: "Search vectors by embedding".to_string(),
            auth_scope: "vectors:read".to_string(),
        },
        VectorEndpoint {
            method: "POST".to_string(),
            path: "/v1/vectors/delete".to_string(),
            description: "Delete vectors by ID".to_string(),
            auth_scope: "vectors:write".to_string(),
        },
        VectorEndpoint {
            method: "GET".to_string(),
            path: "/v1/vectors/stats".to_string(),
            description: "Get vector service stats".to_string(),
            auth_scope: "vectors:read".to_string(),
        },
    ]
}

/// A REST API endpoint descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEndpoint {
    pub method: String,
    pub path: String,
    pub description: String,
    pub auth_scope: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> VectorRestService {
        VectorRestService::new("node-1".to_string())
    }

    #[test]
    fn test_create_collection() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "embeddings".to_string(),
            dimension: 384,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        let info = svc.create_collection(&req).unwrap();
        assert_eq!(info.name, "embeddings");
        assert_eq!(info.dimension, 384);
        assert_eq!(info.vector_count, 0);
        assert_eq!(svc.collection_count(), 1);
    }

    #[test]
    fn test_create_collection_duplicate() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "test".to_string(),
            dimension: 128,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        assert!(matches!(
            svc.create_collection(&req).unwrap_err(),
            VectorRestError::CollectionExists(_)
        ));
    }

    #[test]
    fn test_create_collection_invalid_dimension() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "bad".to_string(),
            dimension: 0,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        assert!(matches!(
            svc.create_collection(&req).unwrap_err(),
            VectorRestError::InvalidRequest(_)
        ));
    }

    #[test]
    fn test_create_collection_empty_name() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: String::new(),
            dimension: 128,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        assert!(matches!(
            svc.create_collection(&req).unwrap_err(),
            VectorRestError::InvalidRequest(_)
        ));
    }

    #[test]
    fn test_delete_collection() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "temp".to_string(),
            dimension: 64,
            metric: DistanceMetric::Euclidean,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        svc.delete_collection("temp").unwrap();
        assert_eq!(svc.collection_count(), 0);
    }

    #[test]
    fn test_delete_collection_not_found() {
        let mut svc = test_service();
        assert!(matches!(
            svc.delete_collection("nonexistent").unwrap_err(),
            VectorRestError::CollectionNotFound(_)
        ));
    }

    #[test]
    fn test_get_collection() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "docs".to_string(),
            dimension: 768,
            metric: DistanceMetric::DotProduct,
            federated: true,
            replication_factor: Some(3),
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        let info = svc.get_collection("docs").unwrap();
        assert_eq!(info.dimension, 768);
        assert_eq!(info.metric, DistanceMetric::DotProduct);
        assert!(info.federated);
        assert_eq!(info.replica_count, 3);
    }

    #[test]
    fn test_list_collections() {
        let mut svc = test_service();
        for name in ["beta", "alpha", "gamma"] {
            let req = CreateCollectionRequest {
                name: name.to_string(),
                dimension: 128,
                metric: DistanceMetric::Cosine,
                federated: false,
                replication_factor: None,
                metadata_schema: HashMap::new(),
            };
            svc.create_collection(&req).unwrap();
        }
        let list = svc.list_collections();
        assert_eq!(list.len(), 3);
        // Sorted alphabetically
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "beta");
        assert_eq!(list[2].name, "gamma");
    }

    #[test]
    fn test_record_insert() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "data".to_string(),
            dimension: 128,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        svc.record_insert("data", 50).unwrap();
        assert_eq!(svc.get_collection("data").unwrap().vector_count, 50);
        svc.record_insert("data", 25).unwrap();
        assert_eq!(svc.get_collection("data").unwrap().vector_count, 75);
    }

    #[test]
    fn test_record_delete() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "data".to_string(),
            dimension: 128,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        svc.record_insert("data", 100).unwrap();
        svc.record_delete("data", 30).unwrap();
        assert_eq!(svc.get_collection("data").unwrap().vector_count, 70);
    }

    #[test]
    fn test_record_delete_underflow() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "data".to_string(),
            dimension: 128,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        svc.record_insert("data", 5).unwrap();
        svc.record_delete("data", 100).unwrap(); // saturates at 0
        assert_eq!(svc.get_collection("data").unwrap().vector_count, 0);
    }

    #[test]
    fn test_validate_insert_correct_dimension() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "emb".to_string(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();

        let insert = InsertVectorsRequest {
            collection: "emb".to_string(),
            vectors: vec![VectorInput {
                id: None,
                values: vec![1.0, 2.0, 3.0],
                content: "test".to_string(),
                metadata: HashMap::new(),
            }],
            sync_replicas: false,
        };
        assert!(svc.validate_insert(&insert).is_ok());
    }

    #[test]
    fn test_validate_insert_wrong_dimension() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "emb".to_string(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();

        let insert = InsertVectorsRequest {
            collection: "emb".to_string(),
            vectors: vec![VectorInput {
                id: None,
                values: vec![1.0, 2.0],
                content: "bad".to_string(),
                metadata: HashMap::new(),
            }],
            sync_replicas: false,
        };
        assert!(matches!(
            svc.validate_insert(&insert).unwrap_err(),
            VectorRestError::DimensionMismatch { expected: 3, got: 2, .. }
        ));
    }

    #[test]
    fn test_validate_search() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "emb".to_string(),
            dimension: 4,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();

        let search = SearchVectorsRequest {
            collection: "emb".to_string(),
            query: vec![1.0, 2.0, 3.0, 4.0],
            top_k: 5,
            filter: HashMap::new(),
            include_content: true,
            include_federated: false,
            min_score: None,
        };
        assert!(svc.validate_search(&search).is_ok());

        let bad_search = SearchVectorsRequest {
            query: vec![1.0, 2.0],
            ..search.clone()
        };
        assert!(matches!(
            svc.validate_search(&bad_search).unwrap_err(),
            VectorRestError::DimensionMismatch { .. }
        ));
    }

    #[test]
    fn test_stats() {
        let mut svc = test_service();
        for (name, federated) in [("local", false), ("shared", true)] {
            let req = CreateCollectionRequest {
                name: name.to_string(),
                dimension: 128,
                metric: DistanceMetric::Cosine,
                federated,
                replication_factor: None,
                metadata_schema: HashMap::new(),
            };
            svc.create_collection(&req).unwrap();
        }
        svc.record_insert("local", 100).unwrap();
        svc.record_insert("shared", 50).unwrap();

        let stats = svc.stats();
        assert_eq!(stats.collections, 2);
        assert_eq!(stats.total_vectors, 150);
        assert_eq!(stats.federated_collections, 1);
        assert_eq!(stats.local_node_id, "node-1");
    }

    #[test]
    fn test_total_vectors() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "a".to_string(),
            dimension: 64,
            metric: DistanceMetric::Cosine,
            federated: false,
            replication_factor: None,
            metadata_schema: HashMap::new(),
        };
        svc.create_collection(&req).unwrap();
        assert_eq!(svc.total_vectors(), 0);
        svc.record_insert("a", 42).unwrap();
        assert_eq!(svc.total_vectors(), 42);
    }

    #[test]
    fn test_vector_api_endpoints() {
        let endpoints = vector_api_endpoints();
        assert_eq!(endpoints.len(), 8);
        assert!(endpoints.iter().any(|e| e.path == "/v1/vectors/search"));
        assert!(endpoints.iter().any(|e| e.path == "/v1/vectors/stats"));
        // All read endpoints require vectors:read
        let reads: Vec<_> = endpoints.iter().filter(|e| e.method == "GET").collect();
        assert!(reads.iter().all(|e| e.auth_scope == "vectors:read"));
    }

    #[test]
    fn test_distance_metric_default() {
        assert_eq!(DistanceMetric::default(), DistanceMetric::Cosine);
    }

    #[test]
    fn test_distance_metric_serialization() {
        let json = serde_json::to_string(&DistanceMetric::Euclidean).unwrap();
        assert_eq!(json, "\"Euclidean\"");
        let parsed: DistanceMetric = serde_json::from_str("\"DotProduct\"").unwrap();
        assert_eq!(parsed, DistanceMetric::DotProduct);
    }

    #[test]
    fn test_search_request_defaults() {
        let json = r#"{"collection":"test","query":[1.0,2.0]}"#;
        let req: SearchVectorsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.top_k, 10);
        assert!(req.include_content);
        assert!(req.include_federated);
        assert!(req.min_score.is_none());
    }

    #[test]
    fn test_insert_response_serialization() {
        let resp = InsertVectorsResponse {
            inserted: 5,
            ids: vec!["a".to_string(), "b".to_string()],
            replicas_synced: 2,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: InsertVectorsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.inserted, 5);
        assert_eq!(parsed.replicas_synced, 2);
    }

    #[test]
    fn test_vector_rest_error_display() {
        assert_eq!(
            VectorRestError::CollectionNotFound("x".to_string()).to_string(),
            "collection not found: x"
        );
        assert_eq!(
            VectorRestError::CollectionExists("x".to_string()).to_string(),
            "collection already exists: x"
        );
        let err = VectorRestError::DimensionMismatch {
            expected: 384,
            got: 128,
            index: 2,
        };
        assert!(err.to_string().contains("384"));
        assert!(err.to_string().contains("128"));
    }

    #[test]
    fn test_collection_info_serialization() {
        let info = CollectionInfo {
            name: "docs".to_string(),
            dimension: 768,
            vector_count: 1000,
            metric: DistanceMetric::Cosine,
            federated: true,
            replica_count: 3,
            created_at: 1710000000,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: CollectionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "docs");
        assert_eq!(parsed.replica_count, 3);
    }

    #[test]
    fn test_search_result_with_source_node() {
        let result = VectorSearchResult {
            id: "v-1".to_string(),
            score: 0.95,
            content: Some("hello world".to_string()),
            metadata: HashMap::from([("lang".to_string(), serde_json::json!("en"))]),
            source_node: Some("node-2".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("node-2"));
        let parsed: VectorSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source_node, Some("node-2".to_string()));
    }

    #[test]
    fn test_federated_collection_info() {
        let mut svc = test_service();
        let req = CreateCollectionRequest {
            name: "shared".to_string(),
            dimension: 256,
            metric: DistanceMetric::Cosine,
            federated: true,
            replication_factor: Some(5),
            metadata_schema: HashMap::new(),
        };
        let info = svc.create_collection(&req).unwrap();
        assert!(info.federated);
        // Initial replica count is 1 (local), but get_collection uses replication_factor
        let info2 = svc.get_collection("shared").unwrap();
        assert_eq!(info2.replica_count, 5);
    }
}
