//! Agent Migration & Checkpointing (Phase 7C — ADR-016)
//!
//! Provides checkpoint creation, validation, compression, and a full migration
//! lifecycle with an enforced state-machine. Agents can be migrated between
//! nodes via warm, cold, or live strategies.
//!
//! Key types:
//! - [`Checkpoint`] — serialisable snapshot of an agent's runtime state.
//! - [`MigrationManager`] — creates, validates, and transforms checkpoints.
//! - [`MigrationTracker`] — tracks in-progress and historical migrations.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

use agnos_common::AgentId;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum checkpoint size (256 MB).
const MAX_CHECKPOINT_SIZE: u64 = 256 * 1024 * 1024;

/// Compression ratio estimate (60 % reduction).
const COMPRESSION_RATIO: f64 = 0.40;

// ---------------------------------------------------------------------------
// Checkpoint types
// ---------------------------------------------------------------------------

/// Type of checkpoint captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointType {
    /// Complete snapshot of all agent state.
    Full,
    /// Only the delta since the last full checkpoint.
    Incremental,
}

impl fmt::Display for CheckpointType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => write!(f, "full"),
            Self::Incremental => write!(f, "incremental"),
        }
    }
}

/// An in-flight IPC message drained before migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    /// Sender agent identifier.
    pub sender: String,
    /// Recipient agent identifier.
    pub recipient: String,
    /// Serialised message payload.
    pub payload: String,
    /// When the message was originally sent.
    pub timestamp: DateTime<Utc>,
}

/// Serialisable snapshot of an agent's runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Agent that owns this checkpoint.
    pub agent_id: AgentId,
    /// Unique checkpoint identifier.
    pub checkpoint_id: Uuid,
    /// When the checkpoint was created.
    pub created_at: DateTime<Utc>,
    /// Node that created the checkpoint.
    pub source_node: String,
    /// Full or incremental.
    pub checkpoint_type: CheckpointType,

    // --- state components ---
    /// Key-value memory dump.
    pub memory_snapshot: HashMap<String, serde_json::Value>,
    /// Names of vector indices to transfer.
    pub vector_indices: Vec<String>,
    /// Drained in-flight IPC messages.
    pub ipc_queue: Vec<PendingMessage>,
    /// Sandbox configuration (recreated on destination).
    pub sandbox_config: serde_json::Value,

    // --- metadata ---
    /// Total estimated size in bytes.
    pub total_size_bytes: u64,
    /// Whether the checkpoint payload is compressed.
    pub compressed: bool,
}

// ---------------------------------------------------------------------------
// Migration plan / type / state
// ---------------------------------------------------------------------------

/// Strategy used for the migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationType {
    /// Quiesce -> checkpoint -> transfer -> restore.  Target < 500 ms.
    Warm,
    /// Stop -> full image -> transfer -> start.  Target < 5 s.
    Cold,
    /// Iterative copy while running (stretch goal).
    Live,
}

impl fmt::Display for MigrationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Warm => write!(f, "warm"),
            Self::Cold => write!(f, "cold"),
            Self::Live => write!(f, "live"),
        }
    }
}

/// A plan describing a pending migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Node the agent is currently running on.
    pub source_node: String,
    /// Destination node.
    pub destination_node: String,
    /// Agent being migrated.
    pub agent_id: AgentId,
    /// Checkpoint to transfer.
    pub checkpoint_id: Uuid,
    /// Estimated downtime for the agent.
    pub estimated_downtime: Duration,
    /// Migration strategy.
    pub migration_type: MigrationType,
}

/// State machine for a single migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationState {
    Pending,
    Quiescing,
    Checkpointing,
    Transferring,
    Restoring,
    Verifying,
    Complete,
    Failed(String),
}

impl MigrationState {
    /// Returns `true` if transitioning from `self` to `to` is valid.
    pub fn valid_transition(&self, to: &MigrationState) -> bool {
        if self == to {
            return true;
        }
        match self {
            MigrationState::Pending => {
                matches!(to, MigrationState::Quiescing | MigrationState::Failed(_))
            }
            MigrationState::Quiescing => {
                matches!(
                    to,
                    MigrationState::Checkpointing | MigrationState::Failed(_)
                )
            }
            MigrationState::Checkpointing => {
                matches!(
                    to,
                    MigrationState::Transferring | MigrationState::Failed(_)
                )
            }
            MigrationState::Transferring => {
                matches!(
                    to,
                    MigrationState::Restoring | MigrationState::Failed(_)
                )
            }
            MigrationState::Restoring => {
                matches!(
                    to,
                    MigrationState::Verifying | MigrationState::Failed(_)
                )
            }
            MigrationState::Verifying => {
                matches!(
                    to,
                    MigrationState::Complete | MigrationState::Failed(_)
                )
            }
            MigrationState::Complete => false,
            MigrationState::Failed(_) => {
                // Allow retry from failed back to pending.
                matches!(to, MigrationState::Pending)
            }
        }
    }

    /// Returns `true` if the migration is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, MigrationState::Complete | MigrationState::Failed(_))
    }
}

impl fmt::Display for MigrationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Quiescing => write!(f, "quiescing"),
            Self::Checkpointing => write!(f, "checkpointing"),
            Self::Transferring => write!(f, "transferring"),
            Self::Restoring => write!(f, "restoring"),
            Self::Verifying => write!(f, "verifying"),
            Self::Complete => write!(f, "complete"),
            Self::Failed(reason) => write!(f, "failed: {}", reason),
        }
    }
}

// ---------------------------------------------------------------------------
// Migration record
// ---------------------------------------------------------------------------

/// A timestamped state-machine entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub state: MigrationState,
    pub timestamp: DateTime<Utc>,
}

/// Full record of a migration (in-progress or historical).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    /// Unique migration identifier.
    pub migration_id: String,
    /// The plan that describes the migration.
    pub plan: MigrationPlan,
    /// Current state of the migration.
    pub current_state: MigrationState,
    /// When the migration was initiated.
    pub started_at: DateTime<Utc>,
    /// When the migration reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,
    /// Ordered history of state transitions.
    pub state_history: Vec<StateTransition>,
}

// ---------------------------------------------------------------------------
// MigrationManager
// ---------------------------------------------------------------------------

/// Creates, validates, and transforms checkpoints.
pub struct MigrationManager {
    /// Node identity for tagging checkpoints.
    node_name: String,
}

impl MigrationManager {
    /// Create a new `MigrationManager` for the given node.
    pub fn new(node_name: String) -> Self {
        Self { node_name }
    }

    /// Create a full checkpoint of an agent's state.
    ///
    /// In a real system this would drain the agent's memory store, IPC queue,
    /// and sandbox configuration.  For now we build a skeleton checkpoint that
    /// callers populate.
    pub fn create_checkpoint(&self, agent_id: AgentId) -> Result<Checkpoint> {
        let checkpoint_id = Uuid::new_v4();
        info!(
            %agent_id,
            %checkpoint_id,
            node = %self.node_name,
            "creating checkpoint"
        );

        let checkpoint = Checkpoint {
            agent_id,
            checkpoint_id,
            created_at: Utc::now(),
            source_node: self.node_name.clone(),
            checkpoint_type: CheckpointType::Full,
            memory_snapshot: HashMap::new(),
            vector_indices: Vec::new(),
            ipc_queue: Vec::new(),
            sandbox_config: serde_json::json!({}),
            total_size_bytes: 0,
            compressed: false,
        };

        debug!(%checkpoint_id, "checkpoint created (empty skeleton)");
        Ok(checkpoint)
    }

    /// Create a checkpoint pre-populated with the supplied state components.
    pub fn create_checkpoint_with_state(
        &self,
        agent_id: AgentId,
        memory: HashMap<String, serde_json::Value>,
        vector_indices: Vec<String>,
        ipc_queue: Vec<PendingMessage>,
        sandbox_config: serde_json::Value,
    ) -> Result<Checkpoint> {
        let checkpoint_id = Uuid::new_v4();
        info!(
            %agent_id,
            %checkpoint_id,
            memory_keys = memory.len(),
            ipc_pending = ipc_queue.len(),
            "creating checkpoint with state"
        );

        // Rough size estimate: serialise to JSON and measure bytes.
        let size_estimate = {
            let mem_size: u64 = memory
                .values()
                .map(|v| v.to_string().len() as u64)
                .sum();
            let ipc_size: u64 = ipc_queue.iter().map(|m| m.payload.len() as u64).sum();
            let sandbox_size = sandbox_config.to_string().len() as u64;
            mem_size + ipc_size + sandbox_size
        };

        let checkpoint = Checkpoint {
            agent_id,
            checkpoint_id,
            created_at: Utc::now(),
            source_node: self.node_name.clone(),
            checkpoint_type: CheckpointType::Full,
            memory_snapshot: memory,
            vector_indices,
            ipc_queue,
            sandbox_config,
            total_size_bytes: size_estimate,
            compressed: false,
        };

        debug!(%checkpoint_id, size = size_estimate, "checkpoint created");
        Ok(checkpoint)
    }

    /// Validate a checkpoint for basic integrity.
    ///
    /// Returns a list of validation warnings.  An empty list means the
    /// checkpoint is valid.  Hard failures return an `Err`.
    pub fn validate_checkpoint(&self, checkpoint: &Checkpoint) -> Result<Vec<String>> {
        let mut warnings: Vec<String> = Vec::new();

        // Agent ID must not be nil.
        if checkpoint.agent_id.0.is_nil() {
            bail!("checkpoint has nil agent_id");
        }

        // Must contain some state.
        if checkpoint.memory_snapshot.is_empty() && checkpoint.ipc_queue.is_empty() {
            warnings.push("checkpoint contains no memory or IPC data".into());
        }

        // Size limit.
        if checkpoint.total_size_bytes > MAX_CHECKPOINT_SIZE {
            bail!(
                "checkpoint size {} exceeds maximum {}",
                checkpoint.total_size_bytes,
                MAX_CHECKPOINT_SIZE
            );
        }

        if checkpoint.source_node.is_empty() {
            warnings.push("checkpoint has empty source_node".into());
        }

        debug!(
            checkpoint_id = %checkpoint.checkpoint_id,
            warnings = warnings.len(),
            "checkpoint validated"
        );
        Ok(warnings)
    }

    /// Estimate transfer time for a checkpoint at the given bandwidth.
    pub fn estimate_transfer_time(
        &self,
        checkpoint: &Checkpoint,
        bandwidth_mbps: f64,
    ) -> Duration {
        if bandwidth_mbps <= 0.0 {
            return Duration::from_secs(u64::MAX);
        }
        let bytes = checkpoint.total_size_bytes as f64;
        let bandwidth_bytes_per_sec = bandwidth_mbps * 125_000.0; // Mbps → B/s
        let secs = bytes / bandwidth_bytes_per_sec;
        Duration::from_secs_f64(secs)
    }

    /// Mark a checkpoint as compressed and reduce its size estimate.
    pub fn compress_checkpoint(&self, mut checkpoint: Checkpoint) -> Result<Checkpoint> {
        if checkpoint.compressed {
            bail!("checkpoint is already compressed");
        }
        let original = checkpoint.total_size_bytes;
        checkpoint.total_size_bytes =
            (checkpoint.total_size_bytes as f64 * COMPRESSION_RATIO) as u64;
        checkpoint.compressed = true;
        debug!(
            checkpoint_id = %checkpoint.checkpoint_id,
            original_bytes = original,
            compressed_bytes = checkpoint.total_size_bytes,
            "checkpoint compressed"
        );
        Ok(checkpoint)
    }

    /// Reverse a previous compression (restores original size estimate).
    pub fn decompress_checkpoint(&self, mut checkpoint: Checkpoint) -> Result<Checkpoint> {
        if !checkpoint.compressed {
            bail!("checkpoint is not compressed");
        }
        checkpoint.total_size_bytes =
            (checkpoint.total_size_bytes as f64 / COMPRESSION_RATIO) as u64;
        checkpoint.compressed = false;
        debug!(
            checkpoint_id = %checkpoint.checkpoint_id,
            decompressed_bytes = checkpoint.total_size_bytes,
            "checkpoint decompressed"
        );
        Ok(checkpoint)
    }

    /// Build a migration plan for moving an agent between nodes.
    pub fn create_plan(
        &self,
        agent_id: AgentId,
        checkpoint: &Checkpoint,
        destination_node: String,
        migration_type: MigrationType,
    ) -> MigrationPlan {
        let estimated_downtime = match &migration_type {
            MigrationType::Warm => Duration::from_millis(500),
            MigrationType::Cold => Duration::from_secs(5),
            MigrationType::Live => Duration::from_millis(50),
        };

        info!(
            %agent_id,
            destination = %destination_node,
            kind = %migration_type,
            "migration plan created"
        );

        MigrationPlan {
            source_node: self.node_name.clone(),
            destination_node,
            agent_id,
            checkpoint_id: checkpoint.checkpoint_id,
            estimated_downtime,
            migration_type,
        }
    }
}

// ---------------------------------------------------------------------------
// MigrationTracker
// ---------------------------------------------------------------------------

/// Tracks in-progress and historical migrations.
pub struct MigrationTracker {
    /// Active and completed migrations keyed by migration_id.
    records: Arc<RwLock<HashMap<String, MigrationRecord>>>,
}

impl MigrationTracker {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new migration from the given plan.  Returns the migration ID.
    pub async fn start_migration(&self, plan: MigrationPlan) -> Result<String> {
        let migration_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let record = MigrationRecord {
            migration_id: migration_id.clone(),
            plan,
            current_state: MigrationState::Pending,
            started_at: now,
            completed_at: None,
            state_history: vec![StateTransition {
                state: MigrationState::Pending,
                timestamp: now,
            }],
        };

        info!(migration_id = %migration_id, "migration started");
        self.records.write().await.insert(migration_id.clone(), record);
        Ok(migration_id)
    }

    /// Advance a migration to a new state, enforcing valid transitions.
    pub async fn advance_state(
        &self,
        migration_id: &str,
        new_state: MigrationState,
    ) -> Result<()> {
        let mut records = self.records.write().await;
        let record = records
            .get_mut(migration_id)
            .context("migration not found")?;

        if !record.current_state.valid_transition(&new_state) {
            bail!(
                "invalid transition from {} to {} for migration {}",
                record.current_state,
                new_state,
                migration_id
            );
        }

        debug!(
            migration_id = %migration_id,
            from = %record.current_state,
            to = %new_state,
            "advancing migration state"
        );

        record.current_state = new_state.clone();
        record.state_history.push(StateTransition {
            state: new_state.clone(),
            timestamp: Utc::now(),
        });

        if new_state.is_terminal() {
            record.completed_at = Some(Utc::now());
        }

        Ok(())
    }

    /// Get a snapshot of a migration record.
    pub async fn get_migration(&self, migration_id: &str) -> Option<MigrationRecord> {
        self.records.read().await.get(migration_id).cloned()
    }

    /// List all currently active (non-terminal) migrations.
    pub async fn active_migrations(&self) -> Vec<MigrationRecord> {
        self.records
            .read()
            .await
            .values()
            .filter(|r| !r.current_state.is_terminal())
            .cloned()
            .collect()
    }

    /// Return migration history for a specific agent.
    pub async fn migration_history(&self, agent_id: AgentId) -> Vec<MigrationRecord> {
        self.records
            .read()
            .await
            .values()
            .filter(|r| r.plan.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// Total number of tracked migrations (active + historical).
    pub async fn total_count(&self) -> usize {
        self.records.read().await.len()
    }
}

impl Default for MigrationTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn test_agent_id() -> AgentId {
        AgentId(Uuid::new_v4())
    }

    fn nil_agent_id() -> AgentId {
        AgentId(Uuid::nil())
    }

    fn make_manager() -> MigrationManager {
        MigrationManager::new("node-alpha".into())
    }

    fn sample_memory() -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("key1".into(), serde_json::json!("value1"));
        m.insert("key2".into(), serde_json::json!(42));
        m
    }

    fn sample_ipc_queue() -> Vec<PendingMessage> {
        vec![PendingMessage {
            sender: "agent-a".into(),
            recipient: "agent-b".into(),
            payload: "hello world".into(),
            timestamp: Utc::now(),
        }]
    }

    fn populated_checkpoint(mgr: &MigrationManager) -> Checkpoint {
        mgr.create_checkpoint_with_state(
            test_agent_id(),
            sample_memory(),
            vec!["idx-embed".into()],
            sample_ipc_queue(),
            serde_json::json!({"seccomp": "basic"}),
        )
        .unwrap()
    }

    // -----------------------------------------------------------------------
    // Checkpoint creation
    // -----------------------------------------------------------------------

    #[test]
    fn create_empty_checkpoint() {
        let mgr = make_manager();
        let cp = mgr.create_checkpoint(test_agent_id()).unwrap();
        assert!(!cp.checkpoint_id.is_nil());
        assert_eq!(cp.source_node, "node-alpha");
        assert_eq!(cp.checkpoint_type, CheckpointType::Full);
        assert!(cp.memory_snapshot.is_empty());
        assert!(!cp.compressed);
    }

    #[test]
    fn create_checkpoint_with_state_populates_fields() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        assert_eq!(cp.memory_snapshot.len(), 2);
        assert_eq!(cp.ipc_queue.len(), 1);
        assert_eq!(cp.vector_indices.len(), 1);
        assert!(cp.total_size_bytes > 0);
    }

    #[test]
    fn checkpoint_size_estimate_accounts_for_all_components() {
        let mgr = make_manager();
        let cp = mgr
            .create_checkpoint_with_state(
                test_agent_id(),
                sample_memory(),
                vec![],
                sample_ipc_queue(),
                serde_json::json!({"net": "isolated"}),
            )
            .unwrap();
        // Size should include memory values + IPC payloads + sandbox config.
        assert!(cp.total_size_bytes > 0);
    }

    #[test]
    fn checkpoint_id_is_unique_across_calls() {
        let mgr = make_manager();
        let a = mgr.create_checkpoint(test_agent_id()).unwrap();
        let b = mgr.create_checkpoint(test_agent_id()).unwrap();
        assert_ne!(a.checkpoint_id, b.checkpoint_id);
    }

    // -----------------------------------------------------------------------
    // Checkpoint validation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_valid_checkpoint_no_warnings() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let warnings = mgr.validate_checkpoint(&cp).unwrap();
        assert!(warnings.is_empty(), "expected no warnings: {:?}", warnings);
    }

    #[test]
    fn validate_nil_agent_id_fails() {
        let mgr = make_manager();
        let mut cp = populated_checkpoint(&mgr);
        cp.agent_id = nil_agent_id();
        let result = mgr.validate_checkpoint(&cp);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nil agent_id"));
    }

    #[test]
    fn validate_empty_state_warns() {
        let mgr = make_manager();
        let cp = mgr.create_checkpoint(test_agent_id()).unwrap();
        let warnings = mgr.validate_checkpoint(&cp).unwrap();
        assert!(warnings.iter().any(|w| w.contains("no memory or IPC")));
    }

    #[test]
    fn validate_oversized_checkpoint_fails() {
        let mgr = make_manager();
        let mut cp = populated_checkpoint(&mgr);
        cp.total_size_bytes = MAX_CHECKPOINT_SIZE + 1;
        let result = mgr.validate_checkpoint(&cp);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn validate_empty_source_node_warns() {
        let mgr = make_manager();
        let mut cp = populated_checkpoint(&mgr);
        cp.source_node = String::new();
        let warnings = mgr.validate_checkpoint(&cp).unwrap();
        assert!(warnings.iter().any(|w| w.contains("empty source_node")));
    }

    #[test]
    fn validate_exactly_max_size_succeeds() {
        let mgr = make_manager();
        let mut cp = populated_checkpoint(&mgr);
        cp.total_size_bytes = MAX_CHECKPOINT_SIZE;
        let result = mgr.validate_checkpoint(&cp);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Compression round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn compress_reduces_size() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let original_size = cp.total_size_bytes;
        let compressed = mgr.compress_checkpoint(cp).unwrap();
        assert!(compressed.compressed);
        assert!(compressed.total_size_bytes < original_size);
    }

    #[test]
    fn compress_already_compressed_fails() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let compressed = mgr.compress_checkpoint(cp).unwrap();
        let result = mgr.compress_checkpoint(compressed);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already compressed"));
    }

    #[test]
    fn decompress_restores_size() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let original_size = cp.total_size_bytes;
        let compressed = mgr.compress_checkpoint(cp).unwrap();
        let decompressed = mgr.decompress_checkpoint(compressed).unwrap();
        assert!(!decompressed.compressed);
        // Allow rounding tolerance of 1 byte.
        assert!(
            (decompressed.total_size_bytes as i64 - original_size as i64).unsigned_abs() <= 1,
            "expected ~{} got {}",
            original_size,
            decompressed.total_size_bytes
        );
    }

    #[test]
    fn decompress_uncompressed_fails() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let result = mgr.decompress_checkpoint(cp);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not compressed"));
    }

    #[test]
    fn compress_preserves_data() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let id = cp.checkpoint_id;
        let mem_len = cp.memory_snapshot.len();
        let compressed = mgr.compress_checkpoint(cp).unwrap();
        assert_eq!(compressed.checkpoint_id, id);
        assert_eq!(compressed.memory_snapshot.len(), mem_len);
    }

    // -----------------------------------------------------------------------
    // Transfer time estimation
    // -----------------------------------------------------------------------

    #[test]
    fn estimate_transfer_time_basic() {
        let mgr = make_manager();
        let mut cp = populated_checkpoint(&mgr);
        cp.total_size_bytes = 125_000; // 125 KB
        // At 1 Mbps = 125,000 B/s → should be ~1 second.
        let dur = mgr.estimate_transfer_time(&cp, 1.0);
        assert!(dur.as_millis() >= 900 && dur.as_millis() <= 1100);
    }

    #[test]
    fn estimate_transfer_time_zero_bandwidth() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let dur = mgr.estimate_transfer_time(&cp, 0.0);
        assert!(dur.as_secs() > 1_000_000);
    }

    #[test]
    fn estimate_transfer_time_negative_bandwidth() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let dur = mgr.estimate_transfer_time(&cp, -10.0);
        assert!(dur.as_secs() > 1_000_000);
    }

    #[test]
    fn estimate_transfer_time_large_bandwidth_is_fast() {
        let mgr = make_manager();
        let mut cp = populated_checkpoint(&mgr);
        cp.total_size_bytes = 1_000_000; // 1 MB
        let dur = mgr.estimate_transfer_time(&cp, 10_000.0); // 10 Gbps
        assert!(dur.as_millis() < 10);
    }

    #[test]
    fn estimate_transfer_zero_size() {
        let mgr = make_manager();
        let cp = mgr.create_checkpoint(test_agent_id()).unwrap();
        let dur = mgr.estimate_transfer_time(&cp, 100.0);
        assert_eq!(dur.as_nanos(), 0);
    }

    // -----------------------------------------------------------------------
    // Migration plan
    // -----------------------------------------------------------------------

    #[test]
    fn create_warm_plan() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        assert_eq!(plan.source_node, "node-alpha");
        assert_eq!(plan.destination_node, "node-beta");
        assert_eq!(plan.migration_type, MigrationType::Warm);
        assert_eq!(plan.estimated_downtime, Duration::from_millis(500));
    }

    #[test]
    fn create_cold_plan() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-gamma".into(), MigrationType::Cold);
        assert_eq!(plan.migration_type, MigrationType::Cold);
        assert_eq!(plan.estimated_downtime, Duration::from_secs(5));
    }

    #[test]
    fn create_live_plan() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-delta".into(), MigrationType::Live);
        assert_eq!(plan.migration_type, MigrationType::Live);
        assert_eq!(plan.estimated_downtime, Duration::from_millis(50));
    }

    #[test]
    fn plan_carries_checkpoint_id() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        assert_eq!(plan.checkpoint_id, cp.checkpoint_id);
    }

    // -----------------------------------------------------------------------
    // State transitions
    // -----------------------------------------------------------------------

    #[test]
    fn valid_forward_transitions() {
        assert!(MigrationState::Pending.valid_transition(&MigrationState::Quiescing));
        assert!(MigrationState::Quiescing.valid_transition(&MigrationState::Checkpointing));
        assert!(MigrationState::Checkpointing.valid_transition(&MigrationState::Transferring));
        assert!(MigrationState::Transferring.valid_transition(&MigrationState::Restoring));
        assert!(MigrationState::Restoring.valid_transition(&MigrationState::Verifying));
        assert!(MigrationState::Verifying.valid_transition(&MigrationState::Complete));
    }

    #[test]
    fn any_state_can_fail() {
        let fail = MigrationState::Failed("oops".into());
        assert!(MigrationState::Pending.valid_transition(&fail));
        assert!(MigrationState::Quiescing.valid_transition(&fail));
        assert!(MigrationState::Checkpointing.valid_transition(&fail));
        assert!(MigrationState::Transferring.valid_transition(&fail));
        assert!(MigrationState::Restoring.valid_transition(&fail));
        assert!(MigrationState::Verifying.valid_transition(&fail));
    }

    #[test]
    fn complete_is_terminal() {
        assert!(!MigrationState::Complete.valid_transition(&MigrationState::Pending));
        assert!(!MigrationState::Complete.valid_transition(&MigrationState::Quiescing));
        assert!(MigrationState::Complete.is_terminal());
    }

    #[test]
    fn failed_can_retry() {
        let fail = MigrationState::Failed("timeout".into());
        assert!(fail.valid_transition(&MigrationState::Pending));
    }

    #[test]
    fn same_state_is_noop() {
        assert!(MigrationState::Pending.valid_transition(&MigrationState::Pending));
        assert!(MigrationState::Transferring.valid_transition(&MigrationState::Transferring));
        let fail = MigrationState::Failed("x".into());
        assert!(fail.valid_transition(&fail));
    }

    #[test]
    fn invalid_skip_transitions() {
        // Cannot skip from Pending straight to Transferring.
        assert!(!MigrationState::Pending.valid_transition(&MigrationState::Transferring));
        assert!(!MigrationState::Pending.valid_transition(&MigrationState::Complete));
        assert!(!MigrationState::Quiescing.valid_transition(&MigrationState::Restoring));
        assert!(!MigrationState::Checkpointing.valid_transition(&MigrationState::Verifying));
    }

    #[test]
    fn invalid_backward_transitions() {
        assert!(!MigrationState::Checkpointing.valid_transition(&MigrationState::Quiescing));
        assert!(!MigrationState::Transferring.valid_transition(&MigrationState::Checkpointing));
        assert!(!MigrationState::Verifying.valid_transition(&MigrationState::Restoring));
    }

    #[test]
    fn failed_cannot_skip_to_running_states() {
        let fail = MigrationState::Failed("err".into());
        assert!(!fail.valid_transition(&MigrationState::Transferring));
        assert!(!fail.valid_transition(&MigrationState::Complete));
    }

    #[test]
    fn is_terminal_for_non_terminal_states() {
        assert!(!MigrationState::Pending.is_terminal());
        assert!(!MigrationState::Quiescing.is_terminal());
        assert!(!MigrationState::Transferring.is_terminal());
        assert!(MigrationState::Failed("x".into()).is_terminal());
    }

    // -----------------------------------------------------------------------
    // MigrationTracker lifecycle
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn start_migration_returns_id() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let id = tracker.start_migration(plan).await.unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn get_migration_returns_record() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let id = tracker.start_migration(plan).await.unwrap();
        let record = tracker.get_migration(&id).await.unwrap();
        assert_eq!(record.current_state, MigrationState::Pending);
        assert_eq!(record.state_history.len(), 1);
    }

    #[tokio::test]
    async fn get_unknown_migration_returns_none() {
        let tracker = MigrationTracker::new();
        assert!(tracker.get_migration("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn advance_through_full_lifecycle() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let id = tracker.start_migration(plan).await.unwrap();

        let states = [
            MigrationState::Quiescing,
            MigrationState::Checkpointing,
            MigrationState::Transferring,
            MigrationState::Restoring,
            MigrationState::Verifying,
            MigrationState::Complete,
        ];

        for state in &states {
            tracker.advance_state(&id, state.clone()).await.unwrap();
        }

        let record = tracker.get_migration(&id).await.unwrap();
        assert_eq!(record.current_state, MigrationState::Complete);
        // 1 initial + 6 transitions = 7
        assert_eq!(record.state_history.len(), 7);
        assert!(record.completed_at.is_some());
    }

    #[tokio::test]
    async fn advance_invalid_transition_fails() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let id = tracker.start_migration(plan).await.unwrap();

        // Pending → Transferring is invalid (skips quiescing + checkpointing).
        let result = tracker
            .advance_state(&id, MigrationState::Transferring)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid transition"));
    }

    #[tokio::test]
    async fn advance_nonexistent_migration_fails() {
        let tracker = MigrationTracker::new();
        let result = tracker
            .advance_state("nope", MigrationState::Quiescing)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn active_migrations_excludes_completed() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();

        // Start two migrations.
        let cp1 = populated_checkpoint(&mgr);
        let plan1 =
            mgr.create_plan(cp1.agent_id, &cp1, "node-beta".into(), MigrationType::Warm);
        let id1 = tracker.start_migration(plan1).await.unwrap();

        let cp2 = populated_checkpoint(&mgr);
        let plan2 =
            mgr.create_plan(cp2.agent_id, &cp2, "node-gamma".into(), MigrationType::Cold);
        let _id2 = tracker.start_migration(plan2).await.unwrap();

        // Complete the first one.
        for s in [
            MigrationState::Quiescing,
            MigrationState::Checkpointing,
            MigrationState::Transferring,
            MigrationState::Restoring,
            MigrationState::Verifying,
            MigrationState::Complete,
        ] {
            tracker.advance_state(&id1, s).await.unwrap();
        }

        let active = tracker.active_migrations().await;
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn active_migrations_excludes_failed() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let id = tracker.start_migration(plan).await.unwrap();
        tracker
            .advance_state(&id, MigrationState::Failed("boom".into()))
            .await
            .unwrap();

        let active = tracker.active_migrations().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn migration_history_filters_by_agent() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();

        let agent_a = test_agent_id();
        let agent_b = test_agent_id();

        let cp_a = mgr
            .create_checkpoint_with_state(
                agent_a,
                sample_memory(),
                vec![],
                sample_ipc_queue(),
                serde_json::json!({}),
            )
            .unwrap();
        let plan_a =
            mgr.create_plan(agent_a, &cp_a, "node-beta".into(), MigrationType::Warm);
        tracker.start_migration(plan_a).await.unwrap();

        let cp_b = mgr
            .create_checkpoint_with_state(
                agent_b,
                sample_memory(),
                vec![],
                vec![],
                serde_json::json!({}),
            )
            .unwrap();
        let plan_b =
            mgr.create_plan(agent_b, &cp_b, "node-beta".into(), MigrationType::Cold);
        tracker.start_migration(plan_b).await.unwrap();

        let history_a = tracker.migration_history(agent_a).await;
        assert_eq!(history_a.len(), 1);
        assert_eq!(history_a[0].plan.agent_id, agent_a);

        let history_b = tracker.migration_history(agent_b).await;
        assert_eq!(history_b.len(), 1);
    }

    #[tokio::test]
    async fn migration_history_empty_for_unknown_agent() {
        let tracker = MigrationTracker::new();
        let history = tracker.migration_history(test_agent_id()).await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn total_count_tracks_all() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();

        assert_eq!(tracker.total_count().await, 0);

        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        tracker.start_migration(plan).await.unwrap();
        assert_eq!(tracker.total_count().await, 1);
    }

    #[tokio::test]
    async fn failed_then_retry_lifecycle() {
        let tracker = MigrationTracker::new();
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let id = tracker.start_migration(plan).await.unwrap();

        // Advance to quiescing, then fail.
        tracker
            .advance_state(&id, MigrationState::Quiescing)
            .await
            .unwrap();
        tracker
            .advance_state(&id, MigrationState::Failed("network error".into()))
            .await
            .unwrap();

        // Retry: Failed → Pending.
        tracker
            .advance_state(&id, MigrationState::Pending)
            .await
            .unwrap();

        let record = tracker.get_migration(&id).await.unwrap();
        assert_eq!(record.current_state, MigrationState::Pending);
        // completed_at should have been set when it failed, remains set.
        // History: Pending, Quiescing, Failed, Pending = 4 entries.
        assert_eq!(record.state_history.len(), 4);
    }

    // -----------------------------------------------------------------------
    // Display impls
    // -----------------------------------------------------------------------

    #[test]
    fn checkpoint_type_display() {
        assert_eq!(CheckpointType::Full.to_string(), "full");
        assert_eq!(CheckpointType::Incremental.to_string(), "incremental");
    }

    #[test]
    fn migration_type_display() {
        assert_eq!(MigrationType::Warm.to_string(), "warm");
        assert_eq!(MigrationType::Cold.to_string(), "cold");
        assert_eq!(MigrationType::Live.to_string(), "live");
    }

    #[test]
    fn migration_state_display() {
        assert_eq!(MigrationState::Pending.to_string(), "pending");
        assert_eq!(MigrationState::Quiescing.to_string(), "quiescing");
        assert_eq!(MigrationState::Complete.to_string(), "complete");
        assert_eq!(
            MigrationState::Failed("timeout".into()).to_string(),
            "failed: timeout"
        );
    }

    // -----------------------------------------------------------------------
    // Serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn checkpoint_serialization_roundtrip() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let json = serde_json::to_string(&cp).unwrap();
        let deserialized: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.checkpoint_id, cp.checkpoint_id);
        assert_eq!(deserialized.memory_snapshot.len(), cp.memory_snapshot.len());
    }

    #[test]
    fn migration_plan_serialization_roundtrip() {
        let mgr = make_manager();
        let cp = populated_checkpoint(&mgr);
        let plan = mgr.create_plan(cp.agent_id, &cp, "node-beta".into(), MigrationType::Warm);
        let json = serde_json::to_string(&plan).unwrap();
        let deserialized: MigrationPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.source_node, plan.source_node);
        assert_eq!(deserialized.migration_type, plan.migration_type);
    }

    #[test]
    fn migration_state_serialization_roundtrip() {
        let states = vec![
            MigrationState::Pending,
            MigrationState::Quiescing,
            MigrationState::Complete,
            MigrationState::Failed("err".into()),
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: MigrationState = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, state);
        }
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn pending_message_fields() {
        let msg = PendingMessage {
            sender: "s".into(),
            recipient: "r".into(),
            payload: "p".into(),
            timestamp: Utc::now(),
        };
        assert_eq!(msg.sender, "s");
        assert_eq!(msg.recipient, "r");
        assert_eq!(msg.payload, "p");
    }

    #[test]
    fn checkpoint_with_large_memory() {
        let mgr = make_manager();
        let mut mem = HashMap::new();
        for i in 0..1000 {
            mem.insert(format!("key-{}", i), serde_json::json!(i));
        }
        let cp = mgr
            .create_checkpoint_with_state(
                test_agent_id(),
                mem,
                vec![],
                vec![],
                serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(cp.memory_snapshot.len(), 1000);
        assert!(cp.total_size_bytes > 0);
    }

    #[test]
    fn default_tracker_has_zero_count() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let tracker = MigrationTracker::default();
            assert_eq!(tracker.total_count().await, 0);
        });
    }
}
