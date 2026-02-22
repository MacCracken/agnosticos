use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{error, info, warn};
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

    pub fn execute_command(&self, command: String) -> Result<String, AppError> {
        info!("Terminal executing: {}", command);
        Ok(format!("Executed: {}", command))
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

    pub fn list_agents(&self) -> Vec<AgentInfo> {
        self.running_agents.clone()
    }

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
            status: "Running".to_string(),
            capabilities,
        });
        info!("Started agent: {}", name);
        Ok(id)
    }

    pub fn stop_agent(&mut self, id: Uuid) -> Result<(), AppError> {
        self.running_agents.retain(|a| a.id != id);
        info!("Stopped agent: {}", id);
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

    pub fn get_logs(&self) -> Vec<AuditEntry> {
        vec![AuditEntry {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            event_type: "Agent Action".to_string(),
            description: "Sample audit entry".to_string(),
            source: "agent-runtime".to_string(),
        }]
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

impl ModelManagerApp {
    pub fn new() -> Self {
        Self {
            id: "model-manager".to_string(),
            name: "Model Manager".to_string(),
            installed_models: Vec::new(),
            active_model: None,
        }
    }

    pub fn list_models(&self) -> Vec<ModelInfo> {
        self.installed_models.clone()
    }

    pub fn download_model(&self, model_id: String) -> Result<(), AppError> {
        info!("Downloading model: {}", model_id);
        Ok(())
    }

    pub fn select_model(&mut self, model_id: String) -> Result<(), AppError> {
        let model_id_clone = model_id.clone();
        self.active_model = Some(model_id);
        info!("Selected model: {}", model_id_clone);
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

    #[test]
    fn test_terminal_execute_command() {
        let terminal = TerminalApp::new();
        let result = terminal.execute_command("ls".to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Executed: ls");
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
        assert!(!logs.is_empty());
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
        let am = AgentManagerApp::new();
        assert!(am.list_agents().is_empty());
    }

    #[test]
    fn test_agent_manager_start_agent() {
        let mut am = AgentManagerApp::new();
        let id = am
            .start_agent("test-agent".to_string(), vec!["read".to_string()])
            .unwrap();
        let agents = am.list_agents();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "test-agent");
        assert_eq!(agents[0].status, "Running");
    }

    #[test]
    fn test_agent_manager_stop_agent() {
        let mut am = AgentManagerApp::new();
        let id = am.start_agent("test".to_string(), vec![]).unwrap();
        assert_eq!(am.list_agents().len(), 1);
        am.stop_agent(id).unwrap();
        assert!(am.list_agents().is_empty());
    }

    #[test]
    fn test_model_manager_list_models() {
        let mm = ModelManagerApp::new();
        assert!(mm.list_models().is_empty());
    }

    #[test]
    fn test_model_manager_download_model() {
        let mm = ModelManagerApp::new();
        assert!(mm.download_model("llama2".to_string()).is_ok());
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
