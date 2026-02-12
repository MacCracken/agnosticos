use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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
        alerts.push(alert.clone());

        if alert.threat_level == ThreatLevel::Critical {
            warn!("CRITICAL SECURITY ALERT: {}", alert.title);
        }

        debug!("Security alert: {}", alert.title);
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
}
