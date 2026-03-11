//! Shared types for the Wayland protocol integration.

use std::collections::HashMap;

use crate::compositor::{Rectangle, SurfaceId};

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
