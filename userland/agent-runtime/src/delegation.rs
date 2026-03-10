//! Cross-Project Agent Delegation
//!
//! External orchestrators can delegate tasks to AGNOS agents via the A2A
//! (Agent-to-Agent) protocol. This module handles delegation policies,
//! capability routing, sandboxed execution, and audit trails.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Delegation Request / Response
// ---------------------------------------------------------------------------

/// A delegation request from an external orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationRequest {
    /// Unique request ID.
    pub request_id: String,
    /// External orchestrator identifier.
    pub orchestrator_id: String,
    /// Requested capability (e.g., "code-review", "file-analysis").
    pub capability: String,
    /// Task payload (opaque JSON).
    pub payload: serde_json::Value,
    /// Required sandbox level.
    pub sandbox_level: SandboxLevel,
    /// Maximum execution time in seconds.
    pub timeout_secs: u64,
    /// Callback URL for async result delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// Authentication token from the orchestrator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    /// Priority (0 = lowest, 10 = highest).
    #[serde(default)]
    pub priority: u8,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Sandbox level for delegated tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxLevel {
    /// Minimal sandbox — Landlock only.
    Minimal,
    /// Standard sandbox — Landlock + seccomp.
    Standard,
    /// Strict sandbox — Landlock + seccomp + network isolation.
    Strict,
    /// Maximum sandbox — full isolation including encrypted storage.
    Maximum,
}

impl Default for SandboxLevel {
    fn default() -> Self {
        Self::Standard
    }
}

/// Result of a delegation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResponse {
    /// Original request ID.
    pub request_id: String,
    /// Delegation ticket ID (for tracking).
    pub delegation_id: String,
    /// Status of the delegation.
    pub status: DelegationStatus,
    /// Agent that accepted the task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
    /// Result payload (when completed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error message (when failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Execution duration in milliseconds.
    #[serde(default)]
    pub execution_ms: u64,
}

/// Status of a delegated task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationStatus {
    /// Request accepted, queued for execution.
    Accepted,
    /// Task is being executed by an agent.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed.
    Failed,
    /// Task was rejected (policy violation).
    Rejected,
    /// Task timed out.
    TimedOut,
    /// Task was cancelled.
    Cancelled,
}

impl std::fmt::Display for DelegationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted => write!(f, "accepted"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Rejected => write!(f, "rejected"),
            Self::TimedOut => write!(f, "timed_out"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

// ---------------------------------------------------------------------------
// Delegation Policy
// ---------------------------------------------------------------------------

/// Policy for accepting or rejecting delegation requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPolicy {
    /// Whether delegation is enabled.
    pub enabled: bool,
    /// Allowed orchestrator IDs (empty = allow all authenticated).
    pub allowed_orchestrators: Vec<String>,
    /// Capabilities that can be delegated.
    pub allowed_capabilities: Vec<String>,
    /// Maximum concurrent delegated tasks.
    pub max_concurrent: usize,
    /// Maximum payload size in bytes.
    pub max_payload_bytes: usize,
    /// Minimum sandbox level for delegated tasks.
    pub min_sandbox_level: SandboxLevel,
    /// Whether to require authentication tokens.
    pub require_auth: bool,
    /// Rate limit: max requests per minute per orchestrator.
    pub rate_limit_per_minute: u32,
}

impl Default for DelegationPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_orchestrators: vec![],
            allowed_capabilities: vec![
                "code-review".to_string(),
                "file-analysis".to_string(),
                "security-scan".to_string(),
                "data-transform".to_string(),
                "inference".to_string(),
            ],
            max_concurrent: 10,
            max_payload_bytes: 10 * 1024 * 1024, // 10 MB
            min_sandbox_level: SandboxLevel::Standard,
            require_auth: true,
            rate_limit_per_minute: 60,
        }
    }
}

impl DelegationPolicy {
    /// Check if a request is allowed by this policy.
    pub fn evaluate(&self, request: &DelegationRequest) -> Result<(), PolicyViolation> {
        if !self.enabled {
            return Err(PolicyViolation::DelegationDisabled);
        }

        // Check orchestrator allow list.
        if !self.allowed_orchestrators.is_empty()
            && !self
                .allowed_orchestrators
                .contains(&request.orchestrator_id)
        {
            return Err(PolicyViolation::OrchestratorNotAllowed(
                request.orchestrator_id.clone(),
            ));
        }

        // Check capability allow list.
        if !self.allowed_capabilities.is_empty()
            && !self.allowed_capabilities.contains(&request.capability)
        {
            return Err(PolicyViolation::CapabilityNotAllowed(
                request.capability.clone(),
            ));
        }

        // Check sandbox level.
        if (request.sandbox_level as u8) < (self.min_sandbox_level as u8) {
            return Err(PolicyViolation::InsufficientSandbox {
                requested: request.sandbox_level,
                minimum: self.min_sandbox_level,
            });
        }

        // Check auth requirement.
        if self.require_auth && request.auth_token.is_none() {
            return Err(PolicyViolation::AuthenticationRequired);
        }

        // Check payload size.
        let payload_size = serde_json::to_vec(&request.payload)
            .map(|v| v.len())
            .unwrap_or(0);
        if payload_size > self.max_payload_bytes {
            return Err(PolicyViolation::PayloadTooLarge {
                size: payload_size,
                limit: self.max_payload_bytes,
            });
        }

        Ok(())
    }
}

/// Reason a delegation was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyViolation {
    DelegationDisabled,
    OrchestratorNotAllowed(String),
    CapabilityNotAllowed(String),
    InsufficientSandbox {
        requested: SandboxLevel,
        minimum: SandboxLevel,
    },
    AuthenticationRequired,
    PayloadTooLarge {
        size: usize,
        limit: usize,
    },
    RateLimitExceeded,
    MaxConcurrentExceeded,
}

impl std::fmt::Display for PolicyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DelegationDisabled => write!(f, "delegation is disabled"),
            Self::OrchestratorNotAllowed(id) => write!(f, "orchestrator not allowed: {}", id),
            Self::CapabilityNotAllowed(c) => write!(f, "capability not delegatable: {}", c),
            Self::InsufficientSandbox { .. } => write!(f, "sandbox level too low"),
            Self::AuthenticationRequired => write!(f, "authentication required"),
            Self::PayloadTooLarge { size, limit } => {
                write!(f, "payload {} bytes exceeds limit {}", size, limit)
            }
            Self::RateLimitExceeded => write!(f, "rate limit exceeded"),
            Self::MaxConcurrentExceeded => write!(f, "max concurrent delegations exceeded"),
        }
    }
}

impl std::error::Error for PolicyViolation {}

// ---------------------------------------------------------------------------
// Delegation Manager
// ---------------------------------------------------------------------------

/// Manages delegation lifecycle: accept, route, execute, respond.
#[derive(Debug, Clone)]
pub struct DelegationManager {
    policy: DelegationPolicy,
    /// Active delegations: delegation_id → record.
    active: HashMap<String, DelegationRecord>,
    /// Completed delegations (ring buffer for audit).
    completed: Vec<DelegationRecord>,
    /// Maximum completed records to retain.
    max_completed: usize,
    /// Capability → agent routing table.
    capability_routes: HashMap<String, Vec<AgentRoute>>,
}

/// A delegation execution record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationRecord {
    pub delegation_id: String,
    pub request: DelegationRequest,
    pub status: DelegationStatus,
    pub assigned_agent: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

/// Routing entry: which agent handles which capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoute {
    /// Agent ID.
    pub agent_id: String,
    /// Agent name.
    pub agent_name: String,
    /// Capability it provides.
    pub capability: String,
    /// Priority (higher = preferred).
    pub priority: u8,
    /// Current load (active tasks).
    pub active_tasks: u32,
    /// Maximum concurrent tasks this agent accepts.
    pub max_tasks: u32,
}

impl AgentRoute {
    /// Whether the agent can accept more tasks.
    pub fn has_capacity(&self) -> bool {
        self.active_tasks < self.max_tasks
    }
}

impl DelegationManager {
    /// Create a new delegation manager with the given policy.
    pub fn new(policy: DelegationPolicy) -> Self {
        info!("Delegation manager initialised");
        Self {
            policy,
            active: HashMap::new(),
            completed: Vec::new(),
            max_completed: 1000,
            capability_routes: HashMap::new(),
        }
    }

    /// Get the delegation policy.
    pub fn policy(&self) -> &DelegationPolicy {
        &self.policy
    }

    /// Register an agent as a handler for a capability.
    pub fn register_route(&mut self, route: AgentRoute) {
        info!(
            agent = %route.agent_id,
            capability = %route.capability,
            "Registered delegation route"
        );
        self.capability_routes
            .entry(route.capability.clone())
            .or_default()
            .push(route);
    }

    /// Remove all routes for an agent.
    pub fn remove_agent_routes(&mut self, agent_id: &str) {
        for routes in self.capability_routes.values_mut() {
            routes.retain(|r| r.agent_id != agent_id);
        }
    }

    /// List capabilities that have registered handlers.
    pub fn available_capabilities(&self) -> Vec<String> {
        let mut caps: Vec<_> = self
            .capability_routes
            .iter()
            .filter(|(_, routes)| routes.iter().any(|r| r.has_capacity()))
            .map(|(cap, _)| cap.clone())
            .collect();
        caps.sort();
        caps
    }

    /// Select the best agent for a capability.
    fn select_agent(&self, capability: &str) -> Option<&AgentRoute> {
        self.capability_routes
            .get(capability)?
            .iter()
            .filter(|r| r.has_capacity())
            .max_by_key(|r| (r.priority, r.max_tasks - r.active_tasks))
    }

    /// Submit a delegation request. Returns a response (accepted or rejected).
    pub fn submit(&mut self, request: DelegationRequest) -> DelegationResponse {
        // Evaluate policy.
        if let Err(violation) = self.policy.evaluate(&request) {
            warn!(
                request_id = %request.request_id,
                orchestrator = %request.orchestrator_id,
                reason = %violation,
                "Delegation rejected"
            );
            return DelegationResponse {
                request_id: request.request_id.clone(),
                delegation_id: String::new(),
                status: DelegationStatus::Rejected,
                assigned_agent: None,
                result: None,
                error: Some(violation.to_string()),
                execution_ms: 0,
            };
        }

        // Check concurrent limit.
        if self.active.len() >= self.policy.max_concurrent {
            return DelegationResponse {
                request_id: request.request_id.clone(),
                delegation_id: String::new(),
                status: DelegationStatus::Rejected,
                assigned_agent: None,
                result: None,
                error: Some("max concurrent delegations exceeded".to_string()),
                execution_ms: 0,
            };
        }

        // Find an agent.
        let agent = self.select_agent(&request.capability).cloned();

        let delegation_id = Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let record = DelegationRecord {
            delegation_id: delegation_id.clone(),
            request: request.clone(),
            status: if agent.is_some() {
                DelegationStatus::Running
            } else {
                DelegationStatus::Accepted
            },
            assigned_agent: agent.as_ref().map(|a| a.agent_id.clone()),
            result: None,
            error: None,
            created_at: now,
            completed_at: None,
        };

        self.active.insert(delegation_id.clone(), record);

        info!(
            delegation_id = %delegation_id,
            capability = %request.capability,
            agent = ?agent.as_ref().map(|a| &a.agent_id),
            "Delegation accepted"
        );

        DelegationResponse {
            request_id: request.request_id,
            delegation_id,
            status: if agent.is_some() {
                DelegationStatus::Running
            } else {
                DelegationStatus::Accepted
            },
            assigned_agent: agent.map(|a| a.agent_id),
            result: None,
            error: None,
            execution_ms: 0,
        }
    }

    /// Complete a delegation with a result.
    pub fn complete(
        &mut self,
        delegation_id: &str,
        result: serde_json::Value,
    ) -> Option<DelegationResponse> {
        let mut record = self.active.remove(delegation_id)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        record.status = DelegationStatus::Completed;
        record.result = Some(result.clone());
        record.completed_at = Some(now);

        let execution_ms = (now - record.created_at) * 1000;

        let response = DelegationResponse {
            request_id: record.request.request_id.clone(),
            delegation_id: delegation_id.to_string(),
            status: DelegationStatus::Completed,
            assigned_agent: record.assigned_agent.clone(),
            result: Some(result),
            error: None,
            execution_ms,
        };

        // Archive.
        self.completed.push(record);
        if self.completed.len() > self.max_completed {
            self.completed.remove(0);
        }

        Some(response)
    }

    /// Fail a delegation with an error message.
    pub fn fail(&mut self, delegation_id: &str, error: &str) -> Option<DelegationResponse> {
        let mut record = self.active.remove(delegation_id)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        record.status = DelegationStatus::Failed;
        record.error = Some(error.to_string());
        record.completed_at = Some(now);

        let response = DelegationResponse {
            request_id: record.request.request_id.clone(),
            delegation_id: delegation_id.to_string(),
            status: DelegationStatus::Failed,
            assigned_agent: record.assigned_agent.clone(),
            result: None,
            error: Some(error.to_string()),
            execution_ms: (now - record.created_at) * 1000,
        };

        self.completed.push(record);
        if self.completed.len() > self.max_completed {
            self.completed.remove(0);
        }

        Some(response)
    }

    /// Cancel an active delegation.
    pub fn cancel(&mut self, delegation_id: &str) -> Option<DelegationResponse> {
        let mut record = self.active.remove(delegation_id)?;
        record.status = DelegationStatus::Cancelled;
        record.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );

        let response = DelegationResponse {
            request_id: record.request.request_id.clone(),
            delegation_id: delegation_id.to_string(),
            status: DelegationStatus::Cancelled,
            assigned_agent: record.assigned_agent.clone(),
            result: None,
            error: None,
            execution_ms: 0,
        };

        self.completed.push(record);
        Some(response)
    }

    /// Get a delegation by ID.
    pub fn get(&self, delegation_id: &str) -> Option<&DelegationRecord> {
        self.active.get(delegation_id)
    }

    /// Number of active delegations.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Number of completed delegations.
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }

    /// Get delegation statistics.
    pub fn stats(&self) -> DelegationStats {
        let completed_count = self.completed.len();
        let succeeded = self
            .completed
            .iter()
            .filter(|r| r.status == DelegationStatus::Completed)
            .count();
        let failed = self
            .completed
            .iter()
            .filter(|r| r.status == DelegationStatus::Failed)
            .count();

        DelegationStats {
            active: self.active.len(),
            completed: completed_count,
            succeeded,
            failed,
            rejected: self
                .completed
                .iter()
                .filter(|r| r.status == DelegationStatus::Rejected)
                .count(),
            available_capabilities: self.available_capabilities().len(),
            registered_routes: self.capability_routes.values().map(|v| v.len()).sum(),
        }
    }
}

/// Delegation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationStats {
    pub active: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub rejected: usize,
    pub available_capabilities: usize,
    pub registered_routes: usize,
}

// ---------------------------------------------------------------------------
// A2A Protocol Types
// ---------------------------------------------------------------------------

/// A2A protocol envelope for inter-service delegation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AEnvelope {
    /// Protocol version.
    pub version: String,
    /// Message type.
    pub message_type: A2AMessageType,
    /// Source service.
    pub source: String,
    /// Destination service.
    pub destination: String,
    /// Correlation ID for request/response matching.
    pub correlation_id: String,
    /// Payload.
    pub payload: serde_json::Value,
}

/// A2A message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum A2AMessageType {
    /// Capability discovery.
    Discover,
    /// Delegation request.
    Delegate,
    /// Delegation result.
    Result,
    /// Heartbeat/keepalive.
    Heartbeat,
    /// Cancellation.
    Cancel,
}

impl A2AEnvelope {
    /// Create a delegation request envelope.
    pub fn delegation(source: &str, dest: &str, request: &DelegationRequest) -> Self {
        Self {
            version: "1.0".to_string(),
            message_type: A2AMessageType::Delegate,
            source: source.to_string(),
            destination: dest.to_string(),
            correlation_id: request.request_id.clone(),
            payload: serde_json::to_value(request).unwrap_or_default(),
        }
    }

    /// Create a discovery request envelope.
    pub fn discover(source: &str, dest: &str) -> Self {
        Self {
            version: "1.0".to_string(),
            message_type: A2AMessageType::Discover,
            source: source.to_string(),
            destination: dest.to_string(),
            correlation_id: Uuid::new_v4().to_string(),
            payload: serde_json::json!({}),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request() -> DelegationRequest {
        DelegationRequest {
            request_id: "req-001".to_string(),
            orchestrator_id: "external-orch".to_string(),
            capability: "code-review".to_string(),
            payload: serde_json::json!({"file": "main.rs"}),
            sandbox_level: SandboxLevel::Standard,
            timeout_secs: 60,
            callback_url: None,
            auth_token: Some("token-abc".to_string()),
            priority: 5,
            metadata: HashMap::new(),
        }
    }

    fn test_manager() -> DelegationManager {
        let mut manager = DelegationManager::new(DelegationPolicy::default());
        manager.register_route(AgentRoute {
            agent_id: "agent-1".to_string(),
            agent_name: "code-reviewer".to_string(),
            capability: "code-review".to_string(),
            priority: 5,
            active_tasks: 0,
            max_tasks: 10,
        });
        manager
    }

    #[test]
    fn test_sandbox_level_default() {
        assert_eq!(SandboxLevel::default(), SandboxLevel::Standard);
    }

    #[test]
    fn test_delegation_status_display() {
        assert_eq!(DelegationStatus::Accepted.to_string(), "accepted");
        assert_eq!(DelegationStatus::Running.to_string(), "running");
        assert_eq!(DelegationStatus::Completed.to_string(), "completed");
        assert_eq!(DelegationStatus::TimedOut.to_string(), "timed_out");
    }

    #[test]
    fn test_policy_default() {
        let policy = DelegationPolicy::default();
        assert!(policy.enabled);
        assert!(policy.require_auth);
        assert_eq!(policy.max_concurrent, 10);
        assert!(policy.allowed_capabilities.contains(&"code-review".to_string()));
    }

    #[test]
    fn test_policy_evaluate_success() {
        let policy = DelegationPolicy::default();
        let request = test_request();
        assert!(policy.evaluate(&request).is_ok());
    }

    #[test]
    fn test_policy_evaluate_disabled() {
        let policy = DelegationPolicy {
            enabled: false,
            ..Default::default()
        };
        let request = test_request();
        assert_eq!(
            policy.evaluate(&request).unwrap_err(),
            PolicyViolation::DelegationDisabled
        );
    }

    #[test]
    fn test_policy_evaluate_orchestrator_not_allowed() {
        let policy = DelegationPolicy {
            allowed_orchestrators: vec!["allowed-only".to_string()],
            ..Default::default()
        };
        let request = test_request();
        assert!(matches!(
            policy.evaluate(&request).unwrap_err(),
            PolicyViolation::OrchestratorNotAllowed(_)
        ));
    }

    #[test]
    fn test_policy_evaluate_capability_not_allowed() {
        let policy = DelegationPolicy {
            allowed_capabilities: vec!["only-this".to_string()],
            ..Default::default()
        };
        let request = test_request();
        assert!(matches!(
            policy.evaluate(&request).unwrap_err(),
            PolicyViolation::CapabilityNotAllowed(_)
        ));
    }

    #[test]
    fn test_policy_evaluate_auth_required() {
        let policy = DelegationPolicy::default();
        let mut request = test_request();
        request.auth_token = None;
        assert_eq!(
            policy.evaluate(&request).unwrap_err(),
            PolicyViolation::AuthenticationRequired
        );
    }

    #[test]
    fn test_policy_evaluate_insufficient_sandbox() {
        let policy = DelegationPolicy {
            min_sandbox_level: SandboxLevel::Strict,
            ..Default::default()
        };
        let request = test_request(); // Standard < Strict
        assert!(matches!(
            policy.evaluate(&request).unwrap_err(),
            PolicyViolation::InsufficientSandbox { .. }
        ));
    }

    #[test]
    fn test_register_route() {
        let manager = test_manager();
        assert_eq!(manager.available_capabilities(), vec!["code-review"]);
    }

    #[test]
    fn test_submit_delegation() {
        let mut manager = test_manager();
        let request = test_request();
        let response = manager.submit(request);
        assert_eq!(response.status, DelegationStatus::Running);
        assert_eq!(response.assigned_agent, Some("agent-1".to_string()));
        assert!(!response.delegation_id.is_empty());
        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn test_submit_delegation_rejected_no_auth() {
        let mut manager = test_manager();
        let mut request = test_request();
        request.auth_token = None;
        let response = manager.submit(request);
        assert_eq!(response.status, DelegationStatus::Rejected);
        assert!(response.error.is_some());
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_submit_delegation_no_agent() {
        let mut manager = DelegationManager::new(DelegationPolicy::default());
        let request = test_request();
        let response = manager.submit(request);
        // Accepted but no agent assigned yet
        assert_eq!(response.status, DelegationStatus::Accepted);
        assert!(response.assigned_agent.is_none());
    }

    #[test]
    fn test_complete_delegation() {
        let mut manager = test_manager();
        let request = test_request();
        let response = manager.submit(request);
        let delegation_id = response.delegation_id;

        let result = serde_json::json!({"review": "LGTM"});
        let completed = manager.complete(&delegation_id, result.clone()).unwrap();
        assert_eq!(completed.status, DelegationStatus::Completed);
        assert_eq!(completed.result, Some(result));
        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.completed_count(), 1);
    }

    #[test]
    fn test_fail_delegation() {
        let mut manager = test_manager();
        let request = test_request();
        let response = manager.submit(request);
        let delegation_id = response.delegation_id;

        let failed = manager.fail(&delegation_id, "agent crashed").unwrap();
        assert_eq!(failed.status, DelegationStatus::Failed);
        assert_eq!(failed.error, Some("agent crashed".to_string()));
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_cancel_delegation() {
        let mut manager = test_manager();
        let request = test_request();
        let response = manager.submit(request);
        let delegation_id = response.delegation_id;

        let cancelled = manager.cancel(&delegation_id).unwrap();
        assert_eq!(cancelled.status, DelegationStatus::Cancelled);
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_complete_nonexistent() {
        let mut manager = test_manager();
        assert!(manager
            .complete("nonexistent", serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn test_stats() {
        let mut manager = test_manager();

        // Submit and complete one
        let r1 = test_request();
        let resp1 = manager.submit(r1);
        manager.complete(&resp1.delegation_id, serde_json::json!({}));

        // Submit and fail one
        let mut r2 = test_request();
        r2.request_id = "req-002".to_string();
        let resp2 = manager.submit(r2);
        manager.fail(&resp2.delegation_id, "error");

        let stats = manager.stats();
        assert_eq!(stats.active, 0);
        assert_eq!(stats.completed, 2);
        assert_eq!(stats.succeeded, 1);
        assert_eq!(stats.failed, 1);
    }

    #[test]
    fn test_remove_agent_routes() {
        let mut manager = test_manager();
        manager.register_route(AgentRoute {
            agent_id: "agent-2".to_string(),
            agent_name: "scanner".to_string(),
            capability: "security-scan".to_string(),
            priority: 3,
            active_tasks: 0,
            max_tasks: 5,
        });
        assert_eq!(manager.available_capabilities().len(), 2);

        manager.remove_agent_routes("agent-1");
        assert_eq!(manager.available_capabilities(), vec!["security-scan"]);
    }

    #[test]
    fn test_agent_route_capacity() {
        let route = AgentRoute {
            agent_id: "a".to_string(),
            agent_name: "a".to_string(),
            capability: "x".to_string(),
            priority: 1,
            active_tasks: 5,
            max_tasks: 5,
        };
        assert!(!route.has_capacity());

        let route2 = AgentRoute {
            active_tasks: 4,
            ..route
        };
        assert!(route2.has_capacity());
    }

    #[test]
    fn test_max_concurrent_exceeded() {
        let policy = DelegationPolicy {
            max_concurrent: 1,
            ..Default::default()
        };
        let mut manager = DelegationManager::new(policy);
        manager.register_route(AgentRoute {
            agent_id: "a".to_string(),
            agent_name: "a".to_string(),
            capability: "code-review".to_string(),
            priority: 1,
            active_tasks: 0,
            max_tasks: 100,
        });

        let r1 = test_request();
        let resp1 = manager.submit(r1);
        assert_eq!(resp1.status, DelegationStatus::Running);

        let mut r2 = test_request();
        r2.request_id = "req-002".to_string();
        let resp2 = manager.submit(r2);
        assert_eq!(resp2.status, DelegationStatus::Rejected);
        assert!(resp2.error.unwrap().contains("concurrent"));
    }

    #[test]
    fn test_a2a_envelope_delegation() {
        let request = test_request();
        let envelope = A2AEnvelope::delegation("service-a", "agnos-daimon", &request);
        assert_eq!(envelope.version, "1.0");
        assert_eq!(envelope.message_type, A2AMessageType::Delegate);
        assert_eq!(envelope.source, "service-a");
        assert_eq!(envelope.destination, "agnos-daimon");
        assert_eq!(envelope.correlation_id, "req-001");
    }

    #[test]
    fn test_a2a_envelope_discover() {
        let envelope = A2AEnvelope::discover("external", "agnos-daimon");
        assert_eq!(envelope.message_type, A2AMessageType::Discover);
        assert!(!envelope.correlation_id.is_empty());
    }

    #[test]
    fn test_a2a_message_types() {
        let json = serde_json::to_string(&A2AMessageType::Delegate).unwrap();
        assert_eq!(json, "\"Delegate\"");
        let parsed: A2AMessageType = serde_json::from_str("\"Cancel\"").unwrap();
        assert_eq!(parsed, A2AMessageType::Cancel);
    }

    #[test]
    fn test_delegation_request_serialization() {
        let request = test_request();
        let json = serde_json::to_string(&request).unwrap();
        let parsed: DelegationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.request_id, "req-001");
        assert_eq!(parsed.capability, "code-review");
        assert_eq!(parsed.sandbox_level, SandboxLevel::Standard);
    }

    #[test]
    fn test_policy_violation_display() {
        assert_eq!(
            PolicyViolation::DelegationDisabled.to_string(),
            "delegation is disabled"
        );
        assert_eq!(
            PolicyViolation::AuthenticationRequired.to_string(),
            "authentication required"
        );
        let pv = PolicyViolation::PayloadTooLarge {
            size: 20_000_000,
            limit: 10_000_000,
        };
        assert!(pv.to_string().contains("20000000"));
    }

    #[test]
    fn test_delegation_stats_serialization() {
        let stats = DelegationStats {
            active: 2,
            completed: 10,
            succeeded: 8,
            failed: 2,
            rejected: 1,
            available_capabilities: 3,
            registered_routes: 5,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: DelegationStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.active, 2);
        assert_eq!(parsed.succeeded, 8);
    }
}
