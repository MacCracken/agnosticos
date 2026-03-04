use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Maximum number of security alerts retained in memory.
const MAX_ALERTS: usize = 500;
/// Maximum number of permission requests retained.
const MAX_PERMISSION_REQUESTS: usize = 200;
/// Maximum number of override requests retained.
const MAX_OVERRIDE_REQUESTS: usize = 100;

#[derive(Debug, Error)]
pub enum SecurityUIError {
    #[error("Permission not found: {0}")]
    PermissionNotFound(String),
    #[error("Alert not found: {0}")]
    AlertNotFound(Uuid),
    #[error("Action blocked: {0}")]
    ActionBlocked(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreatLevel {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct SecurityAlert {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub threat_level: ThreatLevel,
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub requires_action: bool,
    pub is_resolved: bool,
}

impl Default for SecurityAlert {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            title: String::new(),
            description: String::new(),
            threat_level: ThreatLevel::Low,
            source: String::new(),
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_resolved: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: String,
    pub permission: String,
    pub resource: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub is_granted: bool,
}

#[derive(Debug, Clone)]
pub struct PermissionDefinition {
    pub name: String,
    pub description: String,
    pub category: PermissionCategory,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCategory {
    FileSystem,
    Network,
    Process,
    Hardware,
    Agent,
    System,
}

#[derive(Debug, Clone)]
pub struct AgentPermission {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub permissions: Vec<String>,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct SecurityDashboard {
    pub threat_level: ThreatLevel,
    pub active_alerts: usize,
    pub pending_permissions: usize,
    pub running_agents: usize,
    pub last_scan: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct OverrideRequest {
    pub id: Uuid,
    pub agent_name: String,
    pub action: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub is_approved: bool,
    pub approved_by: Option<String>,
}

#[derive(Debug)]
pub struct SecurityUI {
    alerts: Arc<RwLock<Vec<SecurityAlert>>>,
    permission_requests: Arc<RwLock<Vec<PermissionRequest>>>,
    agent_permissions: Arc<RwLock<HashMap<Uuid, AgentPermission>>>,
    override_requests: Arc<RwLock<Vec<OverrideRequest>>>,
    permission_definitions: Arc<RwLock<Vec<PermissionDefinition>>>,
    security_level: Arc<RwLock<SecurityLevel>>,
    emergency_mode: Arc<RwLock<bool>>,
    human_override_enabled: Arc<RwLock<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityLevel {
    Standard,
    Elevated,
    Lockdown,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        SecurityLevel::Standard
    }
}

impl SecurityUI {
    pub fn new() -> Self {
        let mut ui = Self {
            alerts: Arc::new(RwLock::new(Vec::new())),
            permission_requests: Arc::new(RwLock::new(Vec::new())),
            agent_permissions: Arc::new(RwLock::new(HashMap::new())),
            override_requests: Arc::new(RwLock::new(Vec::new())),
            permission_definitions: Arc::new(RwLock::new(Vec::new())),
            security_level: Arc::new(RwLock::new(SecurityLevel::Standard)),
            emergency_mode: Arc::new(RwLock::new(false)),
            human_override_enabled: Arc::new(RwLock::new(true)),
        };

        ui.initialize_permission_definitions();
        ui
    }

    fn initialize_permission_definitions(&self) {
        let mut defs = self.permission_definitions.write().unwrap();

        defs.push(PermissionDefinition {
            name: "file:read".to_string(),
            description: "Read files in specified directories".to_string(),
            category: PermissionCategory::FileSystem,
            requires_confirmation: false,
        });

        defs.push(PermissionDefinition {
            name: "file:write".to_string(),
            description: "Create or modify files".to_string(),
            category: PermissionCategory::FileSystem,
            requires_confirmation: true,
        });

        defs.push(PermissionDefinition {
            name: "file:delete".to_string(),
            description: "Delete files and directories".to_string(),
            category: PermissionCategory::FileSystem,
            requires_confirmation: true,
        });

        defs.push(PermissionDefinition {
            name: "network:outbound".to_string(),
            description: "Make outbound network connections".to_string(),
            category: PermissionCategory::Network,
            requires_confirmation: false,
        });

        defs.push(PermissionDefinition {
            name: "process:spawn".to_string(),
            description: "Start new processes".to_string(),
            category: PermissionCategory::Process,
            requires_confirmation: true,
        });

        defs.push(PermissionDefinition {
            name: "agent:delegate".to_string(),
            description: "Delegate tasks to other agents".to_string(),
            category: PermissionCategory::Agent,
            requires_confirmation: true,
        });

        info!("Initialized {} permission definitions", defs.len());
    }

    pub fn show_security_alert(&self, alert: SecurityAlert) {
        let mut alerts = self.alerts.write().unwrap();

        // Evict oldest resolved alerts when at capacity
        if alerts.len() >= MAX_ALERTS {
            alerts.retain(|a| !a.is_resolved);
            // If still full, drop the oldest
            if alerts.len() >= MAX_ALERTS {
                alerts.remove(0);
            }
        }

        if alert.threat_level == ThreatLevel::Critical {
            warn!("CRITICAL SECURITY ALERT: {}", alert.title);
        }

        debug!("Security alert: {}", alert.title);
        alerts.push(alert);
    }

    pub fn dismiss_alert(&self, alert_id: Uuid) -> Result<(), SecurityUIError> {
        let mut alerts = self.alerts.write().unwrap();
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.is_resolved = true;
        }
        Ok(())
    }

    pub fn request_permission(&self, request: PermissionRequest) {
        let permission = request.permission.clone();
        let mut requests = self.permission_requests.write().unwrap();

        // Evict oldest granted requests when at capacity
        if requests.len() >= MAX_PERMISSION_REQUESTS {
            requests.retain(|r| !r.is_granted);
            if requests.len() >= MAX_PERMISSION_REQUESTS {
                requests.remove(0);
            }
        }

        requests.push(request);
        info!("Permission request: {}", permission);
    }

    pub fn grant_permission(&self, request_id: Uuid) -> Result<(), SecurityUIError> {
        let mut requests = self.permission_requests.write().unwrap();
        if let Some(req) = requests.iter_mut().find(|r| r.id == request_id) {
            req.is_granted = true;
            info!("Permission granted: {}", req.permission);
        }
        Ok(())
    }

    pub fn deny_permission(&self, request_id: Uuid) -> Result<(), SecurityUIError> {
        let mut requests = self.permission_requests.write().unwrap();
        requests.retain(|r| r.id != request_id);
        info!("Permission denied: {}", request_id);
        Ok(())
    }

    pub fn set_agent_permissions(
        &self,
        agent_id: Uuid,
        agent_name: String,
        permissions: Vec<String>,
    ) {
        let mut perms = self.agent_permissions.write().unwrap();
        perms.insert(
            agent_id,
            AgentPermission {
                agent_id,
                agent_name,
                permissions,
                granted_at: chrono::Utc::now(),
                expires_at: None,
            },
        );
        info!("Agent permissions set for {:?}", agent_id);
    }

    pub fn revoke_agent_permissions(&self, agent_id: Uuid) -> Result<(), SecurityUIError> {
        let mut perms = self.agent_permissions.write().unwrap();
        if perms.remove(&agent_id).is_some() {
            info!("Permissions revoked for {:?}", agent_id);
            Ok(())
        } else {
            Err(SecurityUIError::PermissionNotFound(format!(
                "{:?}",
                agent_id
            )))
        }
    }

    pub fn request_human_override(
        &self,
        agent_name: String,
        action: String,
        reason: String,
    ) -> Uuid {
        let agent_name_clone = agent_name.clone();
        let id = Uuid::new_v4();
        let request = OverrideRequest {
            id,
            agent_name: agent_name_clone,
            action,
            reason,
            timestamp: chrono::Utc::now(),
            is_approved: false,
            approved_by: None,
        };

        let mut requests = self.override_requests.write().unwrap();

        // Evict oldest approved overrides when at capacity
        if requests.len() >= MAX_OVERRIDE_REQUESTS {
            requests.retain(|r| !r.is_approved);
            if requests.len() >= MAX_OVERRIDE_REQUESTS {
                requests.remove(0);
            }
        }

        requests.push(request);

        warn!("Human override requested by {}", agent_name);
        id
    }

    pub fn approve_override(
        &self,
        request_id: Uuid,
        approver: String,
    ) -> Result<(), SecurityUIError> {
        let approver_clone = approver.clone();
        let mut requests = self.override_requests.write().unwrap();
        if let Some(req) = requests.iter_mut().find(|r| r.id == request_id) {
            req.is_approved = true;
            req.approved_by = Some(approver);
            info!("Override approved by {}", approver_clone);
            Ok(())
        } else {
            Err(SecurityUIError::AlertNotFound(request_id))
        }
    }

    pub fn emergency_kill_switch(&self) {
        *self.emergency_mode.write().unwrap() = true;
        *self.human_override_enabled.write().unwrap() = false;

        let mut requests = self.override_requests.write().unwrap();
        for req in requests.iter_mut() {
            req.is_approved = false;
        }

        warn!("EMERGENCY KILL SWITCH ACTIVATED");
    }

    pub fn deactivate_emergency(&self) {
        *self.emergency_mode.write().unwrap() = false;
        info!("Emergency mode deactivated");
    }

    pub fn set_security_level(&self, level: SecurityLevel) {
        let level_clone = level.clone();
        *self.security_level.write().unwrap() = level;
        match level_clone {
            SecurityLevel::Standard => info!("Security level: Standard"),
            SecurityLevel::Elevated => warn!("Security level: Elevated"),
            SecurityLevel::Lockdown => {
                warn!("SECURITY LEVEL: LOCKDOWN");
            }
        }
    }

    pub fn get_security_dashboard(&self) -> SecurityDashboard {
        let alerts = self.alerts.read().unwrap();
        let requests = self.permission_requests.read().unwrap();
        let agent_perms = self.agent_permissions.read().unwrap();

        let active_alerts = alerts.iter().filter(|a| !a.is_resolved).count();
        let pending = requests.iter().filter(|r| !r.is_granted).count();

        let max_threat = alerts
            .iter()
            .filter(|a| !a.is_resolved)
            .map(|a| a.threat_level.clone())
            .max()
            .unwrap_or(ThreatLevel::Low);

        SecurityDashboard {
            threat_level: max_threat,
            active_alerts,
            pending_permissions: pending,
            running_agents: agent_perms.len(),
            last_scan: chrono::Utc::now(),
        }
    }

    pub fn get_active_alerts(&self) -> Vec<SecurityAlert> {
        self.alerts
            .read()
            .unwrap()
            .iter()
            .filter(|a| !a.is_resolved)
            .cloned()
            .collect()
    }

    pub fn get_pending_permissions(&self) -> Vec<PermissionRequest> {
        self.permission_requests
            .read()
            .unwrap()
            .iter()
            .filter(|r| !r.is_granted)
            .cloned()
            .collect()
    }

    pub fn get_override_requests(&self) -> Vec<OverrideRequest> {
        self.override_requests
            .read()
            .unwrap()
            .iter()
            .filter(|r| !r.is_approved)
            .cloned()
            .collect()
    }

    pub fn is_emergency_mode(&self) -> bool {
        *self.emergency_mode.read().unwrap()
    }

    pub fn get_security_level(&self) -> SecurityLevel {
        self.security_level.read().unwrap().clone()
    }

    /// Enforce an emergency kill on a specific agent process.
    ///
    /// Sends SIGKILL, removes cgroup, deregisters via API, and writes an
    /// audit log entry.
    pub fn emergency_kill_agent(&self, agent_id: Uuid, pid: Option<u32>) -> Result<(), SecurityUIError> {
        info!("EMERGENCY KILL: agent {} (pid: {:?})", agent_id, pid);

        // Send SIGKILL to the process if PID is known
        if let Some(pid) = pid {
            #[cfg(unix)]
            {
                let result = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                if result == 0 {
                    info!("SIGKILL sent to PID {} (agent {})", pid, agent_id);
                } else {
                    let err = std::io::Error::last_os_error();
                    warn!("Failed to SIGKILL PID {}: {}", pid, err);
                }
            }
        }

        // Remove cgroup (best-effort)
        let cgroup_path = format!("/sys/fs/cgroup/agnos/{}", agent_id);
        if std::path::Path::new(&cgroup_path).exists() {
            if let Err(e) = std::fs::remove_dir_all(&cgroup_path) {
                warn!("Failed to remove cgroup {}: {}", cgroup_path, e);
            } else {
                debug!("Removed cgroup for agent {}", agent_id);
            }
        }

        // Deregister via HTTP API (best-effort, fire-and-forget)
        let agent_id_str = agent_id.to_string();
        tokio::task::spawn(async move {
            let _ = reqwest::Client::new()
                .delete(format!("http://localhost:8090/v1/agents/{}", agent_id_str))
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await;
        });

        // Write audit log entry
        Self::write_audit_log("emergency_kill", &agent_id.to_string(), "Agent killed via emergency kill switch");

        // Record a critical alert
        self.show_security_alert(SecurityAlert {
            id: Uuid::new_v4(),
            title: format!("Emergency Kill: Agent {}", agent_id),
            description: "Agent was terminated via emergency kill switch".to_string(),
            threat_level: ThreatLevel::Critical,
            source: "security-ui".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_resolved: true,
        });

        Ok(())
    }

    /// Grant a permission to an agent with policy validation and audit logging.
    pub fn grant_permission_enforced(
        &self,
        agent_id: Uuid,
        agent_name: &str,
        permission: &str,
    ) -> Result<(), SecurityUIError> {
        // Validate permission exists in definitions
        let defs = self.permission_definitions.read().unwrap();
        let perm_def = defs.iter().find(|d| d.name == permission);

        if perm_def.is_none() {
            return Err(SecurityUIError::PermissionNotFound(permission.to_string()));
        }

        let requires_confirmation = perm_def.unwrap().requires_confirmation;

        if requires_confirmation {
            // Check security level — in Lockdown, no permissions can be granted
            let level = self.security_level.read().unwrap().clone();
            if level == SecurityLevel::Lockdown {
                return Err(SecurityUIError::ActionBlocked(
                    "Cannot grant permissions in Lockdown mode".to_string(),
                ));
            }
        }

        // Update agent permissions
        let mut perms = self.agent_permissions.write().unwrap();
        let entry = perms.entry(agent_id).or_insert_with(|| AgentPermission {
            agent_id,
            agent_name: agent_name.to_string(),
            permissions: Vec::new(),
            granted_at: chrono::Utc::now(),
            expires_at: None,
        });

        if !entry.permissions.contains(&permission.to_string()) {
            entry.permissions.push(permission.to_string());
        }

        Self::write_audit_log(
            "permission_granted",
            &agent_id.to_string(),
            &format!("Permission '{}' granted to '{}'", permission, agent_name),
        );

        info!("Permission '{}' granted to agent {} ({})", permission, agent_name, agent_id);
        Ok(())
    }

    /// Revoke a permission from an agent and send SIGHUP to reload config.
    pub fn revoke_permission_enforced(
        &self,
        agent_id: Uuid,
        permission: &str,
        pid: Option<u32>,
    ) -> Result<(), SecurityUIError> {
        let mut perms = self.agent_permissions.write().unwrap();

        if let Some(entry) = perms.get_mut(&agent_id) {
            entry.permissions.retain(|p| p != permission);

            // Send SIGHUP to agent process to reload configuration
            if let Some(pid) = pid {
                #[cfg(unix)]
                {
                    let result = unsafe { libc::kill(pid as i32, libc::SIGHUP) };
                    if result == 0 {
                        debug!("SIGHUP sent to PID {} to reload config", pid);
                    }
                }
            }

            Self::write_audit_log(
                "permission_revoked",
                &agent_id.to_string(),
                &format!("Permission '{}' revoked", permission),
            );

            info!("Permission '{}' revoked from agent {}", permission, agent_id);
            Ok(())
        } else {
            Err(SecurityUIError::PermissionNotFound(format!(
                "No permissions found for agent {}",
                agent_id
            )))
        }
    }

    /// Write a JSON audit log entry to `/var/log/agnos/audit.log`.
    fn write_audit_log(event_type: &str, agent_id: &str, details: &str) {
        let entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "event_type": event_type,
            "source": "security-ui",
            "agent_id": agent_id,
            "details": details,
        });

        let log_path = "/var/log/agnos/audit.log";
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                let _ = writeln!(file, "{}", entry);
            }
            Err(e) => {
                // Don't fail on audit log write — just warn
                warn!("Could not write audit log to {}: {}", log_path, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threat_level_variants() {
        assert!(matches!(ThreatLevel::Low, ThreatLevel::Low));
        assert!(matches!(ThreatLevel::Medium, ThreatLevel::Medium));
        assert!(matches!(ThreatLevel::High, ThreatLevel::High));
        assert!(matches!(ThreatLevel::Critical, ThreatLevel::Critical));
    }

    #[test]
    fn test_security_alert_default() {
        let alert = SecurityAlert::default();
        assert_eq!(alert.threat_level, ThreatLevel::Low);
        assert!(!alert.requires_action);
        assert!(!alert.is_resolved);
    }

    #[test]
    fn test_security_alert_custom() {
        let alert = SecurityAlert {
            id: Uuid::new_v4(),
            title: "Suspicious Activity".to_string(),
            description: "Detected unusual behavior".to_string(),
            threat_level: ThreatLevel::High,
            source: "agent-runtime".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: true,
            is_resolved: false,
        };
        assert_eq!(alert.threat_level, ThreatLevel::High);
        assert!(alert.requires_action);
    }

    #[test]
    fn test_permission_request() {
        let request = PermissionRequest {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            agent_name: "test-agent".to_string(),
            permission: "file:read".to_string(),
            resource: "/home".to_string(),
            reason: "Reading files".to_string(),
            timestamp: chrono::Utc::now(),
            is_granted: false,
        };
        assert_eq!(request.agent_name, "test-agent");
        assert!(!request.is_granted);
    }

    #[test]
    fn test_permission_definition() {
        let def = PermissionDefinition {
            name: "test:permission".to_string(),
            description: "Test permission".to_string(),
            category: PermissionCategory::Agent,
            requires_confirmation: true,
        };
        assert_eq!(def.name, "test:permission");
        assert!(def.requires_confirmation);
    }

    #[test]
    fn test_permission_category() {
        assert!(matches!(
            PermissionCategory::FileSystem,
            PermissionCategory::FileSystem
        ));
        assert!(matches!(
            PermissionCategory::Network,
            PermissionCategory::Network
        ));
        assert!(matches!(
            PermissionCategory::Agent,
            PermissionCategory::Agent
        ));
    }

    #[test]
    fn test_security_level() {
        assert!(matches!(SecurityLevel::Standard, SecurityLevel::Standard));
        assert!(matches!(SecurityLevel::Elevated, SecurityLevel::Elevated));
        assert!(matches!(SecurityLevel::Lockdown, SecurityLevel::Lockdown));
    }

    #[test]
    fn test_security_level_default() {
        let level = SecurityLevel::default();
        assert!(matches!(level, SecurityLevel::Standard));
    }

    #[test]
    fn test_security_dashboard() {
        let dashboard = SecurityDashboard {
            threat_level: ThreatLevel::Medium,
            active_alerts: 5,
            pending_permissions: 3,
            running_agents: 2,
            last_scan: chrono::Utc::now(),
        };
        assert_eq!(dashboard.active_alerts, 5);
        assert_eq!(dashboard.running_agents, 2);
    }

    #[test]
    fn test_override_request() {
        let request = OverrideRequest {
            id: Uuid::new_v4(),
            agent_name: "test-agent".to_string(),
            action: "delete".to_string(),
            reason: "Cleanup".to_string(),
            timestamp: chrono::Utc::now(),
            is_approved: false,
            approved_by: None,
        };
        assert!(!request.is_approved);
        assert!(request.approved_by.is_none());
    }

    #[test]
    fn test_security_ui_new() {
        let ui = SecurityUI::new();
        assert!(ui.get_active_alerts().is_empty());
        assert!(ui.get_pending_permissions().is_empty());
    }

    #[test]
    fn test_security_ui_show_alert() {
        let ui = SecurityUI::new();
        let alert = SecurityAlert {
            id: Uuid::new_v4(),
            title: "Test Alert".to_string(),
            description: "Test".to_string(),
            threat_level: ThreatLevel::Medium,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: true,
            is_resolved: false,
        };
        ui.show_security_alert(alert);
        let active = ui.get_active_alerts();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_security_ui_dismiss_alert() {
        let ui = SecurityUI::new();
        let alert = SecurityAlert {
            id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: "Test".to_string(),
            threat_level: ThreatLevel::Low,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_resolved: false,
        };
        let id = alert.id;
        ui.show_security_alert(alert);
        assert_eq!(ui.get_active_alerts().len(), 1);
        ui.dismiss_alert(id).unwrap();
        assert!(ui.get_active_alerts().is_empty());
    }

    #[test]
    fn test_security_ui_request_permission() {
        let ui = SecurityUI::new();
        let request = PermissionRequest {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            agent_name: "test".to_string(),
            permission: "file:read".to_string(),
            resource: "/home".to_string(),
            reason: "Read files".to_string(),
            timestamp: chrono::Utc::now(),
            is_granted: false,
        };
        ui.request_permission(request);
        let pending = ui.get_pending_permissions();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_security_ui_grant_permission() {
        let ui = SecurityUI::new();
        let request = PermissionRequest {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            agent_name: "test".to_string(),
            permission: "file:read".to_string(),
            resource: "/home".to_string(),
            reason: "Read".to_string(),
            timestamp: chrono::Utc::now(),
            is_granted: false,
        };
        let id = request.id;
        ui.request_permission(request);
        ui.grant_permission(id).unwrap();
        let pending = ui.get_pending_permissions();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_security_ui_deny_permission() {
        let ui = SecurityUI::new();
        let request = PermissionRequest {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            agent_name: "test".to_string(),
            permission: "file:read".to_string(),
            resource: "/home".to_string(),
            reason: "Read".to_string(),
            timestamp: chrono::Utc::now(),
            is_granted: false,
        };
        let id = request.id;
        ui.request_permission(request);
        ui.deny_permission(id).unwrap();
        assert!(ui.get_pending_permissions().is_empty());
    }

    #[test]
    fn test_security_ui_set_agent_permissions() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();
        ui.set_agent_permissions(agent_id, "test-agent".to_string(), vec!["read".to_string()]);
    }

    #[test]
    fn test_security_ui_revoke_agent_permissions() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();
        ui.set_agent_permissions(agent_id, "test".to_string(), vec!["read".to_string()]);
        ui.revoke_agent_permissions(agent_id).unwrap();
    }

    #[test]
    fn test_security_ui_revoke_nonexistent_permissions() {
        let ui = SecurityUI::new();
        let result = ui.revoke_agent_permissions(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_security_ui_request_human_override() {
        let ui = SecurityUI::new();
        let id = ui.request_human_override(
            "agent".to_string(),
            "delete".to_string(),
            "test".to_string(),
        );
        let requests = ui.get_override_requests();
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn test_security_ui_approve_override() {
        let ui = SecurityUI::new();
        let id = ui.request_human_override(
            "agent".to_string(),
            "delete".to_string(),
            "test".to_string(),
        );
        ui.approve_override(id, "admin".to_string()).unwrap();
        let requests = ui.get_override_requests();
        assert!(requests.is_empty());
    }

    #[test]
    fn test_security_ui_approve_nonexistent_override() {
        let ui = SecurityUI::new();
        let result = ui.approve_override(Uuid::new_v4(), "admin".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_security_ui_emergency_kill_switch() {
        let ui = SecurityUI::new();
        assert!(!ui.is_emergency_mode());
        ui.emergency_kill_switch();
        assert!(ui.is_emergency_mode());
    }

    #[test]
    fn test_security_ui_deactivate_emergency() {
        let ui = SecurityUI::new();
        ui.emergency_kill_switch();
        assert!(ui.is_emergency_mode());
        ui.deactivate_emergency();
        assert!(!ui.is_emergency_mode());
    }

    #[test]
    fn test_security_ui_set_security_level() {
        let ui = SecurityUI::new();
        assert_eq!(ui.get_security_level(), SecurityLevel::Standard);
        ui.set_security_level(SecurityLevel::Elevated);
        assert_eq!(ui.get_security_level(), SecurityLevel::Elevated);
        ui.set_security_level(SecurityLevel::Lockdown);
        assert_eq!(ui.get_security_level(), SecurityLevel::Lockdown);
    }

    #[test]
    fn test_security_ui_get_dashboard() {
        let ui = SecurityUI::new();
        let dashboard = ui.get_security_dashboard();
        assert_eq!(dashboard.active_alerts, 0);
        assert_eq!(dashboard.threat_level, ThreatLevel::Low);
    }

    #[test]
    fn test_security_ui_error_variants() {
        let err = SecurityUIError::PermissionNotFound("test".to_string());
        assert!(err.to_string().contains("not found"));
        let err = SecurityUIError::AlertNotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));
        let err = SecurityUIError::ActionBlocked("test".to_string());
        assert!(err.to_string().contains("blocked"));
    }

    #[test]
    fn test_threat_level_ordering() {
        assert!(ThreatLevel::Critical > ThreatLevel::High);
        assert!(ThreatLevel::High > ThreatLevel::Medium);
        assert!(ThreatLevel::Medium > ThreatLevel::Low);
    }

    #[test]
    fn test_agent_permission() {
        let perm = AgentPermission {
            agent_id: Uuid::new_v4(),
            agent_name: "test".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
            granted_at: chrono::Utc::now(),
            expires_at: None,
        };
        assert_eq!(perm.permissions.len(), 2);
    }

    #[tokio::test]
    async fn test_emergency_kill_agent_no_pid() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();
        // Kill without PID — should succeed (skips SIGKILL)
        let result = ui.emergency_kill_agent(agent_id, None);
        assert!(result.is_ok());
        // Should have logged a critical alert
        let alerts = ui.get_active_alerts();
        // Alert is already resolved, so active_alerts won't include it
        // but we can check the dashboard
    }

    #[test]
    fn test_grant_permission_enforced() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();

        // Grant a known permission
        let result = ui.grant_permission_enforced(agent_id, "test-agent", "file:read");
        assert!(result.is_ok());

        // Grant unknown permission should fail
        let result = ui.grant_permission_enforced(agent_id, "test-agent", "unknown:perm");
        assert!(result.is_err());
    }

    #[test]
    fn test_grant_permission_lockdown() {
        let ui = SecurityUI::new();
        ui.set_security_level(SecurityLevel::Lockdown);

        let agent_id = Uuid::new_v4();
        // file:write requires confirmation → blocked in Lockdown
        let result = ui.grant_permission_enforced(agent_id, "test-agent", "file:write");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Lockdown"));
    }

    #[test]
    fn test_grant_permission_no_confirmation_in_lockdown() {
        let ui = SecurityUI::new();
        ui.set_security_level(SecurityLevel::Lockdown);

        let agent_id = Uuid::new_v4();
        // file:read does NOT require confirmation → allowed even in Lockdown
        let result = ui.grant_permission_enforced(agent_id, "test-agent", "file:read");
        assert!(result.is_ok());
    }

    #[test]
    fn test_revoke_permission_enforced() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();

        // First grant
        ui.grant_permission_enforced(agent_id, "test-agent", "file:read").unwrap();

        // Then revoke (no PID)
        let result = ui.revoke_permission_enforced(agent_id, "file:read", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_revoke_permission_not_found() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();

        let result = ui.revoke_permission_enforced(agent_id, "file:read", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_grant_duplicate_permission() {
        let ui = SecurityUI::new();
        let agent_id = Uuid::new_v4();

        ui.grant_permission_enforced(agent_id, "agent", "file:read").unwrap();
        ui.grant_permission_enforced(agent_id, "agent", "file:read").unwrap();

        // Should only appear once
        let perms = ui.agent_permissions.read().unwrap();
        let entry = perms.get(&agent_id).unwrap();
        let count = entry.permissions.iter().filter(|p| *p == "file:read").count();
        assert_eq!(count, 1);
    }
}
