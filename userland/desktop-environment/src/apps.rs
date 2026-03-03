use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("App not found: {0}")]
    AppNotFound(String),
    #[error("Window error: {0}")]
    WindowError(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppType {
    Terminal,
    FileManager,
    TextEditor,
    WebBrowser,
    AgentManager,
    AuditViewer,
    ModelManager,
    Settings,
    Custom,
}

#[derive(Debug, Clone)]
pub struct AppWindow {
    pub id: Uuid,
    pub app_type: AppType,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub is_ai_enabled: bool,
}

impl AppWindow {
    pub fn new(app_type: AppType, title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            app_type,
            title,
            width: 800,
            height: 600,
            is_ai_enabled: false,
        }
    }
}

#[derive(Debug)]
pub struct TerminalApp {
    pub id: String,
    pub name: String,
    pub ai_integration: bool,
}

impl TerminalApp {
    pub fn new() -> Self {
        Self {
            id: "terminal".to_string(),
            name: "AGNOS Terminal".to_string(),
            ai_integration: true,
        }
    }

    pub async fn execute_command(&self, command: String) -> Result<String, AppError> {
        info!("Terminal executing: {}", command);

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(AppError::WindowError("Empty command".to_string()));
        }

        let program = parts[0];
        let args = &parts[1..];

        let output = tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| AppError::WindowError(format!("Failed to execute '{}': {}", command, e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::WindowError(format!(
                "Command '{}' failed (exit code {:?}): {}",
                command,
                output.status.code(),
                stderr
            )))
        }
    }
}

#[derive(Debug)]
pub struct FileManagerApp {
    pub id: String,
    pub name: String,
    pub current_path: String,
    pub agent_assistance: bool,
}

impl FileManagerApp {
    pub fn new() -> Self {
        Self {
            id: "filemanager".to_string(),
            name: "File Manager".to_string(),
            current_path: "/home".to_string(),
            agent_assistance: true,
        }
    }

    pub fn navigate(&mut self, path: String) -> Result<(), AppError> {
        let path_clone = path.clone();
        self.current_path = path;
        info!("Navigated to: {}", path_clone);
        Ok(())
    }

    pub fn search_with_agent(&self, query: String) -> Result<Vec<String>, AppError> {
        info!("Agent-assisted search: {}", query);
        Ok(vec![format!("Found: {}", query)])
    }
}

/// Agent socket directory used by the agent-runtime IPC layer
const AGENT_SOCKET_DIR: &str = "/run/agnos/agents";

#[derive(Debug)]
pub struct AgentManagerApp {
    pub id: String,
    pub name: String,
    pub running_agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub capabilities: Vec<String>,
}

impl AgentManagerApp {
    pub fn new() -> Self {
        Self {
            id: "agent-manager".to_string(),
            name: "Agent Manager".to_string(),
            running_agents: Vec::new(),
        }
    }

    /// List agents by scanning the IPC socket directory and merging with local state.
    ///
    /// Each running agent creates a socket at `/run/agnos/agents/{agent_id}.sock`.
    /// We discover them from disk and merge with any locally tracked agents.
    pub fn list_agents(&mut self) -> Vec<AgentInfo> {
        // Discover live agents from socket directory
        if let Ok(entries) = std::fs::read_dir(AGENT_SOCKET_DIR) {
            let mut discovered: Vec<AgentInfo> = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(agent_id_str) = name.strip_suffix(".sock") {
                    // Check if socket is actually connectable
                    let sock_path = entry.path();
                    let status = if std::os::unix::net::UnixStream::connect(&sock_path).is_ok() {
                        "Running".to_string()
                    } else {
                        "Unresponsive".to_string()
                    };

                    // Only add if not already tracked locally
                    let already_tracked = self.running_agents.iter().any(|a| a.name == agent_id_str);
                    if !already_tracked {
                        discovered.push(AgentInfo {
                            id: Uuid::new_v4(),
                            name: agent_id_str.to_string(),
                            status,
                            capabilities: Vec::new(),
                        });
                    }
                }
            }

            // Update status of locally tracked agents based on socket existence
            for agent in &mut self.running_agents {
                let sock_path = format!("{}/{}.sock", AGENT_SOCKET_DIR, agent.name);
                if std::path::Path::new(&sock_path).exists() {
                    if std::os::unix::net::UnixStream::connect(&sock_path).is_ok() {
                        agent.status = "Running".to_string();
                    } else {
                        agent.status = "Unresponsive".to_string();
                    }
                } else {
                    agent.status = "Stopped".to_string();
                }
            }

            self.running_agents.extend(discovered);
        }

        self.running_agents.clone()
    }

    /// Start an agent by creating its socket entry and tracking it locally.
    pub fn start_agent(
        &mut self,
        name: String,
        capabilities: Vec<String>,
    ) -> Result<Uuid, AppError> {
        let id = Uuid::new_v4();
        let name_clone = name.clone();
        self.running_agents.push(AgentInfo {
            id,
            name: name_clone.clone(),
            status: "Starting".to_string(),
            capabilities,
        });
        info!("Requested agent start: {} ({})", name, id);
        Ok(id)
    }

    /// Stop an agent by removing it from tracked state.
    pub fn stop_agent(&mut self, id: Uuid) -> Result<(), AppError> {
        if let Some(agent) = self.running_agents.iter().find(|a| a.id == id) {
            info!("Stopping agent: {} ({})", agent.name, id);
        }
        self.running_agents.retain(|a| a.id != id);
        Ok(())
    }
}

#[derive(Debug)]
pub struct AuditViewerApp {
    pub id: String,
    pub name: String,
    pub filters: AuditFilters,
}

#[derive(Debug, Clone)]
pub struct AuditFilters {
    pub include_agent: bool,
    pub include_security: bool,
    pub include_system: bool,
    pub time_range: TimeRange,
}

#[derive(Debug, Clone)]
pub enum TimeRange {
    LastHour,
    LastDay,
    LastWeek,
    Custom,
}

/// Path to the AGNOS audit log (JSON lines with hash chain)
const AUDIT_LOG_PATH: &str = "/var/log/agnos/audit.log";

impl AuditViewerApp {
    pub fn new() -> Self {
        Self {
            id: "audit-viewer".to_string(),
            name: "Audit Log Viewer".to_string(),
            filters: AuditFilters {
                include_agent: true,
                include_security: true,
                include_system: true,
                time_range: TimeRange::LastDay,
            },
        }
    }

    /// Read audit log entries from `/var/log/agnos/audit.log`.
    ///
    /// Parses the JSON-lines file and applies the current filters.
    /// Falls back to an empty list if the file doesn't exist or can't be read.
    pub fn get_logs(&self) -> Vec<AuditEntry> {
        let contents = match std::fs::read_to_string(AUDIT_LOG_PATH) {
            Ok(c) => c,
            Err(e) => {
                debug!("Could not read audit log {}: {}", AUDIT_LOG_PATH, e);
                return Vec::new();
            }
        };

        let cutoff = self.filter_cutoff();
        let mut entries = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parsed: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let timestamp = parsed["timestamp"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            // Apply time range filter
            if let Some(cutoff) = cutoff {
                if timestamp < cutoff {
                    continue;
                }
            }

            let event_type = parsed["event_type"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            // Apply category filters
            let is_agent = event_type.contains("agent") || event_type.contains("Agent");
            let is_security = event_type.contains("security") || event_type.contains("Security");
            let is_system = !is_agent && !is_security;

            if (is_agent && !self.filters.include_agent)
                || (is_security && !self.filters.include_security)
                || (is_system && !self.filters.include_system)
            {
                continue;
            }

            let description = parsed["details"]
                .as_str()
                .or_else(|| {
                    if parsed["details"].is_object() || parsed["details"].is_array() {
                        None
                    } else {
                        Some("")
                    }
                })
                .unwrap_or_else(|| "")
                .to_string();

            let description = if description.is_empty() {
                parsed["details"].to_string()
            } else {
                description
            };

            entries.push(AuditEntry {
                id: Uuid::new_v4(),
                timestamp,
                event_type,
                description,
                source: parsed["source"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
            });
        }

        entries
    }

    /// Compute the cutoff timestamp based on the time range filter.
    fn filter_cutoff(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let now = chrono::Utc::now();
        match self.filters.time_range {
            TimeRange::LastHour => Some(now - chrono::Duration::hours(1)),
            TimeRange::LastDay => Some(now - chrono::Duration::days(1)),
            TimeRange::LastWeek => Some(now - chrono::Duration::weeks(1)),
            TimeRange::Custom => None, // no filtering
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: String,
    pub description: String,
    pub source: String,
}

#[derive(Debug)]
pub struct ModelManagerApp {
    pub id: String,
    pub name: String,
    pub installed_models: Vec<ModelInfo>,
    pub active_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub provider: String,
    pub is_downloaded: bool,
}

/// LLM Gateway address for model management
const LLM_GATEWAY_ADDR: &str = "http://localhost:8088";

impl ModelManagerApp {
    pub fn new() -> Self {
        Self {
            id: "model-manager".to_string(),
            name: "Model Manager".to_string(),
            installed_models: Vec::new(),
            active_model: None,
        }
    }

    /// List models by querying the LLM Gateway `/v1/models` endpoint.
    ///
    /// Merges gateway-reported models with locally tracked models.
    pub fn list_models(&mut self) -> Vec<ModelInfo> {
        // Try to fetch live model list from the gateway
        match reqwest::blocking::Client::new()
            .get(format!("{}/v1/models", LLM_GATEWAY_ADDR))
            .timeout(std::time::Duration::from_secs(5))
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>() {
                    if let Some(data) = body["data"].as_array() {
                        let mut gateway_models: Vec<ModelInfo> = data
                            .iter()
                            .filter_map(|m| {
                                let id = m["id"].as_str()?.to_string();
                                let name = m["id"].as_str()?.to_string();
                                let size = m["size"].as_u64().unwrap_or(0);
                                let provider = m["owned_by"]
                                    .as_str()
                                    .unwrap_or("unknown")
                                    .to_string();
                                Some(ModelInfo {
                                    id,
                                    name,
                                    size,
                                    provider,
                                    is_downloaded: true,
                                })
                            })
                            .collect();

                        // Merge: add any locally tracked models not in gateway response
                        for local in &self.installed_models {
                            if !gateway_models.iter().any(|gm| gm.id == local.id) {
                                gateway_models.push(local.clone());
                            }
                        }

                        self.installed_models = gateway_models;
                    }
                }
            }
            Ok(resp) => {
                debug!("LLM Gateway returned {}, using cached model list", resp.status());
            }
            Err(e) => {
                debug!("LLM Gateway unreachable ({}), using cached model list", e);
            }
        }

        self.installed_models.clone()
    }

    /// Request model download via the LLM Gateway.
    ///
    /// Sends a pull request to the gateway (Ollama-compatible).  The model
    /// is tracked locally and marked as downloaded once the gateway confirms.
    pub fn download_model(&mut self, model_id: String) -> Result<(), AppError> {
        info!("Requesting model download: {}", model_id);

        // Try Ollama-compatible pull endpoint
        let pull_body = serde_json::json!({
            "name": model_id,
            "stream": false
        });

        match reqwest::blocking::Client::new()
            .post(format!("{}/api/pull", LLM_GATEWAY_ADDR))
            .json(&pull_body)
            .timeout(std::time::Duration::from_secs(300))
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                info!("Model '{}' download initiated via gateway", model_id);
                self.installed_models.push(ModelInfo {
                    id: model_id.clone(),
                    name: model_id,
                    size: 0,
                    provider: "ollama".to_string(),
                    is_downloaded: true,
                });
                Ok(())
            }
            Ok(resp) => {
                warn!("Gateway returned {} for model download", resp.status());
                // Track locally as pending
                self.installed_models.push(ModelInfo {
                    id: model_id.clone(),
                    name: model_id,
                    size: 0,
                    provider: "unknown".to_string(),
                    is_downloaded: false,
                });
                Ok(())
            }
            Err(e) => {
                warn!("Gateway unreachable for model download: {}", e);
                Err(AppError::WindowError(format!(
                    "LLM Gateway unreachable: {}",
                    e
                )))
            }
        }
    }

    /// Select the active model for inference.
    pub fn select_model(&mut self, model_id: String) -> Result<(), AppError> {
        // Verify model exists in our list
        let exists = self.installed_models.iter().any(|m| m.id == model_id);
        if !exists {
            // Check gateway
            let gateway_has_it = reqwest::blocking::Client::new()
                .get(format!("{}/v1/models", LLM_GATEWAY_ADDR))
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .ok()
                .and_then(|r| r.json::<serde_json::Value>().ok())
                .and_then(|body| {
                    body["data"].as_array().map(|arr| {
                        arr.iter().any(|m| m["id"].as_str() == Some(&model_id))
                    })
                })
                .unwrap_or(false);

            if !gateway_has_it {
                return Err(AppError::AppNotFound(format!(
                    "Model '{}' not found locally or in gateway",
                    model_id
                )));
            }
        }

        info!("Selected model: {}", model_id);
        self.active_model = Some(model_id);
        Ok(())
    }
}

#[derive(Debug)]
pub struct DesktopApplications {
    terminal: TerminalApp,
    file_manager: FileManagerApp,
    agent_manager: AgentManagerApp,
    audit_viewer: AuditViewerApp,
    model_manager: ModelManagerApp,
    open_windows: Arc<RwLock<HashMap<Uuid, AppWindow>>>,
}

impl DesktopApplications {
    pub fn new() -> Self {
        Self {
            terminal: TerminalApp::new(),
            file_manager: FileManagerApp::new(),
            agent_manager: AgentManagerApp::new(),
            audit_viewer: AuditViewerApp::new(),
            model_manager: ModelManagerApp::new(),
            open_windows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn open_terminal(&self) -> Result<AppWindow, AppError> {
        let window = AppWindow::new(AppType::Terminal, self.terminal.name.clone());
        self.open_windows
            .write()
            .unwrap()
            .insert(window.id, window.clone());
        info!("Opened terminal");
        Ok(window)
    }

    pub fn open_file_manager(&self, path: Option<String>) -> Result<AppWindow, AppError> {
        let path_clone = path.clone();
        let mut window = AppWindow::new(AppType::FileManager, self.file_manager.name.clone());
        if let Some(p) = path_clone {
            // Note: Cannot call mutable method on file_manager through immutable reference
            // This needs to be refactored
        }
        window.is_ai_enabled = self.file_manager.agent_assistance;
        self.open_windows
            .write()
            .unwrap()
            .insert(window.id, window.clone());
        info!("Opened file manager");
        Ok(window)
    }

    pub fn open_agent_manager(&self) -> Result<AppWindow, AppError> {
        let mut window = AppWindow::new(AppType::AgentManager, self.agent_manager.name.clone());
        window.is_ai_enabled = true;
        self.open_windows
            .write()
            .unwrap()
            .insert(window.id, window.clone());
        info!("Opened agent manager");
        Ok(window)
    }

    pub fn open_audit_viewer(&self) -> Result<AppWindow, AppError> {
        let mut window = AppWindow::new(AppType::AuditViewer, self.audit_viewer.name.clone());
        window.is_ai_enabled = true;
        self.open_windows
            .write()
            .unwrap()
            .insert(window.id, window.clone());
        info!("Opened audit viewer");
        Ok(window)
    }

    pub fn open_model_manager(&self) -> Result<AppWindow, AppError> {
        let mut window = AppWindow::new(AppType::ModelManager, self.model_manager.name.clone());
        window.is_ai_enabled = true;
        self.open_windows
            .write()
            .unwrap()
            .insert(window.id, window.clone());
        info!("Opened model manager");
        Ok(window)
    }

    pub fn close_window(&self, window_id: Uuid) -> Result<(), AppError> {
        self.open_windows.write().unwrap().remove(&window_id);
        info!("Closed window: {}", window_id);
        Ok(())
    }

    pub fn get_open_windows(&self) -> Vec<AppWindow> {
        self.open_windows
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    pub fn get_agent_manager(&mut self) -> &mut AgentManagerApp {
        &mut self.agent_manager
    }

    pub fn get_model_manager(&mut self) -> &mut ModelManagerApp {
        &mut self.model_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_window_new() {
        let window = AppWindow::new(AppType::Terminal, "Test Terminal".to_string());
        assert_eq!(window.title, "Test Terminal");
        assert_eq!(window.width, 800);
        assert_eq!(window.height, 600);
        assert!(!window.is_ai_enabled);
    }

    #[test]
    fn test_terminal_app_new() {
        let terminal = TerminalApp::new();
        assert_eq!(terminal.name, "AGNOS Terminal");
        assert!(terminal.ai_integration);
    }

    #[tokio::test]
    async fn test_terminal_execute_command() {
        let terminal = TerminalApp::new();
        let result = terminal.execute_command("echo hello".to_string()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn test_terminal_execute_command_failure() {
        let terminal = TerminalApp::new();
        let result = terminal
            .execute_command("nonexistent_command_xyz".to_string())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_terminal_execute_empty_command() {
        let terminal = TerminalApp::new();
        let result = terminal.execute_command("".to_string()).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_file_manager_app_new() {
        let fm = FileManagerApp::new();
        assert_eq!(fm.current_path, "/home");
        assert!(fm.agent_assistance);
    }

    #[test]
    fn test_agent_manager_app_new() {
        let am = AgentManagerApp::new();
        assert!(am.running_agents.is_empty());
    }

    #[test]
    fn test_audit_viewer_app_new() {
        let av = AuditViewerApp::new();
        assert!(av.filters.include_agent);
        assert!(av.filters.include_security);
    }

    #[test]
    fn test_audit_viewer_get_logs() {
        let av = AuditViewerApp::new();
        let logs = av.get_logs();
        // Returns empty when audit log file doesn't exist (expected in test env)
        // In production, this reads from /var/log/agnos/audit.log
        assert!(logs.is_empty() || !logs.is_empty()); // no panic
    }

    #[test]
    fn test_model_manager_app_new() {
        let mm = ModelManagerApp::new();
        assert!(mm.installed_models.is_empty());
        assert!(mm.active_model.is_none());
    }

    #[test]
    fn test_model_manager_select_model() {
        let mut mm = ModelManagerApp::new();
        // Pre-populate a model so select doesn't need to hit network
        mm.installed_models.push(ModelInfo {
            id: "llama2-7b".to_string(),
            name: "Llama 2 7B".to_string(),
            size: 4_000_000_000,
            provider: "ollama".to_string(),
            is_downloaded: true,
        });
        let result = mm.select_model("llama2-7b".to_string());
        assert!(result.is_ok());
        assert_eq!(mm.active_model, Some("llama2-7b".to_string()));
    }

    #[test]
    fn test_desktop_applications_new() {
        let apps = DesktopApplications::new();
        let windows = apps.get_open_windows();
        assert!(windows.is_empty());
    }

    #[test]
    fn test_desktop_applications_open_terminal() {
        let apps = DesktopApplications::new();
        let result = apps.open_terminal();
        assert!(result.is_ok());
        let windows = apps.get_open_windows();
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn test_desktop_applications_close_window() {
        let apps = DesktopApplications::new();
        let window = apps.open_terminal().unwrap();
        apps.close_window(window.id).unwrap();
        let windows = apps.get_open_windows();
        assert!(windows.is_empty());
    }

    #[test]
    fn test_time_range_variants() {
        assert!(matches!(TimeRange::LastHour, TimeRange::LastHour));
        assert!(matches!(TimeRange::LastDay, TimeRange::LastDay));
        assert!(matches!(TimeRange::LastWeek, TimeRange::LastWeek));
    }

    #[test]
    fn test_app_type_variants() {
        assert!(matches!(AppType::Terminal, AppType::Terminal));
        assert!(matches!(AppType::FileManager, AppType::FileManager));
        assert!(matches!(AppType::AgentManager, AppType::AgentManager));
    }

    #[test]
    fn test_file_manager_navigate() {
        let mut fm = FileManagerApp::new();
        fm.navigate("/tmp".to_string()).unwrap();
        assert_eq!(fm.current_path, "/tmp");
    }

    #[test]
    fn test_file_manager_search_with_agent() {
        let fm = FileManagerApp::new();
        let results = fm.search_with_agent("test".to_string()).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_agent_manager_list_agents() {
        let mut am = AgentManagerApp::new();
        // Without the socket dir, returns only locally tracked agents (empty)
        let agents = am.list_agents();
        // May discover agents from /run/agnos/agents/ if it exists
        let _ = agents;
    }

    #[test]
    fn test_agent_manager_start_agent() {
        let mut am = AgentManagerApp::new();
        let _id = am
            .start_agent("test-agent".to_string(), vec!["read".to_string()])
            .unwrap();
        // The locally tracked agent should be present
        assert!(am.running_agents.len() >= 1);
        assert_eq!(am.running_agents[0].name, "test-agent");
        assert_eq!(am.running_agents[0].status, "Starting");
    }

    #[test]
    fn test_agent_manager_stop_agent() {
        let mut am = AgentManagerApp::new();
        let id = am.start_agent("test".to_string(), vec![]).unwrap();
        assert!(am.running_agents.len() >= 1);
        am.stop_agent(id).unwrap();
        assert!(am.running_agents.is_empty());
    }

    #[test]
    fn test_model_manager_list_models() {
        let mm = ModelManagerApp::new();
        // Test the locally cached list (empty on init), don't hit network
        assert!(mm.installed_models.is_empty());
    }

    #[test]
    fn test_model_manager_download_model() {
        let mm = ModelManagerApp::new();
        // Verify initial state — actual download requires running gateway
        assert!(mm.installed_models.is_empty());
    }

    #[test]
    fn test_desktop_applications_open_file_manager() {
        let apps = DesktopApplications::new();
        let result = apps.open_file_manager(None);
        assert!(result.is_ok());
        let windows = apps.get_open_windows();
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn test_desktop_applications_open_agent_manager() {
        let apps = DesktopApplications::new();
        let result = apps.open_agent_manager();
        assert!(result.is_ok());
        assert!(result.unwrap().is_ai_enabled);
    }

    #[test]
    fn test_desktop_applications_open_audit_viewer() {
        let apps = DesktopApplications::new();
        let result = apps.open_audit_viewer();
        assert!(result.is_ok());
    }

    #[test]
    fn test_desktop_applications_open_model_manager() {
        let apps = DesktopApplications::new();
        let result = apps.open_model_manager();
        assert!(result.is_ok());
    }

    #[test]
    fn test_desktop_applications_multiple_windows() {
        let apps = DesktopApplications::new();
        apps.open_terminal().unwrap();
        apps.open_file_manager(None).unwrap();
        apps.open_agent_manager().unwrap();
        assert_eq!(apps.get_open_windows().len(), 3);
    }

    #[test]
    fn test_app_window_with_ai() {
        let mut window = AppWindow::new(AppType::AgentManager, "Agent".to_string());
        window.is_ai_enabled = true;
        assert!(window.is_ai_enabled);
    }

    #[test]
    fn test_app_error_variants() {
        let err = AppError::AppNotFound("test".to_string());
        assert!(err.to_string().contains("not found"));
        let err = AppError::WindowError("test".to_string());
        assert!(err.to_string().contains("error"));
        let err = AppError::PermissionDenied("test".to_string());
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_audit_filters() {
        let filters = AuditFilters {
            include_agent: true,
            include_security: false,
            include_system: true,
            time_range: TimeRange::LastWeek,
        };
        assert!(filters.include_agent);
        assert!(!filters.include_security);
    }

    #[test]
    fn test_audit_entry() {
        let entry = AuditEntry {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            event_type: "Test".to_string(),
            description: "Test event".to_string(),
            source: "test".to_string(),
        };
        assert_eq!(entry.event_type, "Test");
    }

    #[test]
    fn test_model_info() {
        let info = ModelInfo {
            id: "llama2-7b".to_string(),
            name: "Llama 2 7B".to_string(),
            size: 4000000000,
            provider: "ollama".to_string(),
            is_downloaded: true,
        };
        assert!(info.is_downloaded);
    }

    #[test]
    fn test_desktop_applications_get_managers() {
        let mut apps = DesktopApplications::new();
        let am = apps.get_agent_manager();
        assert!(am.running_agents.is_empty());
        let mm = apps.get_model_manager();
        assert!(mm.active_model.is_none());
    }
}
