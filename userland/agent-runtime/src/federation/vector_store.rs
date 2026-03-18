//! Federated vector store — collection placement, sync messages, and result merging.

use std::collections::HashMap;
use std::net::SocketAddr;

use chrono::Utc;
use tracing::{debug, info};

use super::discovery::FederationCluster;
use super::types::{
    CollectionReplica, FederatedVectorStats, FederationNode, RemoteSearchResult,
    VectorReplicationStrategy, VectorSyncEntry, VectorSyncMessage,
};

// ---------------------------------------------------------------------------
// FederatedVectorStore
// ---------------------------------------------------------------------------

/// Manages vector store federation across cluster nodes.
///
/// Tracks collection placement, generates sync messages, and merges
/// search results from multiple nodes.
#[derive(Debug, Clone)]
pub struct FederatedVectorStore {
    /// Local node ID.
    local_node_id: String,
    /// Collection name → list of replicas (nodes hosting the collection).
    collection_map: HashMap<String, Vec<CollectionReplica>>,
    /// Replication strategy.
    replication_strategy: VectorReplicationStrategy,
    /// Maximum results to merge from remote searches.
    max_remote_results: usize,
}

impl FederatedVectorStore {
    /// Create a new federated vector store for a local node.
    pub fn new(local_node_id: String, strategy: VectorReplicationStrategy) -> Self {
        info!(
            node = %local_node_id,
            strategy = ?strategy,
            "Federated vector store initialised"
        );
        Self {
            local_node_id,
            collection_map: HashMap::new(),
            replication_strategy: strategy,
            max_remote_results: 100,
        }
    }

    /// Register a collection as existing on a given node.
    pub fn register_replica(
        &mut self,
        collection: &str,
        node_id: &str,
        address: SocketAddr,
        vector_count: usize,
    ) {
        let replicas = self
            .collection_map
            .entry(collection.to_string())
            .or_default();

        // Update existing or add new.
        if let Some(existing) = replicas.iter_mut().find(|r| r.node_id == node_id) {
            existing.vector_count = vector_count;
            existing.last_synced = Utc::now();
            debug!(
                collection,
                node_id, vector_count, "Updated collection replica"
            );
        } else {
            replicas.push(CollectionReplica {
                node_id: node_id.to_string(),
                address,
                vector_count,
                last_synced: Utc::now(),
            });
            debug!(
                collection,
                node_id, vector_count, "Registered new collection replica"
            );
        }
    }

    /// Remove a node from all collection replicas (e.g., when node goes dead).
    pub fn remove_node(&mut self, node_id: &str) {
        for replicas in self.collection_map.values_mut() {
            replicas.retain(|r| r.node_id != node_id);
        }
        info!(node_id, "Removed node from all collection replicas");
    }

    /// Get nodes that hold a given collection (excluding local node).
    pub fn remote_replicas(&self, collection: &str) -> Vec<&CollectionReplica> {
        self.collection_map
            .get(collection)
            .map(|replicas| {
                replicas
                    .iter()
                    .filter(|r| r.node_id != self.local_node_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all replicas (including local) for a collection.
    pub fn all_replicas(&self, collection: &str) -> Vec<&CollectionReplica> {
        self.collection_map
            .get(collection)
            .map(|r| r.iter().collect())
            .unwrap_or_default()
    }

    /// List all known collections across the federation.
    pub fn collections(&self) -> Vec<String> {
        let mut names: Vec<_> = self.collection_map.keys().cloned().collect();
        names.sort();
        names
    }

    /// Total number of collections tracked.
    pub fn collection_count(&self) -> usize {
        self.collection_map.len()
    }

    /// Generate insert sync messages for all remote replicas of a collection.
    pub fn insert_sync_messages(
        &self,
        collection: &str,
        entries: Vec<VectorSyncEntry>,
    ) -> Vec<(SocketAddr, VectorSyncMessage)> {
        self.remote_replicas(collection)
            .iter()
            .map(|replica| {
                (
                    replica.address,
                    VectorSyncMessage::Insert {
                        collection: collection.to_string(),
                        entries: entries.clone(),
                    },
                )
            })
            .collect()
    }

    /// Generate search messages for all remote replicas of a collection.
    pub fn search_sync_messages(
        &self,
        collection: &str,
        query: &[f64],
        top_k: usize,
    ) -> Vec<(SocketAddr, VectorSyncMessage)> {
        self.remote_replicas(collection)
            .iter()
            .map(|replica| {
                (
                    replica.address,
                    VectorSyncMessage::Search {
                        collection: collection.to_string(),
                        query: query.to_vec(),
                        top_k,
                    },
                )
            })
            .collect()
    }

    /// Generate a collection announcement message for broadcasting to peers.
    pub fn announce_message(
        &self,
        collection: &str,
        dimension: Option<usize>,
        vector_count: usize,
    ) -> VectorSyncMessage {
        VectorSyncMessage::AnnounceCollection {
            collection: collection.to_string(),
            dimension,
            vector_count,
        }
    }

    /// Merge local results with results from remote nodes, re-ranking by score.
    ///
    /// Returns the top `top_k` results across all sources, sorted by descending score.
    pub fn merge_results(
        &self,
        local_results: Vec<RemoteSearchResult>,
        remote_results: Vec<Vec<RemoteSearchResult>>,
        top_k: usize,
    ) -> Vec<RemoteSearchResult> {
        let mut all: Vec<RemoteSearchResult> = local_results;
        for batch in remote_results {
            all.extend(batch.into_iter().take(self.max_remote_results));
        }

        // Sort by score descending.
        all.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate by vector ID (keep highest score).
        let mut seen = std::collections::HashSet::new();
        all.retain(|r| seen.insert(r.id.clone()));

        all.truncate(top_k);
        all
    }

    /// Determine which nodes should host a new collection based on the replication strategy.
    pub fn select_replica_nodes<'a>(
        &self,
        cluster: &'a FederationCluster,
    ) -> Vec<&'a FederationNode> {
        let live_nodes = cluster.get_live_nodes();

        match self.replication_strategy {
            VectorReplicationStrategy::Full => live_nodes,
            VectorReplicationStrategy::Partial { replication_factor } => live_nodes
                .into_iter()
                .take(replication_factor as usize)
                .collect(),
            VectorReplicationStrategy::Sharded => {
                // For sharding, only the coordinator assigns shards.
                // Return just the local node — the coordinator will assign others.
                live_nodes
                    .into_iter()
                    .filter(|n| n.node_id == self.local_node_id)
                    .collect()
            }
        }
    }

    /// Get federation stats for the vector store.
    pub fn stats(&self) -> FederatedVectorStats {
        let total_vectors: usize = self
            .collection_map
            .values()
            .flat_map(|replicas| replicas.iter().map(|r| r.vector_count))
            .sum();

        let total_replicas: usize = self.collection_map.values().map(|r| r.len()).sum();

        let unique_nodes: usize = {
            let mut nodes = std::collections::HashSet::new();
            for replicas in self.collection_map.values() {
                for r in replicas {
                    nodes.insert(r.node_id.clone());
                }
            }
            nodes.len()
        };

        FederatedVectorStats {
            collection_count: self.collection_map.len(),
            total_replicas,
            total_vectors_across_replicas: total_vectors,
            nodes_with_vectors: unique_nodes,
            replication_strategy: self.replication_strategy,
        }
    }

    /// Get the replication strategy.
    pub fn replication_strategy(&self) -> VectorReplicationStrategy {
        self.replication_strategy
    }

    /// Get the local node ID.
    pub fn local_node_id(&self) -> &str {
        &self.local_node_id
    }
}
