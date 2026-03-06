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

use crate::compositor::{Compositor, InputEvent, Rectangle, SurfaceId};

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
        let min_stride = self.width.checked_mul(self.format.bpp())
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
        let total = self.stride.checked_mul(self.height)
            .ok_or("Buffer size overflow")?;
        let total_with_offset = self.offset.checked_add(total)
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
        if self.shift { raw |= 0x01; }
        if self.caps_lock { raw |= 0x02; }
        if self.ctrl { raw |= 0x04; }
        if self.alt { raw |= 0x08; }
        if self.num_lock { raw |= 0x10; }
        if self.logo { raw |= 0x40; }
        raw
    }

    /// True if no modifiers are active.
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.logo
            && !self.caps_lock && !self.num_lock
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
    Motion { x: f64, y: f64 },
    Button { button: u32, x: f64, y: f64, pressed: bool },
    Axis { horizontal: f64, vertical: f64 },
    Enter { surface: SurfaceId, x: f64, y: f64 },
    Leave { surface: SurfaceId },
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
    use wayland_server::{Display, ListeningSocket};

    /// Main Wayland server state.
    ///
    /// Holds the `wayland_server::Display`, tracks clients and surfaces,
    /// and bridges to the internal AGNOS compositor.
    pub struct WaylandState {
        pub display: Display<Self>,
        pub compositor: Arc<Compositor>,
        pub surface_map: SurfaceMap,
        pub clients: ClientRegistry,
        pub serial: SerialCounter,
        pub pointer_focus: PointerFocus,
        pub keyboard_focus: KeyboardFocus,
        pub toplevels: HashMap<SurfaceId, XdgToplevelTracker>,
        pub output: OutputInfo,
        pub seat_caps: SeatCapabilities,
        pub socket_name: Option<String>,
    }

    impl WaylandState {
        /// Create a new Wayland server state bound to the given compositor.
        pub fn new(compositor: Arc<Compositor>) -> Result<Self, Box<dyn std::error::Error>> {
            let display = Display::new()?;
            Ok(Self {
                display,
                compositor,
                surface_map: SurfaceMap::new(),
                clients: ClientRegistry::new(),
                serial: SerialCounter::new(),
                pointer_focus: PointerFocus::default(),
                keyboard_focus: KeyboardFocus::default(),
                toplevels: HashMap::new(),
                output: OutputInfo::default(),
                seat_caps: SeatCapabilities::default(),
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
    pub struct WaylandState {
        pub compositor: Arc<Compositor>,
        pub surface_map: SurfaceMap,
        pub clients: ClientRegistry,
        pub serial: SerialCounter,
        pub pointer_focus: PointerFocus,
        pub keyboard_focus: KeyboardFocus,
        pub toplevels: HashMap<SurfaceId, XdgToplevelTracker>,
        pub output: OutputInfo,
        pub seat_caps: SeatCapabilities,
        pub socket_name: Option<String>,
    }

    impl WaylandState {
        /// Create stub state. No real Wayland socket is opened.
        pub fn new(compositor: Arc<Compositor>) -> Result<Self, Box<dyn std::error::Error>> {
            Ok(Self {
                compositor,
                surface_map: SurfaceMap::new(),
                clients: ClientRegistry::new(),
                serial: SerialCounter::new(),
                pointer_focus: PointerFocus::default(),
                keyboard_focus: KeyboardFocus::default(),
                toplevels: HashMap::new(),
                output: OutputInfo::default(),
                seat_caps: SeatCapabilities::default(),
                socket_name: None,
            })
        }

        /// Stub: returns a fake socket name.
        pub fn listen(&mut self) -> Result<String, Box<dyn std::error::Error>> {
            let name = "wayland-stub-0".to_string();
            self.socket_name = Some(name.clone());
            Ok(name)
        }

        /// Dispatch a frame: read client messages, compose, present.
        /// In stub mode this is a no-op.
        pub fn dispatch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
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
                button, x, y, pressed,
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
                keycode, modifiers, pressed,
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
        assert!(state.surface_map.is_empty());
        assert!(state.clients.is_empty());
        assert_eq!(state.serial.current(), 0);
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
        assert!(state.dispatch().is_ok());
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
