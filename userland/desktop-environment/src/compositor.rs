use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::accessibility::{
    AccessibilityRole, AccessibilityState, AccessibilityTree, AccessibleNode, HighContrastTheme,
};
use crate::renderer::{
    self, DecorationHit, DesktopRenderer, Layer, ResizeEdge, SceneGraph, SceneSurface,
    TITLEBAR_HEIGHT,
};

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error("Window not found")]
    WindowNotFound(Uuid),
    #[error("Display server error: {0}")]
    DisplayServerError(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

pub type SurfaceId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowState {
    #[default]
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

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
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

/// Result of routing an input event to a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Start dragging a window from its title bar.
    BeginDrag(SurfaceId),
    /// Close a window via its close button.
    Close(SurfaceId),
    /// Minimize a window.
    Minimize(SurfaceId),
    /// Maximize/restore a window.
    ToggleMaximize(SurfaceId),
    /// Begin resizing from an edge/corner.
    BeginResize(SurfaceId, ResizeEdge),
    /// Forward a click to the window's client area.
    ClientClick(SurfaceId, i32, i32),
    /// Forward a key press to the focused window.
    KeyToFocused(u32, u32),
    /// Pointer moved — update cursor position.
    PointerMove(i32, i32),
    /// No action (click on background, etc.).
    None,
}

pub struct Compositor {
    windows: Arc<RwLock<HashMap<SurfaceId, Window>>>,
    workspaces: Arc<RwLock<Vec<Workspace>>>,
    active_workspace: Arc<RwLock<usize>>,
    pub(crate) current_output: Arc<RwLock<Rectangle>>,
    agent_aware_mode: Arc<RwLock<bool>>,
    pub(crate) secure_mode: Arc<RwLock<bool>>,
    pub(crate) scene: Arc<RwLock<SceneGraph>>,
    pub(crate) renderer: Arc<RwLock<DesktopRenderer>>,
    focused_window: Arc<RwLock<Option<SurfaceId>>>,
    /// Window being dragged: (id, offset_x, offset_y)
    drag_state: Arc<RwLock<Option<(SurfaceId, i32, i32)>>>,
    /// Window being resized: (id, edge, original_rect)
    resize_state: Arc<RwLock<Option<(SurfaceId, ResizeEdge, Rectangle)>>>,
    /// Accessibility tree for AT-SPI2 and keyboard navigation.
    accessibility_tree: Arc<RwLock<AccessibilityTree>>,
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compositor {
    pub fn new() -> Self {
        Self::with_resolution(1920, 1080)
    }

    pub fn with_resolution(width: u32, height: u32) -> Self {
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
            current_output: Arc::new(RwLock::new(Rectangle {
                x: 0,
                y: 0,
                width,
                height,
            })),
            agent_aware_mode: Arc::new(RwLock::new(true)),
            secure_mode: Arc::new(RwLock::new(false)),
            scene: Arc::new(RwLock::new(SceneGraph::new())),
            renderer: Arc::new(RwLock::new(DesktopRenderer::new(width, height))),
            focused_window: Arc::new(RwLock::new(None)),
            drag_state: Arc::new(RwLock::new(None)),
            resize_state: Arc::new(RwLock::new(None)),
            accessibility_tree: Arc::new(RwLock::new(AccessibilityTree::new())),
        }
    }

    pub fn create_window(
        &self,
        title: String,
        app_id: String,
        is_agent: bool,
    ) -> Result<SurfaceId, CompositorError> {
        let id = Uuid::new_v4();
        // Calculate placement: cascade from top-left
        let window_count = self.windows.read().unwrap_or_else(|e| e.into_inner()).len();
        let cascade_offset = (window_count as i32 * 30) % 300;
        let output = *self
            .current_output
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let default_width = (output.width / 2).max(400);
        let default_height = (output.height / 2).max(300);

        let geometry = Rectangle {
            x: 50 + cascade_offset,
            y: 50 + cascade_offset,
            width: default_width,
            height: default_height,
        };

        info!("Created window {} for {}", id, app_id);

        let window = Window {
            id,
            title: title.clone(),
            app_id,
            state: WindowState::Normal,
            geometry,
            is_agent_window: is_agent,
            created_at: chrono::Utc::now(),
        };

        self.windows
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, window);
        self.add_window_to_workspace(id);

        // Add to scene graph
        let layer = if is_agent {
            Layer::Floating
        } else {
            Layer::Normal
        };
        self.scene
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .add_surface(SceneSurface {
                id,
                layer,
                geometry,
                visible: true,
                opacity: 1.0,
                title: title.clone(),
                is_active: true,
                window_state: WindowState::Normal,
            });

        // Focus the new window
        *self
            .focused_window
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(id);

        // Create accessibility node for the new window
        {
            let mut tree = self
                .accessibility_tree
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let node = AccessibleNode {
                id,
                role: AccessibilityRole::Window,
                name: title.clone(),
                state: AccessibilityState::default(),
                children: Vec::new(),
                parent: None,
                bounds: geometry,
                actions: vec![
                    crate::accessibility::AccessibleAction::Focus,
                    crate::accessibility::AccessibleAction::Click,
                    crate::accessibility::AccessibleAction::Dismiss,
                ],
            };
            tree.add_node(node);
            tree.announce(&format!("Window opened: {}", title));
        }

        Ok(id)
    }

    fn add_window_to_workspace(&self, window_id: SurfaceId) {
        let active_ws = *self
            .active_workspace
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut workspaces = self.workspaces.write().unwrap_or_else(|e| e.into_inner());
        if let Some(ws) = workspaces.get_mut(active_ws) {
            ws.windows.push(window_id);
            ws.active_window = Some(window_id);
        }
    }

    pub fn close_window(&self, id: SurfaceId) -> Result<(), CompositorError> {
        let mut windows = self.windows.write().unwrap_or_else(|e| e.into_inner());
        if !windows.contains_key(&id) {
            return Err(CompositorError::WindowNotFound(id));
        }

        let window_title = windows
            .get(&id)
            .map(|w| w.title.clone())
            .unwrap_or_default();
        windows.remove(&id);

        let active_ws = *self
            .active_workspace
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut workspaces = self.workspaces.write().unwrap_or_else(|e| e.into_inner());
        if let Some(ws) = workspaces.get_mut(active_ws) {
            ws.windows.retain(|&w| w != id);
            if ws.active_window == Some(id) {
                ws.active_window = ws.windows.last().copied();
            }
        }

        // Remove from scene graph and renderer
        self.scene
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove_surface(id);
        self.renderer
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove_buffer(id);

        // Update focus
        let mut focused = self
            .focused_window
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if *focused == Some(id) {
            // Focus the next window in the workspace
            let ws_windows = workspaces.get(active_ws).map(|ws| ws.windows.clone());
            *focused = ws_windows.and_then(|w| w.last().copied());
        }

        info!("Closed window {}", id);

        // Remove accessibility node
        {
            let mut tree = self
                .accessibility_tree
                .write()
                .unwrap_or_else(|e| e.into_inner());
            tree.remove_node(&id);
            tree.announce(&format!("Window closed: {}", window_title));
        }

        Ok(())
    }

    pub fn set_window_state(
        &self,
        id: SurfaceId,
        state: WindowState,
    ) -> Result<(), CompositorError> {
        let mut windows = self.windows.write().unwrap_or_else(|e| e.into_inner());
        if !windows.contains_key(&id) {
            return Err(CompositorError::WindowNotFound(id));
        }

        let output = *self
            .current_output
            .read()
            .unwrap_or_else(|e| e.into_inner());

        if let Some(w) = windows.get_mut(&id) {
            w.state = state;

            // Sync geometry for maximize/fullscreen
            match &state {
                WindowState::Maximized => {
                    w.geometry = Rectangle {
                        x: 0,
                        y: 0,
                        width: output.width,
                        height: output.height,
                    };
                }
                WindowState::Fullscreen => {
                    w.geometry = Rectangle {
                        x: 0,
                        y: 0,
                        width: output.width,
                        height: output.height,
                    };
                }
                _ => {}
            }

            // Update scene graph
            let mut scene = self.scene.write().unwrap_or_else(|e| e.into_inner());
            if let Some(surface) = scene.get_surface_mut(id) {
                surface.window_state = state;
                surface.visible = state != WindowState::Minimized;
                match &state {
                    WindowState::Maximized | WindowState::Fullscreen => {
                        surface.geometry = w.geometry;
                    }
                    _ => {}
                }
            }

            info!("Window {} state changed to {:?}", id, state);
        }

        Ok(())
    }

    pub fn move_window_to_workspace(
        &self,
        window_id: SurfaceId,
        workspace_id: usize,
    ) -> Result<(), CompositorError> {
        let windows = self.windows.write().unwrap_or_else(|e| e.into_inner());
        if !windows.contains_key(&window_id) {
            return Err(CompositorError::WindowNotFound(window_id));
        }
        let old_ws = *self
            .active_workspace
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut workspaces = self.workspaces.write().unwrap_or_else(|e| e.into_inner());

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
        let workspaces = self.workspaces.read().unwrap_or_else(|e| e.into_inner());
        if workspace_id >= workspaces.len() {
            return Err(CompositorError::DisplayServerError(format!(
                "Invalid workspace: {}",
                workspace_id
            )));
        }

        drop(workspaces);

        *self
            .active_workspace
            .write()
            .unwrap_or_else(|e| e.into_inner()) = workspace_id;
        info!("Switched to workspace {}", workspace_id);
        Ok(())
    }

    pub fn set_agent_aware_mode(&self, enabled: bool) {
        *self
            .agent_aware_mode
            .write()
            .unwrap_or_else(|e| e.into_inner()) = enabled;
        if enabled {
            info!("Agent-aware window management enabled");
        } else {
            warn!("Agent-aware window management disabled");
        }
    }

    pub fn set_secure_mode(&self, enabled: bool) {
        *self.secure_mode.write().unwrap_or_else(|e| e.into_inner()) = enabled;
        if enabled {
            info!("Secure mode enabled - screenshot/access controls active");
        } else {
            warn!("Secure mode disabled");
        }
    }

    pub fn get_windows(&self) -> Vec<Window> {
        self.windows
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    pub fn get_active_windows(&self) -> Vec<Window> {
        let active_ws = *self
            .active_workspace
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let workspaces = self.workspaces.read().unwrap_or_else(|e| e.into_inner());
        let window_ids = if let Some(ws) = workspaces.get(active_ws) {
            ws.windows.clone()
        } else {
            Vec::new()
        };

        self.windows
            .read()
            .unwrap_or_else(|e| e.into_inner())
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
    pub fn render_hud_overlay(&self, agents: &[crate::ai_features::AgentHUDState]) -> String {
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
                status_icon, name, agent.resource_usage.cpu_percent, agent.resource_usage.memory_mb,
            ));
        }

        lines.push("╚══════════════════════════════════════╝".to_string());
        lines.join("\n")
    }

    /// Route an input event through the scene graph and return the action.
    pub fn route_input(&self, event: &InputEvent) -> InputAction {
        match event {
            InputEvent::MouseClick { button: 1, x, y } => self.handle_left_click(*x, *y),
            InputEvent::MouseClick { button: 3, x, y } => {
                // Right-click → just forward to client
                let mut scene = self.scene.write().unwrap_or_else(|e| e.into_inner());
                if let Some(id) = scene.surface_at(*x, *y) {
                    InputAction::ClientClick(id, *x, *y)
                } else {
                    InputAction::None
                }
            }
            InputEvent::MouseMove { x, y } => self.handle_mouse_move(*x, *y),
            InputEvent::KeyPress { keycode, modifiers } => {
                let focused = self
                    .focused_window
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                if focused.is_some() {
                    InputAction::KeyToFocused(*keycode, *modifiers)
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn handle_left_click(&self, x: i32, y: i32) -> InputAction {
        let mut scene = self.scene.write().unwrap_or_else(|e| e.into_inner());
        let surface_id = match scene.surface_at(x, y) {
            Some(id) => id,
            None => return InputAction::None,
        };

        let surface: SceneSurface = match scene.get_surface(surface_id) {
            Some(s) => s.clone(),
            None => return InputAction::None,
        };

        // Hit-test against window decorations
        let hit = renderer::decoration_hit_test(&surface.geometry, x, y, &surface.window_state);

        // Focus and raise the window
        drop(scene);
        self.focus_window(surface_id);

        match hit {
            DecorationHit::TitleBar => {
                // Start drag — store offset from window origin
                let offset_x = x - surface.geometry.x;
                let offset_y = y - surface.geometry.y;
                *self.drag_state.write().unwrap_or_else(|e| e.into_inner()) =
                    Some((surface_id, offset_x, offset_y));
                InputAction::BeginDrag(surface_id)
            }
            DecorationHit::CloseButton => InputAction::Close(surface_id),
            DecorationHit::MinimizeButton => InputAction::Minimize(surface_id),
            DecorationHit::MaximizeButton => InputAction::ToggleMaximize(surface_id),
            DecorationHit::Border(edge) => {
                *self.resize_state.write().unwrap_or_else(|e| e.into_inner()) =
                    Some((surface_id, edge.clone(), surface.geometry));
                InputAction::BeginResize(surface_id, edge)
            }
            DecorationHit::ClientArea => {
                let local_x = x - surface.geometry.x;
                let local_y = y - surface.geometry.y - TITLEBAR_HEIGHT as i32;
                InputAction::ClientClick(surface_id, local_x, local_y)
            }
            DecorationHit::Outside => InputAction::None,
        }
    }

    fn handle_mouse_move(&self, x: i32, y: i32) -> InputAction {
        // Handle active drag
        let drag = *self.drag_state.read().unwrap_or_else(|e| e.into_inner());
        if let Some((id, offset_x, offset_y)) = drag {
            let new_x = x - offset_x;
            let new_y = y - offset_y;
            self.move_window(id, new_x, new_y);
            return InputAction::PointerMove(x, y);
        }

        // Handle active resize
        let resize = self
            .resize_state
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some((id, ref edge, ref original)) = resize {
            self.apply_resize(id, edge, original, x, y);
            return InputAction::PointerMove(x, y);
        }

        InputAction::PointerMove(x, y)
    }

    /// End any active drag or resize operation (call on mouse button release).
    pub fn end_interactive(&self) {
        *self.drag_state.write().unwrap_or_else(|e| e.into_inner()) = None;
        *self.resize_state.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Move a window to a new position.
    pub fn move_window(&self, id: SurfaceId, x: i32, y: i32) {
        if let Some(w) = self
            .windows
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            w.geometry.x = x;
            w.geometry.y = y;
        }
        if let Some(s) = self
            .scene
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_surface_mut(id)
        {
            s.geometry.x = x;
            s.geometry.y = y;
        }
        debug!("Moved window {} to ({}, {})", id, x, y);
    }

    /// Resize a window.
    pub fn resize_window(&self, id: SurfaceId, width: u32, height: u32) {
        let min_w = 200u32;
        let min_h = 100u32;
        let w = width.max(min_w);
        let h = height.max(min_h);

        if let Some(win) = self
            .windows
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            win.geometry.width = w;
            win.geometry.height = h;
        }
        if let Some(s) = self
            .scene
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_surface_mut(id)
        {
            s.geometry.width = w;
            s.geometry.height = h;
        }
    }

    fn apply_resize(
        &self,
        id: SurfaceId,
        edge: &ResizeEdge,
        original: &Rectangle,
        mx: i32,
        my: i32,
    ) {
        let dx = mx - (original.x + original.width as i32);
        let dy = my - (original.y + original.height as i32);

        let (mut x, mut y, mut w, mut h) = (
            original.x,
            original.y,
            original.width as i32,
            original.height as i32,
        );

        match edge {
            ResizeEdge::Right => w = original.width as i32 + dx,
            ResizeEdge::Bottom => h = original.height as i32 + dy,
            ResizeEdge::BottomRight => {
                w = original.width as i32 + dx;
                h = original.height as i32 + dy;
            }
            ResizeEdge::Left => {
                let delta = mx - original.x;
                x = original.x + delta;
                w = original.width as i32 - delta;
            }
            ResizeEdge::Top => {
                let delta = my - original.y;
                y = original.y + delta;
                h = original.height as i32 - delta;
            }
            ResizeEdge::TopLeft => {
                let dx2 = mx - original.x;
                let dy2 = my - original.y;
                x = original.x + dx2;
                y = original.y + dy2;
                w = original.width as i32 - dx2;
                h = original.height as i32 - dy2;
            }
            ResizeEdge::TopRight => {
                let dy2 = my - original.y;
                y = original.y + dy2;
                w = original.width as i32 + dx;
                h = original.height as i32 - dy2;
            }
            ResizeEdge::BottomLeft => {
                let dx2 = mx - original.x;
                x = original.x + dx2;
                w = original.width as i32 - dx2;
                h = original.height as i32 + dy;
            }
        }

        let w = w.max(200) as u32;
        let h = h.max(100) as u32;

        if let Some(win) = self
            .windows
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            win.geometry.x = x;
            win.geometry.y = y;
            win.geometry.width = w;
            win.geometry.height = h;
        }
        if let Some(s) = self
            .scene
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_surface_mut(id)
        {
            s.geometry.x = x;
            s.geometry.y = y;
            s.geometry.width = w;
            s.geometry.height = h;
        }
    }

    /// Focus a window and raise it.
    pub fn focus_window(&self, id: SurfaceId) {
        // Unfocus previous
        let prev = *self
            .focused_window
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut scene = self.scene.write().unwrap_or_else(|e| e.into_inner());
        if let Some(prev_id) = prev {
            if let Some(s) = scene.get_surface_mut(prev_id) {
                s.is_active = false;
            }
        }
        // Focus new
        if let Some(s) = scene.get_surface_mut(id) {
            s.is_active = true;
        }
        scene.raise_surface(id);
        drop(scene);

        *self
            .focused_window
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(id);

        // Sync accessibility focus
        {
            let mut tree = self
                .accessibility_tree
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let _ = tree.set_focus(&id);
        }
    }

    /// Get the currently focused window.
    pub fn focused_window(&self) -> Option<SurfaceId> {
        *self
            .focused_window
            .read()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Navigate accessibility tree forward or backward and focus the corresponding window.
    pub fn navigate_accessibility(&self, forward: bool) -> Option<SurfaceId> {
        let mut tree = self
            .accessibility_tree
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let node_id = if forward {
            tree.navigate_next().map(|n| n.id)
        } else {
            tree.navigate_prev().map(|n| n.id)
        };

        if let Some(nid) = node_id {
            // Focus the window corresponding to this node
            *self
                .focused_window
                .write()
                .unwrap_or_else(|e| e.into_inner()) = Some(nid);
            // Update workspace active window
            let ws_idx = *self
                .active_workspace
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let mut workspaces = self.workspaces.write().unwrap_or_else(|e| e.into_inner());
            if let Some(ws) = workspaces.get_mut(ws_idx) {
                ws.active_window = Some(nid);
            }
            Some(nid)
        } else {
            None
        }
    }

    /// Get a reference to the accessibility tree.
    pub fn accessibility_tree(&self) -> &Arc<RwLock<AccessibilityTree>> {
        &self.accessibility_tree
    }

    /// Queue a screen-reader announcement.
    pub fn announce(&self, message: &str) {
        let mut tree = self
            .accessibility_tree
            .write()
            .unwrap_or_else(|e| e.into_inner());
        tree.announce(message);
    }

    /// Set or clear the high-contrast theme on the renderer.
    pub fn set_high_contrast_theme(&self, theme: Option<HighContrastTheme>) {
        let mut renderer = self.renderer.write().unwrap_or_else(|e| e.into_inner());
        renderer.high_contrast = theme;
    }

    /// Render a frame. Access the result via `with_front_buffer()` to avoid copying.
    pub fn render(&self) {
        let mut renderer = self.renderer.write().unwrap_or_else(|e| e.into_inner());
        let mut scene = self.scene.write().unwrap_or_else(|e| e.into_inner());
        renderer.render_frame(&mut scene);
    }

    /// Access the front buffer bytes without copying.
    /// The callback receives a slice of the rendered ARGB8888 pixels.
    pub fn with_front_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let renderer = self.renderer.read().unwrap_or_else(|e| e.into_inner());
        f(renderer.front_buffer().as_bytes())
    }

    /// Render and return a copy of the front buffer (convenience for tests/snapshots).
    pub fn render_to_vec(&self) -> Vec<u8> {
        self.render();
        self.with_front_buffer(|bytes| bytes.to_vec())
    }

    /// Submit a window content buffer for rendering.
    pub fn submit_window_buffer(&self, id: SurfaceId, buffer: renderer::Framebuffer) {
        self.renderer
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .submit_buffer(id, buffer);
    }

    /// Tile all visible windows in the active workspace in a grid layout.
    pub fn tile_windows(&self) {
        let active_ws = *self
            .active_workspace
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let workspaces = self.workspaces.read().unwrap_or_else(|e| e.into_inner());
        let ws_windows = match workspaces.get(active_ws) {
            Some(ws) => ws.windows.clone(),
            None => return,
        };
        drop(workspaces);

        let output = *self
            .current_output
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let count = ws_windows.len();
        if count == 0 {
            return;
        }

        let cols = ((count as f64).sqrt().ceil()) as u32;
        let rows = ((count as f64) / cols as f64).ceil() as u32;
        let tile_w = output.width / cols;
        let tile_h = output.height / rows;

        for (i, win_id) in ws_windows.iter().enumerate() {
            let col = (i as u32) % cols;
            let row = (i as u32) / cols;
            let geom = Rectangle {
                x: (col * tile_w) as i32,
                y: (row * tile_h) as i32,
                width: tile_w,
                height: tile_h,
            };

            if let Some(w) = self
                .windows
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(win_id)
            {
                w.geometry = geom;
                w.state = WindowState::Normal;
            }
            if let Some(s) = self
                .scene
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .get_surface_mut(*win_id)
            {
                s.geometry = geom;
                s.window_state = WindowState::Normal;
            }
        }
        info!("Tiled {} windows in {}x{} grid", count, cols, rows);
    }

    pub fn get_agent_windows(&self) -> Vec<Window> {
        self.windows
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|w| w.is_agent_window)
            .cloned()
            .collect()
    }

    pub fn get_workspace_context(
        &self,
        workspace_id: usize,
    ) -> Result<ContextType, CompositorError> {
        let workspaces = self.workspaces.read().unwrap_or_else(|e| e.into_inner());
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
        let mut workspaces = self.workspaces.write().unwrap_or_else(|e| e.into_inner());
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

/// Wayland compositor backend.
///
/// Bridges the [`CompositorBackend`] trait to the Wayland protocol layer
/// via [`crate::wayland::ProtocolBridge`]. Input events are routed through
/// the bridge, and renders trigger a compositor frame.
pub struct WaylandBackend {
    bridge: crate::wayland::ProtocolBridge,
    compositor: Option<Arc<Compositor>>,
}

impl WaylandBackend {
    pub fn new() -> Self {
        Self {
            bridge: crate::wayland::ProtocolBridge::new(),
            compositor: None,
        }
    }

    /// Attach a compositor instance for protocol bridge actions.
    pub fn attach_compositor(&mut self, compositor: Arc<Compositor>) {
        self.compositor = Some(compositor);
    }

    /// Access the protocol bridge directly.
    pub fn bridge(&self) -> &crate::wayland::ProtocolBridge {
        &self.bridge
    }

    /// Access the protocol bridge mutably.
    pub fn bridge_mut(&mut self) -> &mut crate::wayland::ProtocolBridge {
        &mut self.bridge
    }
}

impl Default for WaylandBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CompositorBackend for WaylandBackend {
    fn initialize(&mut self) -> Result<(), CompositorError> {
        info!("Initializing Wayland backend with protocol bridge");
        Ok(())
    }

    fn handle_input(&mut self, event: InputEvent) -> Result<(), CompositorError> {
        if let Some(ref compositor) = self.compositor {
            self.bridge.route_input(compositor, &event);
        } else {
            match &event {
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
        }
        Ok(())
    }

    fn render(&mut self) -> Result<(), CompositorError> {
        if let Some(ref compositor) = self.compositor {
            self.bridge.apply_actions(compositor);
            compositor.render();
        }
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
        let mut backend = WaylandBackend::new();
        assert!(backend.initialize().is_ok());
        assert!(backend.render().is_ok());
        assert!(backend.shutdown().is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_input() {
        let mut backend = WaylandBackend::new();
        let event = InputEvent::MouseMove { x: 100, y: 200 };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_click() {
        let mut backend = WaylandBackend::new();
        let event = InputEvent::MouseClick {
            button: 1,
            x: 50,
            y: 50,
        };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_keypress() {
        let mut backend = WaylandBackend::new();
        let event = InputEvent::KeyPress {
            keycode: 65,
            modifiers: 0,
        };
        assert!(backend.handle_input(event).is_ok());
    }

    #[test]
    fn test_wayland_backend_handle_touch() {
        let mut backend = WaylandBackend::new();
        let event = InputEvent::TouchEvent {
            finger_id: 0,
            x: 100.0,
            y: 200.0,
            phase: TouchPhase::Down,
        };
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
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(workspaces[0].windows.contains(&id));
        assert_eq!(workspaces[0].active_window, Some(id));
    }

    #[test]
    fn test_compositor_close_updates_active_window() {
        let compositor = Compositor::new();
        let id1 = compositor
            .create_window("W1".to_string(), "app".to_string(), false)
            .unwrap();
        let id2 = compositor
            .create_window("W2".to_string(), "app".to_string(), false)
            .unwrap();
        // Close the active (last created) window
        compositor.close_window(id2).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        // Active should fall back to id1 (last remaining)
        assert_eq!(workspaces[0].active_window, Some(id1));
    }

    #[test]
    fn test_compositor_close_last_window_clears_active() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.close_window(id).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(workspaces[0].active_window.is_none());
    }

    #[test]
    fn test_compositor_set_all_window_states() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();

        for state in [
            WindowState::Minimized,
            WindowState::Maximized,
            WindowState::Fullscreen,
            WindowState::Floating,
            WindowState::Normal,
        ] {
            compositor.set_window_state(id, state.clone()).unwrap();
            let windows = compositor.get_windows();
            assert_eq!(windows[0].state, state);
        }
    }

    #[test]
    fn test_compositor_move_window_removes_from_source() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.move_window_to_workspace(id, 2).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(!workspaces[0].windows.contains(&id));
        assert!(workspaces[2].windows.contains(&id));
    }

    #[test]
    fn test_compositor_get_active_windows_after_workspace_switch() {
        let compositor = Compositor::new();
        let _id1 = compositor
            .create_window("W1".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.switch_workspace(1).unwrap();
        let _id2 = compositor
            .create_window("W2".to_string(), "app".to_string(), false)
            .unwrap();
        // Active workspace is 1, so only W2 should be returned
        let active = compositor.get_active_windows();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "W2");
    }

    #[test]
    fn test_compositor_agent_windows_filter() {
        let compositor = Compositor::new();
        compositor
            .create_window("Normal".to_string(), "app".to_string(), false)
            .unwrap();
        compositor
            .create_window("Agent1".to_string(), "agent".to_string(), true)
            .unwrap();
        compositor
            .create_window("Agent2".to_string(), "agent".to_string(), true)
            .unwrap();
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
        let id1 = compositor
            .create_window("W1".to_string(), "a".to_string(), false)
            .unwrap();
        let id2 = compositor
            .create_window("W2".to_string(), "b".to_string(), false)
            .unwrap();
        let id3 = compositor
            .create_window("W3".to_string(), "c".to_string(), false)
            .unwrap();
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
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
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
            let w = compositor
                .get_windows()
                .into_iter()
                .find(|w| w.id == id)
                .unwrap();
            assert_eq!(w.state, state);
        }
    }

    #[test]
    fn test_compositor_move_window_to_same_workspace() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        // Move to workspace 0 (same as active)
        compositor.move_window_to_workspace(id, 0).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        // Window should be in workspace 0 (might be duplicated, but that's the current impl)
        assert!(workspaces[0].windows.contains(&id));
    }

    #[test]
    fn test_compositor_move_window_to_last_workspace() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.move_window_to_workspace(id, 3).unwrap();
        let workspaces = compositor.workspaces.read().unwrap();
        assert!(workspaces[3].windows.contains(&id));
        assert_eq!(workspaces[3].active_window, Some(id));
    }

    #[test]
    fn test_compositor_window_created_at_is_recent() {
        let before = chrono::Utc::now();
        let compositor = Compositor::new();
        let _id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let after = chrono::Utc::now();
        let window = &compositor.get_windows()[0];
        assert!(window.created_at >= before);
        assert!(window.created_at <= after);
    }

    #[test]
    fn test_compositor_close_window_double_close_fails() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
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
    fn test_compositor_error_display_window_not_found_hides_uuid() {
        let id = Uuid::new_v4();
        let err = CompositorError::WindowNotFound(id);
        // Surface IDs must NOT leak in error messages (info leakage fix)
        assert!(!err.to_string().contains(&id.to_string()));
        assert_eq!(err.to_string(), "Window not found");
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
        let _id1 = compositor
            .create_window("W1".to_string(), "a".to_string(), false)
            .unwrap();
        let _id2 = compositor
            .create_window("W2".to_string(), "b".to_string(), false)
            .unwrap();
        let id3 = compositor
            .create_window("W3".to_string(), "c".to_string(), false)
            .unwrap();
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
        let id_ws0 = compositor
            .create_window("WS0".to_string(), "a".to_string(), false)
            .unwrap();
        compositor.switch_workspace(1).unwrap();
        let _id_ws1 = compositor
            .create_window("WS1".to_string(), "b".to_string(), false)
            .unwrap();
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

    // --- New: renderer integration tests ---

    #[test]
    fn test_compositor_with_resolution() {
        let compositor = Compositor::with_resolution(800, 600);
        let output = compositor.current_output.read().unwrap();
        assert_eq!(output.width, 800);
        assert_eq!(output.height, 600);
    }

    #[test]
    fn test_compositor_create_window_adds_to_scene() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("Test".to_string(), "app".to_string(), false)
            .unwrap();
        let scene = compositor.scene.read().unwrap();
        assert_eq!(scene.total_count(), 1);
        let surface = scene.get_surface(id).unwrap();
        assert_eq!(surface.title, "Test");
        assert!(surface.is_active);
        assert!(surface.visible);
        assert_eq!(surface.layer, Layer::Normal);
    }

    #[test]
    fn test_compositor_agent_window_gets_floating_layer() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("Agent".to_string(), "agent".to_string(), true)
            .unwrap();
        let scene = compositor.scene.read().unwrap();
        let surface = scene.get_surface(id).unwrap();
        assert_eq!(surface.layer, Layer::Floating);
    }

    #[test]
    fn test_compositor_close_removes_from_scene() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        assert_eq!(compositor.scene.read().unwrap().total_count(), 1);
        compositor.close_window(id).unwrap();
        assert_eq!(compositor.scene.read().unwrap().total_count(), 0);
    }

    #[test]
    fn test_compositor_focus_window() {
        let compositor = Compositor::new();
        let id1 = compositor
            .create_window("W1".to_string(), "a".to_string(), false)
            .unwrap();
        let id2 = compositor
            .create_window("W2".to_string(), "b".to_string(), false)
            .unwrap();
        // id2 should be focused (last created)
        assert_eq!(compositor.focused_window(), Some(id2));

        // Focus id1
        compositor.focus_window(id1);
        assert_eq!(compositor.focused_window(), Some(id1));
        let scene = compositor.scene.read().unwrap();
        assert!(scene.get_surface(id1).unwrap().is_active);
        assert!(!scene.get_surface(id2).unwrap().is_active);
    }

    #[test]
    fn test_compositor_close_updates_focus() {
        let compositor = Compositor::new();
        let id1 = compositor
            .create_window("W1".to_string(), "a".to_string(), false)
            .unwrap();
        let id2 = compositor
            .create_window("W2".to_string(), "b".to_string(), false)
            .unwrap();
        assert_eq!(compositor.focused_window(), Some(id2));
        compositor.close_window(id2).unwrap();
        assert_eq!(compositor.focused_window(), Some(id1));
    }

    #[test]
    fn test_compositor_move_window() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.move_window(id, 300, 200);
        let windows = compositor.get_windows();
        assert_eq!(windows[0].geometry.x, 300);
        assert_eq!(windows[0].geometry.y, 200);
        let scene = compositor.scene.read().unwrap();
        let s = scene.get_surface(id).unwrap();
        assert_eq!(s.geometry.x, 300);
        assert_eq!(s.geometry.y, 200);
    }

    #[test]
    fn test_compositor_resize_window() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.resize_window(id, 500, 400);
        let windows = compositor.get_windows();
        assert_eq!(windows[0].geometry.width, 500);
        assert_eq!(windows[0].geometry.height, 400);
    }

    #[test]
    fn test_compositor_resize_enforces_minimum() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.resize_window(id, 50, 30);
        let windows = compositor.get_windows();
        assert_eq!(windows[0].geometry.width, 200); // min
        assert_eq!(windows[0].geometry.height, 100); // min
    }

    #[test]
    fn test_compositor_set_state_maximized_updates_geometry() {
        let compositor = Compositor::with_resolution(1920, 1080);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor
            .set_window_state(id, WindowState::Maximized)
            .unwrap();
        let windows = compositor.get_windows();
        assert_eq!(windows[0].geometry.x, 0);
        assert_eq!(windows[0].geometry.y, 0);
        assert_eq!(windows[0].geometry.width, 1920);
        assert_eq!(windows[0].geometry.height, 1080);
    }

    #[test]
    fn test_compositor_set_state_minimized_hides_in_scene() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor
            .set_window_state(id, WindowState::Minimized)
            .unwrap();
        let scene = compositor.scene.read().unwrap();
        assert!(!scene.get_surface(id).unwrap().visible);
    }

    #[test]
    fn test_compositor_render_produces_output() {
        let compositor = Compositor::with_resolution(100, 100);
        compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.render();
        compositor.with_front_buffer(|bytes| {
            assert_eq!(bytes.len(), 100 * 100 * 4);
            // Not all zeros (background + window decorations should produce non-black pixels)
            assert!(bytes.iter().any(|&b| b != 0));
        });
    }

    #[test]
    fn test_compositor_route_input_click_on_background() {
        let compositor = Compositor::with_resolution(800, 600);
        let event = InputEvent::MouseClick {
            button: 1,
            x: 0,
            y: 0,
        };
        let action = compositor.route_input(&event);
        assert_eq!(action, InputAction::None);
    }

    #[test]
    fn test_compositor_route_input_click_on_titlebar() {
        let compositor = Compositor::with_resolution(800, 600);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let win = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id)
            .unwrap();
        // Click in the middle of the title bar
        let click_x = win.geometry.x + 20;
        let click_y = win.geometry.y + 10;
        let event = InputEvent::MouseClick {
            button: 1,
            x: click_x,
            y: click_y,
        };
        let action = compositor.route_input(&event);
        assert_eq!(action, InputAction::BeginDrag(id));
    }

    #[test]
    fn test_compositor_route_input_click_on_client() {
        let compositor = Compositor::with_resolution(800, 600);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let win = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id)
            .unwrap();
        // Click inside the client area (below title bar, inside borders)
        let click_x = win.geometry.x + 50;
        let click_y = win.geometry.y + TITLEBAR_HEIGHT as i32 + 10;
        let event = InputEvent::MouseClick {
            button: 1,
            x: click_x,
            y: click_y,
        };
        let action = compositor.route_input(&event);
        match action {
            InputAction::ClientClick(sid, _, _) => assert_eq!(sid, id),
            other => panic!("Expected ClientClick, got {:?}", other),
        }
    }

    #[test]
    fn test_compositor_route_input_key_press_with_focus() {
        let compositor = Compositor::new();
        let _id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let event = InputEvent::KeyPress {
            keycode: 65,
            modifiers: 0,
        };
        let action = compositor.route_input(&event);
        assert_eq!(action, InputAction::KeyToFocused(65, 0));
    }

    #[test]
    fn test_compositor_route_input_key_press_no_focus() {
        let compositor = Compositor::new();
        let event = InputEvent::KeyPress {
            keycode: 65,
            modifiers: 0,
        };
        let action = compositor.route_input(&event);
        assert_eq!(action, InputAction::None);
    }

    #[test]
    fn test_compositor_drag_workflow() {
        let compositor = Compositor::with_resolution(800, 600);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let win = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id)
            .unwrap();
        let start_x = win.geometry.x;

        // Click on title bar to start drag
        let click_x = start_x + 20;
        let click_y = win.geometry.y + 10;
        let event = InputEvent::MouseClick {
            button: 1,
            x: click_x,
            y: click_y,
        };
        let action = compositor.route_input(&event);
        assert_eq!(action, InputAction::BeginDrag(id));

        // Move mouse
        let move_event = InputEvent::MouseMove {
            x: click_x + 100,
            y: click_y + 50,
        };
        compositor.route_input(&move_event);

        // Window should have moved
        let win_after = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id)
            .unwrap();
        assert_eq!(win_after.geometry.x, start_x + 100);

        // End drag
        compositor.end_interactive();
    }

    #[test]
    fn test_compositor_tile_windows() {
        let compositor = Compositor::with_resolution(800, 600);
        let _id1 = compositor
            .create_window("W1".to_string(), "a".to_string(), false)
            .unwrap();
        let _id2 = compositor
            .create_window("W2".to_string(), "b".to_string(), false)
            .unwrap();
        compositor.tile_windows();

        let windows = compositor.get_windows();
        // Two windows should tile side-by-side or in a grid
        let geometries: Vec<_> = windows.iter().map(|w| &w.geometry).collect();
        // They should collectively cover the screen without being all at the same position
        let positions: std::collections::HashSet<_> =
            geometries.iter().map(|g| (g.x, g.y)).collect();
        assert_eq!(positions.len(), 2);
    }

    #[test]
    fn test_compositor_tile_single_window() {
        let compositor = Compositor::with_resolution(800, 600);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        compositor.tile_windows();
        let win = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id)
            .unwrap();
        assert_eq!(win.geometry.x, 0);
        assert_eq!(win.geometry.y, 0);
        assert_eq!(win.geometry.width, 800);
        assert_eq!(win.geometry.height, 600);
    }

    #[test]
    fn test_compositor_tile_no_windows() {
        let compositor = Compositor::new();
        compositor.tile_windows(); // Should not panic
    }

    #[test]
    fn test_compositor_submit_window_buffer() {
        use crate::renderer::Framebuffer;
        let compositor = Compositor::with_resolution(200, 200);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let content = Framebuffer::new(50, 50, 0xFFFF0000); // Red
        compositor.submit_window_buffer(id, content);
        // Should render without panicking
        compositor.render();
    }

    #[test]
    fn test_compositor_end_interactive_clears_state() {
        let compositor = Compositor::new();
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        *compositor.drag_state.write().unwrap() = Some((id, 10, 10));
        compositor.end_interactive();
        assert!(compositor.drag_state.read().unwrap().is_none());
        assert!(compositor.resize_state.read().unwrap().is_none());
    }

    #[test]
    fn test_compositor_cascade_placement() {
        let compositor = Compositor::new();
        let id1 = compositor
            .create_window("W1".to_string(), "a".to_string(), false)
            .unwrap();
        let id2 = compositor
            .create_window("W2".to_string(), "b".to_string(), false)
            .unwrap();
        let w1 = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id1)
            .unwrap();
        let w2 = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id2)
            .unwrap();
        // Windows should be cascaded (different positions)
        assert_ne!(w1.geometry.x, w2.geometry.x);
        assert_ne!(w1.geometry.y, w2.geometry.y);
    }

    #[test]
    fn test_input_action_variants() {
        let id = Uuid::new_v4();
        let actions = vec![
            InputAction::BeginDrag(id),
            InputAction::Close(id),
            InputAction::Minimize(id),
            InputAction::ToggleMaximize(id),
            InputAction::BeginResize(id, ResizeEdge::Bottom),
            InputAction::ClientClick(id, 10, 20),
            InputAction::KeyToFocused(65, 0),
            InputAction::PointerMove(100, 200),
            InputAction::None,
        ];
        assert_eq!(actions.len(), 9);
    }

    #[test]
    fn test_compositor_mouse_move_no_drag() {
        let compositor = Compositor::new();
        let event = InputEvent::MouseMove { x: 100, y: 100 };
        let action = compositor.route_input(&event);
        assert_eq!(action, InputAction::PointerMove(100, 100));
    }

    #[test]
    fn test_compositor_right_click_on_window() {
        let compositor = Compositor::with_resolution(800, 600);
        let id = compositor
            .create_window("W".to_string(), "app".to_string(), false)
            .unwrap();
        let win = compositor
            .get_windows()
            .into_iter()
            .find(|w| w.id == id)
            .unwrap();
        let event = InputEvent::MouseClick {
            button: 3,
            x: win.geometry.x + 50,
            y: win.geometry.y + 50,
        };
        let action = compositor.route_input(&event);
        match action {
            InputAction::ClientClick(sid, _, _) => assert_eq!(sid, id),
            other => panic!("Expected ClientClick, got {:?}", other),
        }
    }
}

// ============================================================================
// Clipboard integration
// ============================================================================

use std::path::PathBuf;

/// Content stored on the clipboard.
#[derive(Debug, Clone, Default)]
pub enum ClipboardContent {
    Text(String),
    Html(String),
    Image {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    Files(Vec<PathBuf>),
    #[default]
    Empty,
}

/// A single clipboard history entry.
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub content: ClipboardContent,
    pub source: SurfaceId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub mime_type: String,
}

/// Manages clipboard state including content and history.
#[derive(Debug)]
pub struct ClipboardManager {
    current: ClipboardContent,
    current_source: Option<SurfaceId>,
    history: Vec<ClipboardEntry>,
    history_limit: usize,
}

impl ClipboardManager {
    /// Create a new clipboard manager with a default history limit of 25.
    pub fn new() -> Self {
        Self {
            current: ClipboardContent::Empty,
            current_source: None,
            history: Vec::new(),
            history_limit: 25,
        }
    }

    /// Copy content to the clipboard from the given surface.
    pub fn set_content(&mut self, content: ClipboardContent, source: SurfaceId) {
        let mime = Self::mime_for_content(&content).to_string();
        let entry = ClipboardEntry {
            content: content.clone(),
            source,
            timestamp: chrono::Utc::now(),
            mime_type: mime,
        };
        self.current = content;
        self.current_source = Some(source);
        self.history.push(entry);
        // Trim history to limit.
        while self.history.len() > self.history_limit {
            self.history.remove(0);
        }
    }

    /// Get a reference to the current clipboard content.
    pub fn get_content(&self) -> &ClipboardContent {
        &self.current
    }

    /// Whether the clipboard has non-empty content.
    pub fn has_content(&self) -> bool {
        !matches!(self.current, ClipboardContent::Empty)
    }

    /// Clear the clipboard.
    pub fn clear(&mut self) {
        self.current = ClipboardContent::Empty;
        self.current_source = None;
    }

    /// MIME type string for the current content.
    pub fn content_type(&self) -> &str {
        Self::mime_for_content(&self.current)
    }

    /// Get the clipboard history.
    pub fn history(&self) -> &[ClipboardEntry] {
        &self.history
    }

    /// Get the configured history limit.
    pub fn history_limit(&self) -> usize {
        self.history_limit
    }

    /// Determine MIME type for a clipboard content variant.
    fn mime_for_content(content: &ClipboardContent) -> &str {
        match content {
            ClipboardContent::Text(_) => "text/plain",
            ClipboardContent::Html(_) => "text/html",
            ClipboardContent::Image { .. } => "image/png",
            ClipboardContent::Files(_) => "text/uri-list",
            ClipboardContent::Empty => "",
        }
    }
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    #[test]
    fn test_clipboard_new_is_empty() {
        let cb = ClipboardManager::new();
        assert!(!cb.has_content());
        assert!(matches!(cb.get_content(), ClipboardContent::Empty));
        assert_eq!(cb.content_type(), "");
    }

    #[test]
    fn test_clipboard_set_text() {
        let mut cb = ClipboardManager::new();
        let source = Uuid::new_v4();
        cb.set_content(ClipboardContent::Text("hello".into()), source);
        assert!(cb.has_content());
        assert_eq!(cb.content_type(), "text/plain");
        match cb.get_content() {
            ClipboardContent::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_clipboard_set_html() {
        let mut cb = ClipboardManager::new();
        cb.set_content(ClipboardContent::Html("<b>bold</b>".into()), Uuid::new_v4());
        assert_eq!(cb.content_type(), "text/html");
    }

    #[test]
    fn test_clipboard_set_image() {
        let mut cb = ClipboardManager::new();
        cb.set_content(
            ClipboardContent::Image {
                width: 10,
                height: 10,
                data: vec![0u8; 400],
            },
            Uuid::new_v4(),
        );
        assert_eq!(cb.content_type(), "image/png");
    }

    #[test]
    fn test_clipboard_set_files() {
        let mut cb = ClipboardManager::new();
        cb.set_content(
            ClipboardContent::Files(vec![PathBuf::from("/tmp/a.txt")]),
            Uuid::new_v4(),
        );
        assert_eq!(cb.content_type(), "text/uri-list");
    }

    #[test]
    fn test_clipboard_clear() {
        let mut cb = ClipboardManager::new();
        cb.set_content(ClipboardContent::Text("data".into()), Uuid::new_v4());
        assert!(cb.has_content());
        cb.clear();
        assert!(!cb.has_content());
    }

    #[test]
    fn test_clipboard_history() {
        let mut cb = ClipboardManager::new();
        let source = Uuid::new_v4();
        cb.set_content(ClipboardContent::Text("first".into()), source);
        cb.set_content(ClipboardContent::Text("second".into()), source);
        assert_eq!(cb.history().len(), 2);
        assert_eq!(cb.history()[0].mime_type, "text/plain");
    }

    #[test]
    fn test_clipboard_history_limit() {
        let mut cb = ClipboardManager::new();
        let source = Uuid::new_v4();
        let limit = cb.history_limit();
        for i in 0..(limit + 10) {
            cb.set_content(ClipboardContent::Text(format!("item {}", i)), source);
        }
        assert_eq!(cb.history().len(), limit);
    }

    #[test]
    fn test_clipboard_default() {
        let cb = ClipboardManager::default();
        assert!(!cb.has_content());
        assert_eq!(cb.history_limit(), 25);
    }

    #[test]
    fn test_clipboard_overwrite_content() {
        let mut cb = ClipboardManager::new();
        let source = Uuid::new_v4();
        cb.set_content(ClipboardContent::Text("old".into()), source);
        cb.set_content(ClipboardContent::Text("new".into()), source);
        match cb.get_content() {
            ClipboardContent::Text(s) => assert_eq!(s, "new"),
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_clipboard_entry_timestamp() {
        let mut cb = ClipboardManager::new();
        let before = chrono::Utc::now();
        cb.set_content(ClipboardContent::Text("ts".into()), Uuid::new_v4());
        let after = chrono::Utc::now();
        let entry = &cb.history()[0];
        assert!(entry.timestamp >= before);
        assert!(entry.timestamp <= after);
    }

    #[test]
    fn test_clipboard_content_default() {
        let content = ClipboardContent::default();
        assert!(matches!(content, ClipboardContent::Empty));
    }
}
