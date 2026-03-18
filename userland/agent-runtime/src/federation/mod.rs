//! Federation — Multi-Node Agent Clusters for AGNOS
//!
//! Implements peer-to-peer federation with coordinator election (simplified Raft),
//! node health monitoring, agent placement scoring, and cluster management.
//! Architecture defined in ADR-016.

pub mod discovery;
pub mod gossip;
pub mod sync;
pub mod types;
pub mod vector_store;

#[cfg(test)]
mod tests;

// Re-export the full public API surface (identical to old federation.rs).
pub use discovery::FederationCluster;
pub use sync::{AgentPlacement, NodeScorer};
pub use types::{
    AgentRequirements, CollectionReplica, FederatedVectorStats, FederationConfig, FederationNode,
    FederationStats, NodeCapabilities, NodeRole, NodeScore, NodeStatus, RemoteSearchResult,
    SchedulingStrategy, ScoreBreakdown, VectorReplicationStrategy, VectorSyncEntry,
    VectorSyncMessage, VoteResponse,
};
pub use vector_store::FederatedVectorStore;
