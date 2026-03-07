//! Cloud Services
//!
//! Optional cloud connectivity for AGNOS nodes. Enables hosted agent
//! deployment, cross-device state synchronisation, collaborative workspaces,
//! and usage-based billing tracking. Every cloud feature is opt-in —
//! AGNOS remains fully functional in local-only mode.

use std::collections::HashMap;
use std::fmt;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Cloud Region
// ---------------------------------------------------------------------------

/// Geographic region for cloud resources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CloudRegion {
    UsEast,
    UsWest,
    EuWest,
    EuCentral,
    AsiaPacific,
    Custom(String),
}

impl fmt::Display for CloudRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UsEast => write!(f, "us-east"),
            Self::UsWest => write!(f, "us-west"),
            Self::EuWest => write!(f, "eu-west"),
            Self::EuCentral => write!(f, "eu-central"),
            Self::AsiaPacific => write!(f, "asia-pacific"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Cloud Config
// ---------------------------------------------------------------------------

/// Configuration for cloud connectivity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub endpoint_url: String,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    pub region: CloudRegion,
    pub sync_enabled: bool,
    pub sync_interval_secs: u64,
    pub max_upload_bytes: u64,
    pub encryption_required: bool,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            endpoint_url: String::new(),
            api_key: None,
            region: CloudRegion::UsEast,
            sync_enabled: false,
            sync_interval_secs: 300,
            max_upload_bytes: 100 * 1024 * 1024, // 100 MB
            encryption_required: true,
        }
    }
}

impl CloudConfig {
    /// Validate the configuration, returning an error on invalid state.
    pub fn validate(&self) -> Result<()> {
        if self.endpoint_url.is_empty() {
            bail!("endpoint_url must not be empty");
        }
        if self.sync_interval_secs == 0 {
            bail!("sync_interval_secs must be > 0");
        }
        if self.max_upload_bytes == 0 {
            bail!("max_upload_bytes must be > 0");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Current status of a cloud connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Health snapshot of a cloud connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHealth {
    pub latency_ms: u64,
    pub status: ConnectionStatus,
    pub last_error: Option<String>,
}

/// Manages connectivity to the cloud backend.
#[derive(Debug, Clone)]
pub struct CloudConnection {
    pub config: CloudConfig,
    pub status: ConnectionStatus,
    pub last_sync: Option<DateTime<Utc>>,
    pub node_id: String,
    /// Last measured health-check latency in milliseconds.
    /// Updated externally via [`record_latency`](Self::record_latency).
    pub last_health_check_latency_ms: u64,
}

impl CloudConnection {
    pub fn new(config: CloudConfig, node_id: String) -> Self {
        Self {
            config,
            status: ConnectionStatus::Disconnected,
            last_sync: None,
            node_id,
            last_health_check_latency_ms: 0,
        }
    }

    /// Validate config and transition to `Connected`.
    pub fn connect(&mut self) -> Result<()> {
        self.config.validate()?;
        self.status = ConnectionStatus::Connecting;
        info!(node_id = %self.node_id, "connecting to cloud endpoint");
        // In a real implementation this would perform a TLS handshake.
        self.status = ConnectionStatus::Connected;
        self.last_sync = Some(Utc::now());
        debug!(node_id = %self.node_id, "cloud connection established");
        Ok(())
    }

    /// Gracefully disconnect from cloud.
    pub fn disconnect(&mut self) {
        info!(node_id = %self.node_id, "disconnecting from cloud");
        self.status = ConnectionStatus::Disconnected;
    }

    /// Whether the connection is currently live.
    pub fn is_connected(&self) -> bool {
        self.status == ConnectionStatus::Connected
    }

    /// Record an externally measured latency value (in milliseconds).
    pub fn record_latency(&mut self, latency_ms: u64) {
        self.last_health_check_latency_ms = latency_ms;
    }

    /// Return a health snapshot.
    pub fn health_check(&self) -> ConnectionHealth {
        let last_error = match &self.status {
            ConnectionStatus::Error(e) => Some(e.clone()),
            _ => None,
        };
        ConnectionHealth {
            latency_ms: if self.is_connected() {
                self.last_health_check_latency_ms
            } else {
                0
            },
            status: self.status.clone(),
            last_error,
        }
    }
}

// ---------------------------------------------------------------------------
// Cloud Agent Deployment
// ---------------------------------------------------------------------------

/// Lifecycle status of a cloud-hosted agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudAgentStatus {
    Pending,
    Provisioning,
    Running,
    Stopped,
    Failed,
    Terminated,
}

impl CloudAgentStatus {
    /// Whether transitioning from `self` to `target` is valid.
    pub fn valid_transition(&self, target: &CloudAgentStatus) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Provisioning)
                | (Self::Provisioning, Self::Running)
                | (Self::Provisioning, Self::Failed)
                | (Self::Running, Self::Stopped)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Terminated)
                | (Self::Stopped, Self::Running)
                | (Self::Stopped, Self::Terminated)
                | (Self::Failed, Self::Pending)
                | (Self::Failed, Self::Terminated)
        )
    }
}

/// Resource tier for cloud agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceTier {
    Free,
    Standard,
    Performance,
    Custom,
}

impl ResourceTier {
    pub fn cpu_cores(&self) -> u32 {
        match self {
            Self::Free => 1,
            Self::Standard => 2,
            Self::Performance => 8,
            Self::Custom => 0,
        }
    }

    pub fn memory_mb(&self) -> u64 {
        match self {
            Self::Free => 512,
            Self::Standard => 2048,
            Self::Performance => 16384,
            Self::Custom => 0,
        }
    }

    pub fn gpu(&self) -> bool {
        matches!(self, Self::Performance)
    }

    pub fn storage_gb(&self) -> u64 {
        match self {
            Self::Free => 1,
            Self::Standard => 10,
            Self::Performance => 100,
            Self::Custom => 0,
        }
    }

    pub fn monthly_cost_cents(&self) -> u64 {
        match self {
            Self::Free => 0,
            Self::Standard => 1999,
            Self::Performance => 9999,
            Self::Custom => 0,
        }
    }
}

/// A cloud-hosted agent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudAgent {
    pub agent_id: String,
    pub name: String,
    pub cloud_instance_id: Uuid,
    pub status: CloudAgentStatus,
    pub region: CloudRegion,
    pub resource_tier: ResourceTier,
    pub created_at: DateTime<Utc>,
    pub monthly_cost_cents: u64,
}

/// Deployment statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentStats {
    pub total_agents: usize,
    pub running: usize,
    pub stopped: usize,
    pub failed: usize,
    pub pending: usize,
    pub provisioning: usize,
    pub terminated: usize,
}

/// Manages cloud agent deployments.
#[derive(Debug, Default)]
pub struct CloudDeploymentManager {
    agents: HashMap<String, CloudAgent>,
}

impl CloudDeploymentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Deploy a new agent to the cloud.
    pub fn deploy_agent(
        &mut self,
        agent_id: String,
        name: String,
        region: CloudRegion,
        resource_tier: ResourceTier,
    ) -> Result<CloudAgent> {
        if self.agents.contains_key(&agent_id) {
            bail!("agent {} is already deployed", agent_id);
        }
        let cost = resource_tier.monthly_cost_cents();
        let agent = CloudAgent {
            agent_id: agent_id.clone(),
            name,
            cloud_instance_id: Uuid::new_v4(),
            status: CloudAgentStatus::Pending,
            region,
            resource_tier,
            created_at: Utc::now(),
            monthly_cost_cents: cost,
        };
        info!(agent_id = %agent.agent_id, "deploying agent to cloud");
        self.agents.insert(agent_id, agent.clone());
        Ok(agent)
    }

    /// Stop a running agent.
    pub fn stop_agent(&mut self, agent_id: &str) -> Result<()> {
        let agent = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| anyhow::anyhow!("agent not found: {agent_id}"))?;
        if !agent.status.valid_transition(&CloudAgentStatus::Stopped) {
            bail!(
                "cannot stop agent in {:?} state",
                agent.status
            );
        }
        agent.status = CloudAgentStatus::Stopped;
        info!(agent_id, "cloud agent stopped");
        Ok(())
    }

    /// Terminate an agent, removing it from active management.
    pub fn terminate_agent(&mut self, agent_id: &str) -> Result<()> {
        let agent = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| anyhow::anyhow!("agent not found: {agent_id}"))?;
        if !agent.status.valid_transition(&CloudAgentStatus::Terminated) {
            bail!(
                "cannot terminate agent in {:?} state",
                agent.status
            );
        }
        agent.status = CloudAgentStatus::Terminated;
        info!(agent_id, "cloud agent terminated");
        Ok(())
    }

    pub fn get_agent(&self, agent_id: &str) -> Option<&CloudAgent> {
        self.agents.get(agent_id)
    }

    pub fn list_agents(&self) -> Vec<&CloudAgent> {
        self.agents.values().collect()
    }

    /// Agents deployed to a specific region.
    pub fn agents_by_region(&self, region: &CloudRegion) -> Vec<&CloudAgent> {
        self.agents
            .values()
            .filter(|a| &a.region == region)
            .collect()
    }

    /// Total monthly cost across all non-terminated agents.
    pub fn total_monthly_cost(&self) -> u64 {
        self.agents
            .values()
            .filter(|a| a.status != CloudAgentStatus::Terminated)
            .map(|a| a.monthly_cost_cents)
            .sum()
    }

    /// Aggregate deployment statistics.
    pub fn deployment_stats(&self) -> DeploymentStats {
        let mut stats = DeploymentStats {
            total_agents: self.agents.len(),
            running: 0,
            stopped: 0,
            failed: 0,
            pending: 0,
            provisioning: 0,
            terminated: 0,
        };
        for agent in self.agents.values() {
            match agent.status {
                CloudAgentStatus::Running => stats.running += 1,
                CloudAgentStatus::Stopped => stats.stopped += 1,
                CloudAgentStatus::Failed => stats.failed += 1,
                CloudAgentStatus::Pending => stats.pending += 1,
                CloudAgentStatus::Provisioning => stats.provisioning += 1,
                CloudAgentStatus::Terminated => stats.terminated += 1,
            }
        }
        stats
    }
}

// ---------------------------------------------------------------------------
// Cross-Device Sync
// ---------------------------------------------------------------------------

/// Type of synchronisable item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SyncItemType {
    AgentConfig,
    MemoryStore,
    Preferences,
    SandboxProfile,
    CustomData,
}

/// A versioned, checksummed item eligible for cross-device sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncItem {
    pub item_id: Uuid,
    pub item_type: SyncItemType,
    pub agent_id: Option<String>,
    pub data: serde_json::Value,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub checksum: String,
}

/// Direction of a sync operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncDirection {
    Push,
    Pull,
    Bidirectional,
}

/// Strategy for resolving a sync conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictResolution {
    LocalWins,
    RemoteWins,
    Manual,
    Merge,
}

/// A detected conflict between local and remote versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    pub item_id: Uuid,
    pub local_version: u64,
    pub remote_version: u64,
    pub resolution: ConflictResolution,
}

/// Summary statistics about the sync engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_items: usize,
    pub pending_push: usize,
    pub pending_pull: usize,
    pub conflicts: usize,
    pub last_sync: Option<DateTime<Utc>>,
}

/// Compute SHA-256 checksum of serialized data.
fn compute_checksum(data: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(data).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    format!("{hash:x}")
}

/// Engine managing cross-device state synchronisation.
#[derive(Debug, Default)]
pub struct SyncEngine {
    items: HashMap<Uuid, SyncItem>,
    pending_push: usize,
    pending_pull: usize,
    conflicts: Vec<SyncConflict>,
    last_sync: Option<DateTime<Utc>>,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a sync item, recomputing its checksum.
    pub fn add_sync_item(&mut self, mut item: SyncItem) -> Result<()> {
        if item.version == 0 {
            bail!("sync item version must be > 0");
        }
        item.checksum = compute_checksum(&item.data);
        item.updated_at = Utc::now();
        debug!(item_id = %item.item_id, version = item.version, "sync item added");
        self.items.insert(item.item_id, item);
        self.pending_push += 1;
        Ok(())
    }

    pub fn get_item(&self, item_id: &Uuid) -> Option<&SyncItem> {
        self.items.get(item_id)
    }

    /// Items changed since a given version number.
    pub fn items_since(&self, version: u64) -> Vec<&SyncItem> {
        self.items
            .values()
            .filter(|i| i.version > version)
            .collect()
    }

    /// Detect conflicts between local items and a set of remote items.
    pub fn detect_conflicts(&mut self, remote_items: &[SyncItem]) -> Vec<SyncConflict> {
        let mut conflicts = Vec::new();
        for remote in remote_items {
            if let Some(local) = self.items.get(&remote.item_id) {
                if local.version != remote.version && local.checksum != remote.checksum {
                    let conflict = SyncConflict {
                        item_id: remote.item_id,
                        local_version: local.version,
                        remote_version: remote.version,
                        resolution: ConflictResolution::LocalWins,
                    };
                    conflicts.push(conflict);
                }
            }
        }
        self.conflicts = conflicts.clone();
        conflicts
    }

    /// Resolve a conflict according to its resolution strategy.
    pub fn resolve_conflict(&mut self, conflict: &SyncConflict) -> Result<SyncItem> {
        let local = self
            .items
            .get(&conflict.item_id)
            .ok_or_else(|| anyhow::anyhow!("item not found for conflict resolution"))?
            .clone();

        let resolved = match conflict.resolution {
            ConflictResolution::LocalWins => {
                info!(item_id = %conflict.item_id, "conflict resolved: local wins");
                local
            }
            ConflictResolution::RemoteWins => {
                // NOTE: This only adopts the remote *version number*. The caller
                // is responsible for supplying the actual remote data before
                // invoking resolution — a real merge requires the remote payload
                // which is not available inside the local SyncEngine.
                info!(item_id = %conflict.item_id, "conflict resolved: remote wins (version only)");
                SyncItem {
                    version: conflict.remote_version,
                    ..local
                }
            }
            ConflictResolution::Manual => {
                bail!(
                    "manual conflict resolution requires user input — \
                     item {} cannot be auto-resolved",
                    conflict.item_id
                );
            }
            ConflictResolution::Merge => {
                info!(item_id = %conflict.item_id, "conflict resolved: merge");
                SyncItem {
                    version: std::cmp::max(conflict.local_version, conflict.remote_version) + 1,
                    ..local
                }
            }
        };
        self.items.insert(resolved.item_id, resolved.clone());
        self.conflicts.retain(|c| c.item_id != conflict.item_id);
        Ok(resolved)
    }

    /// Mark all pending pushes as synced. Resets `pending_push` to 0 and
    /// updates `last_sync` to the current time.
    pub fn mark_synced(&mut self) {
        self.pending_push = 0;
        self.last_sync = Some(Utc::now());
        debug!("sync engine: marked synced, pending_push reset");
    }

    /// Record that `count` items have been pulled from the remote.
    /// Decrements `pending_pull` (saturating) and updates `last_sync`.
    pub fn mark_pulled(&mut self, count: usize) {
        self.pending_pull = self.pending_pull.saturating_sub(count);
        self.last_sync = Some(Utc::now());
        debug!(count, "sync engine: marked pulled");
    }

    /// Current sync statistics.
    pub fn sync_stats(&self) -> SyncStats {
        SyncStats {
            total_items: self.items.len(),
            pending_push: self.pending_push,
            pending_pull: self.pending_pull,
            conflicts: self.conflicts.len(),
            last_sync: self.last_sync,
        }
    }
}

// ---------------------------------------------------------------------------
// Collaborative Workspaces
// ---------------------------------------------------------------------------

/// Role within a collaborative workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkspaceRole {
    Owner,
    Admin,
    Editor,
    Viewer,
}

impl WorkspaceRole {
    pub fn can_deploy_agents(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Editor)
    }

    pub fn can_manage_members(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    pub fn can_view(&self) -> bool {
        // All roles can view.
        true
    }
}

/// A member of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub user_id: String,
    pub role: WorkspaceRole,
    pub joined_at: DateTime<Utc>,
}

/// Settings governing a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    pub max_agents: u32,
    pub max_members: u32,
    pub shared_memory: bool,
    pub shared_vector_store: bool,
    pub audit_all_actions: bool,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            max_agents: 10,
            max_members: 20,
            shared_memory: true,
            shared_vector_store: false,
            audit_all_actions: true,
        }
    }
}

/// A collaborative workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub workspace_id: Uuid,
    pub name: String,
    pub owner: String,
    pub members: Vec<WorkspaceMember>,
    pub created_at: DateTime<Utc>,
    pub settings: WorkspaceSettings,
}

/// Aggregate statistics for a workspace.
///
/// Only contains data that [`WorkspaceManager`] can compute directly.
/// Agent counts, sync-item totals, and storage usage must be correlated
/// externally by querying the relevant subsystems (e.g. `CloudDeploymentManager`,
/// `SyncEngine`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStats {
    pub member_count: usize,
}

/// Manages collaborative workspaces.
#[derive(Debug, Default)]
pub struct WorkspaceManager {
    workspaces: HashMap<Uuid, Workspace>,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a workspace with the caller as owner.
    pub fn create_workspace(&mut self, name: String, owner: String) -> Result<Workspace> {
        if name.is_empty() {
            bail!("workspace name must not be empty");
        }
        if owner.is_empty() {
            bail!("workspace owner must not be empty");
        }
        let workspace = Workspace {
            workspace_id: Uuid::new_v4(),
            name,
            owner: owner.clone(),
            members: vec![WorkspaceMember {
                user_id: owner,
                role: WorkspaceRole::Owner,
                joined_at: Utc::now(),
            }],
            created_at: Utc::now(),
            settings: WorkspaceSettings::default(),
        };
        info!(workspace_id = %workspace.workspace_id, "workspace created");
        self.workspaces
            .insert(workspace.workspace_id, workspace.clone());
        Ok(workspace)
    }

    pub fn get_workspace(&self, workspace_id: &Uuid) -> Option<&Workspace> {
        self.workspaces.get(workspace_id)
    }

    /// Add a member to the workspace.
    pub fn add_member(&mut self, workspace_id: &Uuid, member: WorkspaceMember) -> Result<()> {
        let ws = self
            .workspaces
            .get_mut(workspace_id)
            .ok_or_else(|| anyhow::anyhow!("workspace not found"))?;

        if ws.members.len() as u32 >= ws.settings.max_members {
            bail!("workspace has reached its member limit ({})", ws.settings.max_members);
        }
        if ws.members.iter().any(|m| m.user_id == member.user_id) {
            bail!("user {} is already a member", member.user_id);
        }
        debug!(workspace_id = %workspace_id, user_id = %member.user_id, "member added");
        ws.members.push(member);
        Ok(())
    }

    /// Remove a member from the workspace. Cannot remove the owner.
    pub fn remove_member(&mut self, workspace_id: &Uuid, user_id: &str) -> Result<()> {
        let ws = self
            .workspaces
            .get_mut(workspace_id)
            .ok_or_else(|| anyhow::anyhow!("workspace not found"))?;

        if ws.owner == user_id {
            bail!("cannot remove workspace owner");
        }
        let before = ws.members.len();
        ws.members.retain(|m| m.user_id != user_id);
        if ws.members.len() == before {
            bail!("user {user_id} is not a member");
        }
        info!(workspace_id = %workspace_id, user_id, "member removed");
        Ok(())
    }

    /// Update a member's role. Cannot change the owner's role.
    pub fn update_role(
        &mut self,
        workspace_id: &Uuid,
        user_id: &str,
        new_role: WorkspaceRole,
    ) -> Result<()> {
        let ws = self
            .workspaces
            .get_mut(workspace_id)
            .ok_or_else(|| anyhow::anyhow!("workspace not found"))?;

        if ws.owner == user_id {
            bail!("cannot change owner role");
        }
        let member = ws
            .members
            .iter_mut()
            .find(|m| m.user_id == user_id)
            .ok_or_else(|| anyhow::anyhow!("user {user_id} is not a member"))?;
        member.role = new_role;
        Ok(())
    }

    /// All workspaces where a user is a member.
    pub fn list_workspaces_for_user(&self, user_id: &str) -> Vec<&Workspace> {
        self.workspaces
            .values()
            .filter(|ws| ws.members.iter().any(|m| m.user_id == user_id))
            .collect()
    }

    /// Aggregate statistics for a workspace.
    pub fn workspace_stats(&self, workspace_id: &Uuid) -> Option<WorkspaceStats> {
        self.workspaces.get(workspace_id).map(|ws| WorkspaceStats {
            member_count: ws.members.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Cloud Billing
// ---------------------------------------------------------------------------

/// Type of billable usage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum UsageType {
    Compute,
    Storage,
    NetworkEgress,
    SyncOperations,
}

/// A single usage record for billing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub record_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub agent_id: Option<String>,
    pub usage_type: UsageType,
    pub quantity: f64,
    pub unit: String,
    pub cost_cents: u64,
    pub recorded_at: DateTime<Utc>,
}

/// Tracks cloud usage and costs.
#[derive(Debug, Default)]
pub struct BillingTracker {
    records: Vec<UsageRecord>,
}

impl BillingTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a usage event.
    pub fn add_usage(&mut self, record: UsageRecord) {
        debug!(record_id = %record.record_id, usage_type = ?record.usage_type, "usage recorded");
        self.records.push(record);
    }

    /// All usage records for a given workspace.
    pub fn usage_for_workspace(&self, workspace_id: &Uuid) -> Vec<&UsageRecord> {
        self.records
            .iter()
            .filter(|r| r.workspace_id.as_ref() == Some(workspace_id))
            .collect()
    }

    /// All usage records for a given agent.
    pub fn usage_for_agent(&self, agent_id: &str) -> Vec<&UsageRecord> {
        self.records
            .iter()
            .filter(|r| r.agent_id.as_deref() == Some(agent_id))
            .collect()
    }

    /// Total cost across all records.
    pub fn total_cost_cents(&self) -> u64 {
        self.records.iter().map(|r| r.cost_cents).sum()
    }

    /// Breakdown of total cost by usage type.
    pub fn usage_summary(&self) -> HashMap<UsageType, u64> {
        let mut summary = HashMap::new();
        for r in &self.records {
            *summary.entry(r.usage_type.clone()).or_insert(0) += r.cost_cents;
        }
        summary
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Config ---------------------------------------------------------------

    #[test]
    fn test_cloud_config_default() {
        let cfg = CloudConfig::default();
        assert!(cfg.endpoint_url.is_empty());
        assert!(cfg.api_key.is_none());
        assert_eq!(cfg.region, CloudRegion::UsEast);
        assert!(!cfg.sync_enabled);
        assert_eq!(cfg.sync_interval_secs, 300);
        assert_eq!(cfg.max_upload_bytes, 100 * 1024 * 1024);
        assert!(cfg.encryption_required);
    }

    #[test]
    fn test_cloud_config_validate_ok() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_cloud_config_validate_empty_url() {
        let cfg = CloudConfig::default();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_cloud_config_validate_zero_interval() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        cfg.sync_interval_secs = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_cloud_config_validate_zero_upload() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        cfg.max_upload_bytes = 0;
        assert!(cfg.validate().is_err());
    }

    // -- Region ---------------------------------------------------------------

    #[test]
    fn test_region_display() {
        assert_eq!(CloudRegion::UsEast.to_string(), "us-east");
        assert_eq!(CloudRegion::UsWest.to_string(), "us-west");
        assert_eq!(CloudRegion::EuWest.to_string(), "eu-west");
        assert_eq!(CloudRegion::EuCentral.to_string(), "eu-central");
        assert_eq!(CloudRegion::AsiaPacific.to_string(), "asia-pacific");
        assert_eq!(
            CloudRegion::Custom("mars-1".into()).to_string(),
            "mars-1"
        );
    }

    #[test]
    fn test_region_equality() {
        assert_eq!(CloudRegion::UsEast, CloudRegion::UsEast);
        assert_ne!(CloudRegion::UsEast, CloudRegion::UsWest);
        assert_eq!(
            CloudRegion::Custom("a".into()),
            CloudRegion::Custom("a".into())
        );
    }

    // -- Connection -----------------------------------------------------------

    #[test]
    fn test_connection_new_disconnected() {
        let cfg = CloudConfig::default();
        let conn = CloudConnection::new(cfg, "node-1".into());
        assert_eq!(conn.status, ConnectionStatus::Disconnected);
        assert!(!conn.is_connected());
        assert!(conn.last_sync.is_none());
    }

    #[test]
    fn test_connection_connect_success() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        let mut conn = CloudConnection::new(cfg, "node-1".into());
        assert!(conn.connect().is_ok());
        assert!(conn.is_connected());
        assert!(conn.last_sync.is_some());
    }

    #[test]
    fn test_connection_connect_fail_invalid_config() {
        let cfg = CloudConfig::default(); // empty endpoint
        let mut conn = CloudConnection::new(cfg, "node-1".into());
        assert!(conn.connect().is_err());
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connection_disconnect() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        let mut conn = CloudConnection::new(cfg, "node-1".into());
        conn.connect().unwrap();
        conn.disconnect();
        assert!(!conn.is_connected());
        assert_eq!(conn.status, ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_connection_health_connected() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        let mut conn = CloudConnection::new(cfg, "node-1".into());
        conn.connect().unwrap();
        conn.record_latency(15);
        let health = conn.health_check();
        assert_eq!(health.status, ConnectionStatus::Connected);
        assert_eq!(health.latency_ms, 15);
        assert!(health.last_error.is_none());
    }

    #[test]
    fn test_connection_health_connected_no_latency_recorded() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        let mut conn = CloudConnection::new(cfg, "node-1".into());
        conn.connect().unwrap();
        let health = conn.health_check();
        assert_eq!(health.status, ConnectionStatus::Connected);
        assert_eq!(health.latency_ms, 0);
    }

    #[test]
    fn test_connection_health_disconnected() {
        let cfg = CloudConfig::default();
        let conn = CloudConnection::new(cfg, "node-1".into());
        let health = conn.health_check();
        assert_eq!(health.status, ConnectionStatus::Disconnected);
        assert_eq!(health.latency_ms, 0);
    }

    #[test]
    fn test_connection_health_error_state() {
        let cfg = CloudConfig::default();
        let mut conn = CloudConnection::new(cfg, "node-1".into());
        conn.status = ConnectionStatus::Error("timeout".into());
        let health = conn.health_check();
        assert_eq!(health.last_error, Some("timeout".into()));
    }

    // -- Cloud Agent Status ---------------------------------------------------

    #[test]
    fn test_valid_transitions() {
        assert!(CloudAgentStatus::Pending.valid_transition(&CloudAgentStatus::Provisioning));
        assert!(CloudAgentStatus::Provisioning.valid_transition(&CloudAgentStatus::Running));
        assert!(CloudAgentStatus::Provisioning.valid_transition(&CloudAgentStatus::Failed));
        assert!(CloudAgentStatus::Running.valid_transition(&CloudAgentStatus::Stopped));
        assert!(CloudAgentStatus::Running.valid_transition(&CloudAgentStatus::Failed));
        assert!(CloudAgentStatus::Running.valid_transition(&CloudAgentStatus::Terminated));
        assert!(CloudAgentStatus::Stopped.valid_transition(&CloudAgentStatus::Running));
        assert!(CloudAgentStatus::Stopped.valid_transition(&CloudAgentStatus::Terminated));
        assert!(CloudAgentStatus::Failed.valid_transition(&CloudAgentStatus::Pending));
        assert!(CloudAgentStatus::Failed.valid_transition(&CloudAgentStatus::Terminated));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!CloudAgentStatus::Pending.valid_transition(&CloudAgentStatus::Running));
        assert!(!CloudAgentStatus::Pending.valid_transition(&CloudAgentStatus::Terminated));
        assert!(!CloudAgentStatus::Terminated.valid_transition(&CloudAgentStatus::Running));
        assert!(!CloudAgentStatus::Terminated.valid_transition(&CloudAgentStatus::Pending));
        assert!(!CloudAgentStatus::Running.valid_transition(&CloudAgentStatus::Provisioning));
    }

    // -- Resource Tier --------------------------------------------------------

    #[test]
    fn test_resource_tier_free() {
        let t = ResourceTier::Free;
        assert_eq!(t.cpu_cores(), 1);
        assert_eq!(t.memory_mb(), 512);
        assert!(!t.gpu());
        assert_eq!(t.storage_gb(), 1);
        assert_eq!(t.monthly_cost_cents(), 0);
    }

    #[test]
    fn test_resource_tier_standard() {
        let t = ResourceTier::Standard;
        assert_eq!(t.cpu_cores(), 2);
        assert_eq!(t.memory_mb(), 2048);
        assert!(!t.gpu());
        assert_eq!(t.storage_gb(), 10);
        assert_eq!(t.monthly_cost_cents(), 1999);
    }

    #[test]
    fn test_resource_tier_performance() {
        let t = ResourceTier::Performance;
        assert_eq!(t.cpu_cores(), 8);
        assert_eq!(t.memory_mb(), 16384);
        assert!(t.gpu());
        assert_eq!(t.storage_gb(), 100);
        assert_eq!(t.monthly_cost_cents(), 9999);
    }

    #[test]
    fn test_resource_tier_custom() {
        let t = ResourceTier::Custom;
        assert_eq!(t.cpu_cores(), 0);
        assert_eq!(t.memory_mb(), 0);
        assert!(!t.gpu());
        assert_eq!(t.storage_gb(), 0);
        assert_eq!(t.monthly_cost_cents(), 0);
    }

    // -- Cloud Deployment Manager ---------------------------------------------

    #[test]
    fn test_deploy_agent() {
        let mut mgr = CloudDeploymentManager::new();
        let agent = mgr
            .deploy_agent("a1".into(), "Agent One".into(), CloudRegion::UsEast, ResourceTier::Standard)
            .unwrap();
        assert_eq!(agent.agent_id, "a1");
        assert_eq!(agent.status, CloudAgentStatus::Pending);
        assert_eq!(agent.monthly_cost_cents, 1999);
    }

    #[test]
    fn test_deploy_duplicate_agent() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "Agent".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        assert!(
            mgr.deploy_agent("a1".into(), "Dup".into(), CloudRegion::UsWest, ResourceTier::Free)
                .is_err()
        );
    }

    #[test]
    fn test_stop_running_agent() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        // Transition Pending → Provisioning → Running → Stopped
        mgr.agents.get_mut("a1").unwrap().status = CloudAgentStatus::Running;
        assert!(mgr.stop_agent("a1").is_ok());
        assert_eq!(mgr.get_agent("a1").unwrap().status, CloudAgentStatus::Stopped);
    }

    #[test]
    fn test_stop_pending_agent_fails() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        assert!(mgr.stop_agent("a1").is_err());
    }

    #[test]
    fn test_terminate_stopped_agent() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        mgr.agents.get_mut("a1").unwrap().status = CloudAgentStatus::Stopped;
        assert!(mgr.terminate_agent("a1").is_ok());
        assert_eq!(
            mgr.get_agent("a1").unwrap().status,
            CloudAgentStatus::Terminated
        );
    }

    #[test]
    fn test_terminate_nonexistent_agent() {
        let mut mgr = CloudDeploymentManager::new();
        assert!(mgr.terminate_agent("nope").is_err());
    }

    #[test]
    fn test_stop_nonexistent_agent() {
        let mut mgr = CloudDeploymentManager::new();
        assert!(mgr.stop_agent("nope").is_err());
    }

    #[test]
    fn test_list_agents() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        mgr.deploy_agent("a2".into(), "B".into(), CloudRegion::UsWest, ResourceTier::Standard)
            .unwrap();
        assert_eq!(mgr.list_agents().len(), 2);
    }

    #[test]
    fn test_agents_by_region() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        mgr.deploy_agent("a2".into(), "B".into(), CloudRegion::UsWest, ResourceTier::Free)
            .unwrap();
        mgr.deploy_agent("a3".into(), "C".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        assert_eq!(mgr.agents_by_region(&CloudRegion::UsEast).len(), 2);
        assert_eq!(mgr.agents_by_region(&CloudRegion::UsWest).len(), 1);
        assert_eq!(mgr.agents_by_region(&CloudRegion::EuWest).len(), 0);
    }

    #[test]
    fn test_total_monthly_cost_excludes_terminated() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Standard)
            .unwrap();
        mgr.deploy_agent("a2".into(), "B".into(), CloudRegion::UsEast, ResourceTier::Performance)
            .unwrap();
        mgr.agents.get_mut("a2").unwrap().status = CloudAgentStatus::Terminated;
        assert_eq!(mgr.total_monthly_cost(), 1999);
    }

    #[test]
    fn test_deployment_stats() {
        let mut mgr = CloudDeploymentManager::new();
        mgr.deploy_agent("a1".into(), "A".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        mgr.deploy_agent("a2".into(), "B".into(), CloudRegion::UsEast, ResourceTier::Free)
            .unwrap();
        mgr.agents.get_mut("a1").unwrap().status = CloudAgentStatus::Running;
        mgr.agents.get_mut("a2").unwrap().status = CloudAgentStatus::Stopped;
        let stats = mgr.deployment_stats();
        assert_eq!(stats.total_agents, 2);
        assert_eq!(stats.running, 1);
        assert_eq!(stats.stopped, 1);
    }

    #[test]
    fn test_deployment_stats_empty() {
        let mgr = CloudDeploymentManager::new();
        let stats = mgr.deployment_stats();
        assert_eq!(stats.total_agents, 0);
    }

    // -- Sync Engine ----------------------------------------------------------

    fn make_sync_item(version: u64) -> SyncItem {
        SyncItem {
            item_id: Uuid::new_v4(),
            item_type: SyncItemType::AgentConfig,
            agent_id: Some("agent-1".into()),
            data: json!({"key": "value"}),
            version,
            updated_at: Utc::now(),
            checksum: String::new(),
        }
    }

    #[test]
    fn test_sync_add_item() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(1);
        let id = item.item_id;
        assert!(engine.add_sync_item(item).is_ok());
        assert!(engine.get_item(&id).is_some());
    }

    #[test]
    fn test_sync_add_item_zero_version() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(0);
        assert!(engine.add_sync_item(item).is_err());
    }

    #[test]
    fn test_sync_checksum_computed() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(1);
        let id = item.item_id;
        engine.add_sync_item(item).unwrap();
        let stored = engine.get_item(&id).unwrap();
        assert!(!stored.checksum.is_empty());
    }

    #[test]
    fn test_sync_checksum_deterministic() {
        let data = json!({"hello": "world"});
        assert_eq!(compute_checksum(&data), compute_checksum(&data));
    }

    #[test]
    fn test_sync_items_since() {
        let mut engine = SyncEngine::new();
        let mut i1 = make_sync_item(1);
        i1.item_type = SyncItemType::Preferences;
        let mut i2 = make_sync_item(5);
        i2.item_type = SyncItemType::MemoryStore;
        let mut i3 = make_sync_item(10);
        i3.item_type = SyncItemType::SandboxProfile;
        engine.add_sync_item(i1).unwrap();
        engine.add_sync_item(i2).unwrap();
        engine.add_sync_item(i3).unwrap();
        assert_eq!(engine.items_since(0).len(), 3);
        assert_eq!(engine.items_since(5).len(), 1);
        assert_eq!(engine.items_since(10).len(), 0);
    }

    #[test]
    fn test_sync_detect_conflicts() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(1);
        let id = item.item_id;
        engine.add_sync_item(item).unwrap();

        let remote = SyncItem {
            item_id: id,
            item_type: SyncItemType::AgentConfig,
            agent_id: Some("agent-1".into()),
            data: json!({"key": "different"}),
            version: 2,
            updated_at: Utc::now(),
            checksum: compute_checksum(&json!({"key": "different"})),
        };
        let conflicts = engine.detect_conflicts(&[remote]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].item_id, id);
        assert_eq!(conflicts[0].local_version, 1);
        assert_eq!(conflicts[0].remote_version, 2);
    }

    #[test]
    fn test_sync_no_conflict_same_checksum() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(1);
        let id = item.item_id;
        let data = item.data.clone();
        engine.add_sync_item(item).unwrap();

        let remote = SyncItem {
            item_id: id,
            item_type: SyncItemType::AgentConfig,
            agent_id: Some("agent-1".into()),
            data: data.clone(),
            version: 2,
            updated_at: Utc::now(),
            checksum: compute_checksum(&data),
        };
        let conflicts = engine.detect_conflicts(&[remote]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_sync_no_conflict_unknown_remote() {
        let mut engine = SyncEngine::new();
        let remote = make_sync_item(5);
        let conflicts = engine.detect_conflicts(&[remote]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_sync_resolve_local_wins() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(1);
        let id = item.item_id;
        engine.add_sync_item(item).unwrap();
        let conflict = SyncConflict {
            item_id: id,
            local_version: 1,
            remote_version: 2,
            resolution: ConflictResolution::LocalWins,
        };
        let resolved = engine.resolve_conflict(&conflict).unwrap();
        assert_eq!(resolved.version, 1);
    }

    #[test]
    fn test_sync_resolve_remote_wins() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(1);
        let id = item.item_id;
        engine.add_sync_item(item).unwrap();
        let conflict = SyncConflict {
            item_id: id,
            local_version: 1,
            remote_version: 3,
            resolution: ConflictResolution::RemoteWins,
        };
        let resolved = engine.resolve_conflict(&conflict).unwrap();
        assert_eq!(resolved.version, 3);
    }

    #[test]
    fn test_sync_resolve_merge() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(5);
        let id = item.item_id;
        engine.add_sync_item(item).unwrap();
        let conflict = SyncConflict {
            item_id: id,
            local_version: 5,
            remote_version: 7,
            resolution: ConflictResolution::Merge,
        };
        let resolved = engine.resolve_conflict(&conflict).unwrap();
        assert_eq!(resolved.version, 8); // max(5,7)+1
    }

    #[test]
    fn test_sync_resolve_manual_errors() {
        let mut engine = SyncEngine::new();
        let item = make_sync_item(2);
        let id = item.item_id;
        engine.add_sync_item(item).unwrap();
        let conflict = SyncConflict {
            item_id: id,
            local_version: 2,
            remote_version: 4,
            resolution: ConflictResolution::Manual,
        };
        let result = engine.resolve_conflict(&conflict);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("manual conflict resolution requires user input")
        );
    }

    #[test]
    fn test_sync_resolve_missing_item() {
        let mut engine = SyncEngine::new();
        let conflict = SyncConflict {
            item_id: Uuid::new_v4(),
            local_version: 1,
            remote_version: 2,
            resolution: ConflictResolution::LocalWins,
        };
        assert!(engine.resolve_conflict(&conflict).is_err());
    }

    #[test]
    fn test_sync_stats() {
        let mut engine = SyncEngine::new();
        engine.add_sync_item(make_sync_item(1)).unwrap();
        engine.add_sync_item(make_sync_item(2)).unwrap();
        let stats = engine.sync_stats();
        assert_eq!(stats.total_items, 2);
        assert_eq!(stats.pending_push, 2);
    }

    #[test]
    fn test_sync_stats_empty() {
        let engine = SyncEngine::new();
        let stats = engine.sync_stats();
        assert_eq!(stats.total_items, 0);
        assert_eq!(stats.conflicts, 0);
        assert!(stats.last_sync.is_none());
    }

    #[test]
    fn test_sync_mark_synced() {
        let mut engine = SyncEngine::new();
        engine.add_sync_item(make_sync_item(1)).unwrap();
        engine.add_sync_item(make_sync_item(2)).unwrap();
        assert_eq!(engine.sync_stats().pending_push, 2);
        assert!(engine.sync_stats().last_sync.is_none());
        engine.mark_synced();
        assert_eq!(engine.sync_stats().pending_push, 0);
        assert!(engine.sync_stats().last_sync.is_some());
    }

    #[test]
    fn test_sync_mark_pulled() {
        let mut engine = SyncEngine::new();
        engine.pending_pull = 5;
        engine.mark_pulled(3);
        assert_eq!(engine.sync_stats().pending_pull, 2);
        assert!(engine.sync_stats().last_sync.is_some());
        engine.mark_pulled(10); // saturating
        assert_eq!(engine.sync_stats().pending_pull, 0);
    }

    #[test]
    fn test_sync_item_types() {
        assert_eq!(SyncItemType::AgentConfig, SyncItemType::AgentConfig);
        assert_ne!(SyncItemType::AgentConfig, SyncItemType::CustomData);
        assert_eq!(SyncItemType::MemoryStore, SyncItemType::MemoryStore);
        assert_eq!(SyncItemType::Preferences, SyncItemType::Preferences);
        assert_eq!(SyncItemType::SandboxProfile, SyncItemType::SandboxProfile);
    }

    // -- Workspace Manager ----------------------------------------------------

    #[test]
    fn test_create_workspace() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("Team Alpha".into(), "user-1".into()).unwrap();
        assert_eq!(ws.name, "Team Alpha");
        assert_eq!(ws.owner, "user-1");
        assert_eq!(ws.members.len(), 1);
        assert_eq!(ws.members[0].role, WorkspaceRole::Owner);
    }

    #[test]
    fn test_create_workspace_empty_name() {
        let mut mgr = WorkspaceManager::new();
        assert!(mgr.create_workspace("".into(), "u1".into()).is_err());
    }

    #[test]
    fn test_create_workspace_empty_owner() {
        let mut mgr = WorkspaceManager::new();
        assert!(mgr.create_workspace("ws".into(), "".into()).is_err());
    }

    #[test]
    fn test_get_workspace() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        assert!(mgr.get_workspace(&ws.workspace_id).is_some());
        assert!(mgr.get_workspace(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_add_member() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        let member = WorkspaceMember {
            user_id: "u2".into(),
            role: WorkspaceRole::Editor,
            joined_at: Utc::now(),
        };
        assert!(mgr.add_member(&ws.workspace_id, member).is_ok());
        let ws2 = mgr.get_workspace(&ws.workspace_id).unwrap();
        assert_eq!(ws2.members.len(), 2);
    }

    #[test]
    fn test_add_duplicate_member() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        let member = WorkspaceMember {
            user_id: "u1".into(),
            role: WorkspaceRole::Editor,
            joined_at: Utc::now(),
        };
        assert!(mgr.add_member(&ws.workspace_id, member).is_err());
    }

    #[test]
    fn test_add_member_max_limit() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "owner".into()).unwrap();
        // Set max_members to 2 (owner already occupies one slot)
        mgr.workspaces.get_mut(&ws.workspace_id).unwrap().settings.max_members = 2;
        let m1 = WorkspaceMember {
            user_id: "u2".into(),
            role: WorkspaceRole::Viewer,
            joined_at: Utc::now(),
        };
        assert!(mgr.add_member(&ws.workspace_id, m1).is_ok());
        let m2 = WorkspaceMember {
            user_id: "u3".into(),
            role: WorkspaceRole::Viewer,
            joined_at: Utc::now(),
        };
        assert!(mgr.add_member(&ws.workspace_id, m2).is_err());
    }

    #[test]
    fn test_add_member_nonexistent_workspace() {
        let mut mgr = WorkspaceManager::new();
        let member = WorkspaceMember {
            user_id: "u1".into(),
            role: WorkspaceRole::Viewer,
            joined_at: Utc::now(),
        };
        assert!(mgr.add_member(&Uuid::new_v4(), member).is_err());
    }

    #[test]
    fn test_remove_member() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        let member = WorkspaceMember {
            user_id: "u2".into(),
            role: WorkspaceRole::Editor,
            joined_at: Utc::now(),
        };
        mgr.add_member(&ws.workspace_id, member).unwrap();
        assert!(mgr.remove_member(&ws.workspace_id, "u2").is_ok());
        assert_eq!(mgr.get_workspace(&ws.workspace_id).unwrap().members.len(), 1);
    }

    #[test]
    fn test_remove_owner_fails() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        assert!(mgr.remove_member(&ws.workspace_id, "u1").is_err());
    }

    #[test]
    fn test_remove_nonexistent_member() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        assert!(mgr.remove_member(&ws.workspace_id, "ghost").is_err());
    }

    #[test]
    fn test_update_role() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        let member = WorkspaceMember {
            user_id: "u2".into(),
            role: WorkspaceRole::Viewer,
            joined_at: Utc::now(),
        };
        mgr.add_member(&ws.workspace_id, member).unwrap();
        assert!(mgr.update_role(&ws.workspace_id, "u2", WorkspaceRole::Admin).is_ok());
        let ws2 = mgr.get_workspace(&ws.workspace_id).unwrap();
        let m = ws2.members.iter().find(|m| m.user_id == "u2").unwrap();
        assert_eq!(m.role, WorkspaceRole::Admin);
    }

    #[test]
    fn test_update_owner_role_fails() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        assert!(mgr.update_role(&ws.workspace_id, "u1", WorkspaceRole::Viewer).is_err());
    }

    #[test]
    fn test_update_role_nonexistent_user() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        assert!(mgr.update_role(&ws.workspace_id, "ghost", WorkspaceRole::Admin).is_err());
    }

    #[test]
    fn test_list_workspaces_for_user() {
        let mut mgr = WorkspaceManager::new();
        mgr.create_workspace("W1".into(), "u1".into()).unwrap();
        let ws2 = mgr.create_workspace("W2".into(), "u2".into()).unwrap();
        let member = WorkspaceMember {
            user_id: "u1".into(),
            role: WorkspaceRole::Viewer,
            joined_at: Utc::now(),
        };
        mgr.add_member(&ws2.workspace_id, member).unwrap();
        assert_eq!(mgr.list_workspaces_for_user("u1").len(), 2);
        assert_eq!(mgr.list_workspaces_for_user("u2").len(), 1);
        assert_eq!(mgr.list_workspaces_for_user("u3").len(), 0);
    }

    #[test]
    fn test_workspace_stats() {
        let mut mgr = WorkspaceManager::new();
        let ws = mgr.create_workspace("WS".into(), "u1".into()).unwrap();
        let stats = mgr.workspace_stats(&ws.workspace_id).unwrap();
        assert_eq!(stats.member_count, 1);
    }

    #[test]
    fn test_workspace_stats_nonexistent() {
        let mgr = WorkspaceManager::new();
        assert!(mgr.workspace_stats(&Uuid::new_v4()).is_none());
    }

    // -- Workspace Role Permissions -------------------------------------------

    #[test]
    fn test_role_permissions_owner() {
        let r = WorkspaceRole::Owner;
        assert!(r.can_deploy_agents());
        assert!(r.can_manage_members());
        assert!(r.can_view());
    }

    #[test]
    fn test_role_permissions_admin() {
        let r = WorkspaceRole::Admin;
        assert!(r.can_deploy_agents());
        assert!(r.can_manage_members());
        assert!(r.can_view());
    }

    #[test]
    fn test_role_permissions_editor() {
        let r = WorkspaceRole::Editor;
        assert!(r.can_deploy_agents());
        assert!(!r.can_manage_members());
        assert!(r.can_view());
    }

    #[test]
    fn test_role_permissions_viewer() {
        let r = WorkspaceRole::Viewer;
        assert!(!r.can_deploy_agents());
        assert!(!r.can_manage_members());
        assert!(r.can_view());
    }

    // -- Billing Tracker ------------------------------------------------------

    fn make_usage(usage_type: UsageType, cost: u64) -> UsageRecord {
        UsageRecord {
            record_id: Uuid::new_v4(),
            workspace_id: None,
            agent_id: None,
            usage_type,
            quantity: 1.0,
            unit: "unit".into(),
            cost_cents: cost,
            recorded_at: Utc::now(),
        }
    }

    #[test]
    fn test_billing_add_usage() {
        let mut tracker = BillingTracker::new();
        tracker.add_usage(make_usage(UsageType::Compute, 100));
        assert_eq!(tracker.total_cost_cents(), 100);
    }

    #[test]
    fn test_billing_total_cost() {
        let mut tracker = BillingTracker::new();
        tracker.add_usage(make_usage(UsageType::Compute, 100));
        tracker.add_usage(make_usage(UsageType::Storage, 200));
        tracker.add_usage(make_usage(UsageType::NetworkEgress, 50));
        assert_eq!(tracker.total_cost_cents(), 350);
    }

    #[test]
    fn test_billing_usage_for_workspace() {
        let mut tracker = BillingTracker::new();
        let ws_id = Uuid::new_v4();
        let mut record = make_usage(UsageType::Compute, 100);
        record.workspace_id = Some(ws_id);
        tracker.add_usage(record);
        tracker.add_usage(make_usage(UsageType::Storage, 50));
        assert_eq!(tracker.usage_for_workspace(&ws_id).len(), 1);
    }

    #[test]
    fn test_billing_usage_for_agent() {
        let mut tracker = BillingTracker::new();
        let mut record = make_usage(UsageType::Compute, 100);
        record.agent_id = Some("agent-1".into());
        tracker.add_usage(record);
        tracker.add_usage(make_usage(UsageType::Storage, 50));
        assert_eq!(tracker.usage_for_agent("agent-1").len(), 1);
        assert_eq!(tracker.usage_for_agent("agent-2").len(), 0);
    }

    #[test]
    fn test_billing_usage_summary() {
        let mut tracker = BillingTracker::new();
        tracker.add_usage(make_usage(UsageType::Compute, 100));
        tracker.add_usage(make_usage(UsageType::Compute, 200));
        tracker.add_usage(make_usage(UsageType::Storage, 50));
        tracker.add_usage(make_usage(UsageType::SyncOperations, 10));
        let summary = tracker.usage_summary();
        assert_eq!(*summary.get(&UsageType::Compute).unwrap(), 300);
        assert_eq!(*summary.get(&UsageType::Storage).unwrap(), 50);
        assert_eq!(*summary.get(&UsageType::SyncOperations).unwrap(), 10);
    }

    #[test]
    fn test_billing_empty() {
        let tracker = BillingTracker::new();
        assert_eq!(tracker.total_cost_cents(), 0);
        assert!(tracker.usage_summary().is_empty());
    }

    // -- Serialisation round-trips -------------------------------------------

    #[test]
    fn test_cloud_config_serialization() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: CloudConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.endpoint_url, cfg.endpoint_url);
    }

    #[test]
    fn test_cloud_config_api_key_not_serialized() {
        let mut cfg = CloudConfig::default();
        cfg.endpoint_url = "https://cloud.agnos.dev".into();
        cfg.api_key = Some("super-secret-key".into());
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("super-secret-key"));
        assert!(!json.contains("api_key"));
    }

    #[test]
    fn test_sync_item_serialization() {
        let item = make_sync_item(3);
        let json = serde_json::to_string(&item).unwrap();
        let deserialized: SyncItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.item_id, item.item_id);
        assert_eq!(deserialized.version, 3);
    }

    #[test]
    fn test_workspace_serialization() {
        let ws = Workspace {
            workspace_id: Uuid::new_v4(),
            name: "Test".into(),
            owner: "u1".into(),
            members: vec![],
            created_at: Utc::now(),
            settings: WorkspaceSettings::default(),
        };
        let json = serde_json::to_string(&ws).unwrap();
        let deserialized: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.workspace_id, ws.workspace_id);
    }

    // -- Edge cases -----------------------------------------------------------

    #[test]
    fn test_empty_workspace_manager() {
        let mgr = WorkspaceManager::new();
        assert!(mgr.list_workspaces_for_user("anyone").is_empty());
    }

    #[test]
    fn test_deploy_free_agent_zero_cost() {
        let mut mgr = CloudDeploymentManager::new();
        let agent = mgr
            .deploy_agent("free1".into(), "Free".into(), CloudRegion::AsiaPacific, ResourceTier::Free)
            .unwrap();
        assert_eq!(agent.monthly_cost_cents, 0);
    }

    #[test]
    fn test_all_regions_display() {
        let regions = vec![
            CloudRegion::UsEast,
            CloudRegion::UsWest,
            CloudRegion::EuWest,
            CloudRegion::EuCentral,
            CloudRegion::AsiaPacific,
            CloudRegion::Custom("custom-1".into()),
        ];
        for r in &regions {
            assert!(!r.to_string().is_empty());
        }
        assert_eq!(regions.len(), 6);
    }

    #[test]
    fn test_sync_direction_equality() {
        assert_eq!(SyncDirection::Push, SyncDirection::Push);
        assert_ne!(SyncDirection::Push, SyncDirection::Pull);
        assert_eq!(SyncDirection::Bidirectional, SyncDirection::Bidirectional);
    }
}
