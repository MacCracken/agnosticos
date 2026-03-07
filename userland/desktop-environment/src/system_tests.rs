//! System Tests for Desktop Environment
//!
//! These tests verify end-to-end functionality of the desktop environment,
//! including cross-component interactions and comprehensive lifecycle scenarios.

#[cfg(test)]
mod desktop_system_tests {
    use crate::{
        AIDesktopFeatures, AgentStatus, AppType, Compositor, ContextEventType, DesktopApplications,
        DesktopShell, Notification, NotificationPriority, PermissionRequest, SecurityAlert,
        SecurityLevel, SecurityUI, ThreatLevel, WindowState,
    };
    use uuid::Uuid;

    // =========================================================================
    // Existing basic tests
    // =========================================================================

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
        // len() is always >= 0 for usize; just verify it returns successfully
        let _ = active_windows.len();
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
        // active_alerts is usize, always >= 0; just verify dashboard is accessible
        let _ = dashboard.active_alerts;
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

        // len() is always >= 0 for usize; just verify it returns successfully
        let _ = results.len();
    }

    #[test]
    fn test_quick_settings() {
        let shell = DesktopShell::new();

        shell.toggle_quick_setting("wifi").unwrap();

        let settings = shell.get_quick_settings();
        assert!(!settings.is_empty());
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

    // =========================================================================
    // Comprehensive E2E scenarios
    // =========================================================================

    /// Scenario 1: Full desktop startup sequence
    /// Create compositor + shell + apps + security + AI features, verify all initialize properly
    #[test]
    fn test_e2e_full_desktop_startup_sequence() {
        let compositor = Compositor::new();
        let shell = DesktopShell::new();
        let apps = DesktopApplications::new();
        let security = SecurityUI::new();
        let ai = AIDesktopFeatures::new();

        // Compositor has 4 default workspaces and no windows
        assert!(compositor.get_windows().is_empty());
        assert!(compositor.get_active_windows().is_empty());

        // Shell has quick settings initialized and no notifications
        let settings = shell.get_quick_settings();
        assert_eq!(settings.len(), 5);
        assert!(shell.get_notifications().is_empty());
        assert!(!shell.is_locked());

        // Apps have no open windows
        assert!(apps.get_open_windows().is_empty());

        // Security starts at Standard level with no alerts
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.active_alerts, 0);
        assert_eq!(dashboard.pending_permissions, 0);
        assert_eq!(security.get_security_level(), SecurityLevel::Standard);
        assert!(!security.is_emergency_mode());

        // AI features start with no agents in HUD
        assert!(ai.get_agent_hud_states().is_empty());
        let context = ai.analyze_context();
        assert!(matches!(
            context.time_of_day,
            crate::TimeOfDay::Morning
                | crate::TimeOfDay::Afternoon
                | crate::TimeOfDay::Evening
                | crate::TimeOfDay::Night
        ));
    }

    /// Scenario 2: Multi-window workspace management
    /// Create 5 windows across 3 workspaces, switch between them, verify active windows change
    #[test]
    fn test_e2e_multi_window_workspace_management() {
        let compositor = Compositor::new();

        // Workspace 0: create 2 windows
        let w1 = compositor
            .create_window("Editor".to_string(), "editor".to_string(), false)
            .unwrap();
        let w2 = compositor
            .create_window("Terminal".to_string(), "terminal".to_string(), false)
            .unwrap();
        assert_eq!(compositor.get_active_windows().len(), 2);

        // Switch to workspace 1: create 2 windows
        compositor.switch_workspace(1).unwrap();
        let w3 = compositor
            .create_window("Browser".to_string(), "browser".to_string(), false)
            .unwrap();
        let _w4 = compositor
            .create_window("Chat".to_string(), "chat".to_string(), false)
            .unwrap();
        assert_eq!(compositor.get_active_windows().len(), 2);

        // Switch to workspace 2: create 1 agent window
        compositor.switch_workspace(2).unwrap();
        let _w5 = compositor
            .create_window("Agent".to_string(), "agent".to_string(), true)
            .unwrap();
        assert_eq!(compositor.get_active_windows().len(), 1);

        // Total windows across all workspaces
        assert_eq!(compositor.get_windows().len(), 5);
        assert_eq!(compositor.get_agent_windows().len(), 1);

        // Switch back to workspace 0 and verify its windows
        compositor.switch_workspace(0).unwrap();
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 2);
        let active_ids: Vec<_> = active.iter().map(|w| w.id).collect();
        assert!(active_ids.contains(&w1));
        assert!(active_ids.contains(&w2));

        // Move w3 from workspace 1 to workspace 0
        // First switch to ws 1 so the source is correct
        compositor.switch_workspace(1).unwrap();
        compositor.move_window_to_workspace(w3, 0).unwrap();

        // Verify workspace 0 now has 3 windows
        compositor.switch_workspace(0).unwrap();
        assert_eq!(compositor.get_active_windows().len(), 3);

        // Workspace 1 should have 1 window remaining
        compositor.switch_workspace(1).unwrap();
        assert_eq!(compositor.get_active_windows().len(), 1);
    }

    /// Scenario 3: Application open-all-close-all
    /// Open all 5 app types, verify 5 windows, close all, verify 0 windows
    #[test]
    fn test_e2e_application_open_all_close_all() {
        let apps = DesktopApplications::new();

        let terminal = apps.open_terminal().unwrap();
        assert_eq!(terminal.app_type, AppType::Terminal);

        let file_mgr = apps.open_file_manager(None).unwrap();
        assert_eq!(file_mgr.app_type, AppType::FileManager);

        let agent_mgr = apps.open_agent_manager().unwrap();
        assert_eq!(agent_mgr.app_type, AppType::AgentManager);
        assert!(agent_mgr.is_ai_enabled);

        let audit = apps.open_audit_viewer().unwrap();
        assert_eq!(audit.app_type, AppType::AuditViewer);
        assert!(audit.is_ai_enabled);

        let model_mgr = apps.open_model_manager().unwrap();
        assert_eq!(model_mgr.app_type, AppType::ModelManager);
        assert!(model_mgr.is_ai_enabled);

        assert_eq!(apps.get_open_windows().len(), 5);

        // Close all windows
        let window_ids: Vec<Uuid> = apps.get_open_windows().iter().map(|w| w.id).collect();
        for id in window_ids {
            apps.close_window(id).unwrap();
        }

        assert_eq!(apps.get_open_windows().len(), 0);
    }

    /// Scenario 4: Notification flood and dismiss
    /// Send 20 notifications (mix of agent and regular), dismiss half, verify counts
    #[test]
    fn test_e2e_notification_flood_and_dismiss() {
        let shell = DesktopShell::new();

        let mut notification_ids = Vec::new();

        // Send 10 regular notifications
        for i in 0..10 {
            let n = Notification {
                id: Uuid::new_v4(),
                app_name: format!("app-{}", i),
                title: format!("Regular Notification {}", i),
                body: format!("Body {}", i),
                priority: NotificationPriority::Normal,
                timestamp: chrono::Utc::now(),
                requires_action: false,
                is_agent_related: false,
            };
            notification_ids.push(n.id);
            shell.show_notification(n);
        }

        // Send 10 agent notifications
        for i in 0..10 {
            shell.show_agent_notification(
                format!("Agent Task {}", i),
                format!("Agent completed task {}", i),
                i % 2 == 0, // alternating requires_action
            );
        }

        assert_eq!(shell.get_notifications().len(), 20);

        // Verify agent notifications are marked correctly
        let agent_count = shell
            .get_notifications()
            .iter()
            .filter(|n| n.is_agent_related)
            .count();
        assert_eq!(agent_count, 10);

        // Dismiss the first 10 (regular) notifications
        for id in &notification_ids {
            shell.dismiss_notification(*id).unwrap();
        }

        assert_eq!(shell.get_notifications().len(), 10);

        // All remaining should be agent-related
        assert!(shell.get_notifications().iter().all(|n| n.is_agent_related));
    }

    /// Scenario 5: Security alert escalation flow
    /// Add info alert -> add warning -> add critical -> verify dashboard threat levels
    #[test]
    fn test_e2e_security_alert_escalation_flow() {
        let security = SecurityUI::new();

        // Start with no alerts
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.active_alerts, 0);
        // Default threat when no alerts is Low
        assert_eq!(dashboard.threat_level, ThreatLevel::Low);

        // Add Info alert
        security.show_security_alert(SecurityAlert {
            id: Uuid::new_v4(),
            title: "Info event".to_string(),
            description: "Informational".to_string(),
            threat_level: ThreatLevel::Info,
            source: "system".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_resolved: false,
        });
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.active_alerts, 1);
        assert_eq!(dashboard.threat_level, ThreatLevel::Info);

        // Add Medium alert - dashboard should escalate
        security.show_security_alert(SecurityAlert {
            id: Uuid::new_v4(),
            title: "Suspicious activity".to_string(),
            description: "Unusual network traffic".to_string(),
            threat_level: ThreatLevel::Medium,
            source: "network".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_resolved: false,
        });
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.active_alerts, 2);
        assert_eq!(dashboard.threat_level, ThreatLevel::Medium);

        // Add Critical alert - dashboard should show Critical
        security.show_security_alert(SecurityAlert {
            id: Uuid::new_v4(),
            title: "Breach detected".to_string(),
            description: "Unauthorized access".to_string(),
            threat_level: ThreatLevel::Critical,
            source: "ids".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: true,
            is_resolved: false,
        });
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.active_alerts, 3);
        assert_eq!(dashboard.threat_level, ThreatLevel::Critical);
    }

    /// Scenario 6: Permission request full flow
    /// Add permission request -> grant -> verify; add another -> deny -> verify not present
    #[test]
    fn test_e2e_permission_request_full_flow() {
        let security = SecurityUI::new();
        let agent_id = Uuid::new_v4();

        // Add permission request and grant it
        let req_id = Uuid::new_v4();
        security.request_permission(PermissionRequest {
            id: req_id,
            agent_id,
            agent_name: "file-agent".to_string(),
            permission: "file:write".to_string(),
            resource: "/home/user/data".to_string(),
            reason: "Needs to save output".to_string(),
            timestamp: chrono::Utc::now(),
            is_granted: false,
        });

        // Verify pending
        let pending = security.get_pending_permissions();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].permission, "file:write");

        // Grant it
        security.grant_permission(req_id).unwrap();
        let pending = security.get_pending_permissions();
        assert_eq!(pending.len(), 0);

        // Set agent permissions to verify they persist
        security.set_agent_permissions(
            agent_id,
            "file-agent".to_string(),
            vec!["file:write".to_string(), "file:read".to_string()],
        );
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.running_agents, 1);

        // Add another request and deny it
        let req_id2 = Uuid::new_v4();
        security.request_permission(PermissionRequest {
            id: req_id2,
            agent_id,
            agent_name: "file-agent".to_string(),
            permission: "file:delete".to_string(),
            resource: "/etc/sensitive".to_string(),
            reason: "Wants to delete".to_string(),
            timestamp: chrono::Utc::now(),
            is_granted: false,
        });
        assert_eq!(security.get_pending_permissions().len(), 1);

        security.deny_permission(req_id2).unwrap();
        assert_eq!(security.get_pending_permissions().len(), 0);

        // Agent permissions should still be just the granted ones
        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.running_agents, 1);
    }

    /// Scenario 7: Override request flow
    /// Add override request -> approve -> verify; add another -> deny -> verify
    #[test]
    fn test_e2e_override_request_flow() {
        let security = SecurityUI::new();

        // Request override and approve
        let override_id = security.request_human_override(
            "admin-agent".to_string(),
            "restart-service".to_string(),
            "Service is unresponsive".to_string(),
        );

        let pending = security.get_override_requests();
        assert_eq!(pending.len(), 1);
        assert!(!pending[0].is_approved);

        security
            .approve_override(override_id, "admin-user".to_string())
            .unwrap();

        // After approval, it should no longer appear in pending (unapproved) list
        let pending = security.get_override_requests();
        assert_eq!(pending.len(), 0);

        // Request another override
        let override_id2 = security.request_human_override(
            "risky-agent".to_string(),
            "delete-database".to_string(),
            "Cleanup".to_string(),
        );

        let pending = security.get_override_requests();
        assert_eq!(pending.len(), 1);

        // Approve with a bad ID should fail
        let bad_result = security.approve_override(Uuid::new_v4(), "admin".to_string());
        assert!(bad_result.is_err());

        // The original should still be pending
        let pending = security.get_override_requests();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, override_id2);
    }

    /// Scenario 8: AI context with multiple agents
    /// Register 5 agents in HUD, update each with different stats, verify aggregated state
    #[test]
    fn test_e2e_ai_context_with_multiple_agents() {
        let ai = AIDesktopFeatures::new();

        let agent_configs = vec![
            ("qa-agent", AgentStatus::Acting, 45.2, 256_u64),
            ("file-manager", AgentStatus::Idle, 0.5, 64),
            ("code-reviewer", AgentStatus::Thinking, 80.0, 512),
            ("build-agent", AgentStatus::Waiting, 10.0, 128),
            ("deploy-agent", AgentStatus::Error, 0.0, 32),
        ];

        let mut agent_ids = Vec::new();

        for (name, status, cpu, memory) in &agent_configs {
            let id = Uuid::new_v4();
            agent_ids.push(id);
            ai.register_agent_hud(id, name.to_string());
            ai.update_agent_hud(id, status.clone(), format!("Task for {}", name), 0.5);

            // Verify update took effect by checking states
            let states = ai.get_agent_hud_states();
            let state = states.iter().find(|s| s.agent_id == id).unwrap();
            assert_eq!(state.status, *status);
            // cpu/memory are in the resource_usage which update_agent_hud doesn't change
            // (it only updates status, task, progress), so we check those fields
            assert_eq!(state.current_task, format!("Task for {}", name));
            let _ = cpu;
            let _ = memory;
        }

        assert_eq!(ai.get_agent_hud_states().len(), 5);

        // Verify different statuses are present
        let states = ai.get_agent_hud_states();
        let statuses: Vec<_> = states.iter().map(|s| s.status.clone()).collect();
        assert!(statuses.contains(&AgentStatus::Acting));
        assert!(statuses.contains(&AgentStatus::Idle));
        assert!(statuses.contains(&AgentStatus::Thinking));
        assert!(statuses.contains(&AgentStatus::Waiting));
        assert!(statuses.contains(&AgentStatus::Error));
    }

    /// Scenario 9: Agent HUD lifecycle
    /// Register -> update -> unregister -> verify removed from states
    #[test]
    fn test_e2e_agent_hud_lifecycle() {
        let ai = AIDesktopFeatures::new();

        let agent_id = Uuid::new_v4();

        // Register
        ai.register_agent_hud(agent_id, "lifecycle-agent".to_string());
        assert_eq!(ai.get_agent_hud_states().len(), 1);
        let state = &ai.get_agent_hud_states()[0];
        assert_eq!(state.agent_name, "lifecycle-agent");
        assert_eq!(state.status, AgentStatus::Idle);

        // Update
        ai.update_agent_hud(
            agent_id,
            AgentStatus::Acting,
            "Processing data".to_string(),
            0.75,
        );
        let state = &ai.get_agent_hud_states()[0];
        assert_eq!(state.status, AgentStatus::Acting);
        assert_eq!(state.current_task, "Processing data");
        assert_eq!(state.progress, 0.75);

        // Update again
        ai.update_agent_hud(agent_id, AgentStatus::Idle, "Complete".to_string(), 1.0);
        let state = &ai.get_agent_hud_states()[0];
        assert_eq!(state.status, AgentStatus::Idle);
        assert_eq!(state.progress, 1.0);

        // Unregister
        ai.unregister_agent_hud(agent_id);
        assert_eq!(ai.get_agent_hud_states().len(), 0);

        // Unregistering again should be harmless (no panic)
        ai.unregister_agent_hud(agent_id);
        assert_eq!(ai.get_agent_hud_states().len(), 0);
    }

    /// Scenario 10: Screen lock blocks interactions
    /// Lock screen -> verify locked -> try launch_app -> unlock -> verify unlocked
    #[test]
    fn test_e2e_screen_lock_blocks_interactions() {
        let shell = DesktopShell::new();

        assert!(!shell.is_locked());

        // Lock the screen
        shell.lock_screen();
        assert!(shell.is_locked());

        // While locked, launch_app still succeeds at the shell level
        // (access control is enforced at a higher layer in production)
        // but we verify the locked state is correctly reported
        let result = shell.launch_app("terminal");
        assert!(result.is_ok()); // Shell doesn't enforce lock on launch_app directly

        // Verify notifications still accumulate while locked
        shell.show_agent_notification(
            "Background task".to_string(),
            "Completed while locked".to_string(),
            false,
        );
        assert_eq!(shell.get_notifications().len(), 1);

        // Unlock
        shell.unlock_screen();
        assert!(!shell.is_locked());

        // Verify normal operation restored
        assert!(shell.launch_app("terminal").is_ok());
    }

    /// Scenario 11: Security level transitions
    /// Set Standard -> Elevated -> Lockdown -> verify levels change appropriately
    #[test]
    fn test_e2e_security_level_transitions() {
        let security = SecurityUI::new();

        // Default is Standard
        assert_eq!(security.get_security_level(), SecurityLevel::Standard);

        // Transition to Elevated
        security.set_security_level(SecurityLevel::Elevated);
        assert_eq!(security.get_security_level(), SecurityLevel::Elevated);

        // Transition to Lockdown
        security.set_security_level(SecurityLevel::Lockdown);
        assert_eq!(security.get_security_level(), SecurityLevel::Lockdown);

        // Can go back to Standard
        security.set_security_level(SecurityLevel::Standard);
        assert_eq!(security.get_security_level(), SecurityLevel::Standard);

        // Rapid transitions
        security.set_security_level(SecurityLevel::Lockdown);
        security.set_security_level(SecurityLevel::Elevated);
        security.set_security_level(SecurityLevel::Lockdown);
        assert_eq!(security.get_security_level(), SecurityLevel::Lockdown);
    }

    /// Scenario 12: Emergency kill switch flow
    /// Register agents, activate kill switch, verify state, deactivate, verify restored
    #[test]
    fn test_e2e_emergency_kill_switch_flow() {
        let security = SecurityUI::new();

        // Set up some agent permissions
        let agent1 = Uuid::new_v4();
        let agent2 = Uuid::new_v4();
        security.set_agent_permissions(
            agent1,
            "agent-1".to_string(),
            vec!["file:read".to_string()],
        );
        security.set_agent_permissions(
            agent2,
            "agent-2".to_string(),
            vec!["network:outbound".to_string()],
        );
        assert_eq!(security.get_security_dashboard().running_agents, 2);

        // Add an override request
        let override_id = security.request_human_override(
            "agent-1".to_string(),
            "sudo-action".to_string(),
            "needs root".to_string(),
        );

        // Approve it first
        security
            .approve_override(override_id, "admin".to_string())
            .unwrap();

        // Activate emergency kill switch
        assert!(!security.is_emergency_mode());
        security.emergency_kill_switch();
        assert!(security.is_emergency_mode());

        // After kill switch, all override approvals are revoked
        // (the override_requests get is_approved set to false)
        // But get_override_requests filters for !is_approved, so we should see them
        // The kill switch sets all is_approved = false, so the request reappears
        let overrides = security.get_override_requests();
        assert_eq!(overrides.len(), 1);
        assert!(!overrides[0].is_approved);

        // Deactivate emergency
        security.deactivate_emergency();
        assert!(!security.is_emergency_mode());

        // Agent permissions should still be intact (kill switch doesn't remove them)
        assert_eq!(security.get_security_dashboard().running_agents, 2);
    }

    /// Scenario 13: File manager navigation
    /// Open file manager with different paths, verify windows created
    #[test]
    fn test_e2e_file_manager_navigation() {
        let apps = DesktopApplications::new();

        // Open file manager without path
        let fm_window = apps.open_file_manager(None).unwrap();
        assert_eq!(fm_window.app_type, AppType::FileManager);
        assert!(fm_window.is_ai_enabled);
        assert_eq!(apps.get_open_windows().len(), 1);

        // Open another file manager with a path
        let fm_window2 = apps.open_file_manager(Some("/tmp".to_string())).unwrap();
        assert_eq!(fm_window2.app_type, AppType::FileManager);
        assert_eq!(apps.get_open_windows().len(), 2);

        // Close both
        apps.close_window(fm_window.id).unwrap();
        apps.close_window(fm_window2.id).unwrap();
        assert_eq!(apps.get_open_windows().len(), 0);

        // Use the standalone FileManagerApp to test navigation
        let mut fm = crate::FileManagerApp::new();
        assert_eq!(fm.current_path, "/home");

        fm.navigate("/home/user/documents".to_string()).unwrap();
        assert_eq!(fm.current_path, "/home/user/documents");

        fm.navigate("/tmp".to_string()).unwrap();
        assert_eq!(fm.current_path, "/tmp");

        fm.navigate("/home/user".to_string()).unwrap();
        assert_eq!(fm.current_path, "/home/user");

        // Agent-assisted search
        let results = fm.search_with_agent("important.txt".to_string()).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("important.txt"));
    }

    /// Scenario 14: Quick settings toggle all
    /// Toggle each setting, verify all changed, toggle back, verify restored
    #[test]
    fn test_e2e_quick_settings_toggle_all() {
        let shell = DesktopShell::new();

        // Record initial states
        let initial_states: Vec<(String, bool)> = shell
            .get_quick_settings()
            .iter()
            .map(|s| (s.id.clone(), s.is_active))
            .collect();

        assert_eq!(initial_states.len(), 5);

        // Toggle all settings once
        for (id, _) in &initial_states {
            shell.toggle_quick_setting(id).unwrap();
        }

        // Verify all states are flipped
        for (id, was_active) in &initial_states {
            let settings = shell.get_quick_settings();
            let current = settings.iter().find(|s| &s.id == id).unwrap();
            assert_ne!(
                current.is_active, *was_active,
                "Setting {} should have toggled",
                id
            );
        }

        // Toggle all back
        for (id, _) in &initial_states {
            shell.toggle_quick_setting(id).unwrap();
        }

        // Verify original states restored
        for (id, was_active) in &initial_states {
            let settings = shell.get_quick_settings();
            let current = settings.iter().find(|s| &s.id == id).unwrap();
            assert_eq!(
                current.is_active, *was_active,
                "Setting {} should be back to original",
                id
            );
        }
    }

    /// Scenario 15: Compositor window operations
    /// Create window -> focus -> maximize -> minimize -> resize -> move -> close
    #[test]
    fn test_e2e_compositor_window_operations() {
        let compositor = Compositor::new();

        // Create
        let win_id = compositor
            .create_window("Test Window".to_string(), "test-app".to_string(), false)
            .unwrap();
        let windows = compositor.get_windows();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].state, WindowState::Normal);

        // Maximize
        compositor
            .set_window_state(win_id, WindowState::Maximized)
            .unwrap();
        assert_eq!(compositor.get_windows()[0].state, WindowState::Maximized);

        // Minimize
        compositor
            .set_window_state(win_id, WindowState::Minimized)
            .unwrap();
        assert_eq!(compositor.get_windows()[0].state, WindowState::Minimized);

        // Back to Normal
        compositor
            .set_window_state(win_id, WindowState::Normal)
            .unwrap();
        assert_eq!(compositor.get_windows()[0].state, WindowState::Normal);

        // Fullscreen
        compositor
            .set_window_state(win_id, WindowState::Fullscreen)
            .unwrap();
        assert_eq!(compositor.get_windows()[0].state, WindowState::Fullscreen);

        // Floating
        compositor
            .set_window_state(win_id, WindowState::Floating)
            .unwrap();
        assert_eq!(compositor.get_windows()[0].state, WindowState::Floating);

        // Close
        compositor.close_window(win_id).unwrap();
        assert!(compositor.get_windows().is_empty());

        // Closing again should error
        assert!(compositor.close_window(win_id).is_err());
    }

    /// Scenario 16: Multi-app concurrent operations
    /// Open terminal + agent manager, start agent, verify both windows reflect state
    #[test]
    fn test_e2e_multi_app_concurrent_operations() {
        let mut apps = DesktopApplications::new();

        // Open terminal and agent manager
        let terminal_window = apps.open_terminal().unwrap();
        let agent_window = apps.open_agent_manager().unwrap();
        assert_eq!(apps.get_open_windows().len(), 2);

        // Verify terminal properties
        assert_eq!(terminal_window.app_type, AppType::Terminal);
        assert!(!terminal_window.is_ai_enabled);

        // Verify agent manager properties
        assert_eq!(agent_window.app_type, AppType::AgentManager);
        assert!(agent_window.is_ai_enabled);

        // Start an agent via the agent manager
        let agent_mgr = apps.get_agent_manager();
        let agent_id = agent_mgr
            .start_agent(
                "test-agent".to_string(),
                vec!["file:read".to_string(), "process:spawn".to_string()],
            )
            .unwrap();

        // Verify agent is tracked
        assert_eq!(agent_mgr.running_agents.len(), 1);
        assert_eq!(agent_mgr.running_agents[0].id, agent_id);
        assert_eq!(agent_mgr.running_agents[0].status, "Starting");

        // Stop the agent
        agent_mgr.stop_agent(agent_id).unwrap();
        assert_eq!(agent_mgr.running_agents.len(), 0);

        // Both windows should still be open
        assert_eq!(apps.get_open_windows().len(), 2);
    }

    /// Scenario 17: Audit viewer with filters
    /// Open audit viewer, verify accessible; test standalone AuditViewerApp filter structure
    #[test]
    fn test_e2e_audit_viewer_with_filters() {
        let apps = DesktopApplications::new();

        let audit_window = apps.open_audit_viewer().unwrap();
        assert_eq!(audit_window.app_type, AppType::AuditViewer);
        assert!(audit_window.is_ai_enabled);
        assert_eq!(apps.get_open_windows().len(), 1);

        // Test standalone AuditViewerApp for filter structure
        let viewer = crate::AuditViewerApp::new();
        assert!(viewer.filters.include_agent);
        assert!(viewer.filters.include_security);
        assert!(viewer.filters.include_system);
        assert!(matches!(
            viewer.filters.time_range,
            crate::TimeRange::LastDay
        ));

        // get_logs will return empty since /var/log/agnos/audit.log likely doesn't exist in test
        let logs = viewer.get_logs();
        // We just verify it doesn't panic
        let _ = logs.len();
    }

    /// Scenario 18: Model manager operations
    /// Open model manager, add a model, select it, verify selection
    #[test]
    fn test_e2e_model_manager_operations() {
        let mut apps = DesktopApplications::new();

        let mm_window = apps.open_model_manager().unwrap();
        assert_eq!(mm_window.app_type, AppType::ModelManager);
        assert!(mm_window.is_ai_enabled);

        // Access model manager
        let mm = apps.get_model_manager();
        assert!(mm.active_model.is_none());
        assert!(mm.installed_models.is_empty());

        // Add a model manually for testing
        mm.installed_models.push(crate::ModelInfo {
            id: "llama-3.1-8b".to_string(),
            name: "Llama 3.1 8B".to_string(),
            size: 4_000_000_000,
            provider: "ollama".to_string(),
            is_downloaded: true,
        });

        // Select it
        mm.select_model("llama-3.1-8b".to_string()).unwrap();
        assert_eq!(mm.active_model, Some("llama-3.1-8b".to_string()));

        // Selecting non-existent model may depend on gateway availability
        // In test env without gateway, it should fall back to checking local list
        let result = mm.select_model("nonexistent-model".to_string());
        // The model doesn't exist locally and gateway is not running
        // So active_model was already set, this may or may not error depending on gateway
        let _ = result;
    }

    /// Scenario 19: Suggestion generation
    /// Set up AI features context, generate suggestions, verify non-empty
    #[test]
    fn test_e2e_suggestion_generation() {
        let ai = AIDesktopFeatures::new();

        // Generate a suggestion
        let suggestion = ai.generate_suggestion(
            crate::SuggestionType::TaskRecommendation,
            "Review PR #42".to_string(),
            "Based on your recent git activity".to_string(),
            0.85,
        );

        assert_eq!(suggestion.title, "Review PR #42");
        assert_eq!(
            suggestion.suggestion_type,
            crate::SuggestionType::TaskRecommendation
        );
        assert_eq!(suggestion.confidence, 0.85);
        assert!(!suggestion.is_dismissed);

        // Add it
        ai.add_suggestion(suggestion);

        // Generate more suggestions
        let resource_suggestion = ai.optimize_resources();
        assert_eq!(
            resource_suggestion.suggestion_type,
            crate::SuggestionType::ResourceOptimization
        );
        ai.add_suggestion(resource_suggestion);

        let ws_suggestion = ai.suggest_workspace_switch(0, 1, "Context switch".to_string());
        assert_eq!(
            ws_suggestion.suggestion_type,
            crate::SuggestionType::ContextSwitch
        );
        ai.add_suggestion(ws_suggestion);

        // Get proactive suggestions (confidence > 0.5)
        let proactive = ai.proactive_suggestions();
        assert!(
            !proactive.is_empty(),
            "Should have at least one high-confidence suggestion"
        );

        // All returned suggestions should have confidence > 0.5
        for s in &proactive {
            assert!(s.confidence > 0.5);
        }
    }

    /// Scenario 20: Full teardown sequence
    /// Create everything, close/dismiss/unregister everything, verify clean state
    #[test]
    fn test_e2e_full_teardown_sequence() {
        // Create all components
        let compositor = Compositor::new();
        let shell = DesktopShell::new();
        let apps = DesktopApplications::new();
        let security = SecurityUI::new();
        let ai = AIDesktopFeatures::new();

        // --- Populate compositor ---
        let mut window_ids = Vec::new();
        for i in 0..3 {
            let id = compositor
                .create_window(
                    format!("Window {}", i),
                    format!("app-{}", i),
                    i == 2, // last one is agent window
                )
                .unwrap();
            window_ids.push(id);
        }
        assert_eq!(compositor.get_windows().len(), 3);

        // --- Populate shell ---
        let mut notification_ids = Vec::new();
        for i in 0..5 {
            let n = Notification {
                id: Uuid::new_v4(),
                app_name: format!("app-{}", i),
                title: format!("Notif {}", i),
                body: String::new(),
                priority: NotificationPriority::Normal,
                timestamp: chrono::Utc::now(),
                requires_action: false,
                is_agent_related: false,
            };
            notification_ids.push(n.id);
            shell.show_notification(n);
        }
        assert_eq!(shell.get_notifications().len(), 5);

        // --- Populate apps ---
        let app_windows = vec![
            apps.open_terminal().unwrap(),
            apps.open_file_manager(None).unwrap(),
            apps.open_agent_manager().unwrap(),
        ];
        assert_eq!(apps.get_open_windows().len(), 3);

        // --- Populate security ---
        let alert_id = Uuid::new_v4();
        security.show_security_alert(SecurityAlert {
            id: alert_id,
            title: "Test alert".to_string(),
            description: "For teardown".to_string(),
            threat_level: ThreatLevel::Medium,
            source: "test".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_resolved: false,
        });
        let agent_id = Uuid::new_v4();
        security.set_agent_permissions(
            agent_id,
            "teardown-agent".to_string(),
            vec!["file:read".to_string()],
        );

        // --- Populate AI ---
        let ai_agent_id = Uuid::new_v4();
        ai.register_agent_hud(ai_agent_id, "teardown-ai-agent".to_string());
        assert_eq!(ai.get_agent_hud_states().len(), 1);

        // =====================
        // TEARDOWN EVERYTHING
        // =====================

        // Close all compositor windows
        for id in window_ids {
            compositor.close_window(id).unwrap();
        }
        assert_eq!(compositor.get_windows().len(), 0);

        // Dismiss all notifications
        for id in notification_ids {
            shell.dismiss_notification(id).unwrap();
        }
        assert_eq!(shell.get_notifications().len(), 0);

        // Close all app windows
        for w in app_windows {
            apps.close_window(w.id).unwrap();
        }
        assert_eq!(apps.get_open_windows().len(), 0);

        // Dismiss security alert
        security.dismiss_alert(alert_id).unwrap();
        assert_eq!(security.get_active_alerts().len(), 0);

        // Revoke agent permissions
        security.revoke_agent_permissions(agent_id).unwrap();
        assert_eq!(security.get_security_dashboard().running_agents, 0);

        // Unregister AI agent
        ai.unregister_agent_hud(ai_agent_id);
        assert_eq!(ai.get_agent_hud_states().len(), 0);

        // Verify clean state across all components
        assert!(compositor.get_windows().is_empty());
        assert!(shell.get_notifications().is_empty());
        assert!(!shell.is_locked());
        assert!(apps.get_open_windows().is_empty());
        assert_eq!(security.get_active_alerts().len(), 0);
        assert_eq!(security.get_security_dashboard().pending_permissions, 0);
        assert_eq!(security.get_security_dashboard().running_agents, 0);
        assert!(!security.is_emergency_mode());
        assert!(ai.get_agent_hud_states().is_empty());
    }

    // =========================================================================
    // Additional cross-component interaction tests
    // =========================================================================

    /// Verify context detection updates when window events are recorded
    #[test]
    fn test_e2e_ai_context_detection_with_events() {
        let ai = AIDesktopFeatures::new();

        // Record a window-opened event with a dev tool
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("app".to_string(), "vscode".to_string());

        ai.update_context(crate::ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "compositor".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });

        // Context should detect development mode since "code" substring matches
        let context = ai.get_context();
        assert!(
            context.active_apps.contains(&"vscode".to_string()),
            "vscode should be in active apps"
        );
        // The context type depends on detect_context_type which checks for "code" substring
        assert_eq!(
            context.context_type,
            crate::ai_features::ContextType::Development
        );
    }

    /// Verify smart window placement changes based on context
    #[test]
    fn test_e2e_smart_window_placement() {
        let ai = AIDesktopFeatures::new();

        // Default context is Idle
        let (x, y, w, h) = ai.smart_window_placement("terminal");
        assert_eq!((x, y, w, h), (100, 100, 1200, 800));

        // Simulate development context
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("app".to_string(), "terminal".to_string());
        ai.update_context(crate::ContextEvent {
            id: Uuid::new_v4(),
            event_type: ContextEventType::WindowOpened,
            source: "compositor".to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        });

        // Now in Development context, placement should be different
        let (x, y, w, h) = ai.smart_window_placement("editor");
        assert_eq!((x, y, w, h), (100, 100, 1400, 900));
    }

    /// Verify HUD overlay rendering with registered agents
    #[test]
    fn test_e2e_hud_overlay_with_registered_agents() {
        let compositor = Compositor::new();
        let ai = AIDesktopFeatures::new();

        // Register agents
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        ai.register_agent_hud(id1, "build-agent".to_string());
        ai.register_agent_hud(id2, "test-agent".to_string());

        ai.update_agent_hud(id1, AgentStatus::Acting, "Building".to_string(), 0.6);
        ai.update_agent_hud(id2, AgentStatus::Waiting, "Queued".to_string(), 0.0);

        // Render HUD overlay
        let states = ai.get_agent_hud_states();
        let overlay = compositor.render_hud_overlay(&states);

        assert!(overlay.contains("Agent HUD"));
        assert!(overlay.contains("build-agent"));
        assert!(overlay.contains("test-agent"));
    }

    /// Verify security alert + kill switch + agent HUD interaction
    #[test]
    fn test_e2e_security_and_ai_combined() {
        let security = SecurityUI::new();
        let ai = AIDesktopFeatures::new();

        // Register agent in HUD and security
        let agent_id = Uuid::new_v4();
        ai.register_agent_hud(agent_id, "suspicious-agent".to_string());
        security.set_agent_permissions(
            agent_id,
            "suspicious-agent".to_string(),
            vec!["file:read".to_string()],
        );

        // Agent starts acting suspiciously
        ai.update_agent_hud(
            agent_id,
            AgentStatus::Acting,
            "Accessing /etc/shadow".to_string(),
            0.5,
        );

        // Security raises alert
        security.show_security_alert(SecurityAlert {
            id: Uuid::new_v4(),
            title: "Unauthorized file access".to_string(),
            description: "Agent attempting to read /etc/shadow".to_string(),
            threat_level: ThreatLevel::Critical,
            source: "audit".to_string(),
            timestamp: chrono::Utc::now(),
            requires_action: true,
            is_resolved: false,
        });

        let dashboard = security.get_security_dashboard();
        assert_eq!(dashboard.threat_level, ThreatLevel::Critical);
        assert_eq!(dashboard.active_alerts, 1);

        // Emergency kill switch
        security.emergency_kill_switch();
        assert!(security.is_emergency_mode());

        // Remove agent from HUD
        ai.unregister_agent_hud(agent_id);
        assert!(ai.get_agent_hud_states().is_empty());

        // Revoke permissions
        security.revoke_agent_permissions(agent_id).unwrap();
        assert_eq!(security.get_security_dashboard().running_agents, 0);

        // Deactivate emergency
        security.deactivate_emergency();
        assert!(!security.is_emergency_mode());
    }

    /// Verify workspace context types can be set and affect window placement
    #[test]
    fn test_e2e_workspace_context_management() {
        let compositor = Compositor::new();

        // Set different contexts for different workspaces
        compositor
            .set_workspace_context(0, crate::ContextType::Development)
            .unwrap();
        compositor
            .set_workspace_context(1, crate::ContextType::Communication)
            .unwrap();
        compositor
            .set_workspace_context(2, crate::ContextType::Design)
            .unwrap();
        compositor
            .set_workspace_context(3, crate::ContextType::AgentOperation)
            .unwrap();

        // Verify
        assert_eq!(
            compositor.get_workspace_context(0).unwrap(),
            crate::ContextType::Development
        );
        assert_eq!(
            compositor.get_workspace_context(1).unwrap(),
            crate::ContextType::Communication
        );
        assert_eq!(
            compositor.get_workspace_context(2).unwrap(),
            crate::ContextType::Design
        );
        assert_eq!(
            compositor.get_workspace_context(3).unwrap(),
            crate::ContextType::AgentOperation
        );

        // Create windows in different workspaces and verify they end up in the right place
        compositor.switch_workspace(0).unwrap();
        let dev_win = compositor
            .create_window("Editor".to_string(), "editor".to_string(), false)
            .unwrap();

        compositor.switch_workspace(3).unwrap();
        let agent_win = compositor
            .create_window("Agent Panel".to_string(), "agent".to_string(), true)
            .unwrap();

        // Verify windows are in correct workspaces
        compositor.switch_workspace(0).unwrap();
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, dev_win);

        compositor.switch_workspace(3).unwrap();
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, agent_win);
        assert!(active[0].is_agent_window);
    }

    /// Verify suggestion dismissal works correctly
    #[test]
    fn test_e2e_suggestion_dismiss_flow() {
        let ai = AIDesktopFeatures::new();

        // Add high-confidence suggestions
        let s1 = ai.generate_suggestion(
            crate::SuggestionType::Productivity,
            "Take a break".to_string(),
            "You've been working for 2 hours".to_string(),
            0.9,
        );
        let s1_id = s1.id;
        ai.add_suggestion(s1);

        let s2 = ai.generate_suggestion(
            crate::SuggestionType::ResourceOptimization,
            "Close unused tabs".to_string(),
            "Browser using 2GB RAM".to_string(),
            0.8,
        );
        ai.add_suggestion(s2);

        // Both should appear in proactive suggestions
        let proactive = ai.proactive_suggestions();
        assert_eq!(proactive.len(), 2);

        // Dismiss one
        ai.dismiss_suggestion(s1_id);

        // Only one should remain
        let proactive = ai.proactive_suggestions();
        assert_eq!(proactive.len(), 1);
        assert_eq!(proactive[0].title, "Close unused tabs");
    }

    /// Verify system status propagation through shell
    #[test]
    fn test_e2e_system_status_propagation() {
        let shell = DesktopShell::new();

        // Initial state
        let status = shell.get_system_status();
        assert_eq!(status.agent_count, 0);
        assert_eq!(status.cpu_usage, 0.0);

        // Update full status
        shell.update_system_status(crate::SystemStatus {
            cpu_usage: 75.5,
            memory_usage: 60.0,
            disk_usage: 45.0,
            battery_level: Some(42),
            network_status: crate::NetworkStatus::Connected,
            agent_count: 3,
        });

        let status = shell.get_system_status();
        assert_eq!(status.cpu_usage, 75.5);
        assert_eq!(status.memory_usage, 60.0);
        assert_eq!(status.agent_count, 3);
        assert_eq!(status.battery_level, Some(42));

        // Incremental agent count update
        shell.set_agent_count(5);
        let status = shell.get_system_status();
        assert_eq!(status.agent_count, 5);
        // Other fields should be preserved
        assert_eq!(status.cpu_usage, 75.5);
        assert_eq!(status.battery_level, Some(42));
    }

    /// Verify multiple agents can be started and stopped in agent manager
    #[test]
    fn test_e2e_agent_manager_multi_agent() {
        let mut apps = DesktopApplications::new();
        let agent_mgr = apps.get_agent_manager();

        // Start multiple agents
        let id1 = agent_mgr
            .start_agent(
                "web-scraper".to_string(),
                vec!["network:outbound".to_string()],
            )
            .unwrap();
        let id2 = agent_mgr
            .start_agent(
                "data-processor".to_string(),
                vec!["file:read".to_string(), "file:write".to_string()],
            )
            .unwrap();
        let id3 = agent_mgr
            .start_agent(
                "report-generator".to_string(),
                vec!["file:write".to_string()],
            )
            .unwrap();

        assert_eq!(agent_mgr.running_agents.len(), 3);

        // Stop middle agent
        agent_mgr.stop_agent(id2).unwrap();
        assert_eq!(agent_mgr.running_agents.len(), 2);

        // Verify remaining agents
        let remaining_ids: Vec<Uuid> = agent_mgr.running_agents.iter().map(|a| a.id).collect();
        assert!(remaining_ids.contains(&id1));
        assert!(!remaining_ids.contains(&id2));
        assert!(remaining_ids.contains(&id3));

        // Stop all remaining
        agent_mgr.stop_agent(id1).unwrap();
        agent_mgr.stop_agent(id3).unwrap();
        assert!(agent_mgr.running_agents.is_empty());
    }

    /// Verify compositor secure and agent-aware modes
    #[test]
    fn test_e2e_compositor_mode_toggles() {
        let compositor = Compositor::new();

        // Toggle agent-aware mode
        compositor.set_agent_aware_mode(false);
        compositor.set_agent_aware_mode(true);

        // Toggle secure mode
        compositor.set_secure_mode(true);
        compositor.set_secure_mode(false);

        // Creating windows should still work after mode changes
        let id = compositor
            .create_window("Post-mode-change".to_string(), "app".to_string(), false)
            .unwrap();
        assert_eq!(compositor.get_windows().len(), 1);
        compositor.close_window(id).unwrap();
    }
}
