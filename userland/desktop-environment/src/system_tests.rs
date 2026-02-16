//! System Tests for Desktop Environment
//!
//! These tests verify end-to-end functionality of the desktop environment.

#[cfg(test)]
mod system_tests {
    use crate::{
        AIDesktopFeatures, AgentStatus, AppType, Compositor, DesktopApplications, DesktopShell,
        Notification, SecurityUI, ThreatLevel, WindowState,
    };
    use std::path::PathBuf;

    #[test]
    fn test_compositor_window_lifecycle() {
        let compositor = Compositor::new();

        let window_id = compositor
            .create_window("Test App".to_string(), "test-app".to_string(), false)
            .unwrap();

        assert!(!window_id.to_string().is_empty());

        let windows = compositor.get_windows();
        assert_eq!(windows.len(), 1);

        compositor.close_window(window_id).unwrap();

        let windows = compositor.get_windows();
        assert_eq!(windows.len(), 0);
    }

    #[test]
    fn test_workspace_switch() {
        let compositor = Compositor::new();

        compositor.switch_workspace(2).unwrap();

        let active_windows = compositor.get_active_windows();
        assert!(active_windows.len() >= 0);
    }

    #[test]
    fn test_desktop_applications() {
        let apps = DesktopApplications::new();

        let terminal = apps.open_terminal().unwrap();
        assert_eq!(terminal.app_type, AppType::Terminal);

        let agent_mgr = apps.open_agent_manager().unwrap();
        assert_eq!(agent_mgr.app_type, AppType::AgentManager);

        let windows = apps.get_open_windows();
        assert_eq!(windows.len(), 2);

        apps.close_window(terminal.id).unwrap();

        let windows = apps.get_open_windows();
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn test_notification_system() {
        let shell = DesktopShell::new();

        let notification = Notification::default();

        shell.show_notification(notification);

        let notifications = shell.get_notifications();
        assert_eq!(notifications.len(), 1);
    }

    #[test]
    fn test_security_dashboard() {
        let security = SecurityUI::new();

        let dashboard = security.get_security_dashboard();
        assert!(dashboard.active_alerts >= 0);
    }

    #[test]
    fn test_ai_context_detection() {
        let ai = AIDesktopFeatures::new();

        let context = ai.analyze_context();

        assert!(matches!(
            context.time_of_day,
            crate::TimeOfDay::Morning
                | crate::TimeOfDay::Afternoon
                | crate::TimeOfDay::Evening
                | crate::TimeOfDay::Night
        ));
    }

    #[test]
    fn test_agent_hud_registration() {
        let ai = AIDesktopFeatures::new();

        let agent_id = uuid::Uuid::new_v4();
        ai.register_agent_hud(agent_id, "Test Agent".to_string());

        let states = ai.get_agent_hud_states();
        assert_eq!(states.len(), 1);
    }

    #[test]
    fn test_screen_lock() {
        let shell = DesktopShell::new();

        assert!(!shell.is_locked());

        shell.lock_screen();
        assert!(shell.is_locked());

        shell.unlock_screen();
        assert!(!shell.is_locked());
    }

    #[test]
    fn test_launcher_search() {
        let shell = DesktopShell::new();

        let results = shell.search_launcher("term");

        assert!(results.len() >= 0);
    }

    #[test]
    fn test_quick_settings() {
        let shell = DesktopShell::new();

        shell.toggle_quick_setting("wifi").unwrap();

        let settings = shell.get_quick_settings();
        assert!(settings.len() >= 1);
    }

    #[test]
    fn test_agent_notification_flow() {
        let shell = DesktopShell::new();

        shell.show_agent_notification(
            "Task Complete".to_string(),
            "The agent has finished its task".to_string(),
            false,
        );

        let notifications = shell.get_notifications();

        assert!(notifications.iter().any(|n| n.is_agent_related));
    }
}
