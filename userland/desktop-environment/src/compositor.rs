use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{error, info, warn};
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
        let windows = self.windows.read().unwrap();
        let window = windows
            .get(&id)
            .ok_or(CompositorError::WindowNotFound(id))?;

        let mut windows = self.windows.write().unwrap();
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
        let windows = self.windows.read().unwrap();
        if !windows.contains_key(&window_id) {
            return Err(CompositorError::WindowNotFound(window_id));
        }

        let mut windows = self.windows.write().unwrap();
        let window = windows.get(&window_id).unwrap().clone();

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
}
