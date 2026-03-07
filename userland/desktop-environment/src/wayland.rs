//! Wayland protocol integration for AGNOS compositor.
//!
//! Bridges the internal compositor (scene graph, renderer, window management)
//! to the Wayland wire protocol, allowing real Wayland clients to connect.
//!
//! When the `wayland` feature is enabled, this module provides:
//! - [`WaylandState`] — the main server state holding the `wayland_server::Display`
//! - Surface tracking (wl_surface -> internal [`SurfaceId`])
//! - XDG Shell handling (xdg_wm_base, xdg_surface, xdg_toplevel)
//! - SHM buffer support (wl_shm shared-memory pixel buffers)
//! - Seat/Input forwarding (wl_seat, wl_keyboard, wl_pointer)
//! - Output advertising (wl_output screen geometry)
//!
//! Without the feature, a compile-time stub is provided so dependent code
//! still builds.

use std::collections::HashMap;
use std::sync::Arc;

use crate::compositor::{Compositor, InputAction, InputEvent, Rectangle, SurfaceId, WindowState};

// ============================================================================
// Shared types (available with or without the wayland feature)
// ============================================================================

/// Pixel format advertised to Wayland clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShmFormat {
    /// ARGB8888 — 32-bit with alpha.
    Argb8888,
    /// XRGB8888 — 32-bit without alpha (alpha ignored).
    Xrgb8888,
}

impl ShmFormat {
    /// Bytes per pixel for this format.
    pub fn bpp(&self) -> u32 {
        match self {
            ShmFormat::Argb8888 => 4,
            ShmFormat::Xrgb8888 => 4,
        }
    }

    /// All formats the compositor supports.
    pub fn supported_formats() -> &'static [ShmFormat] {
        &[ShmFormat::Argb8888, ShmFormat::Xrgb8888]
    }
}

/// Describes an output (monitor) as seen by Wayland clients.
#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub name: String,
    pub description: String,
    pub x: i32,
    pub y: i32,
    pub width_px: u32,
    pub height_px: u32,
    pub width_mm: u32,
    pub height_mm: u32,
    pub refresh_mhz: u32,
    pub scale: u32,
    pub make: String,
    pub model: String,
    pub transform: OutputTransform,
    pub subpixel: SubpixelLayout,
}

impl Default for OutputInfo {
    fn default() -> Self {
        Self {
            name: "AGNOS-1".to_string(),
            description: "AGNOS virtual output".to_string(),
            x: 0,
            y: 0,
            width_px: 1920,
            height_px: 1080,
            width_mm: 530,
            height_mm: 300,
            refresh_mhz: 60_000,
            scale: 1,
            make: "AGNOS".to_string(),
            model: "Virtual Display".to_string(),
            transform: OutputTransform::Normal,
            subpixel: SubpixelLayout::Unknown,
        }
    }
}

impl OutputInfo {
    /// Physical DPI (horizontal).
    pub fn dpi_x(&self) -> f64 {
        if self.width_mm == 0 {
            return 96.0;
        }
        self.width_px as f64 / (self.width_mm as f64 / 25.4)
    }

    /// Physical DPI (vertical).
    pub fn dpi_y(&self) -> f64 {
        if self.height_mm == 0 {
            return 96.0;
        }
        self.height_px as f64 / (self.height_mm as f64 / 25.4)
    }

    /// Effective logical size (after scale).
    pub fn logical_size(&self) -> (u32, u32) {
        if self.scale == 0 {
            return (self.width_px, self.height_px);
        }
        (self.width_px / self.scale, self.height_px / self.scale)
    }

    /// Refresh rate in Hz (floating point).
    pub fn refresh_hz(&self) -> f64 {
        self.refresh_mhz as f64 / 1000.0
    }

    /// Build an output from a compositor Rectangle (used for internal conversions).
    pub fn from_rectangle(rect: &Rectangle) -> Self {
        Self {
            width_px: rect.width,
            height_px: rect.height,
            ..Default::default()
        }
    }
}

/// Screen rotation / flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    FlippedRotate90,
    FlippedRotate180,
    FlippedRotate270,
}

/// Subpixel geometry of the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubpixelLayout {
    Unknown,
    None,
    HorizontalRgb,
    HorizontalBgr,
    VerticalRgb,
    VerticalBgr,
}

/// Describes a client-submitted SHM buffer.
#[derive(Debug, Clone)]
pub struct ShmBufferInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: ShmFormat,
    pub offset: u32,
}

impl ShmBufferInfo {
    /// Validate the buffer dimensions are internally consistent.
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("Buffer dimensions must be non-zero".to_string());
        }
        let min_stride = self
            .width
            .checked_mul(self.format.bpp())
            .ok_or("Stride overflow")?;
        if self.stride < min_stride {
            return Err(format!(
                "Stride {} too small for width {} at {} bpp (need >= {})",
                self.stride,
                self.width,
                self.format.bpp(),
                min_stride,
            ));
        }
        let total = self
            .stride
            .checked_mul(self.height)
            .ok_or("Buffer size overflow")?;
        let total_with_offset = self
            .offset
            .checked_add(total)
            .ok_or("Offset + buffer size overflow")?;
        let _ = total_with_offset; // just checking for overflow
        Ok(())
    }

    /// Total bytes required for the buffer (including offset).
    pub fn total_bytes(&self) -> Option<u64> {
        let body = (self.stride as u64).checked_mul(self.height as u64)?;
        (self.offset as u64).checked_add(body)
    }
}

/// XDG toplevel state flags (sent to clients in configure events).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XdgToplevelState {
    Maximized,
    Fullscreen,
    Resizing,
    Activated,
    TiledLeft,
    TiledRight,
    TiledTop,
    TiledBottom,
}

/// Seat capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatCapabilities {
    pub pointer: bool,
    pub keyboard: bool,
    pub touch: bool,
}

impl Default for SeatCapabilities {
    fn default() -> Self {
        Self {
            pointer: true,
            keyboard: true,
            touch: false,
        }
    }
}

impl SeatCapabilities {
    /// Convert to the wayland bitmask representation.
    pub fn to_bitmask(self) -> u32 {
        let mut mask = 0u32;
        if self.pointer {
            mask |= 1;
        }
        if self.keyboard {
            mask |= 2;
        }
        if self.touch {
            mask |= 4;
        }
        mask
    }

    /// Parse from a wayland bitmask.
    pub fn from_bitmask(mask: u32) -> Self {
        Self {
            pointer: mask & 1 != 0,
            keyboard: mask & 2 != 0,
            touch: mask & 4 != 0,
        }
    }
}

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl ModifierState {
    /// Decompose a raw modifier bitmask (as from InputEvent).
    pub fn from_raw(mods: u32) -> Self {
        Self {
            shift: mods & 0x01 != 0,
            ctrl: mods & 0x04 != 0,
            alt: mods & 0x08 != 0,
            logo: mods & 0x40 != 0,
            caps_lock: mods & 0x02 != 0,
            num_lock: mods & 0x10 != 0,
        }
    }

    /// Encode back to a raw bitmask.
    pub fn to_raw(self) -> u32 {
        let mut raw = 0u32;
        if self.shift {
            raw |= 0x01;
        }
        if self.caps_lock {
            raw |= 0x02;
        }
        if self.ctrl {
            raw |= 0x04;
        }
        if self.alt {
            raw |= 0x08;
        }
        if self.num_lock {
            raw |= 0x10;
        }
        if self.logo {
            raw |= 0x40;
        }
        raw
    }

    /// True if no modifiers are active.
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.logo && !self.caps_lock && !self.num_lock
    }
}

/// Tracks the mapping between Wayland protocol surface IDs and internal
/// compositor [`SurfaceId`]s.
#[derive(Debug, Clone)]
pub struct SurfaceMap {
    /// Protocol-side numeric ID -> compositor UUID.
    proto_to_internal: HashMap<u32, SurfaceId>,
    /// Compositor UUID -> protocol-side numeric ID.
    internal_to_proto: HashMap<SurfaceId, u32>,
    /// Next protocol ID to assign.
    next_proto_id: u32,
}

impl SurfaceMap {
    pub fn new() -> Self {
        Self {
            proto_to_internal: HashMap::new(),
            internal_to_proto: HashMap::new(),
            next_proto_id: 1,
        }
    }

    /// Register a new surface, returning its protocol-side ID.
    pub fn register(&mut self, surface_id: SurfaceId) -> u32 {
        if let Some(&proto) = self.internal_to_proto.get(&surface_id) {
            return proto;
        }
        let proto_id = self.next_proto_id;
        self.next_proto_id = self.next_proto_id.wrapping_add(1);
        self.proto_to_internal.insert(proto_id, surface_id);
        self.internal_to_proto.insert(surface_id, proto_id);
        proto_id
    }

    /// Remove a surface by its internal ID.
    pub fn unregister(&mut self, surface_id: &SurfaceId) -> Option<u32> {
        if let Some(proto_id) = self.internal_to_proto.remove(surface_id) {
            self.proto_to_internal.remove(&proto_id);
            Some(proto_id)
        } else {
            None
        }
    }

    /// Remove a surface by its protocol ID.
    pub fn unregister_proto(&mut self, proto_id: u32) -> Option<SurfaceId> {
        if let Some(surface_id) = self.proto_to_internal.remove(&proto_id) {
            self.internal_to_proto.remove(&surface_id);
            Some(surface_id)
        } else {
            None
        }
    }

    /// Look up internal ID from protocol ID.
    pub fn get_internal(&self, proto_id: u32) -> Option<&SurfaceId> {
        self.proto_to_internal.get(&proto_id)
    }

    /// Look up protocol ID from internal ID.
    pub fn get_proto(&self, surface_id: &SurfaceId) -> Option<u32> {
        self.internal_to_proto.get(surface_id).copied()
    }

    /// Number of tracked surfaces.
    pub fn len(&self) -> usize {
        self.proto_to_internal.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.proto_to_internal.is_empty()
    }

    /// Iterate over all (proto_id, surface_id) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &SurfaceId)> {
        self.proto_to_internal.iter()
    }
}

impl Default for SurfaceMap {
    fn default() -> Self {
        Self::new()
    }
}

/// XDG toplevel configure state to be sent to a client window.
#[derive(Debug, Clone)]
pub struct ToplevelConfigure {
    pub surface_id: SurfaceId,
    pub width: u32,
    pub height: u32,
    pub states: Vec<XdgToplevelState>,
    pub serial: u32,
}

impl ToplevelConfigure {
    /// Build a configure for a maximized window matching the given output.
    pub fn maximized(surface_id: SurfaceId, output: &OutputInfo, serial: u32) -> Self {
        Self {
            surface_id,
            width: output.width_px,
            height: output.height_px,
            states: vec![XdgToplevelState::Maximized, XdgToplevelState::Activated],
            serial,
        }
    }

    /// Build a default "initial" configure (0x0 means client picks size).
    pub fn initial(surface_id: SurfaceId, serial: u32) -> Self {
        Self {
            surface_id,
            width: 0,
            height: 0,
            states: vec![XdgToplevelState::Activated],
            serial,
        }
    }

    /// Whether the state set includes Activated.
    pub fn is_activated(&self) -> bool {
        self.states.contains(&XdgToplevelState::Activated)
    }

    /// Whether the configure asks for maximized.
    pub fn is_maximized(&self) -> bool {
        self.states.contains(&XdgToplevelState::Maximized)
    }
}

/// Tracks an XDG toplevel window lifecycle.
#[derive(Debug, Clone)]
pub struct XdgToplevelTracker {
    pub surface_id: SurfaceId,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub parent: Option<SurfaceId>,
    pub configured: bool,
    pub mapped: bool,
    pub pending_configure: Option<ToplevelConfigure>,
    pub last_serial: u32,
    pub min_size: Option<(u32, u32)>,
    pub max_size: Option<(u32, u32)>,
}

impl XdgToplevelTracker {
    pub fn new(surface_id: SurfaceId) -> Self {
        Self {
            surface_id,
            title: None,
            app_id: None,
            parent: None,
            configured: false,
            mapped: false,
            pending_configure: None,
            last_serial: 0,
            min_size: None,
            max_size: None,
        }
    }

    /// Record that the client acknowledged a configure.
    pub fn ack_configure(&mut self, serial: u32) -> bool {
        if let Some(ref pending) = self.pending_configure {
            if pending.serial == serial {
                self.configured = true;
                self.last_serial = serial;
                self.pending_configure = None;
                return true;
            }
        }
        false
    }

    /// Set the pending configure and return it for sending.
    pub fn send_configure(&mut self, configure: ToplevelConfigure) -> &ToplevelConfigure {
        self.pending_configure = Some(configure);
        self.pending_configure.as_ref().unwrap()
    }

    /// Validate a requested size against min/max constraints.
    pub fn constrain_size(&self, width: u32, height: u32) -> (u32, u32) {
        let mut w = width;
        let mut h = height;
        if let Some((min_w, min_h)) = self.min_size {
            w = w.max(min_w);
            h = h.max(min_h);
        }
        if let Some((max_w, max_h)) = self.max_size {
            if max_w > 0 {
                w = w.min(max_w);
            }
            if max_h > 0 {
                h = h.min(max_h);
            }
        }
        (w, h)
    }

    /// Mark as mapped (client committed first buffer after configure ack).
    pub fn map(&mut self) -> bool {
        if self.configured && !self.mapped {
            self.mapped = true;
            true
        } else {
            false
        }
    }

    /// Mark as unmapped.
    pub fn unmap(&mut self) {
        self.mapped = false;
    }
}

/// Pointer focus state for forwarding events to the correct client.
#[derive(Debug, Clone, Default)]
pub struct PointerFocus {
    pub surface_id: Option<SurfaceId>,
    pub surface_x: f64,
    pub surface_y: f64,
    pub serial: u32,
}

impl PointerFocus {
    /// Update the focused surface. Returns true if focus changed.
    pub fn set_focus(&mut self, surface: Option<SurfaceId>, x: f64, y: f64, serial: u32) -> bool {
        let changed = self.surface_id != surface;
        self.surface_id = surface;
        self.surface_x = x;
        self.surface_y = y;
        self.serial = serial;
        changed
    }

    /// Update position within the current surface.
    pub fn motion(&mut self, x: f64, y: f64) {
        self.surface_x = x;
        self.surface_y = y;
    }
}

/// Keyboard focus state.
#[derive(Debug, Clone, Default)]
pub struct KeyboardFocus {
    pub surface_id: Option<SurfaceId>,
    pub serial: u32,
    pub modifiers: ModifierState,
}

impl KeyboardFocus {
    /// Update keyboard focus. Returns true if focus changed.
    pub fn set_focus(&mut self, surface: Option<SurfaceId>, serial: u32) -> bool {
        let changed = self.surface_id != surface;
        self.surface_id = surface;
        self.serial = serial;
        changed
    }

    /// Update modifier state.
    pub fn set_modifiers(&mut self, modifiers: ModifierState) {
        self.modifiers = modifiers;
    }
}

/// Client tracking entry.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: u32,
    pub pid: Option<u32>,
    pub surfaces: Vec<SurfaceId>,
    pub connected_at: std::time::Instant,
}

impl ClientInfo {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            pid: None,
            surfaces: Vec::new(),
            connected_at: std::time::Instant::now(),
        }
    }

    /// Add a surface owned by this client.
    pub fn add_surface(&mut self, surface: SurfaceId) {
        if !self.surfaces.contains(&surface) {
            self.surfaces.push(surface);
        }
    }

    /// Remove a surface.
    pub fn remove_surface(&mut self, surface: &SurfaceId) {
        self.surfaces.retain(|s| s != surface);
    }
}

/// Client registry — tracks all connected Wayland clients.
#[derive(Debug, Clone)]
pub struct ClientRegistry {
    clients: HashMap<u32, ClientInfo>,
    next_id: u32,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            next_id: 1,
        }
    }

    /// Register a new client, returning its assigned ID.
    pub fn register(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.clients.insert(id, ClientInfo::new(id));
        id
    }

    /// Register a client with a known PID.
    pub fn register_with_pid(&mut self, pid: u32) -> u32 {
        let id = self.register();
        if let Some(client) = self.clients.get_mut(&id) {
            client.pid = Some(pid);
        }
        id
    }

    /// Remove a client and return its info.
    pub fn unregister(&mut self, id: u32) -> Option<ClientInfo> {
        self.clients.remove(&id)
    }

    /// Get a client by ID.
    pub fn get(&self, id: u32) -> Option<&ClientInfo> {
        self.clients.get(&id)
    }

    /// Get a mutable reference to a client.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut ClientInfo> {
        self.clients.get_mut(&id)
    }

    /// Number of connected clients.
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    /// Whether there are no clients.
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    /// Iterate over all clients.
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &ClientInfo)> {
        self.clients.iter()
    }

    /// Find the client that owns a given surface.
    pub fn find_by_surface(&self, surface: &SurfaceId) -> Option<u32> {
        self.clients
            .iter()
            .find(|(_, info)| info.surfaces.contains(surface))
            .map(|(id, _)| *id)
    }
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Serial number generator for the Wayland protocol.
#[derive(Debug, Clone)]
pub struct SerialCounter {
    current: u32,
}

impl SerialCounter {
    pub fn new() -> Self {
        Self { current: 0 }
    }

    /// Get the next serial number.
    pub fn next_serial(&mut self) -> u32 {
        self.current = self.current.wrapping_add(1);
        self.current
    }

    /// Get the current serial without advancing.
    pub fn current(&self) -> u32 {
        self.current
    }
}

impl Default for SerialCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Protocol bridge — shared logic for handling Wayland protocol events
// ============================================================================

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
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

// ============================================================================
// Feature-gated: real wayland-server integration
// ============================================================================

#[cfg(feature = "wayland")]
mod wayland_live {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use wayland_protocols::xdg::shell::server::{xdg_surface, xdg_toplevel, xdg_wm_base};
    use wayland_server::{
        backend::ClientId,
        protocol::{wl_buffer, wl_compositor, wl_output, wl_seat, wl_shm, wl_shm_pool, wl_surface},
        Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, ListeningSocket, New,
        Resource,
    };

    /// Per-surface user data stored on `wl_surface` resources.
    #[derive(Debug)]
    pub struct SurfaceData {
        pub surface_id: SurfaceId,
        pub client_id: u32,
    }

    /// Per-toplevel user data stored on `xdg_toplevel` resources.
    #[derive(Debug)]
    pub struct ToplevelData {
        pub surface_id: SurfaceId,
        pub client_id: u32,
    }

    /// Per-xdg_surface user data.
    #[derive(Debug)]
    pub struct XdgSurfaceData {
        pub surface_id: SurfaceId,
        pub client_id: u32,
    }

    /// Internal dispatch state — separated from Display so
    /// `Display::dispatch_clients(&mut inner)` can borrow correctly.
    pub struct WaylandInner {
        pub compositor: Arc<Compositor>,
        pub bridge: ProtocolBridge,
        pub dh: DisplayHandle,
        client_ids: StdHashMap<ClientId, u32>,
        next_client_id: u32,
    }

    impl WaylandInner {
        fn get_client_id(&mut self, client: &Client) -> u32 {
            let cid = client.id();
            if let Some(&id) = self.client_ids.get(&cid) {
                return id;
            }
            let id = self.next_client_id;
            self.next_client_id = self.next_client_id.wrapping_add(1);
            self.client_ids.insert(cid, id);
            id
        }
    }

    /// Main Wayland server state.
    ///
    /// Holds the `wayland_server::Display` separately from the dispatch state
    /// (`WaylandInner`) so that `dispatch_clients` can borrow the inner state
    /// mutably without conflicting with the display borrow.
    pub struct WaylandState {
        display: Display<WaylandInner>,
        inner: WaylandInner,
        socket_name: Option<String>,
    }

    impl WaylandState {
        /// Create a new Wayland server state bound to the given compositor.
        pub fn new(compositor: Arc<Compositor>) -> Result<Self, Box<dyn std::error::Error>> {
            let display: Display<WaylandInner> = Display::new()?;
            let dh = display.handle();

            Ok(Self {
                display,
                inner: WaylandInner {
                    compositor,
                    bridge: ProtocolBridge::new(),
                    dh,
                    client_ids: StdHashMap::new(),
                    next_client_id: 1,
                },
                socket_name: None,
            })
        }

        /// Start listening on a Wayland socket.
        pub fn listen(&mut self) -> Result<String, Box<dyn std::error::Error>> {
            let socket = ListeningSocket::bind_auto("wayland", 0..33)?;
            let name = socket
                .socket_name()
                .and_then(|n| n.to_str().map(String::from))
                .unwrap_or_else(|| "wayland-0".to_string());
            self.socket_name = Some(name.clone());
            Ok(name)
        }

        /// Dispatch one frame: accept clients, process events, apply to compositor.
        pub fn dispatch(&mut self) -> Result<Vec<ProtocolAction>, Box<dyn std::error::Error>> {
            self.display.dispatch_clients(&mut self.inner)?;
            let actions = self.inner.bridge.apply_actions(&self.inner.compositor);
            Ok(actions)
        }

        /// Handle an input event by routing through the bridge.
        pub fn handle_input(&mut self, event: &InputEvent) {
            self.inner.bridge.route_input(&self.inner.compositor, event);
        }

        /// Initialize all global protocol objects on the display.
        pub fn init_globals(&mut self) {
            let dh = &self.inner.dh;
            dh.create_global::<WaylandInner, wl_compositor::WlCompositor, ()>(5, ());
            dh.create_global::<WaylandInner, wl_shm::WlShm, ()>(1, ());
            dh.create_global::<WaylandInner, wl_seat::WlSeat, ()>(7, ());
            dh.create_global::<WaylandInner, wl_output::WlOutput, ()>(4, ());
            dh.create_global::<WaylandInner, xdg_wm_base::XdgWmBase, ()>(3, ());
        }
    }

    // ========================================================================
    // GlobalDispatch impls — called when a client binds to a global
    // ========================================================================

    impl GlobalDispatch<wl_compositor::WlCompositor, ()> for WaylandInner {
        fn bind(
            _state: &mut Self,
            _client: &Client,
            resource: New<wl_compositor::WlCompositor>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    impl GlobalDispatch<wl_shm::WlShm, ()> for WaylandInner {
        fn bind(
            _state: &mut Self,
            _client: &Client,
            resource: New<wl_shm::WlShm>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            let shm = data_init.init(resource, ());
            shm.format(wl_shm::Format::Argb8888);
            shm.format(wl_shm::Format::Xrgb8888);
        }
    }

    impl GlobalDispatch<wl_seat::WlSeat, ()> for WaylandInner {
        fn bind(
            state: &mut Self,
            _client: &Client,
            resource: New<wl_seat::WlSeat>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            let seat = data_init.init(resource, ());
            let caps = state.bridge.seat_caps;
            seat.capabilities(wl_seat::Capability::from_bits_truncate(caps.to_bitmask()));
        }
    }

    impl GlobalDispatch<wl_output::WlOutput, ()> for WaylandInner {
        fn bind(
            state: &mut Self,
            _client: &Client,
            resource: New<wl_output::WlOutput>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            let output = data_init.init(resource, ());
            let info = &state.bridge.output;

            output.geometry(
                info.x,
                info.y,
                info.width_mm as i32,
                info.height_mm as i32,
                wl_output::Subpixel::Unknown,
                info.make.clone(),
                info.model.clone(),
                wl_output::Transform::Normal,
            );
            output.mode(
                wl_output::Mode::Current | wl_output::Mode::Preferred,
                info.width_px as i32,
                info.height_px as i32,
                info.refresh_mhz as i32,
            );
            output.scale(info.scale as i32);
            output.done();
        }
    }

    impl GlobalDispatch<xdg_wm_base::XdgWmBase, ()> for WaylandInner {
        fn bind(
            _state: &mut Self,
            _client: &Client,
            resource: New<xdg_wm_base::XdgWmBase>,
            _global_data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    // ========================================================================
    // Dispatch: wl_compositor — creates wl_surface instances
    // ========================================================================

    impl Dispatch<wl_compositor::WlCompositor, ()> for WaylandInner {
        fn request(
            state: &mut Self,
            client: &Client,
            _resource: &wl_compositor::WlCompositor,
            request: wl_compositor::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_compositor::Request::CreateSurface { id } => {
                    let client_id = state.get_client_id(client);
                    if let Some((surface_id, _proto_id)) = state.bridge.create_surface(client_id) {
                        data_init.init(
                            id,
                            SurfaceData {
                                surface_id,
                                client_id,
                            },
                        );
                    }
                }
                wl_compositor::Request::CreateRegion { id: _ } => {
                    // Region management — no-op for basic compositor
                }
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: wl_surface — surface commit, damage, attach
    // ========================================================================

    impl Dispatch<wl_surface::WlSurface, SurfaceData> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            _resource: &wl_surface::WlSurface,
            request: wl_surface::Request,
            data: &SurfaceData,
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_surface::Request::Commit => {
                    state.bridge.surface_commit(data.surface_id);
                }
                wl_surface::Request::Destroy => {
                    state.bridge.destroy_surface(data.surface_id);
                }
                wl_surface::Request::Attach {
                    buffer: _,
                    x: _,
                    y: _,
                } => {}
                wl_surface::Request::Damage {
                    x: _,
                    y: _,
                    width: _,
                    height: _,
                } => {}
                wl_surface::Request::Frame { callback: _ } => {}
                wl_surface::Request::SetInputRegion { region: _ } => {}
                wl_surface::Request::SetOpaqueRegion { region: _ } => {}
                wl_surface::Request::SetBufferTransform { transform: _ } => {}
                wl_surface::Request::SetBufferScale { scale: _ } => {}
                wl_surface::Request::DamageBuffer {
                    x: _,
                    y: _,
                    width: _,
                    height: _,
                } => {}
                wl_surface::Request::Offset { x: _, y: _ } => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: wl_shm — shared memory buffer management
    // ========================================================================

    impl Dispatch<wl_shm::WlShm, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_shm::WlShm,
            request: wl_shm::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_shm::Request::CreatePool { id, fd: _, size: _ } => {
                    data_init.init(id, ());
                }
                _ => {}
            }
        }
    }

    impl Dispatch<wl_shm_pool::WlShmPool, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_shm_pool::WlShmPool,
            request: wl_shm_pool::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_shm_pool::Request::CreateBuffer {
                    id,
                    offset: _,
                    width: _,
                    height: _,
                    stride: _,
                    format: _,
                } => {
                    data_init.init(id, ());
                }
                wl_shm_pool::Request::Resize { size: _ } => {}
                wl_shm_pool::Request::Destroy => {}
                _ => {}
            }
        }
    }

    impl Dispatch<wl_buffer::WlBuffer, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_buffer::WlBuffer,
            request: wl_buffer::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_buffer::Request::Destroy => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: wl_seat — input device capabilities
    // ========================================================================

    impl Dispatch<wl_seat::WlSeat, ()> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            resource: &wl_seat::WlSeat,
            request: wl_seat::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_seat::Request::GetPointer { id: _ } => {}
                wl_seat::Request::GetKeyboard { id: _ } => {}
                wl_seat::Request::GetTouch { id: _ } => {}
                wl_seat::Request::Release => {}
                _ => {}
            }
            let caps = state.bridge.seat_caps;
            resource.capabilities(wl_seat::Capability::from_bits_truncate(caps.to_bitmask()));
        }
    }

    // ========================================================================
    // Dispatch: wl_output — screen geometry advertising
    // ========================================================================

    impl Dispatch<wl_output::WlOutput, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &wl_output::WlOutput,
            request: wl_output::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_output::Request::Release => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: xdg_wm_base — XDG Shell entry point
    // ========================================================================

    impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WaylandInner {
        fn request(
            _state: &mut Self,
            _client: &Client,
            _resource: &xdg_wm_base::XdgWmBase,
            request: xdg_wm_base::Request,
            _data: &(),
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                xdg_wm_base::Request::GetXdgSurface { id, surface } => {
                    let Some(sdata): Option<&SurfaceData> = surface.data() else {
                        tracing::error!("GetXdgSurface: surface missing data, rejecting");
                        return;
                    };
                    data_init.init(
                        id,
                        XdgSurfaceData {
                            surface_id: sdata.surface_id,
                            client_id: sdata.client_id,
                        },
                    );
                }
                xdg_wm_base::Request::Pong { serial: _ } => {}
                xdg_wm_base::Request::CreatePositioner { id: _ } => {}
                xdg_wm_base::Request::Destroy => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: xdg_surface — surface role assignment
    // ========================================================================

    impl Dispatch<xdg_surface::XdgSurface, XdgSurfaceData> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            _resource: &xdg_surface::XdgSurface,
            request: xdg_surface::Request,
            data: &XdgSurfaceData,
            _dhandle: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                xdg_surface::Request::GetToplevel { id } => {
                    state
                        .bridge
                        .create_toplevel(data.surface_id, data.client_id);
                    data_init.init(
                        id,
                        ToplevelData {
                            surface_id: data.surface_id,
                            client_id: data.client_id,
                        },
                    );
                }
                xdg_surface::Request::AckConfigure { serial } => {
                    state.bridge.ack_configure(data.surface_id, serial);
                }
                xdg_surface::Request::SetWindowGeometry {
                    x: _,
                    y: _,
                    width: _,
                    height: _,
                } => {}
                xdg_surface::Request::GetPopup { .. } => {}
                xdg_surface::Request::Destroy => {}
                _ => {}
            }
        }
    }

    // ========================================================================
    // Dispatch: xdg_toplevel — window management requests
    // ========================================================================

    impl Dispatch<xdg_toplevel::XdgToplevel, ToplevelData> for WaylandInner {
        fn request(
            state: &mut Self,
            _client: &Client,
            _resource: &xdg_toplevel::XdgToplevel,
            request: xdg_toplevel::Request,
            data: &ToplevelData,
            _dhandle: &DisplayHandle,
            _data_init: &mut DataInit<'_, Self>,
        ) {
            match request {
                xdg_toplevel::Request::SetTitle { title } => {
                    state.bridge.set_title(data.surface_id, title);
                }
                xdg_toplevel::Request::SetAppId { app_id } => {
                    state.bridge.set_app_id(data.surface_id, app_id);
                }
                xdg_toplevel::Request::SetMaxSize { width, height } => {
                    let max = if width > 0 && height > 0 {
                        Some((width as u32, height as u32))
                    } else {
                        None
                    };
                    state.bridge.set_size_bounds(data.surface_id, None, max);
                }
                xdg_toplevel::Request::SetMinSize { width, height } => {
                    let min = if width > 0 && height > 0 {
                        Some((width as u32, height as u32))
                    } else {
                        None
                    };
                    state.bridge.set_size_bounds(data.surface_id, min, None);
                }
                xdg_toplevel::Request::SetMaximized => {
                    state.bridge.set_maximized(data.surface_id, true);
                }
                xdg_toplevel::Request::UnsetMaximized => {
                    state.bridge.set_maximized(data.surface_id, false);
                }
                xdg_toplevel::Request::SetFullscreen { output: _ } => {
                    state.bridge.set_fullscreen(data.surface_id, true);
                }
                xdg_toplevel::Request::UnsetFullscreen => {
                    state.bridge.set_fullscreen(data.surface_id, false);
                }
                xdg_toplevel::Request::SetMinimized => {
                    state.bridge.set_minimized(data.surface_id);
                }
                xdg_toplevel::Request::Move { seat: _, serial: _ } => {
                    state.bridge.request_move(data.surface_id);
                }
                xdg_toplevel::Request::Resize {
                    seat: _,
                    serial: _,
                    edges,
                } => {
                    state.bridge.request_resize(data.surface_id, edges.into());
                }
                xdg_toplevel::Request::SetParent { parent: _ } => {}
                xdg_toplevel::Request::ShowWindowMenu {
                    seat: _,
                    serial: _,
                    x: _,
                    y: _,
                } => {}
                xdg_toplevel::Request::Destroy => {
                    state.bridge.destroy_surface(data.surface_id);
                }
                _ => {}
            }
        }
    }
}

#[cfg(feature = "wayland")]
pub use wayland_live::WaylandState;

// ============================================================================
// Stub when wayland feature is NOT enabled
// ============================================================================

#[cfg(not(feature = "wayland"))]
mod wayland_stub {
    use super::*;

    /// Stub Wayland server state (no real protocol, for compilation only).
    /// Uses the same [`ProtocolBridge`] as the live mode for consistent behavior.
    pub struct WaylandState {
        pub compositor: Arc<Compositor>,
        pub bridge: ProtocolBridge,
        pub socket_name: Option<String>,
    }

    impl WaylandState {
        /// Create stub state. No real Wayland socket is opened.
        pub fn new(compositor: Arc<Compositor>) -> Result<Self, Box<dyn std::error::Error>> {
            Ok(Self {
                compositor,
                bridge: ProtocolBridge::new(),
                socket_name: None,
            })
        }

        /// Stub: returns a fake socket name.
        pub fn listen(&mut self) -> Result<String, Box<dyn std::error::Error>> {
            let name = "wayland-stub-0".to_string();
            self.socket_name = Some(name.clone());
            Ok(name)
        }

        /// Dispatch a frame: apply pending bridge actions to the compositor.
        /// In stub mode this processes any programmatically enqueued actions.
        pub fn dispatch(&mut self) -> Result<Vec<ProtocolAction>, Box<dyn std::error::Error>> {
            let actions = self.bridge.apply_actions(&self.compositor);
            Ok(actions)
        }

        /// Handle an input event by routing through the bridge.
        pub fn handle_input(&mut self, event: &InputEvent) {
            self.bridge.route_input(&self.compositor, event);
        }
    }
}

#[cfg(not(feature = "wayland"))]
pub use wayland_stub::WaylandState;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // -- ShmFormat tests --

    #[test]
    fn test_shm_format_bpp() {
        assert_eq!(ShmFormat::Argb8888.bpp(), 4);
        assert_eq!(ShmFormat::Xrgb8888.bpp(), 4);
    }

    #[test]
    fn test_shm_supported_formats() {
        let fmts = ShmFormat::supported_formats();
        assert!(fmts.contains(&ShmFormat::Argb8888));
        assert!(fmts.contains(&ShmFormat::Xrgb8888));
    }

    // -- ShmBufferInfo validation tests --

    #[test]
    fn test_shm_buffer_validate_ok() {
        let info = ShmBufferInfo {
            width: 100,
            height: 100,
            stride: 400,
            format: ShmFormat::Argb8888,
            offset: 0,
        };
        assert!(info.validate().is_ok());
    }

    #[test]
    fn test_shm_buffer_validate_zero_width() {
        let info = ShmBufferInfo {
            width: 0,
            height: 100,
            stride: 400,
            format: ShmFormat::Argb8888,
            offset: 0,
        };
        assert!(info.validate().is_err());
    }

    #[test]
    fn test_shm_buffer_validate_stride_too_small() {
        let info = ShmBufferInfo {
            width: 100,
            height: 100,
            stride: 100, // needs 400
            format: ShmFormat::Argb8888,
            offset: 0,
        };
        assert!(info.validate().is_err());
    }

    #[test]
    fn test_shm_buffer_total_bytes() {
        let info = ShmBufferInfo {
            width: 100,
            height: 50,
            stride: 400,
            format: ShmFormat::Argb8888,
            offset: 64,
        };
        assert_eq!(info.total_bytes(), Some(64 + 400 * 50));
    }

    // -- OutputInfo tests --

    #[test]
    fn test_output_default() {
        let out = OutputInfo::default();
        assert_eq!(out.width_px, 1920);
        assert_eq!(out.height_px, 1080);
        assert_eq!(out.refresh_mhz, 60_000);
        assert_eq!(out.scale, 1);
    }

    #[test]
    fn test_output_logical_size() {
        let mut out = OutputInfo::default();
        out.scale = 2;
        assert_eq!(out.logical_size(), (960, 540));
    }

    #[test]
    fn test_output_logical_size_zero_scale() {
        let mut out = OutputInfo::default();
        out.scale = 0;
        assert_eq!(out.logical_size(), (1920, 1080));
    }

    #[test]
    fn test_output_refresh_hz() {
        let out = OutputInfo::default();
        assert!((out.refresh_hz() - 60.0).abs() < 0.01);
    }

    #[test]
    fn test_output_dpi() {
        let out = OutputInfo::default();
        // 1920px / (530mm / 25.4) ~= 92 DPI
        assert!(out.dpi_x() > 80.0 && out.dpi_x() < 110.0);
    }

    #[test]
    fn test_output_dpi_zero_mm() {
        let mut out = OutputInfo::default();
        out.width_mm = 0;
        assert_eq!(out.dpi_x(), 96.0);
    }

    #[test]
    fn test_output_from_rectangle() {
        let rect = Rectangle {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let out = OutputInfo::from_rectangle(&rect);
        assert_eq!(out.width_px, 2560);
        assert_eq!(out.height_px, 1440);
    }

    // -- SurfaceMap tests --

    #[test]
    fn test_surface_map_register_and_lookup() {
        let mut map = SurfaceMap::new();
        let sid = Uuid::new_v4();
        let proto = map.register(sid);
        assert_eq!(map.get_internal(proto), Some(&sid));
        assert_eq!(map.get_proto(&sid), Some(proto));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_surface_map_register_idempotent() {
        let mut map = SurfaceMap::new();
        let sid = Uuid::new_v4();
        let p1 = map.register(sid);
        let p2 = map.register(sid);
        assert_eq!(p1, p2);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_surface_map_unregister() {
        let mut map = SurfaceMap::new();
        let sid = Uuid::new_v4();
        let proto = map.register(sid);
        let removed = map.unregister(&sid);
        assert_eq!(removed, Some(proto));
        assert!(map.is_empty());
        assert_eq!(map.get_internal(proto), None);
    }

    #[test]
    fn test_surface_map_unregister_proto() {
        let mut map = SurfaceMap::new();
        let sid = Uuid::new_v4();
        let proto = map.register(sid);
        let removed = map.unregister_proto(proto);
        assert_eq!(removed, Some(sid));
        assert!(map.is_empty());
    }

    // -- SeatCapabilities tests --

    #[test]
    fn test_seat_capabilities_bitmask_roundtrip() {
        let caps = SeatCapabilities {
            pointer: true,
            keyboard: true,
            touch: true,
        };
        let mask = caps.to_bitmask();
        assert_eq!(mask, 7);
        let caps2 = SeatCapabilities::from_bitmask(mask);
        assert_eq!(caps, caps2);
    }

    #[test]
    fn test_seat_capabilities_default() {
        let caps = SeatCapabilities::default();
        assert!(caps.pointer);
        assert!(caps.keyboard);
        assert!(!caps.touch);
        assert_eq!(caps.to_bitmask(), 3);
    }

    // -- ModifierState tests --

    #[test]
    fn test_modifier_state_roundtrip() {
        let mods = ModifierState {
            shift: true,
            ctrl: true,
            alt: false,
            logo: false,
            caps_lock: false,
            num_lock: false,
        };
        let raw = mods.to_raw();
        let mods2 = ModifierState::from_raw(raw);
        assert_eq!(mods, mods2);
    }

    #[test]
    fn test_modifier_state_empty() {
        let mods = ModifierState::default();
        assert!(mods.is_empty());
        assert_eq!(mods.to_raw(), 0);
    }

    #[test]
    fn test_modifier_state_all() {
        let mods = ModifierState {
            shift: true,
            ctrl: true,
            alt: true,
            logo: true,
            caps_lock: true,
            num_lock: true,
        };
        assert!(!mods.is_empty());
        let raw = mods.to_raw();
        let mods2 = ModifierState::from_raw(raw);
        assert_eq!(mods, mods2);
    }

    // -- SerialCounter tests --

    #[test]
    fn test_serial_counter() {
        let mut counter = SerialCounter::new();
        assert_eq!(counter.current(), 0);
        assert_eq!(counter.next_serial(), 1);
        assert_eq!(counter.next_serial(), 2);
        assert_eq!(counter.current(), 2);
    }

    // -- ClientRegistry tests --

    #[test]
    fn test_client_registry_register_unregister() {
        let mut reg = ClientRegistry::new();
        assert!(reg.is_empty());
        let id = reg.register();
        assert_eq!(reg.len(), 1);
        assert!(reg.get(id).is_some());
        let info = reg.unregister(id);
        assert!(info.is_some());
        assert!(reg.is_empty());
    }

    #[test]
    fn test_client_registry_with_pid() {
        let mut reg = ClientRegistry::new();
        let id = reg.register_with_pid(42);
        assert_eq!(reg.get(id).unwrap().pid, Some(42));
    }

    #[test]
    fn test_client_registry_find_by_surface() {
        let mut reg = ClientRegistry::new();
        let cid = reg.register();
        let sid = Uuid::new_v4();
        reg.get_mut(cid).unwrap().add_surface(sid);
        assert_eq!(reg.find_by_surface(&sid), Some(cid));
        assert_eq!(reg.find_by_surface(&Uuid::new_v4()), None);
    }

    // -- XdgToplevelTracker tests --

    #[test]
    fn test_toplevel_tracker_lifecycle() {
        let sid = Uuid::new_v4();
        let mut tracker = XdgToplevelTracker::new(sid);
        assert!(!tracker.configured);
        assert!(!tracker.mapped);

        // Cannot map before configure
        assert!(!tracker.map());

        // Send configure
        let cfg = ToplevelConfigure::initial(sid, 1);
        tracker.send_configure(cfg);
        assert!(!tracker.configured);

        // Ack configure
        assert!(tracker.ack_configure(1));
        assert!(tracker.configured);

        // Now can map
        assert!(tracker.map());
        assert!(tracker.mapped);

        // Map again -> false (already mapped)
        assert!(!tracker.map());

        // Unmap
        tracker.unmap();
        assert!(!tracker.mapped);
    }

    #[test]
    fn test_toplevel_constrain_size() {
        let sid = Uuid::new_v4();
        let mut tracker = XdgToplevelTracker::new(sid);
        tracker.min_size = Some((100, 100));
        tracker.max_size = Some((800, 600));

        assert_eq!(tracker.constrain_size(50, 50), (100, 100));
        assert_eq!(tracker.constrain_size(400, 300), (400, 300));
        assert_eq!(tracker.constrain_size(1000, 1000), (800, 600));
    }

    #[test]
    fn test_toplevel_ack_wrong_serial() {
        let sid = Uuid::new_v4();
        let mut tracker = XdgToplevelTracker::new(sid);
        let cfg = ToplevelConfigure::initial(sid, 5);
        tracker.send_configure(cfg);
        assert!(!tracker.ack_configure(99));
        assert!(!tracker.configured);
    }

    // -- ToplevelConfigure tests --

    #[test]
    fn test_toplevel_configure_maximized() {
        let sid = Uuid::new_v4();
        let out = OutputInfo::default();
        let cfg = ToplevelConfigure::maximized(sid, &out, 1);
        assert!(cfg.is_maximized());
        assert!(cfg.is_activated());
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 1080);
    }

    #[test]
    fn test_toplevel_configure_initial() {
        let sid = Uuid::new_v4();
        let cfg = ToplevelConfigure::initial(sid, 1);
        assert_eq!(cfg.width, 0);
        assert_eq!(cfg.height, 0);
        assert!(cfg.is_activated());
        assert!(!cfg.is_maximized());
    }

    // -- Input mapping tests --

    #[test]
    fn test_map_input_mouse_move() {
        let event = InputEvent::MouseMove { x: 100, y: 200 };
        let result = map_input_to_pointer_event(&event);
        assert!(result.is_some());
        match result.unwrap() {
            WaylandPointerEvent::Motion { x, y } => {
                assert_eq!(x, 100.0);
                assert_eq!(y, 200.0);
            }
            _ => panic!("Expected Motion"),
        }
    }

    #[test]
    fn test_map_input_mouse_click() {
        let event = InputEvent::MouseClick {
            button: 1,
            x: 50,
            y: 75,
        };
        let result = map_input_to_pointer_event(&event);
        assert!(result.is_some());
        match result.unwrap() {
            WaylandPointerEvent::Button {
                button,
                x,
                y,
                pressed,
            } => {
                assert_eq!(button, 1);
                assert_eq!(x, 50.0);
                assert_eq!(y, 75.0);
                assert!(pressed);
            }
            _ => panic!("Expected Button"),
        }
    }

    #[test]
    fn test_map_input_key_to_keyboard() {
        let event = InputEvent::KeyPress {
            keycode: 30,
            modifiers: 0x05, // shift + ctrl
        };
        let result = map_input_to_keyboard_event(&event);
        assert!(result.is_some());
        match result.unwrap() {
            WaylandKeyboardEvent::Key {
                keycode,
                modifiers,
                pressed,
            } => {
                assert_eq!(keycode, 30);
                assert!(modifiers.shift);
                assert!(modifiers.ctrl);
                assert!(pressed);
            }
            _ => panic!("Expected Key"),
        }
    }

    #[test]
    fn test_map_input_irrelevant_events() {
        let event = InputEvent::KeyPress {
            keycode: 1,
            modifiers: 0,
        };
        assert!(map_input_to_pointer_event(&event).is_none());

        let event = InputEvent::MouseMove { x: 0, y: 0 };
        assert!(map_input_to_keyboard_event(&event).is_none());
    }

    // -- PointerFocus tests --

    #[test]
    fn test_pointer_focus_set_and_motion() {
        let mut focus = PointerFocus::default();
        let sid = Uuid::new_v4();
        assert!(focus.set_focus(Some(sid), 10.0, 20.0, 1));
        assert_eq!(focus.surface_id, Some(sid));

        // Same surface -> no change
        assert!(!focus.set_focus(Some(sid), 15.0, 25.0, 2));

        focus.motion(30.0, 40.0);
        assert_eq!(focus.surface_x, 30.0);
        assert_eq!(focus.surface_y, 40.0);
    }

    // -- KeyboardFocus tests --

    #[test]
    fn test_keyboard_focus() {
        let mut focus = KeyboardFocus::default();
        let sid = Uuid::new_v4();
        assert!(focus.set_focus(Some(sid), 1));
        assert!(!focus.set_focus(Some(sid), 2)); // same surface
        assert!(focus.set_focus(None, 3)); // changed

        focus.set_modifiers(ModifierState {
            shift: true,
            ..Default::default()
        });
        assert!(focus.modifiers.shift);
    }

    // -- WaylandState stub tests --

    #[test]
    fn test_wayland_state_stub_new() {
        let comp = Arc::new(Compositor::new());
        let state = WaylandState::new(comp);
        assert!(state.is_ok());
        let state = state.unwrap();
        assert!(state.bridge.surface_map.is_empty());
        assert!(state.bridge.clients.is_empty());
        assert_eq!(state.bridge.serial.current(), 0);
    }

    #[cfg(not(feature = "wayland"))]
    #[test]
    fn test_wayland_state_stub_listen() {
        let comp = Arc::new(Compositor::new());
        let mut state = WaylandState::new(comp).unwrap();
        let name = state.listen().unwrap();
        assert!(name.contains("stub"));
        assert_eq!(state.socket_name, Some(name));
    }

    #[cfg(not(feature = "wayland"))]
    #[test]
    fn test_wayland_state_stub_dispatch() {
        let comp = Arc::new(Compositor::new());
        let mut state = WaylandState::new(comp).unwrap();
        let actions = state.dispatch().unwrap();
        assert!(actions.is_empty());
    }

    // -- ProtocolBridge tests --

    #[test]
    fn test_bridge_client_lifecycle() {
        let mut bridge = ProtocolBridge::new();
        assert_eq!(bridge.client_count(), 0);

        let id = bridge.client_connect(Some(1234));
        assert_eq!(bridge.client_count(), 1);
        assert_eq!(bridge.clients.get(id).unwrap().pid, Some(1234));

        bridge.client_disconnect(id);
        assert_eq!(bridge.client_count(), 0);
    }

    #[test]
    fn test_bridge_surface_creation() {
        let mut bridge = ProtocolBridge::new();
        let client_id = bridge.client_connect(None);

        let (surface_id, proto_id) = bridge.create_surface(client_id).unwrap();
        assert_eq!(bridge.surface_count(), 1);
        assert_eq!(bridge.surface_map.get_internal(proto_id), Some(&surface_id));

        // Client should track the surface
        assert_eq!(bridge.clients.get(client_id).unwrap().surfaces.len(), 1);
    }

    #[test]
    fn test_bridge_toplevel_lifecycle() {
        let mut bridge = ProtocolBridge::new();
        let client_id = bridge.client_connect(None);
        let (surface_id, _) = bridge.create_surface(client_id).unwrap();

        // Create toplevel — sends initial configure
        let configure = bridge.create_toplevel(surface_id, client_id);
        assert!(configure.is_activated());
        assert_eq!(configure.width, 0); // initial = client picks size
        let serial = configure.serial;

        // Ack configure
        assert!(bridge.ack_configure(surface_id, serial));
        assert!(bridge.toplevels.get(&surface_id).unwrap().configured);

        // First commit maps the window
        assert!(bridge.surface_commit(surface_id));
        assert!(bridge.toplevels.get(&surface_id).unwrap().mapped);
        assert_eq!(bridge.mapped_toplevel_count(), 1);
    }

    #[test]
    fn test_bridge_set_title_and_app_id() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        bridge.set_title(sid, "My Window".to_string());
        bridge.set_app_id(sid, "com.example.app".to_string());

        let tracker = bridge.toplevels.get(&sid).unwrap();
        assert_eq!(tracker.title.as_deref(), Some("My Window"));
        assert_eq!(tracker.app_id.as_deref(), Some("com.example.app"));
    }

    #[test]
    fn test_bridge_maximize() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        bridge.set_maximized(sid, true);
        let tracker = bridge.toplevels.get(&sid).unwrap();
        let pending = tracker.pending_configure.as_ref().unwrap();
        assert!(pending.is_maximized());
        assert_eq!(pending.width, 1920);
        assert_eq!(pending.height, 1080);
    }

    #[test]
    fn test_bridge_fullscreen() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        bridge.set_fullscreen(sid, true);
        let tracker = bridge.toplevels.get(&sid).unwrap();
        let pending = tracker.pending_configure.as_ref().unwrap();
        assert!(pending.states.contains(&XdgToplevelState::Fullscreen));
    }

    #[test]
    fn test_bridge_size_bounds() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        bridge.set_size_bounds(sid, Some((200, 150)), Some((800, 600)));
        let tracker = bridge.toplevels.get(&sid).unwrap();
        assert_eq!(tracker.min_size, Some((200, 150)));
        assert_eq!(tracker.max_size, Some((800, 600)));
        assert_eq!(tracker.constrain_size(100, 100), (200, 150));
        assert_eq!(tracker.constrain_size(1000, 1000), (800, 600));
    }

    #[test]
    fn test_bridge_destroy_surface() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        bridge.destroy_surface(sid);
        assert_eq!(bridge.surface_count(), 0);
        assert!(bridge.toplevels.get(&sid).is_none());
    }

    #[test]
    fn test_bridge_client_disconnect_cleans_surfaces() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid1, _) = bridge.create_surface(cid).unwrap();
        let (sid2, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid1, cid);
        bridge.create_toplevel(sid2, cid);

        let removed = bridge.client_disconnect(cid);
        assert_eq!(removed.len(), 2);
        assert_eq!(bridge.surface_count(), 0);
        assert_eq!(bridge.mapped_toplevel_count(), 0);
    }

    #[test]
    fn test_bridge_ack_wrong_serial() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        assert!(!bridge.ack_configure(sid, 9999));
        assert!(!bridge.toplevels.get(&sid).unwrap().configured);
    }

    #[test]
    fn test_bridge_commit_before_ack_does_not_map() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        // Commit without acking configure should not map
        assert!(!bridge.surface_commit(sid));
        assert!(!bridge.toplevels.get(&sid).unwrap().mapped);
    }

    #[test]
    fn test_bridge_drain_actions() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        let actions = bridge.drain_actions();
        assert!(!actions.is_empty());

        // Second drain should be empty
        let actions2 = bridge.drain_actions();
        assert!(actions2.is_empty());
    }

    #[test]
    fn test_bridge_disconnect_clears_focus() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();
        bridge.create_toplevel(sid, cid);

        // Set focus to this surface
        let serial = bridge.serial.next_serial();
        bridge
            .pointer_focus
            .set_focus(Some(sid), 50.0, 50.0, serial);
        bridge.keyboard_focus.set_focus(Some(sid), serial);

        bridge.client_disconnect(cid);
        assert_eq!(bridge.pointer_focus.surface_id, None);
        assert_eq!(bridge.keyboard_focus.surface_id, None);
    }

    #[test]
    fn test_bridge_input_routing() {
        let comp = Compositor::new();
        let mut bridge = ProtocolBridge::new();

        // Route a mouse move — should produce a ForwardPointer action
        let event = InputEvent::MouseMove { x: 100, y: 200 };
        bridge.route_input(&comp, &event);

        let actions = bridge.drain_actions();
        let has_pointer = actions
            .iter()
            .any(|a| matches!(a, ProtocolAction::ForwardPointer { .. }));
        assert!(has_pointer);
    }

    #[test]
    fn test_bridge_multiple_clients() {
        let mut bridge = ProtocolBridge::new();
        let c1 = bridge.client_connect(Some(100));
        let c2 = bridge.client_connect(Some(200));

        let (_s1, _) = bridge.create_surface(c1).unwrap();
        let (s2, _) = bridge.create_surface(c2).unwrap();

        assert_eq!(bridge.client_count(), 2);
        assert_eq!(bridge.surface_count(), 2);

        bridge.client_disconnect(c1);
        assert_eq!(bridge.client_count(), 1);
        assert_eq!(bridge.surface_count(), 1);
        assert!(bridge.surface_map.get_proto(&s2).is_some());
    }

    #[test]
    fn test_bridge_request_move_and_resize() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();

        bridge.request_move(sid);
        bridge.request_resize(sid, 4); // right edge

        let actions = bridge.drain_actions();
        assert!(actions
            .iter()
            .any(|a| matches!(a, ProtocolAction::RequestMove { .. })));
        assert!(actions
            .iter()
            .any(|a| matches!(a, ProtocolAction::RequestResize { edge: 4, .. })));
    }

    #[test]
    fn test_bridge_set_minimized() {
        let mut bridge = ProtocolBridge::new();
        let cid = bridge.client_connect(None);
        let (sid, _) = bridge.create_surface(cid).unwrap();

        bridge.set_minimized(sid);
        let actions = bridge.drain_actions();
        assert!(actions
            .iter()
            .any(|a| matches!(a, ProtocolAction::SetMinimized { .. })));
    }

    // -- ClientInfo tests --

    #[test]
    fn test_client_info_surfaces() {
        let mut client = ClientInfo::new(1);
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        client.add_surface(s1);
        client.add_surface(s1); // duplicate
        assert_eq!(client.surfaces.len(), 1);

        client.add_surface(s2);
        assert_eq!(client.surfaces.len(), 2);

        client.remove_surface(&s1);
        assert_eq!(client.surfaces.len(), 1);
        assert_eq!(client.surfaces[0], s2);
    }
}

// ============================================================================
// XDG Popup / Positioner support
// ============================================================================

use uuid::Uuid;

/// Edge anchor for popup positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub enum Edge {
    #[default]
    None,
    Top,
    Bottom,
    Left,
    Right,
}


/// Bitflags-style constraint adjustment for popup repositioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConstraintAdjustment {
    pub slide_x: bool,
    pub slide_y: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl ConstraintAdjustment {
    /// All adjustments disabled.
    pub fn new() -> Self {
        Self {
            slide_x: false,
            slide_y: false,
            flip_x: false,
            flip_y: false,
            resize_x: false,
            resize_y: false,
        }
    }

    /// Slide adjustments on both axes.
    pub fn slide() -> Self {
        Self {
            slide_x: true,
            slide_y: true,
            ..Self::new()
        }
    }

    /// Flip adjustments on both axes.
    pub fn flip() -> Self {
        Self {
            flip_x: true,
            flip_y: true,
            ..Self::new()
        }
    }

    /// All adjustments enabled.
    pub fn all() -> Self {
        Self {
            slide_x: true,
            slide_y: true,
            flip_x: true,
            flip_y: true,
            resize_x: true,
            resize_y: true,
        }
    }
}

impl Default for ConstraintAdjustment {
    fn default() -> Self {
        Self::new()
    }
}

/// Describes how a popup should be positioned relative to its parent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PopupPosition {
    pub anchor_rect: Rectangle,
    pub anchor_edge: Edge,
    pub gravity: Edge,
    pub offset_x: i32,
    pub offset_y: i32,
    pub constraint_adjustment: ConstraintAdjustment,
}

impl Default for PopupPosition {
    fn default() -> Self {
        Self {
            anchor_rect: Rectangle::default(),
            anchor_edge: Edge::None,
            gravity: Edge::None,
            offset_x: 0,
            offset_y: 0,
            constraint_adjustment: ConstraintAdjustment::new(),
        }
    }
}

/// An XDG popup surface.
#[derive(Debug, Clone)]
pub struct Popup {
    pub id: SurfaceId,
    pub parent: SurfaceId,
    pub position: PopupPosition,
    pub size: Rectangle,
    pub visible: bool,
    pub grab: bool,
}

/// Manages popup lifecycle and positioning.
#[derive(Debug)]
pub struct PopupManager {
    popups: HashMap<SurfaceId, Popup>,
    next_counter: u64,
}

impl PopupManager {
    /// Create a new popup manager.
    pub fn new() -> Self {
        Self {
            popups: HashMap::new(),
            next_counter: 0,
        }
    }

    /// Create a new popup attached to the given parent surface.
    /// Returns the id assigned to the new popup.
    pub fn create_popup(&mut self, parent: SurfaceId, position: PopupPosition) -> SurfaceId {
        let id = Uuid::new_v4();
        let popup = Popup {
            id,
            parent,
            position,
            size: Rectangle {
                x: 0,
                y: 0,
                width: 200,
                height: 100,
            },
            visible: true,
            grab: false,
        };
        self.popups.insert(id, popup);
        self.next_counter += 1;
        id
    }

    /// Dismiss (close) a popup by id. Returns the removed popup if found.
    pub fn dismiss_popup(&mut self, id: &SurfaceId) -> Option<Popup> {
        self.popups.remove(id)
    }

    /// Dismiss all popups.
    pub fn dismiss_all(&mut self) {
        self.popups.clear();
    }

    /// Get a reference to a popup by id.
    pub fn get_popup(&self, id: &SurfaceId) -> Option<&Popup> {
        self.popups.get(id)
    }

    /// List all visible popups.
    pub fn active_popups(&self) -> Vec<&Popup> {
        self.popups.values().filter(|p| p.visible).collect()
    }

    /// Reposition an existing popup.
    pub fn reposition(&mut self, id: &SurfaceId, position: PopupPosition) -> anyhow::Result<()> {
        if let Some(popup) = self.popups.get_mut(id) {
            popup.position = position;
            Ok(())
        } else {
            anyhow::bail!("Popup {} not found", id)
        }
    }

    /// Total number of managed popups.
    pub fn len(&self) -> usize {
        self.popups.len()
    }

    /// Whether there are no popups.
    pub fn is_empty(&self) -> bool {
        self.popups.is_empty()
    }
}

impl Default for PopupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod popup_tests {
    use super::*;

    fn default_position() -> PopupPosition {
        PopupPosition::default()
    }

    #[test]
    fn test_popup_manager_new_empty() {
        let mgr = PopupManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_create_popup() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id = mgr.create_popup(parent, default_position());
        assert_eq!(mgr.len(), 1);
        let popup = mgr.get_popup(&id).unwrap();
        assert_eq!(popup.parent, parent);
        assert!(popup.visible);
    }

    #[test]
    fn test_dismiss_popup() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id = mgr.create_popup(parent, default_position());
        let removed = mgr.dismiss_popup(&id);
        assert!(removed.is_some());
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_dismiss_nonexistent() {
        let mut mgr = PopupManager::new();
        let result = mgr.dismiss_popup(&Uuid::new_v4());
        assert!(result.is_none());
    }

    #[test]
    fn test_dismiss_all() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        mgr.create_popup(parent, default_position());
        mgr.create_popup(parent, default_position());
        mgr.create_popup(parent, default_position());
        assert_eq!(mgr.len(), 3);
        mgr.dismiss_all();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_active_popups() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id1 = mgr.create_popup(parent, default_position());
        let _id2 = mgr.create_popup(parent, default_position());
        // Hide one
        mgr.popups.get_mut(&id1).unwrap().visible = false;
        let active = mgr.active_popups();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_reposition() {
        let mut mgr = PopupManager::new();
        let parent = Uuid::new_v4();
        let id = mgr.create_popup(parent, default_position());

        let new_pos = PopupPosition {
            offset_x: 50,
            offset_y: 100,
            ..default_position()
        };
        mgr.reposition(&id, new_pos).unwrap();
        let popup = mgr.get_popup(&id).unwrap();
        assert_eq!(popup.position.offset_x, 50);
        assert_eq!(popup.position.offset_y, 100);
    }

    #[test]
    fn test_reposition_nonexistent() {
        let mut mgr = PopupManager::new();
        let result = mgr.reposition(&Uuid::new_v4(), default_position());
        assert!(result.is_err());
    }

    #[test]
    fn test_constraint_adjustment_new() {
        let ca = ConstraintAdjustment::new();
        assert!(!ca.slide_x);
        assert!(!ca.slide_y);
        assert!(!ca.flip_x);
        assert!(!ca.flip_y);
        assert!(!ca.resize_x);
        assert!(!ca.resize_y);
    }

    #[test]
    fn test_constraint_adjustment_slide() {
        let ca = ConstraintAdjustment::slide();
        assert!(ca.slide_x);
        assert!(ca.slide_y);
        assert!(!ca.flip_x);
    }

    #[test]
    fn test_constraint_adjustment_flip() {
        let ca = ConstraintAdjustment::flip();
        assert!(ca.flip_x);
        assert!(ca.flip_y);
        assert!(!ca.slide_x);
    }

    #[test]
    fn test_constraint_adjustment_all() {
        let ca = ConstraintAdjustment::all();
        assert!(ca.slide_x && ca.slide_y && ca.flip_x && ca.flip_y && ca.resize_x && ca.resize_y);
    }

    #[test]
    fn test_edge_variants() {
        let edges = vec![Edge::None, Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];
        assert_eq!(edges.len(), 5);
        assert_eq!(Edge::default(), Edge::None);
    }

    #[test]
    fn test_popup_default_manager() {
        let mgr = PopupManager::default();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_popup_position_default() {
        let pos = PopupPosition::default();
        assert_eq!(pos.anchor_edge, Edge::None);
        assert_eq!(pos.gravity, Edge::None);
        assert_eq!(pos.offset_x, 0);
        assert_eq!(pos.offset_y, 0);
    }
}

#[cfg(test)]
mod protocol_extension_tests {
    use super::*;

    fn test_surface_id() -> SurfaceId {
        uuid::Uuid::new_v4()
    }

    // ── DataDeviceManager ──────────────────────────────────────────

    #[test]
    fn test_data_device_manager_new() {
        let mgr = DataDeviceManager::new();
        assert!(mgr.selections.is_empty());
        assert!(mgr.drag_source.is_none());
    }

    #[test]
    fn test_data_device_manager_default() {
        let mgr = DataDeviceManager::default();
        assert!(mgr.selections.is_empty());
        assert!(mgr.drag_source.is_none());
    }

    #[test]
    fn test_set_selection() {
        let mut mgr = DataDeviceManager::new();
        let sid = test_surface_id();
        mgr.set_selection(sid, vec!["text/plain".into()], 1);
        let offer = mgr.get_selection(&sid).unwrap();
        assert_eq!(offer.mime_types, vec!["text/plain"]);
        assert_eq!(offer.source_surface, sid);
        assert_eq!(offer.serial, 1);
    }

    #[test]
    fn test_set_selection_overwrite() {
        let mut mgr = DataDeviceManager::new();
        let sid = test_surface_id();
        mgr.set_selection(sid, vec!["text/plain".into()], 1);
        mgr.set_selection(sid, vec!["text/html".into()], 2);
        let offer = mgr.get_selection(&sid).unwrap();
        assert_eq!(offer.mime_types, vec!["text/html"]);
        assert_eq!(offer.serial, 2);
    }

    #[test]
    fn test_clear_selection() {
        let mut mgr = DataDeviceManager::new();
        let sid = test_surface_id();
        mgr.set_selection(sid, vec!["text/plain".into()], 1);
        mgr.clear_selection(&sid);
        assert!(mgr.get_selection(&sid).is_none());
    }

    #[test]
    fn test_clear_selection_nonexistent() {
        let mut mgr = DataDeviceManager::new();
        let sid = test_surface_id();
        mgr.clear_selection(&sid); // should not panic
        assert!(mgr.get_selection(&sid).is_none());
    }

    #[test]
    fn test_start_and_end_drag() {
        let mut mgr = DataDeviceManager::new();
        let src = test_surface_id();
        let icon = test_surface_id();
        mgr.start_drag(src, Some(icon), vec!["text/uri-list".into()]);
        let drag = mgr.drag_source.as_ref().unwrap();
        assert_eq!(drag.source_surface, src);
        assert_eq!(drag.icon_surface, Some(icon));
        assert!(drag.active);
        assert_eq!(drag.position, (0.0, 0.0));
        assert_eq!(drag.mime_types, vec!["text/uri-list"]);

        mgr.end_drag();
        assert!(mgr.drag_source.is_none());
    }

    #[test]
    fn test_start_drag_no_icon() {
        let mut mgr = DataDeviceManager::new();
        let src = test_surface_id();
        mgr.start_drag(src, None, vec![]);
        let drag = mgr.drag_source.as_ref().unwrap();
        assert!(drag.icon_surface.is_none());
        assert!(drag.mime_types.is_empty());
    }

    #[test]
    fn test_end_drag_when_none() {
        let mut mgr = DataDeviceManager::new();
        mgr.end_drag(); // should not panic
        assert!(mgr.drag_source.is_none());
    }

    #[test]
    fn test_multiple_surfaces_selection() {
        let mut mgr = DataDeviceManager::new();
        let s1 = test_surface_id();
        let s2 = test_surface_id();
        mgr.set_selection(s1, vec!["a".into()], 1);
        mgr.set_selection(s2, vec!["b".into()], 2);
        assert_eq!(mgr.selections.len(), 2);
        assert_eq!(mgr.get_selection(&s1).unwrap().mime_types, vec!["a"]);
        assert_eq!(mgr.get_selection(&s2).unwrap().mime_types, vec!["b"]);
    }

    // ── TextInputState ─────────────────────────────────────────────

    #[test]
    fn test_text_input_new() {
        let ti = TextInputState::new();
        assert!(ti.surface_id.is_none());
        assert!(!ti.enabled);
        assert_eq!(ti.content_type, ContentType::Normal);
        assert!(ti.surrounding_text.is_empty());
        assert_eq!(ti.cursor_position, 0);
        assert!(ti.preedit.is_none());
    }

    #[test]
    fn test_text_input_default() {
        let ti = TextInputState::default();
        assert!(!ti.enabled);
        assert_eq!(ti.content_type, ContentType::Normal);
    }

    #[test]
    fn test_content_type_default() {
        assert_eq!(ContentType::default(), ContentType::Normal);
    }

    #[test]
    fn test_text_input_enable_disable() {
        let mut ti = TextInputState::new();
        let sid = test_surface_id();
        ti.enable(sid);
        assert!(ti.enabled);
        assert_eq!(ti.surface_id, Some(sid));

        ti.disable();
        assert!(!ti.enabled);
        assert!(ti.surface_id.is_none());
        assert!(ti.preedit.is_none());
    }

    #[test]
    fn test_text_input_disable_clears_preedit() {
        let mut ti = TextInputState::new();
        let sid = test_surface_id();
        ti.enable(sid);
        ti.preedit = Some(PreeditState {
            text: "pre".into(),
            cursor_begin: 0,
            cursor_end: 3,
        });
        ti.disable();
        assert!(ti.preedit.is_none());
    }

    #[test]
    fn test_set_surrounding_text() {
        let mut ti = TextInputState::new();
        ti.set_surrounding_text("hello world".into(), 5);
        assert_eq!(ti.surrounding_text, "hello world");
        assert_eq!(ti.cursor_position, 5);
    }

    #[test]
    fn test_commit_preedit() {
        let mut ti = TextInputState::new();
        ti.preedit = Some(PreeditState {
            text: "composing".into(),
            cursor_begin: 0,
            cursor_end: 9,
        });
        let text = ti.commit_preedit();
        assert_eq!(text, Some("composing".to_string()));
        assert!(ti.preedit.is_none());
    }

    #[test]
    fn test_commit_preedit_none() {
        let mut ti = TextInputState::new();
        assert_eq!(ti.commit_preedit(), None);
    }

    #[test]
    fn test_clear_preedit() {
        let mut ti = TextInputState::new();
        ti.preedit = Some(PreeditState {
            text: "x".into(),
            cursor_begin: 0,
            cursor_end: 1,
        });
        ti.clear_preedit();
        assert!(ti.preedit.is_none());
    }

    #[test]
    fn test_clear_preedit_when_none() {
        let mut ti = TextInputState::new();
        ti.clear_preedit(); // should not panic
        assert!(ti.preedit.is_none());
    }

    #[test]
    fn test_content_type_variants() {
        let variants = [
            ContentType::Normal,
            ContentType::Password,
            ContentType::Email,
            ContentType::Number,
            ContentType::Phone,
            ContentType::Url,
            ContentType::Terminal,
        ];
        assert_eq!(variants.len(), 7);
        // All are distinct
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // ── DecorationMode / DecorationState ───────────────────────────

    #[test]
    fn test_decoration_mode_default() {
        assert_eq!(DecorationMode::default(), DecorationMode::ServerSide);
    }

    #[test]
    fn test_decoration_state_new() {
        let sid = test_surface_id();
        let ds = DecorationState::new(sid);
        assert_eq!(ds.surface_id, sid);
        assert_eq!(ds.preferred, DecorationMode::ServerSide);
        assert_eq!(ds.current, DecorationMode::ServerSide);
    }

    #[test]
    fn test_decoration_negotiate_server_side() {
        let sid = test_surface_id();
        let mut ds = DecorationState::new(sid);
        ds.preferred = DecorationMode::ServerSide;
        let mode = ds.negotiate();
        assert_eq!(mode, DecorationMode::ServerSide);
        assert_eq!(ds.current, DecorationMode::ServerSide);
    }

    #[test]
    fn test_decoration_negotiate_client_side() {
        let sid = test_surface_id();
        let mut ds = DecorationState::new(sid);
        ds.preferred = DecorationMode::ClientSide;
        let mode = ds.negotiate();
        assert_eq!(mode, DecorationMode::ClientSide);
        assert_eq!(ds.current, DecorationMode::ClientSide);
    }

    #[test]
    fn test_decoration_mode_equality() {
        assert_eq!(DecorationMode::ClientSide, DecorationMode::ClientSide);
        assert_eq!(DecorationMode::ServerSide, DecorationMode::ServerSide);
        assert_ne!(DecorationMode::ClientSide, DecorationMode::ServerSide);
    }

    // ── ViewportState ──────────────────────────────────────────────

    #[test]
    fn test_viewport_state_new() {
        let sid = test_surface_id();
        let vs = ViewportState::new(sid);
        assert_eq!(vs.surface_id, sid);
        assert!(vs.source.is_none());
        assert!(vs.destination.is_none());
    }

    #[test]
    fn test_viewport_set_source() {
        let sid = test_surface_id();
        let mut vs = ViewportState::new(sid);
        vs.set_source(10.0, 20.0, 100.0, 200.0);
        let src = vs.source.unwrap();
        assert_eq!(src.x, 10.0);
        assert_eq!(src.y, 20.0);
        assert_eq!(src.width, 100.0);
        assert_eq!(src.height, 200.0);
    }

    #[test]
    fn test_viewport_set_destination() {
        let sid = test_surface_id();
        let mut vs = ViewportState::new(sid);
        vs.set_destination(800, 600);
        assert_eq!(vs.destination, Some((800, 600)));
    }

    #[test]
    fn test_viewport_effective_size_destination() {
        let sid = test_surface_id();
        let mut vs = ViewportState::new(sid);
        vs.set_source(0.0, 0.0, 1920.0, 1080.0);
        vs.set_destination(960, 540);
        assert_eq!(vs.effective_size(), Some((960, 540)));
    }

    #[test]
    fn test_viewport_effective_size_source_only() {
        let sid = test_surface_id();
        let mut vs = ViewportState::new(sid);
        vs.set_source(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(vs.effective_size(), Some((1920, 1080)));
    }

    #[test]
    fn test_viewport_effective_size_none() {
        let sid = test_surface_id();
        let vs = ViewportState::new(sid);
        assert_eq!(vs.effective_size(), None);
    }

    #[test]
    fn test_viewport_source_fractional() {
        let sid = test_surface_id();
        let mut vs = ViewportState::new(sid);
        vs.set_source(0.5, 0.5, 99.5, 49.5);
        // effective_size truncates to u32
        assert_eq!(vs.effective_size(), Some((99, 49)));
    }

    // ── FractionalScale ────────────────────────────────────────────

    #[test]
    fn test_fractional_scale_new() {
        let sid = test_surface_id();
        let fs = FractionalScale::new(sid, 120);
        assert_eq!(fs.surface_id, sid);
        assert_eq!(fs.scale_120, 120);
    }

    #[test]
    fn test_fractional_scale_factor_1x() {
        let sid = test_surface_id();
        let fs = FractionalScale::new(sid, 120);
        assert!((fs.scale_factor() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fractional_scale_factor_125() {
        let sid = test_surface_id();
        let fs = FractionalScale::new(sid, 150);
        assert!((fs.scale_factor() - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fractional_scale_factor_2x() {
        let sid = test_surface_id();
        let fs = FractionalScale::new(sid, 240);
        assert!((fs.scale_factor() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fractional_scale_from_scale() {
        let sid = test_surface_id();
        let fs = FractionalScale::from_scale(sid, 1.5);
        assert_eq!(fs.scale_120, 180);
        assert!((fs.scale_factor() - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fractional_scale_from_scale_1x() {
        let sid = test_surface_id();
        let fs = FractionalScale::from_scale(sid, 1.0);
        assert_eq!(fs.scale_120, 120);
    }

    #[test]
    fn test_fractional_scale_from_scale_rounding() {
        let sid = test_surface_id();
        // 1.33333... * 120 = 160.0 (rounds to 160)
        let fs = FractionalScale::from_scale(sid, 1.3333333333);
        assert_eq!(fs.scale_120, 160);
    }

    #[test]
    fn test_fractional_scale_zero() {
        let sid = test_surface_id();
        let fs = FractionalScale::new(sid, 0);
        assert!((fs.scale_factor()).abs() < f64::EPSILON);
    }

    // ── ProtocolAction variants ────────────────────────────────────

    #[test]
    fn test_protocol_action_set_selection() {
        let sid = test_surface_id();
        let action = ProtocolAction::SetSelection {
            surface_id: sid,
            mime_types: vec!["text/plain".into()],
        };
        match action {
            ProtocolAction::SetSelection {
                surface_id,
                mime_types,
            } => {
                assert_eq!(surface_id, sid);
                assert_eq!(mime_types, vec!["text/plain"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_protocol_action_start_drag() {
        let src = test_surface_id();
        let icon = test_surface_id();
        let action = ProtocolAction::StartDrag {
            source: src,
            icon: Some(icon),
            mime_types: vec![],
        };
        match action {
            ProtocolAction::StartDrag {
                source,
                icon: i,
                mime_types,
            } => {
                assert_eq!(source, src);
                assert_eq!(i, Some(icon));
                assert!(mime_types.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_protocol_action_text_input_enable() {
        let sid = test_surface_id();
        let action = ProtocolAction::TextInputEnable { surface_id: sid };
        matches!(action, ProtocolAction::TextInputEnable { .. });
    }

    #[test]
    fn test_protocol_action_text_input_disable() {
        let sid = test_surface_id();
        let action = ProtocolAction::TextInputDisable { surface_id: sid };
        matches!(action, ProtocolAction::TextInputDisable { .. });
    }

    #[test]
    fn test_protocol_action_text_input_commit() {
        let sid = test_surface_id();
        let action = ProtocolAction::TextInputCommit {
            surface_id: sid,
            text: "hello".into(),
        };
        match action {
            ProtocolAction::TextInputCommit { text, .. } => assert_eq!(text, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_protocol_action_set_decoration_mode() {
        let sid = test_surface_id();
        let action = ProtocolAction::SetDecorationMode {
            surface_id: sid,
            mode: DecorationMode::ClientSide,
        };
        match action {
            ProtocolAction::SetDecorationMode { mode, .. } => {
                assert_eq!(mode, DecorationMode::ClientSide);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_protocol_action_set_viewport() {
        let sid = test_surface_id();
        let action = ProtocolAction::SetViewport {
            surface_id: sid,
            source: Some(ViewportSource {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }),
            destination: Some((50, 50)),
        };
        match action {
            ProtocolAction::SetViewport {
                source,
                destination,
                ..
            } => {
                assert!(source.is_some());
                assert_eq!(destination, Some((50, 50)));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_protocol_action_set_fractional_scale() {
        let sid = test_surface_id();
        let action = ProtocolAction::SetFractionalScale {
            surface_id: sid,
            scale_120: 180,
        };
        match action {
            ProtocolAction::SetFractionalScale { scale_120, .. } => {
                assert_eq!(scale_120, 180);
            }
            _ => panic!("wrong variant"),
        }
    }
}
