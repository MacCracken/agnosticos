//! Shell integration: bridges external app APIs (system tray, window management,
//! notifications) to the AGNOS compositor and shell panel.

use thiserror::Error;
use uuid::Uuid;

use crate::compositor::SurfaceId;
use crate::shell::{Notification, NotificationPriority};

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum ShellIntegrationError {
    #[error("Tray item already exists: {0}")]
    DuplicateTrayItem(String),
    #[error("Tray item not found: {0}")]
    TrayItemNotFound(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

// ============================================================================
// System tray types
// ============================================================================

/// An item displayed in the AGNOS system tray / shell panel.
#[derive(Debug, Clone)]
pub struct SystemTrayItem {
    pub id: String,
    pub app_name: String,
    pub icon: String,
    pub tooltip: String,
    pub visible: bool,
    pub menu_items: Vec<TrayMenuItem>,
}

/// A single entry in a tray item's context menu.
#[derive(Debug, Clone)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub action: TrayAction,
    pub enabled: bool,
}

/// Action triggered when a tray menu item is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    Activate,
    ShowMenu,
    Quit,
    Custom(String),
}

// ============================================================================
// Window management
// ============================================================================

/// Requests that external applications can make to the compositor.
#[derive(Debug, Clone)]
pub enum WindowManagementRequest {
    Minimize(SurfaceId),
    Maximize(SurfaceId),
    Close(SurfaceId),
    SetTitle(SurfaceId, String),
    SetPosition(SurfaceId, i32, i32),
    SetSize(SurfaceId, u32, u32),
    Focus(SurfaceId),
    SetAlwaysOnTop(SurfaceId, bool),
}

/// Result of processing a window management request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowManagementResult {
    /// The request was accepted; the compositor will carry it out.
    Accepted,
    /// The target surface was not found.
    SurfaceNotFound(SurfaceId),
    /// The request was denied (e.g. sandbox restriction).
    Denied(String),
}

// ============================================================================
// Notification bridging
// ============================================================================

/// Urgency level used by external notification senders (freedesktop-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

/// A notification in the external (freedesktop) format.
#[derive(Debug, Clone)]
pub struct ExternalNotification {
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub urgency: Urgency,
    pub icon: String,
    pub timeout_ms: u32,
    pub actions: Vec<(String, String)>,
}

/// Converts between external notification formats and AGNOS `Notification`.
#[derive(Debug)]
pub struct NotificationBridge;

impl NotificationBridge {
    /// Convert an external notification into an AGNOS shell `Notification`.
    pub fn convert(external: &ExternalNotification) -> Notification {
        Notification {
            id: Uuid::new_v4(),
            app_name: external.app_name.clone(),
            title: external.title.clone(),
            body: external.body.clone(),
            priority: ShellIntegrationManager::map_urgency(&external.urgency),
            timestamp: chrono::Utc::now(),
            requires_action: !external.actions.is_empty(),
            is_agent_related: false,
        }
    }
}

// ============================================================================
// Shell integration manager
// ============================================================================

/// Central manager that bridges external app APIs to the AGNOS shell and compositor.
#[derive(Debug)]
pub struct ShellIntegrationManager {
    tray_items: Vec<SystemTrayItem>,
}

impl ShellIntegrationManager {
    /// Create a new, empty shell integration manager.
    pub fn new() -> Self {
        Self {
            tray_items: Vec::new(),
        }
    }

    // -- Tray item CRUD -----------------------------------------------------

    /// Register a new system tray item. Returns an error if the id is already taken.
    pub fn register_tray_item(
        &mut self,
        item: SystemTrayItem,
    ) -> Result<(), ShellIntegrationError> {
        if self.tray_items.iter().any(|t| t.id == item.id) {
            return Err(ShellIntegrationError::DuplicateTrayItem(item.id));
        }
        self.tray_items.push(item);
        Ok(())
    }

    /// Remove a system tray item by id.
    pub fn remove_tray_item(&mut self, id: &str) -> Result<(), ShellIntegrationError> {
        let pos = self
            .tray_items
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| ShellIntegrationError::TrayItemNotFound(id.to_string()))?;
        self.tray_items.remove(pos);
        Ok(())
    }

    /// Get an immutable slice of all registered tray items.
    pub fn get_tray_items(&self) -> &[SystemTrayItem] {
        &self.tray_items
    }

    // -- Window management --------------------------------------------------

    /// Process a window management request and return the result.
    ///
    /// In a full implementation this would forward to the compositor; here we
    /// validate the request and return `Accepted`.
    pub fn process_window_request(
        &self,
        request: &WindowManagementRequest,
    ) -> WindowManagementResult {
        match request {
            WindowManagementRequest::SetSize(_, w, h) => {
                if *w == 0 || *h == 0 {
                    return WindowManagementResult::Denied(
                        "Width and height must be non-zero".to_string(),
                    );
                }
                WindowManagementResult::Accepted
            }
            WindowManagementRequest::SetTitle(_, title) => {
                if title.is_empty() {
                    return WindowManagementResult::Denied("Title must not be empty".to_string());
                }
                WindowManagementResult::Accepted
            }
            _ => WindowManagementResult::Accepted,
        }
    }

    // -- Notification bridging ----------------------------------------------

    /// Convert an external notification to the AGNOS internal format.
    pub fn bridge_notification(&self, external: &ExternalNotification) -> Notification {
        NotificationBridge::convert(external)
    }

    /// Map freedesktop `Urgency` to AGNOS `NotificationPriority`.
    pub fn map_urgency(urgency: &Urgency) -> NotificationPriority {
        match urgency {
            Urgency::Low => NotificationPriority::Low,
            Urgency::Normal => NotificationPriority::Normal,
            Urgency::Critical => NotificationPriority::Critical,
        }
    }
}

impl Default for ShellIntegrationManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tray_item(id: &str) -> SystemTrayItem {
        SystemTrayItem {
            id: id.to_string(),
            app_name: format!("App {}", id),
            icon: "icon.png".to_string(),
            tooltip: format!("Tooltip for {}", id),
            visible: true,
            menu_items: vec![TrayMenuItem {
                id: "quit".to_string(),
                label: "Quit".to_string(),
                action: TrayAction::Quit,
                enabled: true,
            }],
        }
    }

    fn make_external_notification() -> ExternalNotification {
        ExternalNotification {
            app_name: "Firefox".to_string(),
            title: "Download complete".to_string(),
            body: "file.zip has been downloaded".to_string(),
            urgency: Urgency::Normal,
            icon: "download".to_string(),
            timeout_ms: 5000,
            actions: vec![],
        }
    }

    // -- Tray CRUD ----------------------------------------------------------

    #[test]
    fn test_register_tray_item() {
        let mut mgr = ShellIntegrationManager::new();
        let item = make_tray_item("app1");
        assert!(mgr.register_tray_item(item).is_ok());
        assert_eq!(mgr.get_tray_items().len(), 1);
    }

    #[test]
    fn test_register_multiple_tray_items() {
        let mut mgr = ShellIntegrationManager::new();
        mgr.register_tray_item(make_tray_item("a")).unwrap();
        mgr.register_tray_item(make_tray_item("b")).unwrap();
        mgr.register_tray_item(make_tray_item("c")).unwrap();
        assert_eq!(mgr.get_tray_items().len(), 3);
    }

    #[test]
    fn test_register_duplicate_tray_item() {
        let mut mgr = ShellIntegrationManager::new();
        mgr.register_tray_item(make_tray_item("dup")).unwrap();
        let result = mgr.register_tray_item(make_tray_item("dup"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ShellIntegrationError::DuplicateTrayItem(_)
        ));
    }

    #[test]
    fn test_remove_tray_item() {
        let mut mgr = ShellIntegrationManager::new();
        mgr.register_tray_item(make_tray_item("rm")).unwrap();
        assert!(mgr.remove_tray_item("rm").is_ok());
        assert!(mgr.get_tray_items().is_empty());
    }

    #[test]
    fn test_remove_nonexistent_tray_item() {
        let mut mgr = ShellIntegrationManager::new();
        let result = mgr.remove_tray_item("ghost");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ShellIntegrationError::TrayItemNotFound(_)
        ));
    }

    #[test]
    fn test_get_tray_items_empty() {
        let mgr = ShellIntegrationManager::new();
        assert!(mgr.get_tray_items().is_empty());
    }

    #[test]
    fn test_tray_item_fields() {
        let item = make_tray_item("test");
        assert_eq!(item.id, "test");
        assert_eq!(item.app_name, "App test");
        assert!(item.visible);
        assert_eq!(item.menu_items.len(), 1);
        assert_eq!(item.menu_items[0].action, TrayAction::Quit);
    }

    #[test]
    fn test_tray_action_variants() {
        assert_eq!(TrayAction::Activate, TrayAction::Activate);
        assert_eq!(TrayAction::ShowMenu, TrayAction::ShowMenu);
        assert_eq!(TrayAction::Quit, TrayAction::Quit);
        assert_eq!(
            TrayAction::Custom("open".to_string()),
            TrayAction::Custom("open".to_string())
        );
        assert_ne!(TrayAction::Activate, TrayAction::Quit);
    }

    #[test]
    fn test_register_then_remove_then_reregister() {
        let mut mgr = ShellIntegrationManager::new();
        mgr.register_tray_item(make_tray_item("cycle")).unwrap();
        mgr.remove_tray_item("cycle").unwrap();
        // Should be able to re-register the same id after removal
        assert!(mgr.register_tray_item(make_tray_item("cycle")).is_ok());
        assert_eq!(mgr.get_tray_items().len(), 1);
    }

    // -- Window management --------------------------------------------------

    #[test]
    fn test_process_minimize_request() {
        let mgr = ShellIntegrationManager::new();
        let id = Uuid::new_v4();
        let result = mgr.process_window_request(&WindowManagementRequest::Minimize(id));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_maximize_request() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::Maximize(Uuid::new_v4()));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_close_request() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::Close(Uuid::new_v4()));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_focus_request() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::Focus(Uuid::new_v4()));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_set_position_request() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::SetPosition(
            Uuid::new_v4(),
            100,
            200,
        ));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_set_size_valid() {
        let mgr = ShellIntegrationManager::new();
        let result =
            mgr.process_window_request(&WindowManagementRequest::SetSize(Uuid::new_v4(), 800, 600));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_set_size_zero_denied() {
        let mgr = ShellIntegrationManager::new();
        let result =
            mgr.process_window_request(&WindowManagementRequest::SetSize(Uuid::new_v4(), 0, 600));
        assert!(matches!(result, WindowManagementResult::Denied(_)));

        let result2 =
            mgr.process_window_request(&WindowManagementRequest::SetSize(Uuid::new_v4(), 800, 0));
        assert!(matches!(result2, WindowManagementResult::Denied(_)));
    }

    #[test]
    fn test_process_set_title_valid() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::SetTitle(
            Uuid::new_v4(),
            "My Window".to_string(),
        ));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    #[test]
    fn test_process_set_title_empty_denied() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::SetTitle(
            Uuid::new_v4(),
            String::new(),
        ));
        assert!(matches!(result, WindowManagementResult::Denied(_)));
    }

    #[test]
    fn test_process_set_always_on_top() {
        let mgr = ShellIntegrationManager::new();
        let result = mgr.process_window_request(&WindowManagementRequest::SetAlwaysOnTop(
            Uuid::new_v4(),
            true,
        ));
        assert_eq!(result, WindowManagementResult::Accepted);
    }

    // -- Notification bridging ----------------------------------------------

    #[test]
    fn test_bridge_notification_basic() {
        let mgr = ShellIntegrationManager::new();
        let ext = make_external_notification();
        let notif = mgr.bridge_notification(&ext);
        assert_eq!(notif.app_name, "Firefox");
        assert_eq!(notif.title, "Download complete");
        assert_eq!(notif.body, "file.zip has been downloaded");
        assert_eq!(notif.priority, NotificationPriority::Normal);
        assert!(!notif.requires_action);
        assert!(!notif.is_agent_related);
    }

    #[test]
    fn test_bridge_notification_with_actions_sets_requires_action() {
        let mgr = ShellIntegrationManager::new();
        let ext = ExternalNotification {
            actions: vec![("open".to_string(), "Open File".to_string())],
            ..make_external_notification()
        };
        let notif = mgr.bridge_notification(&ext);
        assert!(notif.requires_action);
    }

    #[test]
    fn test_urgency_mapping_low() {
        assert_eq!(
            ShellIntegrationManager::map_urgency(&Urgency::Low),
            NotificationPriority::Low
        );
    }

    #[test]
    fn test_urgency_mapping_normal() {
        assert_eq!(
            ShellIntegrationManager::map_urgency(&Urgency::Normal),
            NotificationPriority::Normal
        );
    }

    #[test]
    fn test_urgency_mapping_critical() {
        assert_eq!(
            ShellIntegrationManager::map_urgency(&Urgency::Critical),
            NotificationPriority::Critical
        );
    }

    #[test]
    fn test_bridge_critical_notification() {
        let mgr = ShellIntegrationManager::new();
        let ext = ExternalNotification {
            urgency: Urgency::Critical,
            ..make_external_notification()
        };
        let notif = mgr.bridge_notification(&ext);
        assert_eq!(notif.priority, NotificationPriority::Critical);
    }

    #[test]
    fn test_default_impl() {
        let mgr = ShellIntegrationManager::default();
        assert!(mgr.get_tray_items().is_empty());
    }
}
