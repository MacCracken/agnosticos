use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Maximum number of notifications kept in memory.
const MAX_NOTIFICATIONS: usize = 200;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("Notification not found: {0}")]
    NotificationNotFound(Uuid),
    #[error("App not found: {0}")]
    AppNotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

pub type NotificationId = Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub priority: NotificationPriority,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub requires_action: bool,
    pub is_agent_related: bool,
}

impl Default for Notification {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            app_name: String::new(),
            title: String::new(),
            body: String::new(),
            priority: NotificationPriority::Normal,
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_agent_related: false,
        }
    }
}

pub struct QuickSetting {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub is_active: bool,
    #[doc(hidden)]
    pub on_activate: Box<dyn Fn() + Send + Sync>,
}

impl Clone for QuickSetting {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            icon: self.icon.clone(),
            is_active: self.is_active,
            on_activate: Box::new(|| {}),
        }
    }
}

impl std::fmt::Debug for QuickSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickSetting")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("icon", &self.icon)
            .field("is_active", &self.is_active)
            .field("on_activate", &"<function>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub disk_usage: f32,
    pub battery_level: Option<u8>,
    pub network_status: NetworkStatus,
    pub agent_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkStatus {
    Connected,
    Disconnected,
    Connecting,
    Error,
}

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub category: AppCategory,
    pub is_ai_app: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCategory {
    System,
    Office,
    Development,
    Communication,
    Media,
    Graphics,
    AI,
    Other,
}

#[derive(Debug, Clone)]
pub struct LauncherItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub app: Option<AppEntry>,
    pub action: Option<LauncherAction>,
    pub is_suggested: bool,
    pub relevance_score: f32,
}

#[derive(Debug, Clone)]
pub enum LauncherAction {
    OpenApp(String),
    RunCommand(String),
    SearchFiles(String),
    WebSearch(String),
}

#[derive(Debug)]
pub struct DesktopShell {
    notifications: Arc<RwLock<HashMap<NotificationId, Notification>>>,
    quick_settings: Arc<RwLock<Vec<QuickSetting>>>,
    system_status: Arc<RwLock<SystemStatus>>,
    launcher_items: Arc<RwLock<Vec<LauncherItem>>>,
    app_registry: Arc<RwLock<HashMap<String, AppEntry>>>,
    is_locked: Arc<RwLock<bool>>,
    panel_visible: Arc<RwLock<bool>>,
}

impl DesktopShell {
    pub fn new() -> Self {
        let mut shell = Self {
            notifications: Arc::new(RwLock::new(HashMap::new())),
            quick_settings: Arc::new(RwLock::new(Vec::new())),
            system_status: Arc::new(RwLock::new(SystemStatus {
                cpu_usage: 0.0,
                memory_usage: 0.0,
                disk_usage: 0.0,
                battery_level: None,
                network_status: NetworkStatus::Connected,
                agent_count: 0,
            })),
            launcher_items: Arc::new(RwLock::new(Vec::new())),
            app_registry: Arc::new(RwLock::new(HashMap::new())),
            is_locked: Arc::new(RwLock::new(false)),
            panel_visible: Arc::new(RwLock::new(true)),
        };

        shell.initialize_quick_settings();
        shell.initialize_app_registry();
        shell.populate_launcher_items();

        shell
    }

    fn initialize_quick_settings(&self) {
        let mut settings = self.quick_settings.write().unwrap();

        settings.push(QuickSetting {
            id: "wifi".to_string(),
            name: "Wi-Fi".to_string(),
            icon: "wifi".to_string(),
            is_active: true,
            on_activate: Box::new(|| {
                info!("Wi-Fi toggle activated");
            }),
        });

        settings.push(QuickSetting {
            id: "bluetooth".to_string(),
            name: "Bluetooth".to_string(),
            icon: "bluetooth".to_string(),
            is_active: false,
            on_activate: Box::new(|| {
                info!("Bluetooth toggle activated");
            }),
        });

        settings.push(QuickSetting {
            id: "airplane".to_string(),
            name: "Airplane Mode".to_string(),
            icon: "airplane".to_string(),
            is_active: false,
            on_activate: Box::new(|| {
                info!("Airplane mode toggle activated");
            }),
        });

        settings.push(QuickSetting {
            id: "dnd".to_string(),
            name: "Do Not Disturb".to_string(),
            icon: "dnd".to_string(),
            is_active: false,
            on_activate: Box::new(|| {
                info!("Do Not Disturb toggle activated");
            }),
        });

        settings.push(QuickSetting {
            id: "nightlight".to_string(),
            name: "Night Light".to_string(),
            icon: "nightlight".to_string(),
            is_active: true,
            on_activate: Box::new(|| {
                info!("Night Light toggle activated");
            }),
        });

        info!("Quick settings initialized");
    }

    fn initialize_app_registry(&self) {
        let mut registry = self.app_registry.write().unwrap();

        let system_apps = [
            ("terminal", "Terminal", "terminal", true),
            ("filemanager", "File Manager", "folder", false),
            ("settings", "Settings", "settings", false),
            ("agent-manager", "Agent Manager", "bot", true),
            ("audit-viewer", "Audit Viewer", "shield", true),
            ("model-manager", "Model Manager", "cpu", true),
        ];

        for (id, name, icon, is_ai) in system_apps {
            registry.insert(
                id.to_string(),
                AppEntry {
                    id: id.to_string(),
                    name: name.to_string(),
                    icon: icon.to_string(),
                    category: AppCategory::System,
                    is_ai_app: is_ai,
                },
            );
        }

        info!("App registry initialized with {} entries", registry.len());
    }

    fn populate_launcher_items(&self) {
        let registry = self.app_registry.read().unwrap();
        let mut items = self.launcher_items.write().unwrap();

        for app in registry.values() {
            items.push(LauncherItem {
                id: format!("app-{}", app.id),
                name: app.name.clone(),
                description: format!("Launch {}", app.name),
                icon: app.icon.clone(),
                app: Some(app.clone()),
                action: Some(LauncherAction::OpenApp(app.id.clone())),
                is_suggested: false,
                relevance_score: 1.0,
            });
        }

        items.push(LauncherItem {
            id: "search-files".to_string(),
            name: "Search Files".to_string(),
            description: "Search for files on the system".to_string(),
            icon: "search".to_string(),
            app: None,
            action: Some(LauncherAction::SearchFiles(String::new())),
            is_suggested: false,
            relevance_score: 0.8,
        });

        info!("Launcher populated with {} items", items.len());
    }

    pub fn show_notification(&self, notification: Notification) {
        let title = notification.title.clone();
        let mut notifications = self.notifications.write().unwrap();

        // Evict oldest non-action notifications when at capacity
        if notifications.len() >= MAX_NOTIFICATIONS {
            let oldest_non_action = notifications
                .values()
                .filter(|n| !n.requires_action)
                .min_by_key(|n| n.timestamp)
                .map(|n| n.id);
            if let Some(id) = oldest_non_action {
                notifications.remove(&id);
            }
        }

        notifications.insert(notification.id, notification);
        info!("Notification shown: {}", title);
    }

    pub fn dismiss_notification(&self, id: NotificationId) -> Result<(), ShellError> {
        let mut notifications = self.notifications.write().unwrap();
        if !notifications.contains_key(&id) {
            return Err(ShellError::NotificationNotFound(id));
        }
        notifications.remove(&id);
        info!("Notification dismissed: {}", id);
        Ok(())
    }

    pub fn show_agent_notification(&self, title: String, body: String, requires_action: bool) {
        let notification = Notification {
            id: Uuid::new_v4(),
            app_name: "AGNOS Agent".to_string(),
            title,
            body,
            priority: if requires_action {
                NotificationPriority::High
            } else {
                NotificationPriority::Normal
            },
            timestamp: chrono::Utc::now(),
            requires_action,
            is_agent_related: true,
        };
        self.show_notification(notification);
    }

    pub fn request_human_override(&self, agent_name: String, action: String, reason: String) {
        let agent_name_clone = agent_name.clone();
        let notification = Notification {
            id: Uuid::new_v4(),
            app_name: format!("Agent: {}", agent_name_clone),
            title: "Human Override Requested".to_string(),
            body: format!(
                "{}\n\nAction: {}\nReason: {}",
                agent_name_clone, action, reason
            ),
            priority: NotificationPriority::Critical,
            timestamp: chrono::Utc::now(),
            requires_action: true,
            is_agent_related: true,
        };
        self.show_notification(notification);
        warn!("Human override requested by {}", agent_name);
    }

    pub fn toggle_quick_setting(&self, setting_id: &str) -> Result<(), ShellError> {
        let mut settings = self.quick_settings.write().unwrap();
        let setting = settings
            .iter_mut()
            .find(|s| s.id == setting_id)
            .ok_or(ShellError::AppNotFound(setting_id.to_string()))?;

        setting.is_active = !setting.is_active;
        (setting.on_activate)();

        info!(
            "Quick setting {} toggled to {}",
            setting_id, setting.is_active
        );
        Ok(())
    }

    pub fn search_launcher(&self, query: &str) -> Vec<LauncherItem> {
        let items = self.launcher_items.read().unwrap();
        let query = query.to_lowercase();

        let mut results: Vec<_> = items
            .iter()
            .filter(|item| {
                item.name.to_lowercase().contains(&query)
                    || item.description.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());

        if !query.is_empty() {
            for item in &mut results {
                if item.name.to_lowercase().starts_with(&query) {
                    item.relevance_score *= 1.5;
                }
            }
        }

        results
    }

    pub fn launch_app(&self, app_id: &str) -> Result<(), ShellError> {
        let registry = self.app_registry.read().unwrap();
        if !registry.contains_key(app_id) {
            return Err(ShellError::AppNotFound(app_id.to_string()));
        }

        info!("Launching application: {}", app_id);
        Ok(())
    }

    pub fn lock_screen(&self) {
        *self.is_locked.write().unwrap() = true;
        info!("Screen locked");
    }

    pub fn unlock_screen(&self) {
        *self.is_locked.write().unwrap() = false;
        info!("Screen unlocked");
    }

    pub fn is_locked(&self) -> bool {
        *self.is_locked.read().unwrap()
    }

    pub fn toggle_panel(&self) {
        let mut visible = self.panel_visible.write().unwrap();
        *visible = !*visible;
        info!("Panel visibility: {}", *visible);
    }

    pub fn get_notifications(&self) -> Vec<Notification> {
        self.notifications
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    pub fn get_quick_settings(&self) -> Vec<QuickSetting> {
        self.quick_settings.read().unwrap().clone()
    }

    pub fn get_system_status(&self) -> SystemStatus {
        self.system_status.read().unwrap().clone()
    }

    pub fn update_system_status(&self, status: SystemStatus) {
        *self.system_status.write().unwrap() = status;
    }

    pub fn set_agent_count(&self, count: usize) {
        let mut status = self.system_status.write().unwrap();
        status.agent_count = count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_priority_variants() {
        assert!(matches!(
            NotificationPriority::Low,
            NotificationPriority::Low
        ));
        assert!(matches!(
            NotificationPriority::Normal,
            NotificationPriority::Normal
        ));
        assert!(matches!(
            NotificationPriority::High,
            NotificationPriority::High
        ));
        assert!(matches!(
            NotificationPriority::Critical,
            NotificationPriority::Critical
        ));
    }

    #[test]
    fn test_notification_default() {
        let notification = Notification::default();
        assert_eq!(notification.priority, NotificationPriority::Normal);
        assert!(!notification.requires_action);
        assert!(!notification.is_agent_related);
    }

    #[test]
    fn test_notification_custom() {
        let notification = Notification {
            id: Uuid::new_v4(),
            app_name: "test".to_string(),
            title: "Test Title".to_string(),
            body: "Test Body".to_string(),
            priority: NotificationPriority::High,
            timestamp: chrono::Utc::now(),
            requires_action: true,
            is_agent_related: true,
        };
        assert_eq!(notification.title, "Test Title");
        assert!(notification.requires_action);
    }

    #[test]
    fn test_system_status() {
        let status = SystemStatus {
            cpu_usage: 50.0,
            memory_usage: 70.0,
            disk_usage: 40.0,
            battery_level: Some(80),
            agent_count: 3,
            network_status: NetworkStatus::Connected,
        };
        assert_eq!(status.cpu_usage, 50.0);
        assert_eq!(status.agent_count, 3);
    }

    #[test]
    fn test_network_status() {
        assert!(matches!(NetworkStatus::Connected, NetworkStatus::Connected));
        assert!(matches!(
            NetworkStatus::Disconnected,
            NetworkStatus::Disconnected
        ));
        assert!(matches!(
            NetworkStatus::Connecting,
            NetworkStatus::Connecting
        ));
    }

    #[test]
    fn test_app_category() {
        assert!(matches!(AppCategory::System, AppCategory::System));
        assert!(matches!(AppCategory::AI, AppCategory::AI));
        assert!(matches!(AppCategory::Other, AppCategory::Other));
    }

    #[test]
    fn test_launcher_item() {
        let item = LauncherItem {
            id: "test".to_string(),
            name: "Test App".to_string(),
            description: "A test application".to_string(),
            icon: "app".to_string(),
            app: None,
            action: None,
            is_suggested: false,
            relevance_score: 0.9,
        };
        assert_eq!(item.name, "Test App");
        assert_eq!(item.relevance_score, 0.9);
    }

    #[test]
    fn test_desktop_shell_new() {
        let shell = DesktopShell::new();
        let quick_settings = shell.get_quick_settings();
        assert!(!quick_settings.is_empty());
        assert!(shell.get_notifications().is_empty());
    }

    #[test]
    fn test_desktop_shell_notification_lifecycle() {
        let shell = DesktopShell::new();
        let notification = Notification {
            id: Uuid::new_v4(),
            app_name: "test".to_string(),
            title: "Test".to_string(),
            body: "Body".to_string(),
            priority: NotificationPriority::Normal,
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_agent_related: false,
        };
        let id = notification.id;
        shell.show_notification(notification);
        assert_eq!(shell.get_notifications().len(), 1);
        shell.dismiss_notification(id).unwrap();
        assert!(shell.get_notifications().is_empty());
    }

    #[test]
    fn test_desktop_shell_dismiss_nonexistent() {
        let shell = DesktopShell::new();
        let result = shell.dismiss_notification(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_desktop_shell_agent_notification() {
        let shell = DesktopShell::new();
        shell.show_agent_notification("Title".to_string(), "Body".to_string(), false);
        let notifications = shell.get_notifications();
        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].is_agent_related);
        assert!(!notifications[0].requires_action);
    }

    #[test]
    fn test_desktop_shell_agent_notification_requires_action() {
        let shell = DesktopShell::new();
        shell.show_agent_notification("Title".to_string(), "Body".to_string(), true);
        let notifications = shell.get_notifications();
        assert!(notifications[0].requires_action);
        assert_eq!(notifications[0].priority, NotificationPriority::High);
    }

    #[test]
    fn test_desktop_shell_lock_screen() {
        let shell = DesktopShell::new();
        assert!(!shell.is_locked());
        shell.lock_screen();
        assert!(shell.is_locked());
        shell.unlock_screen();
        assert!(!shell.is_locked());
    }

    #[test]
    fn test_desktop_shell_toggle_quick_setting() {
        let shell = DesktopShell::new();
        let settings_before = shell.get_quick_settings();
        let wifi_before = settings_before.iter().find(|s| s.id == "wifi").unwrap();
        assert!(wifi_before.is_active);
        shell.toggle_quick_setting("wifi").unwrap();
        let settings_after = shell.get_quick_settings();
        let wifi_after = settings_after.iter().find(|s| s.id == "wifi").unwrap();
        assert!(!wifi_after.is_active);
    }

    #[test]
    fn test_desktop_shell_toggle_quick_setting_nonexistent() {
        let shell = DesktopShell::new();
        let result = shell.toggle_quick_setting("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_desktop_shell_launch_app() {
        let shell = DesktopShell::new();
        assert!(shell.launch_app("terminal").is_ok());
        assert!(shell.launch_app("nonexistent").is_err());
    }

    #[test]
    fn test_desktop_shell_search_launcher() {
        let shell = DesktopShell::new();
        let results = shell.search_launcher("terminal");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .any(|r| r.name.to_lowercase().contains("terminal")));
    }

    #[test]
    fn test_desktop_shell_toggle_panel() {
        let shell = DesktopShell::new();
        shell.toggle_panel();
    }

    #[test]
    fn test_desktop_shell_update_system_status() {
        let shell = DesktopShell::new();
        let status = SystemStatus {
            cpu_usage: 50.0,
            memory_usage: 60.0,
            disk_usage: 70.0,
            battery_level: Some(80),
            agent_count: 5,
            network_status: NetworkStatus::Connected,
        };
        shell.update_system_status(status);
        let current = shell.get_system_status();
        assert_eq!(current.cpu_usage, 50.0);
        assert_eq!(current.agent_count, 5);
    }

    #[test]
    fn test_desktop_shell_set_agent_count() {
        let shell = DesktopShell::new();
        shell.set_agent_count(10);
        assert_eq!(shell.get_system_status().agent_count, 10);
    }

    #[test]
    fn test_desktop_shell_human_override_request() {
        let shell = DesktopShell::new();
        shell.request_human_override(
            "test-agent".to_string(),
            "delete".to_string(),
            "dangerous".to_string(),
        );
        let notifications = shell.get_notifications();
        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].requires_action);
        assert_eq!(notifications[0].priority, NotificationPriority::Critical);
    }

    #[test]
    fn test_quick_setting_clone() {
        let qs = QuickSetting {
            id: "test".to_string(),
            name: "Test".to_string(),
            icon: "test-icon".to_string(),
            is_active: true,
            on_activate: Box::new(|| {}),
        };
        let cloned = qs.clone();
        assert_eq!(cloned.id, qs.id);
        assert_eq!(cloned.name, qs.name);
    }

    #[test]
    fn test_launcher_action() {
        let action = LauncherAction::OpenApp("terminal".to_string());
        assert!(matches!(action, LauncherAction::OpenApp(_)));
        let action = LauncherAction::RunCommand("ls".to_string());
        assert!(matches!(action, LauncherAction::RunCommand(_)));
        let action = LauncherAction::SearchFiles("query".to_string());
        assert!(matches!(action, LauncherAction::SearchFiles(_)));
        let action = LauncherAction::WebSearch("search".to_string());
        assert!(matches!(action, LauncherAction::WebSearch(_)));
    }

    #[test]
    fn test_shell_error_variants() {
        let err = ShellError::NotificationNotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));
        let err = ShellError::AppNotFound("test".to_string());
        assert!(err.to_string().contains("not found"));
        let err = ShellError::PermissionDenied("test".to_string());
        assert!(err.to_string().contains("denied"));
    }
}
