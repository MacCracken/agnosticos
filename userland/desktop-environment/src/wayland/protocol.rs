//! Protocol bridge and extension types for Wayland integration.

use std::collections::HashMap;

use crate::compositor::{Compositor, InputAction, InputEvent, SurfaceId, WindowState};

use super::types::*;

/// Actions the compositor should take in response to protocol events.
#[derive(Debug, Clone)]
pub enum ProtocolAction {
    /// Create a new window for a client surface.
    CreateWindow {
        client_id: u32,
        surface_id: SurfaceId,
        title: String,
        app_id: String,
    },
    /// Destroy a surface and its window.
    DestroyWindow { surface_id: SurfaceId },
    /// Submit a pixel buffer to a window.
    SubmitBuffer {
        surface_id: SurfaceId,
        buffer: ShmBufferInfo,
    },
    /// Set window title.
    SetTitle {
        surface_id: SurfaceId,
        title: String,
    },
    /// Set window app_id.
    SetAppId {
        surface_id: SurfaceId,
        app_id: String,
    },
    /// Client requests window move (interactive drag).
    RequestMove { surface_id: SurfaceId },
    /// Client requests window resize from an edge.
    RequestResize { surface_id: SurfaceId, edge: u32 },
    /// Client requests maximized state.
    SetMaximized {
        surface_id: SurfaceId,
        maximized: bool,
    },
    /// Client requests fullscreen state.
    SetFullscreen {
        surface_id: SurfaceId,
        fullscreen: bool,
    },
    /// Client requests minimize.
    SetMinimized { surface_id: SurfaceId },
    /// Client sets min/max size constraints.
    SetSizeBounds {
        surface_id: SurfaceId,
        min_size: Option<(u32, u32)>,
        max_size: Option<(u32, u32)>,
    },
    /// Client acknowledged a configure event.
    AckConfigure { surface_id: SurfaceId, serial: u32 },
    /// Client committed a surface (buffer attached + damage).
    SurfaceCommit { surface_id: SurfaceId },
    /// Send configure event to client.
    SendConfigure { configure: ToplevelConfigure },
    /// Forward pointer event to focused client.
    ForwardPointer { event: WaylandPointerEvent },
    /// Forward keyboard event to focused client.
    ForwardKeyboard { event: WaylandKeyboardEvent },
    /// A new client connected.
    ClientConnected { client_id: u32 },
    /// A client disconnected — clean up all its surfaces.
    ClientDisconnected { client_id: u32 },
    /// Set clipboard selection for a surface.
    SetSelection {
        surface_id: SurfaceId,
        mime_types: Vec<String>,
    },
    /// Start a drag-and-drop operation.
    StartDrag {
        source: SurfaceId,
        icon: Option<SurfaceId>,
        mime_types: Vec<String>,
    },
    /// Enable text input on a surface.
    TextInputEnable { surface_id: SurfaceId },
    /// Disable text input on a surface.
    TextInputDisable { surface_id: SurfaceId },
    /// Commit text input.
    TextInputCommit { surface_id: SurfaceId, text: String },
    /// Set decoration mode for a surface.
    SetDecorationMode {
        surface_id: SurfaceId,
        mode: DecorationMode,
    },
    /// Set viewport for a surface.
    SetViewport {
        surface_id: SurfaceId,
        source: Option<ViewportSource>,
        destination: Option<(u32, u32)>,
    },
    /// Set fractional scale for a surface.
    SetFractionalScale {
        surface_id: SurfaceId,
        scale_120: u32,
    },
}

// ============================================================================
// Protocol extension types (data device, text input, decorations, viewporter,
// fractional scale)
// ============================================================================

/// Data device manager — manages clipboard and drag-and-drop.
#[derive(Debug, Clone)]
pub struct DataDeviceManager {
    pub selections: HashMap<SurfaceId, DataOffer>,
    pub drag_source: Option<DragState>,
}

#[derive(Debug, Clone)]
pub struct DataOffer {
    pub mime_types: Vec<String>,
    pub source_surface: SurfaceId,
    pub serial: u32,
}

#[derive(Debug, Clone)]
pub struct DragState {
    pub source_surface: SurfaceId,
    pub icon_surface: Option<SurfaceId>,
    pub mime_types: Vec<String>,
    pub position: (f64, f64),
    pub active: bool,
}

impl DataDeviceManager {
    pub fn new() -> Self {
        Self {
            selections: HashMap::new(),
            drag_source: None,
        }
    }

    pub fn set_selection(&mut self, surface_id: SurfaceId, mime_types: Vec<String>, serial: u32) {
        self.selections.insert(
            surface_id,
            DataOffer {
                mime_types,
                source_surface: surface_id,
                serial,
            },
        );
    }

    pub fn clear_selection(&mut self, surface_id: &SurfaceId) {
        self.selections.remove(surface_id);
    }

    pub fn start_drag(
        &mut self,
        source_surface: SurfaceId,
        icon_surface: Option<SurfaceId>,
        mime_types: Vec<String>,
    ) {
        self.drag_source = Some(DragState {
            source_surface,
            icon_surface,
            mime_types,
            position: (0.0, 0.0),
            active: true,
        });
    }

    pub fn end_drag(&mut self) {
        self.drag_source = None;
    }

    pub fn get_selection(&self, surface_id: &SurfaceId) -> Option<&DataOffer> {
        self.selections.get(surface_id)
    }
}

impl Default for DataDeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Text input state for IME integration (zwp_text_input_v3).
#[derive(Debug, Clone)]
pub struct TextInputState {
    pub surface_id: Option<SurfaceId>,
    pub enabled: bool,
    pub content_type: ContentType,
    pub surrounding_text: String,
    pub cursor_position: u32,
    pub preedit: Option<PreeditState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentType {
    #[default]
    Normal,
    Password,
    Email,
    Number,
    Phone,
    Url,
    Terminal,
}

#[derive(Debug, Clone)]
pub struct PreeditState {
    pub text: String,
    pub cursor_begin: i32,
    pub cursor_end: i32,
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            surface_id: None,
            enabled: false,
            content_type: ContentType::default(),
            surrounding_text: String::new(),
            cursor_position: 0,
            preedit: None,
        }
    }

    pub fn enable(&mut self, surface_id: SurfaceId) {
        self.surface_id = Some(surface_id);
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.surface_id = None;
        self.preedit = None;
    }

    pub fn set_surrounding_text(&mut self, text: String, cursor_position: u32) {
        self.surrounding_text = text;
        self.cursor_position = cursor_position;
    }

    pub fn commit_preedit(&mut self) -> Option<String> {
        self.preedit.take().map(|p| p.text)
    }

    pub fn clear_preedit(&mut self) {
        self.preedit = None;
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoration mode negotiation (xdg_decoration_unstable_v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecorationMode {
    ClientSide,
    #[default]
    ServerSide,
}

#[derive(Debug, Clone)]
pub struct DecorationState {
    pub surface_id: SurfaceId,
    pub preferred: DecorationMode,
    pub current: DecorationMode,
}

impl DecorationState {
    pub fn new(surface_id: SurfaceId) -> Self {
        Self {
            surface_id,
            preferred: DecorationMode::ServerSide,
            current: DecorationMode::ServerSide,
        }
    }

    /// Negotiate decoration mode. Returns the mode that will be used.
    /// The compositor prefers server-side decorations; the client's preference
    /// is honoured only when it also requests server-side, otherwise the
    /// compositor falls back to client-side if the client insists.
    pub fn negotiate(&mut self) -> DecorationMode {
        self.current = self.preferred;
        self.current
    }
}

/// Viewport state for surface scaling (wp_viewporter).
#[derive(Debug, Clone)]
pub struct ViewportState {
    pub surface_id: SurfaceId,
    pub source: Option<ViewportSource>,
    pub destination: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Copy)]
pub struct ViewportSource {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ViewportState {
    pub fn new(surface_id: SurfaceId) -> Self {
        Self {
            surface_id,
            source: None,
            destination: None,
        }
    }

    pub fn set_source(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.source = Some(ViewportSource {
            x,
            y,
            width,
            height,
        });
    }

    pub fn set_destination(&mut self, width: u32, height: u32) {
        self.destination = Some((width, height));
    }

    /// Returns the effective size: destination if set, otherwise source dimensions
    /// (truncated to u32), otherwise None.
    pub fn effective_size(&self) -> Option<(u32, u32)> {
        if let Some(dest) = self.destination {
            Some(dest)
        } else {
            self.source.map(|s| (s.width as u32, s.height as u32))
        }
    }
}

/// Fractional scale factor (wp_fractional_scale_v1).
#[derive(Debug, Clone)]
pub struct FractionalScale {
    pub surface_id: SurfaceId,
    /// Scale factor in 1/120ths (e.g., 120 = 1x, 150 = 1.25x, 240 = 2x).
    pub scale_120: u32,
}

impl FractionalScale {
    pub fn new(surface_id: SurfaceId, scale_120: u32) -> Self {
        Self {
            surface_id,
            scale_120,
        }
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_120 as f64 / 120.0
    }

    pub fn from_scale(surface_id: SurfaceId, scale: f64) -> Self {
        Self {
            surface_id,
            scale_120: (scale * 120.0).round() as u32,
        }
    }
}

/// Protocol bridge between Wayland protocol events and the AGNOS compositor.
///
/// This is feature-independent — the logic works identically in stub and live modes.
/// The live mode feeds real protocol events; the stub can be driven programmatically.
#[derive(Debug)]
pub struct ProtocolBridge {
    pub surface_map: SurfaceMap,
    pub clients: ClientRegistry,
    pub serial: SerialCounter,
    pub pointer_focus: PointerFocus,
    pub keyboard_focus: KeyboardFocus,
    pub toplevels: HashMap<SurfaceId, XdgToplevelTracker>,
    pub output: OutputInfo,
    pub seat_caps: SeatCapabilities,
    pending_actions: Vec<ProtocolAction>,
}

impl ProtocolBridge {
    pub fn new() -> Self {
        Self {
            surface_map: SurfaceMap::new(),
            clients: ClientRegistry::new(),
            serial: SerialCounter::new(),
            pointer_focus: PointerFocus::default(),
            keyboard_focus: KeyboardFocus::default(),
            toplevels: HashMap::new(),
            output: OutputInfo::default(),
            seat_caps: SeatCapabilities::default(),
            pending_actions: Vec::new(),
        }
    }

    /// Handle a new client connection.
    pub fn client_connect(&mut self, pid: Option<u32>) -> u32 {
        let id = if let Some(pid) = pid {
            self.clients.register_with_pid(pid)
        } else {
            self.clients.register()
        };
        self.pending_actions
            .push(ProtocolAction::ClientConnected { client_id: id });
        id
    }

    /// Handle client disconnection — removes all its surfaces.
    pub fn client_disconnect(&mut self, client_id: u32) -> Vec<SurfaceId> {
        let mut removed_surfaces = Vec::new();
        if let Some(info) = self.clients.unregister(client_id) {
            for surface_id in &info.surfaces {
                self.surface_map.unregister(surface_id);
                self.toplevels.remove(surface_id);
                self.pending_actions.push(ProtocolAction::DestroyWindow {
                    surface_id: *surface_id,
                });
                removed_surfaces.push(*surface_id);
            }
            // Clear focus if it belonged to this client
            if let Some(focused) = self.pointer_focus.surface_id {
                if info.surfaces.contains(&focused) {
                    self.pointer_focus.surface_id = None;
                }
            }
            if let Some(focused) = self.keyboard_focus.surface_id {
                if info.surfaces.contains(&focused) {
                    self.keyboard_focus.surface_id = None;
                }
            }
        }
        self.pending_actions
            .push(ProtocolAction::ClientDisconnected { client_id });
        removed_surfaces
    }

    /// Create a new wl_surface for a client.
    pub fn create_surface(&mut self, client_id: u32) -> Option<(SurfaceId, u32)> {
        let surface_id = uuid::Uuid::new_v4();
        let proto_id = self.surface_map.register(surface_id);
        if let Some(client) = self.clients.get_mut(client_id) {
            client.add_surface(surface_id);
        }
        Some((surface_id, proto_id))
    }

    /// Handle xdg_surface.get_toplevel — creates the toplevel tracker and triggers window creation.
    pub fn create_toplevel(&mut self, surface_id: SurfaceId, client_id: u32) -> &ToplevelConfigure {
        let tracker = XdgToplevelTracker::new(surface_id);
        self.toplevels.insert(surface_id, tracker);

        // Send initial configure
        let serial = self.serial.next_serial();
        let configure = ToplevelConfigure::initial(surface_id, serial);

        self.pending_actions.push(ProtocolAction::CreateWindow {
            client_id,
            surface_id,
            title: String::new(),
            app_id: String::new(),
        });

        // Safe: we just inserted the tracker above
        let tracker = self.toplevels.get_mut(&surface_id).expect("just inserted");
        tracker.send_configure(configure)
    }

    /// Handle xdg_toplevel.set_title.
    pub fn set_title(&mut self, surface_id: SurfaceId, title: String) {
        if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
            tracker.title = Some(title.clone());
        }
        self.pending_actions
            .push(ProtocolAction::SetTitle { surface_id, title });
    }

    /// Handle xdg_toplevel.set_app_id.
    pub fn set_app_id(&mut self, surface_id: SurfaceId, app_id: String) {
        if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
            tracker.app_id = Some(app_id.clone());
        }
        self.pending_actions
            .push(ProtocolAction::SetAppId { surface_id, app_id });
    }

    /// Handle xdg_surface.ack_configure.
    pub fn ack_configure(&mut self, surface_id: SurfaceId, serial: u32) -> bool {
        if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
            let result = tracker.ack_configure(serial);
            if result {
                self.pending_actions
                    .push(ProtocolAction::AckConfigure { surface_id, serial });
            }
            result
        } else {
            false
        }
    }

    /// Handle wl_surface.commit — maps the window if first commit after configure ack.
    pub fn surface_commit(&mut self, surface_id: SurfaceId) -> bool {
        let mapped = if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
            tracker.map()
        } else {
            false
        };
        self.pending_actions
            .push(ProtocolAction::SurfaceCommit { surface_id });
        mapped
    }

    /// Destroy a surface.
    pub fn destroy_surface(&mut self, surface_id: SurfaceId) {
        self.surface_map.unregister(&surface_id);
        self.toplevels.remove(&surface_id);
        // Remove from client's surface list
        if let Some(client_id) = self.clients.find_by_surface(&surface_id) {
            if let Some(client) = self.clients.get_mut(client_id) {
                client.remove_surface(&surface_id);
            }
        }
        self.pending_actions
            .push(ProtocolAction::DestroyWindow { surface_id });
    }

    /// Handle xdg_toplevel.set_maximized / unset_maximized.
    pub fn set_maximized(&mut self, surface_id: SurfaceId, maximized: bool) {
        if maximized {
            if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
                let serial = self.serial.next_serial();
                let configure = ToplevelConfigure::maximized(surface_id, &self.output, serial);
                tracker.send_configure(configure);
            }
        }
        self.pending_actions.push(ProtocolAction::SetMaximized {
            surface_id,
            maximized,
        });
    }

    /// Handle xdg_toplevel.set_fullscreen.
    pub fn set_fullscreen(&mut self, surface_id: SurfaceId, fullscreen: bool) {
        if fullscreen {
            if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
                let serial = self.serial.next_serial();
                let configure = ToplevelConfigure {
                    surface_id,
                    width: self.output.width_px,
                    height: self.output.height_px,
                    states: vec![XdgToplevelState::Fullscreen, XdgToplevelState::Activated],
                    serial,
                };
                tracker.send_configure(configure);
            }
        }
        self.pending_actions.push(ProtocolAction::SetFullscreen {
            surface_id,
            fullscreen,
        });
    }

    /// Handle xdg_toplevel.set_minimized.
    pub fn set_minimized(&mut self, surface_id: SurfaceId) {
        self.pending_actions
            .push(ProtocolAction::SetMinimized { surface_id });
    }

    /// Handle xdg_toplevel.set_min_size / set_max_size.
    pub fn set_size_bounds(
        &mut self,
        surface_id: SurfaceId,
        min_size: Option<(u32, u32)>,
        max_size: Option<(u32, u32)>,
    ) {
        if let Some(tracker) = self.toplevels.get_mut(&surface_id) {
            if min_size.is_some() {
                tracker.min_size = min_size;
            }
            if max_size.is_some() {
                tracker.max_size = max_size;
            }
        }
        self.pending_actions.push(ProtocolAction::SetSizeBounds {
            surface_id,
            min_size,
            max_size,
        });
    }

    /// Handle xdg_toplevel.move request.
    pub fn request_move(&mut self, surface_id: SurfaceId) {
        self.pending_actions
            .push(ProtocolAction::RequestMove { surface_id });
    }

    /// Handle xdg_toplevel.resize request.
    pub fn request_resize(&mut self, surface_id: SurfaceId, edge: u32) {
        self.pending_actions
            .push(ProtocolAction::RequestResize { surface_id, edge });
    }

    /// Route an input event to the appropriate client surface.
    pub fn route_input(&mut self, compositor: &Compositor, event: &InputEvent) {
        let action = compositor.route_input(event);
        match action {
            InputAction::ClientClick(surface_id, x, y) => {
                let serial = self.serial.next_serial();
                let focus_changed =
                    self.pointer_focus
                        .set_focus(Some(surface_id), x as f64, y as f64, serial);
                if focus_changed {
                    // Send pointer enter
                    self.pending_actions.push(ProtocolAction::ForwardPointer {
                        event: WaylandPointerEvent::Enter {
                            surface: surface_id,
                            x: x as f64,
                            y: y as f64,
                        },
                    });
                    // Update keyboard focus too
                    let kb_serial = self.serial.next_serial();
                    self.keyboard_focus.set_focus(Some(surface_id), kb_serial);
                    self.pending_actions.push(ProtocolAction::ForwardKeyboard {
                        event: WaylandKeyboardEvent::Enter {
                            surface: surface_id,
                        },
                    });
                }
                // Forward button event
                if let InputEvent::MouseClick { button, .. } = event {
                    self.pending_actions.push(ProtocolAction::ForwardPointer {
                        event: WaylandPointerEvent::Button {
                            button: *button,
                            x: x as f64,
                            y: y as f64,
                            pressed: true,
                        },
                    });
                }
            }
            InputAction::PointerMove(x, y) => {
                self.pointer_focus.motion(x as f64, y as f64);
                self.pending_actions.push(ProtocolAction::ForwardPointer {
                    event: WaylandPointerEvent::Motion {
                        x: x as f64,
                        y: y as f64,
                    },
                });
            }
            InputAction::KeyToFocused(keycode, modifiers) => {
                let mods = ModifierState::from_raw(modifiers);
                self.keyboard_focus.set_modifiers(mods);
                self.pending_actions.push(ProtocolAction::ForwardKeyboard {
                    event: WaylandKeyboardEvent::Key {
                        keycode,
                        modifiers: mods,
                        pressed: true,
                    },
                });
            }
            _ => {
                // BeginDrag, Close, Minimize, etc. are handled by the compositor directly
            }
        }
    }

    /// Drain all pending protocol actions for processing.
    pub fn drain_actions(&mut self) -> Vec<ProtocolAction> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Apply pending actions to the compositor.
    pub fn apply_actions(&mut self, compositor: &Compositor) -> Vec<ProtocolAction> {
        let actions = self.drain_actions();
        for action in &actions {
            match action {
                ProtocolAction::CreateWindow { title, app_id, .. } => {
                    let t = if title.is_empty() {
                        "Untitled".to_string()
                    } else {
                        title.clone()
                    };
                    let a = if app_id.is_empty() {
                        "unknown".to_string()
                    } else {
                        app_id.clone()
                    };
                    let _ = compositor.create_window(t, a, false);
                }
                ProtocolAction::DestroyWindow { surface_id } => {
                    let _ = compositor.close_window(*surface_id);
                }
                ProtocolAction::SetMaximized {
                    surface_id,
                    maximized,
                } => {
                    if *maximized {
                        let _ = compositor.set_window_state(*surface_id, WindowState::Maximized);
                    } else {
                        let _ = compositor.set_window_state(*surface_id, WindowState::Normal);
                    }
                }
                ProtocolAction::SetFullscreen {
                    surface_id,
                    fullscreen,
                } => {
                    if *fullscreen {
                        let _ = compositor.set_window_state(*surface_id, WindowState::Fullscreen);
                    } else {
                        let _ = compositor.set_window_state(*surface_id, WindowState::Normal);
                    }
                }
                ProtocolAction::SetMinimized { surface_id } => {
                    let _ = compositor.set_window_state(*surface_id, WindowState::Minimized);
                }
                ProtocolAction::SetTitle { .. } => {
                    // Title tracked in XdgToplevelTracker, applied on next render
                }
                ProtocolAction::RequestMove { surface_id } => {
                    compositor.focus_window(*surface_id);
                }
                _ => {}
            }
        }
        actions
    }

    /// Get the number of connected clients.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Get the number of tracked surfaces.
    pub fn surface_count(&self) -> usize {
        self.surface_map.len()
    }

    /// Get the number of mapped toplevels.
    pub fn mapped_toplevel_count(&self) -> usize {
        self.toplevels.values().filter(|t| t.mapped).count()
    }
}

impl Default for ProtocolBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps internal [`InputEvent`]s to Wayland protocol actions.
pub fn map_input_to_pointer_event(event: &InputEvent) -> Option<WaylandPointerEvent> {
    match event {
        InputEvent::MouseMove { x, y } => Some(WaylandPointerEvent::Motion {
            x: *x as f64,
            y: *y as f64,
        }),
        InputEvent::MouseClick { button, x, y } => Some(WaylandPointerEvent::Button {
            button: *button,
            x: *x as f64,
            y: *y as f64,
            pressed: true,
        }),
        _ => None,
    }
}

/// Maps internal [`InputEvent`]s to Wayland keyboard protocol actions.
pub fn map_input_to_keyboard_event(event: &InputEvent) -> Option<WaylandKeyboardEvent> {
    match event {
        InputEvent::KeyPress { keycode, modifiers } => Some(WaylandKeyboardEvent::Key {
            keycode: *keycode,
            modifiers: ModifierState::from_raw(*modifiers),
            pressed: true,
        }),
        _ => None,
    }
}

/// Wayland pointer event (protocol-level).
#[derive(Debug, Clone)]
pub enum WaylandPointerEvent {
    Motion {
        x: f64,
        y: f64,
    },
    Button {
        button: u32,
        x: f64,
        y: f64,
        pressed: bool,
    },
    Axis {
        horizontal: f64,
        vertical: f64,
    },
    Enter {
        surface: SurfaceId,
        x: f64,
        y: f64,
    },
    Leave {
        surface: SurfaceId,
    },
}

/// Wayland keyboard event (protocol-level).
#[derive(Debug, Clone)]
pub enum WaylandKeyboardEvent {
    Key {
        keycode: u32,
        modifiers: ModifierState,
        pressed: bool,
    },
    Enter {
        surface: SurfaceId,
    },
    Leave {
        surface: SurfaceId,
    },
    Modifiers {
        state: ModifierState,
    },
}
