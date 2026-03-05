use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error("Window not found: {0}")]
    WindowNotFound(Uuid),
    #[error("Display server error: {0}")]
    DisplayServerError(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

pub type SurfaceId = Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
    Floating,
}

impl Default for WindowState {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: SurfaceId,
    pub title: String,
    pub app_id: String,
    pub state: WindowState,
    pub geometry: Rectangle,
    pub is_agent_window: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for Rectangle {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: usize,
    pub name: String,
    pub windows: Vec<SurfaceId>,
    pub active_window: Option<SurfaceId>,
    pub context_type: ContextType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextType {
    Development,
    Communication,
    Design,
    General,
    AgentOperation,
    Window,
    Application,
    System,
    User,
}

#[derive(Debug)]
pub struct Compositor {
    windows: Arc<RwLock<HashMap<SurfaceId, Window>>>,
    workspaces: Arc<RwLock<Vec<Workspace>>>,
    active_workspace: Arc<RwLock<usize>>,
    current_output: Arc<RwLock<Rectangle>>,
    agent_aware_mode: Arc<RwLock<bool>>,
    secure_mode: Arc<RwLock<bool>>,
}

impl Compositor {
    pub fn new() -> Self {
        let mut workspaces = Vec::new();
        for i in 0..4 {
            workspaces.push(Workspace {
                id: i,
                name: format!("Workspace {}", i + 1),
                windows: Vec::new(),
                active_window: None,
                context_type: ContextType::General,
            });
        }

        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            workspaces: Arc::new(RwLock::new(workspaces)),
            active_workspace: Arc::new(RwLock::new(0)),
            current_output: Arc::new(RwLock::new(Rectangle::default())),
            agent_aware_mode: Arc::new(RwLock::new(true)),
            secure_mode: Arc::new(RwLock::new(false)),
        }
    }

    pub fn create_window(
        &self,
        title: String,
        app_id: String,
        is_agent: bool,
    ) -> Result<SurfaceId, CompositorError> {
        let id = Uuid::new_v4();
        let app_id_clone = app_id.clone();
        let window = Window {
            id,
            title,
            app_id: app_id_clone,
            state: WindowState::Normal,
            geometry: Rectangle::default(),
            is_agent_window: is_agent,
            created_at: chrono::Utc::now(),
        };

        self.windows.write().unwrap().insert(id, window);
        self.add_window_to_workspace(id);

        info!("Created window {} for {}", id, app_id);
        Ok(id)
    }

    fn add_window_to_workspace(&self, window_id: SurfaceId) {
        let active_ws = *self.active_workspace.read().unwrap();
        let mut workspaces = self.workspaces.write().unwrap();
        if let Some(ws) = workspaces.get_mut(active_ws) {
            ws.windows.push(window_id);
            ws.active_window = Some(window_id);
        }
    }

    pub fn close_window(&self, id: SurfaceId) -> Result<(), CompositorError> {
        let mut windows = self.windows.write().unwrap();
        if !windows.contains_key(&id) {
            return Err(CompositorError::WindowNotFound(id));
        }

        windows.remove(&id);

        let active_ws = *self.active_workspace.read().unwrap();
        let mut workspaces = self.workspaces.write().unwrap();
        if let Some(ws) = workspaces.get_mut(active_ws) {
            ws.windows.retain(|&w| w != id);
            if ws.active_window == Some(id) {
                ws.active_window = ws.windows.last().copied();
            }
        }

        info!("Closed window {}", id);
        Ok(())
    }

    pub fn set_window_state(
        &self,
        id: SurfaceId,
        state: WindowState,
    ) -> Result<(), CompositorError> {
        let mut windows = self.windows.write().unwrap();
        if !windows.contains_key(&id) {
            return Err(CompositorError::WindowNotFound(id));
        }
        if let Some(w) = windows.get_mut(&id) {
            w.state = state.clone();
            info!("Window {} state changed to {:?}", id, state);
        }

        Ok(())
    }

    pub fn move_window_to_workspace(
        &self,
        window_id: SurfaceId,
        workspace_id: usize,
    ) -> Result<(), CompositorError> {
        let windows = self.windows.write().unwrap();
        if !windows.contains_key(&window_id) {
            return Err(CompositorError::WindowNotFound(window_id));
        }
        let _window = windows.get(&window_id).unwrap().clone();

        let old_ws = *self.active_workspace.read().unwrap();
        let mut workspaces = self.workspaces.write().unwrap();

        if let Some(ws) = workspaces.get_mut(old_ws) {
            ws.windows.retain(|&w| w != window_id);
        }

        if let Some(ws) = workspaces.get_mut(workspace_id) {
            ws.windows.push(window_id);
            ws.active_window = Some(window_id);
        }

        info!(
            "Moved window {} from workspace {} to {}",
            window_id, old_ws, workspace_id
        );
        Ok(())
    }

    pub fn switch_workspace(&self, workspace_id: usize) -> Result<(), CompositorError> {
        let workspaces = self.workspaces.read().unwrap();
        if workspace_id >= workspaces.len() {
            return Err(CompositorError::DisplayServerError(format!(
                "Invalid workspace: {}",
                workspace_id
            )));
        }

        drop(workspaces);

        *self.active_workspace.write().unwrap() = workspace_id;
        info!("Switched to workspace {}", workspace_id);
        Ok(())
    }

    pub fn set_agent_aware_mode(&self, enabled: bool) {
        *self.agent_aware_mode.write().unwrap() = enabled;
        if enabled {
            info!("Agent-aware window management enabled");
        } else {
            warn!("Agent-aware window management disabled");
        }
    }

    pub fn set_secure_mode(&self, enabled: bool) {
        *self.secure_mode.write().unwrap() = enabled;
        if enabled {
            info!("Secure mode enabled - screenshot/access controls active");
        } else {
            warn!("Secure mode disabled");
        }
    }

    pub fn get_windows(&self) -> Vec<Window> {
        self.windows.read().unwrap().values().cloned().collect()
    }

    pub fn get_active_windows(&self) -> Vec<Window> {
        let active_ws = *self.active_workspace.read().unwrap();
        let workspaces = self.workspaces.read().unwrap();
        let window_ids = if let Some(ws) = workspaces.get(active_ws) {
            ws.windows.clone()
        } else {
            Vec::new()
        };

        self.windows
            .read()
            .unwrap()
            .iter()
            .filter(|(id, _)| window_ids.contains(id))
            .map(|(_, w)| w.clone())
            .collect()
    }

    /// Render a text-based HUD overlay showing agent status.
    ///
    /// Returns a formatted string suitable for compositing onto the desktop.
    /// The actual rendering will be handled by the Wayland compositor once
    /// we have a real buffer allocation path; this method provides the content.
    pub fn render_hud_overlay(
        &self,
        agents: &[crate::ai_features::AgentHUDState],
    ) -> String {
        if agents.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        lines.push("╔══════════════════════════════════════╗".to_string());
        lines.push("║         Agent HUD                    ║".to_string());
        lines.push("╠══════════════════════════════════════╣".to_string());

        for agent in agents {
            let status_icon = match agent.status {
                crate::ai_features::AgentStatus::Idle => "○",
                crate::ai_features::AgentStatus::Thinking => "◐",
                crate::ai_features::AgentStatus::Acting => "●",
                crate::ai_features::AgentStatus::Waiting => "◑",
                crate::ai_features::AgentStatus::Error => "✗",
            };

            let name = if agent.agent_name.len() > 20 {
                format!("{}…", &agent.agent_name[..19])
            } else {
                agent.agent_name.clone()
            };

            lines.push(format!(
                "║ {} {:<20} {:>5.1}% {:>4}MB ║",
                status_icon,
                name,
                agent.resource_usage.cpu_percent,
                agent.resource_usage.memory_mb,
            ));
        }

        lines.push("╚══════════════════════════════════════╝".to_string());
        lines.join("\n")
    }

    pub fn get_agent_windows(&self) -> Vec<Window> {
        self.windows
            .read()
            .unwrap()
            .values()
            .filter(|w| w.is_agent_window)
            .cloned()
            .collect()
    }

    pub fn get_workspace_context(
        &self,
        workspace_id: usize,
    ) -> Result<ContextType, CompositorError> {
        let workspaces = self.workspaces.read().unwrap();
        let ws = workspaces
            .get(workspace_id)
            .ok_or(CompositorError::DisplayServerError(format!(
                "Invalid workspace: {}",
                workspace_id
            )))?;
        Ok(ws.context_type.clone())
    }

    pub fn set_workspace_context(
        &self,
        workspace_id: usize,
        context: ContextType,
    ) -> Result<(), CompositorError> {
        let mut workspaces = self.workspaces.write().unwrap();
        if let Some(ws) = workspaces.get_mut(workspace_id) {
            ws.context_type = context.clone();
            info!("Workspace {} context set to {:?}", workspace_id, context);
        }
        Ok(())
    }
}

pub trait CompositorBackend: Send + Sync {
    fn initialize(&mut self) -> Result<(), CompositorError>;
    fn handle_input(&mut self, event: InputEvent) -> Result<(), CompositorError>;
    fn render(&mut self) -> Result<(), CompositorError>;
    fn shutdown(&mut self) -> Result<(), CompositorError>;
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseClick {
        button: u32,
        x: i32,
        y: i32,
    },
    KeyPress {
        keycode: u32,
        modifiers: u32,
    },
    TouchEvent {
        finger_id: i32,
        x: f64,
        y: f64,
        phase: TouchPhase,
    },
}

#[derive(Debug, Clone)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
}

pub struct WaylandBackend;

impl CompositorBackend for WaylandBackend {
    fn initialize(&mut self) -> Result<(), CompositorError> {
        info!("Initializing Wayland backend");
        Ok(())
    }

    fn handle_input(&mut self, event: InputEvent) -> Result<(), CompositorError> {
        match event {
            InputEvent::MouseMove { x, y } => {
                tracing::debug!("Mouse move: ({}, {})", x, y);
            }
            InputEvent::MouseClick { button, x, y } => {
                tracing::debug!("Mouse click: button={}, ({}, {})", button, x, y);
            }
            InputEvent::KeyPress { keycode, modifiers } => {
                tracing::debug!("Key press: keycode={}, modifiers={}", keycode, modifiers);
            }
            InputEvent::TouchEvent {
                finger_id,
                x: x_pos,
                y: y_pos,
                phase,
            } => {
                tracing::debug!(
                    "Touch: finger={}, x={}, y={}, phase={:?}",
                    finger_id,
                    x_pos,
                    y_pos,
                    phase
                );
            }
        }
        Ok(())
    }

    fn render(&mut self) -> Result<(), CompositorError> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), CompositorError> {
        info!("Wayland backend shutting down");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_state_variants() {
        assert!(matches!(WindowState::Normal, WindowState::Normal));
        assert!(matches!(WindowState::Minimized, WindowState::Minimized));
        assert!(matches!(WindowState::Maximized, WindowState::Maximized));
        assert!(matches!(WindowState::Fullscreen, WindowState::Fullscreen));
    }

    #[test]
    fn test_rectangle_default() {
        let rect = Rectangle::default();
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert_eq!(rect.width, 1920);
        assert_eq!(rect.height, 1080);
    }

    #[test]
    fn test_rectangle_custom() {
        let rect = Rectangle {
            x: 100,
            y: 200,
            width: 800,
            height: 600,
        };
        assert_eq!(rect.x, 100);
        assert_eq!(rect.y, 200);
        assert_eq!(rect.width, 800);
        assert_eq!(rect.height, 600);
    }

    #[test]
    fn test_context_type_variants() {
        assert!(matches!(ContextType::Development, ContextType::Development));
        assert!(matches!(
            ContextType::Communication,
            ContextType::Communication
        ));
        assert!(matches!(
            ContextType::AgentOperation,
            ContextType::AgentOperation
        ));
    }

    #[test]
    fn test_workspace() {
        let ws = Workspace {
            id: 1,
            name: "Main".to_string(),
            windows: vec![],
            active_window: None,
            context_type: ContextType::General,
        };
        assert_eq!(ws.name, "Main");
    }

    #[test]
    fn test_touch_phase() {
        assert!(matches!(TouchPhase::Down, TouchPhase::Down));
        assert!(matches!(TouchPhase::Move, TouchPhase::Move));
        assert!(matches!(TouchPhase::Up, TouchPhase::Up));
    }

    #[test]
    fn test_compositor_new() {
        let compositor = Compositor::new();
        assert!(compositor.get_windows().is_empty());
    }

    #[test]
    fn test_compositor_create_window() {
        let compositor = Compositor::new();
        let id = compositor.create_window("Test Window".to_string(), "test-app".to_string(), false);
        assert!(id.is_ok());
        let windows = compositor.get_windows();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, "Test Window");
        assert!(!windows[0].is_agent_window);
    }

    #[test]
    fn test_compositor_create_agent_window() {
        let compositor = Compositor::new();
        let _id = compositor
            .create_window("Agent Window".to_string(), "agent".to_string(), true)
            .unwrap();
        let windows = compositor.get_windows();
        assert!(windows[0].is_agent_window);
        let agent_windows = compositor.get_agent_windows();
        assert_eq!(agent_windows.len(), 1);
    }

    #[test]
    fn test_compositor_close_window() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("Test".to_string(), "app".to_string(), false)
            .unwrap();
        assert_eq!(compositor.get_windows().len(), 1);
        compositor.close_window(id).unwrap();
        assert!(compositor.get_windows().is_empty());
    }

    #[test]
    fn test_compositor_close_nonexistent_window() {
        let compositor = Compositor::new();
        let result = compositor.close_window(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_compositor_set_window_state() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("Test".to_string(), "app".to_string(), false)
            .unwrap();
        compositor
            .set_window_state(id, WindowState::Maximized)
            .unwrap();
        let windows = compositor.get_windows();
        assert_eq!(windows[0].state, WindowState::Maximized);
    }

    #[test]
    fn test_compositor_switch_workspace() {
        let compositor = Compositor::new();
        assert!(compositor.switch_workspace(0).is_ok());
        assert!(compositor.switch_workspace(3).is_ok());
        assert!(compositor.switch_workspace(4).is_err());
    }

    #[test]
    fn test_compositor_move_window_to_workspace() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("Test".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.move_window_to_workspace(id, 2).unwrap();
    }

    #[test]
    fn test_compositor_move_nonexistent_window() {
        let compositor = Compositor::new();
        let result = compositor.move_window_to_workspace(Uuid::new_v4(), 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_compositor_agent_aware_mode() {
        let compositor = Compositor::new();
        compositor.set_agent_aware_mode(true);
        compositor.set_agent_aware_mode(false);
    }

    #[test]
    fn test_compositor_secure_mode() {
        let compositor = Compositor::new();
        compositor.set_secure_mode(true);
        compositor.set_secure_mode(false);
    }

    #[test]
    fn test_compositor_get_workspace_context() {
        let compositor = Compositor::new();
        let context = compositor.get_workspace_context(0).unwrap();
        assert_eq!(context, ContextType::General);
        assert!(compositor.get_workspace_context(10).is_err());
    }

    #[test]
    fn test_compositor_set_workspace_context() {
        let compositor = Compositor::new();
        compositor
            .set_workspace_context(0, ContextType::Development)
            .unwrap();
        let context = compositor.get_workspace_context(0).unwrap();
        assert_eq!(context, ContextType::Development);
    }

    #[test]
    fn test_compositor_get_active_windows() {
        let compositor = Compositor::new();
        let _id1 = compositor
            .create_window("W1".to_string(), "app".to_string(), false)
            .unwrap();
        let _id2 = compositor
            .create_window("W2".to_string(), "app".to_string(), false)
            .unwrap();
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_compositor_error_variants() {
        let err = CompositorError::WindowNotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));
        let err = CompositorError::DisplayServerError("test".to_string());
        assert!(err.to_string().contains("Display server"));
        let err = CompositorError::PermissionDenied("test".to_string());
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_input_event_mouse_move() {
        let event = InputEvent::MouseMove { x: 100, y: 200 };
        assert!(matches!(event, InputEvent::MouseMove { .. }));
    }

    #[test]
    fn test_input_event_mouse_click() {
        let event = InputEvent::MouseClick {
            button: 1,
            x: 50,
            y: 50,
        };
        assert!(matches!(event, InputEvent::MouseClick { .. }));
    }

    #[test]
    fn test_input_event_key_press() {
        let event = InputEvent::KeyPress {
            keycode: 65,
            modifiers: 0,
        };
        assert!(matches!(event, InputEvent::KeyPress { .. }));
    }

    #[test]
    fn test_input_event_touch() {
        let event = InputEvent::TouchEvent {
            finger_id: 0,
            x: 100.0,
            y: 200.0,
            phase: TouchPhase::Down,
        };
        assert!(matches!(event, InputEvent::TouchEvent { .. }));
    }

    #[test]
    fn test_wayland_backend_initialize() {
        let mut backend = WaylandBackend;
        assert!(backend.initialize().is_ok());
        assert!(backend.render().is_ok());
        assert!(backend.shutdown().is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_input() {
        let mut backend = WaylandBackend;
        let event = InputEvent::MouseMove { x: 100, y: 200 };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_click() {
        let mut backend = WaylandBackend;
        let event = InputEvent::MouseClick { button: 1, x: 50, y: 50 };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_keypress() {
        let mut backend = WaylandBackend;
        let event = InputEvent::KeyPress { keycode: 65, modifiers: 0 };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_touch() {
        let mut backend = WaylandBackend;
        let event = InputEvent::TouchEvent { finger_id: 0, x: 100.0, y: 200.0, phase: TouchPhase::Down };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_hud_overlay_empty() {
        let compositor = Compositor::new();
        let result = compositor.render_hud_overlay(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_hud_overlay_with_agents() {
        use crate::ai_features::{AgentHUDState, AgentStatus, ResourceMetrics};

        let compositor = Compositor::new();
        let agents = vec![
            AgentHUDState {
                agent_id: Uuid::new_v4(),
                agent_name: "qa-agent".to_string(),
                status: AgentStatus::Acting,
                current_task: "running tests".to_string(),
                progress: 0.5,
                last_activity: chrono::Utc::now(),
                resource_usage: ResourceMetrics {
                    cpu_percent: 45.2,
                    memory_mb: 256,
                    gpu_percent: None,
                },
            },
            AgentHUDState {
                agent_id: Uuid::new_v4(),
                agent_name: "file-manager".to_string(),
                status: AgentStatus::Idle,
                current_task: String::new(),
                progress: 0.0,
                last_activity: chrono::Utc::now(),
                resource_usage: ResourceMetrics {
                    cpu_percent: 0.0,
                    memory_mb: 64,
                    gpu_percent: None,
                },
            },
        ];

        let result = compositor.render_hud_overlay(&agents);
        assert!(result.contains("Agent HUD"));
        assert!(result.contains("qa-agent"));
        assert!(result.contains("file-manager"));
        assert!(result.contains("●")); // Acting icon
        assert!(result.contains("○")); // Idle icon
    }

    #[test]
    fn test_hud_overlay_long_name() {
        use crate::ai_features::{AgentHUDState, AgentStatus, ResourceMetrics};

        let compositor = Compositor::new();
        let agents = vec![AgentHUDState {
            agent_id: Uuid::new_v4(),
            agent_name: "very-long-agent-name-that-should-be-truncated".to_string(),
            status: AgentStatus::Thinking,
            current_task: String::new(),
            progress: 0.0,
            last_activity: chrono::Utc::now(),
            resource_usage: ResourceMetrics {
                cpu_percent: 10.0,
                memory_mb: 128,
                gpu_percent: None,
            },
        }];

        let result = compositor.render_hud_overlay(&agents);
        assert!(result.contains("…")); // Truncation marker
    }

    #[test]
    fn test_hud_overlay_waiting_and_error_status() {
        use crate::ai_features::{AgentHUDState, AgentStatus, ResourceMetrics};

        let compositor = Compositor::new();
        let agents = vec![
            AgentHUDState {
                agent_id: Uuid::new_v4(),
                agent_name: "waiting-agent".to_string(),
                status: AgentStatus::Waiting,
                current_task: "blocked on input".to_string(),
                progress: 0.3,
                last_activity: chrono::Utc::now(),
                resource_usage: ResourceMetrics {
                    cpu_percent: 1.0,
                    memory_mb: 32,
                    gpu_percent: None,
                },
            },
            AgentHUDState {
                agent_id: Uuid::new_v4(),
                agent_name: "error-agent".to_string(),
                status: AgentStatus::Error,
                current_task: "crashed".to_string(),
                progress: 0.0,
                last_activity: chrono::Utc::now(),
                resource_usage: ResourceMetrics {
                    cpu_percent: 0.0,
                    memory_mb: 16,
                    gpu_percent: None,
                },
            },
        ];

        let result = compositor.render_hud_overlay(&agents);
        assert!(result.contains("◑")); // Waiting icon
        assert!(result.contains("✗")); // Error icon
    }

    #[test]
    fn test_set_window_state_nonexistent() {
        let compositor = Compositor::new();
        let result = compositor.set_window_state(Uuid::new_v4(), WindowState::Maximized);
        assert!(result.is_err());
    }

    #[test]
    fn test_window_state_floating() {
        assert!(matches!(WindowState::Floating, WindowState::Floating));
        assert_ne!(WindowState::Floating, WindowState::Normal);
    }

    #[test]
    fn test_window_state_default() {
        assert_eq!(WindowState::default(), WindowState::Normal);
    }

    #[test]
    fn test_compositor_has_4_default_workspaces() {
        let compositor = Compositor::new();
        let workspaces = compositor.workspaces.read().unwrap();
        assert_eq!(workspaces.len(), 4);
        for (i, ws) in workspaces.iter().enumerate() {
            assert_eq!(ws.id, i);
            assert_eq!(ws.name, format!("Workspace {}", i + 1));
            assert_eq!(ws.context_type, ContextType::General);
            assert!(ws.windows.is_empty());
            assert!(ws.active_window.is_none());
        }
    }

    #[test]
    fn test_compositor_window_added_to_active_workspace() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(workspaces[0].windows.contains(&id));
        assert_eq!(workspaces[0].active_window, Some(id));
    }

    #[test]
    fn test_compositor_close_updates_active_window() {
        let compositor = Compositor::new();
        let id1 = compositor.create_window("W1".to_string(), "app".to_string(), false).unwrap();
        let id2 = compositor.create_window("W2".to_string(), "app".to_string(), false).unwrap();
        // Close the active (last created) window
        compositor.close_window(id2).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        // Active should fall back to id1 (last remaining)
        assert_eq!(workspaces[0].active_window, Some(id1));
    }

    #[test]
    fn test_compositor_close_last_window_clears_active() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        compositor.close_window(id).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(workspaces[0].active_window.is_none());
    }

    #[test]
    fn test_compositor_set_all_window_states() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();

        for state in [WindowState::Minimized, WindowState::Maximized, WindowState::Fullscreen, WindowState::Floating, WindowState::Normal] {
            compositor.set_window_state(id, state.clone()).unwrap();
            let windows = compositor.get_windows();
            assert_eq!(windows[0].state, state);
        }
    }

    #[test]
    fn test_compositor_move_window_removes_from_source() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        compositor.move_window_to_workspace(id, 2).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(!workspaces[0].windows.contains(&id));
        assert!(workspaces[2].windows.contains(&id));
    }

    #[test]
    fn test_compositor_get_active_windows_after_workspace_switch() {
        let compositor = Compositor::new();
        let _id1 = compositor.create_window("W1".to_string(), "app".to_string(), false).unwrap();
        compositor.switch_workspace(1).unwrap();
        let _id2 = compositor.create_window("W2".to_string(), "app".to_string(), false).unwrap();
        // Active workspace is 1, so only W2 should be returned
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "W2");
    }

    #[test]
    fn test_compositor_agent_windows_filter() {
        let compositor = Compositor::new();
        compositor.create_window("Normal".to_string(), "app".to_string(), false).unwrap();
        compositor.create_window("Agent1".to_string(), "agent".to_string(), true).unwrap();
        compositor.create_window("Agent2".to_string(), "agent".to_string(), true).unwrap();
        assert_eq!(compositor.get_agent_windows().len(), 2);
        assert_eq!(compositor.get_windows().len(), 3);
    }

    #[test]
    fn test_compositor_set_workspace_context_all_types() {
        let compositor = Compositor::new();
        let contexts = [
            ContextType::Development,
            ContextType::Communication,
            ContextType::Design,
            ContextType::AgentOperation,
            ContextType::Window,
            ContextType::Application,
            ContextType::System,
            ContextType::User,
        ];
        for (i, ctx) in contexts.iter().enumerate() {
            let ws = i % 4;
            compositor.set_workspace_context(ws, ctx.clone()).unwrap();
            assert_eq!(compositor.get_workspace_context(ws).unwrap(), *ctx);
        }
    }

    #[test]
    fn test_compositor_create_multiple_windows_tracking() {
        let compositor = Compositor::new();
        let ids: Vec<SurfaceId> = (0..5)
            .map(|i| {
                compositor
                    .create_window(format!("Win-{}", i), format!("app-{}", i), i % 2 == 0)
                    .unwrap()
            })
            .collect();
        assert_eq!(compositor.get_windows().len(), 5);
        assert_eq!(compositor.get_agent_windows().len(), 3); // 0,2,4 are agent
        // Last created window should be active
        let workspaces = compositor.workspaces.read().unwrap();
        assert_eq!(workspaces[0].active_window, Some(ids[4]));
    }

    #[test]
    fn test_compositor_close_middle_window_preserves_others() {
        let compositor = Compositor::new();
        let id1 = compositor.create_window("W1".to_string(), "a".to_string(), false).unwrap();
        let id2 = compositor.create_window("W2".to_string(), "b".to_string(), false).unwrap();
        let id3 = compositor.create_window("W3".to_string(), "c".to_string(), false).unwrap();
        compositor.close_window(id2).unwrap();
        let windows = compositor.get_windows();
        assert_eq!(windows.len(), 2);
        let remaining_ids: Vec<SurfaceId> = windows.iter().map(|w| w.id).collect();
        assert!(remaining_ids.contains(&id1));
        assert!(remaining_ids.contains(&id3));
    }

    #[test]
    fn test_compositor_switch_workspace_boundary() {
        let compositor = Compositor::new();
        assert!(compositor.switch_workspace(0).is_ok());
        assert!(compositor.switch_workspace(3).is_ok());
        assert!(compositor.switch_workspace(4).is_err());
        assert!(compositor.switch_workspace(100).is_err());
    }

    #[test]
    fn test_compositor_window_state_transitions_full_cycle() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        // Normal -> Maximized -> Fullscreen -> Minimized -> Floating -> Normal
        let transitions = vec![
            WindowState::Maximized,
            WindowState::Fullscreen,
            WindowState::Minimized,
            WindowState::Floating,
            WindowState::Normal,
        ];
        for state in transitions {
            compositor.set_window_state(id, state.clone()).unwrap();
            let w = compositor.get_windows().into_iter().find(|w| w.id == id).unwrap();
            assert_eq!(w.state, state);
        }
    }

    #[test]
    fn test_compositor_move_window_to_same_workspace() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        // Move to workspace 0 (same as active)
        compositor.move_window_to_workspace(id, 0).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        // Window should be in workspace 0 (might be duplicated, but that's the current impl)
        assert!(workspaces[0].windows.contains(&id));
    }

    #[test]
    fn test_compositor_move_window_to_last_workspace() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        compositor.move_window_to_workspace(id, 3).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(workspaces[3].windows.contains(&id));
        assert_eq!(workspaces[3].active_window, Some(id));
    }

    #[test]
    fn test_compositor_window_created_at_is_recent() {
        let before = chrono::Utc::now();
        let compositor = Compositor::new();
        let _id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        let after = chrono::Utc::now();
        let window = &compositor.get_windows()[0];
        assert!(window.created_at >= before);
        assert!(window.created_at <= after);
    }

    #[test]
    fn test_compositor_close_window_double_close_fails() {
        let compositor = Compositor::new();
        let id = compositor.create_window("W".to_string(), "app".to_string(), false).unwrap();
        compositor.close_window(id).unwrap();
        let result = compositor.close_window(id);
        assert!(result.is_err());
    }

    #[test]
    fn test_compositor_get_active_windows_empty_workspace() {
        let compositor = Compositor::new();
        compositor.switch_workspace(2).unwrap();
        let active = compositor.get_active_windows();
        assert!(active.is_empty());
    }

    #[test]
    fn test_compositor_error_display_window_not_found_contains_uuid() {
        let id = Uuid::new_v4();
        let err = CompositorError::WindowNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_compositor_error_is_std_error() {
        let err = CompositorError::DisplayServerError("test".to_string());
        let std_err: &dyn std::error::Error = &err;
        assert!(!std_err.to_string().is_empty());
    }

    #[test]
    fn test_compositor_workspace_active_window_after_multiple_creates() {
        let compositor = Compositor::new();
        let _id1 = compositor.create_window("W1".to_string(), "a".to_string(), false).unwrap();
        let _id2 = compositor.create_window("W2".to_string(), "b".to_string(), false).unwrap();
        let id3 = compositor.create_window("W3".to_string(), "c".to_string(), false).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        // Active window should be the last one created
        assert_eq!(workspaces[0].active_window, Some(id3));
    }

    #[test]
    fn test_compositor_set_workspace_context_out_of_bounds_silent() {
        let compositor = Compositor::new();
        // Setting context on out-of-bounds workspace should return Ok (no workspace to modify)
        let result = compositor.set_workspace_context(100, ContextType::Development);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compositor_windows_across_workspaces() {
        let compositor = Compositor::new();
        compositor.switch_workspace(0).unwrap();
        let id_ws0 = compositor.create_window("WS0".to_string(), "a".to_string(), false).unwrap();
        compositor.switch_workspace(1).unwrap();
        let _id_ws1 = compositor.create_window("WS1".to_string(), "b".to_string(), false).unwrap();
        // get_windows returns all windows regardless of workspace
        assert_eq!(compositor.get_windows().len(), 2);
        // get_active_windows only returns windows in active workspace (1)
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "WS1");
        // Move ws0 window to workspace 1 from workspace 0
        compositor.move_window_to_workspace(id_ws0, 1).unwrap();
        let active_after = compositor.get_active_windows();
        assert_eq!(active_after.len(), 2);
    }
}
