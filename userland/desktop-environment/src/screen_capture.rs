//! Screen capture and screenshot subsystem for the AGNOS desktop environment.
//!
//! Provides full-screen, per-window, and region-based capture with security
//! integration (secure-mode blocking, per-agent permission checks, audit logging).
//! Output formats: raw ARGB8888, PNG, and BMP.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::compositor::Compositor;
use crate::SurfaceId;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for a capture request.
pub type CaptureId = Uuid;

/// What region of the screen to capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CaptureTarget {
    /// Capture the entire composited output.
    FullScreen,
    /// Capture a specific window by its surface ID.
    Window { surface_id: SurfaceId },
    /// Capture an arbitrary rectangle (x, y, width, height).
    Region {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
}

/// Desired output format for the captured image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureFormat {
    /// Raw ARGB8888 pixel data.
    RawArgb,
    /// PNG (lossless, compressed).
    #[default]
    Png,
    /// BMP (uncompressed, simple).
    Bmp,
}

/// A completed screen capture.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureResult {
    /// Unique ID for this capture.
    pub id: CaptureId,
    /// What was captured.
    pub target: CaptureTarget,
    /// Output format.
    pub format: CaptureFormat,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Timestamp of capture.
    pub captured_at: DateTime<Utc>,
    /// The image data (raw bytes in the requested format).
    #[serde(skip)]
    pub data: Vec<u8>,
    /// Size of the image data in bytes.
    pub data_size: usize,
    /// Agent that requested the capture (if any).
    pub requesting_agent: Option<String>,
}

/// Permission grant for an agent to take screenshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturePermission {
    /// Agent ID this permission applies to.
    pub agent_id: String,
    /// What targets the agent may capture.
    pub allowed_targets: Vec<CaptureTargetKind>,
    /// When this permission was granted.
    pub granted_at: DateTime<Utc>,
    /// Optional expiry (None = no expiry).
    pub expires_at: Option<DateTime<Utc>>,
    /// Maximum captures per minute (rate limiting).
    pub max_captures_per_minute: u32,
}

/// Simplified target kind for permission checks (no embedded IDs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTargetKind {
    FullScreen,
    Window,
    Region,
}

impl CaptureTargetKind {
    /// Check if a concrete target matches this kind.
    pub fn matches(&self, target: &CaptureTarget) -> bool {
        matches!(
            (self, target),
            (CaptureTargetKind::FullScreen, CaptureTarget::FullScreen)
                | (CaptureTargetKind::Window, CaptureTarget::Window { .. })
                | (CaptureTargetKind::Region, CaptureTarget::Region { .. })
        )
    }
}

/// Errors from the screen capture subsystem.
#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("secure mode is active — screen capture is blocked")]
    SecureModeActive,
    #[error("agent '{0}' does not have screen capture permission")]
    PermissionDenied(String),
    #[error("agent '{0}' is not permitted to capture {1:?} targets")]
    TargetNotAllowed(String, CaptureTargetKind),
    #[error("agent '{0}' has exceeded the capture rate limit ({1}/min)")]
    RateLimitExceeded(String, u32),
    #[error("capture permission for agent '{0}' has expired")]
    PermissionExpired(String),
    #[error("window '{0}' not found")]
    WindowNotFound(SurfaceId),
    #[error("capture region is out of bounds")]
    RegionOutOfBounds,
    #[error("encoding failed: {0}")]
    EncodingError(String),
}

// ---------------------------------------------------------------------------
// Rate-limit tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RateEntry {
    timestamps: Vec<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// ScreenCaptureManager
// ---------------------------------------------------------------------------

/// Manages screen capture permissions, rate limits, and capture execution.
pub struct ScreenCaptureManager {
    /// Per-agent capture permissions.
    permissions: Arc<RwLock<HashMap<String, CapturePermission>>>,
    /// Per-agent rate-limit tracking (agent_id -> recent capture timestamps).
    rate_limits: Arc<RwLock<HashMap<String, RateEntry>>>,
    /// Completed capture history (ring buffer, max 100 entries — metadata only).
    capture_history: Arc<RwLock<Vec<CaptureHistoryEntry>>>,
}

/// Metadata-only record of a past capture (no pixel data retained).
#[derive(Debug, Clone, Serialize)]
pub struct CaptureHistoryEntry {
    pub id: CaptureId,
    pub target: CaptureTarget,
    pub format: CaptureFormat,
    pub width: u32,
    pub height: u32,
    pub data_size: usize,
    pub captured_at: DateTime<Utc>,
    pub requesting_agent: Option<String>,
}

const MAX_HISTORY: usize = 100;

impl ScreenCaptureManager {
    /// Create a new manager with no permissions granted.
    pub fn new() -> Self {
        Self {
            permissions: Arc::new(RwLock::new(HashMap::new())),
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
            capture_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    // -----------------------------------------------------------------------
    // Permission management
    // -----------------------------------------------------------------------

    /// Grant capture permission to an agent.
    pub fn grant_permission(&self, permission: CapturePermission) {
        let agent_id = permission.agent_id.clone();
        info!(
            agent = %agent_id,
            targets = ?permission.allowed_targets,
            "Screen capture permission granted"
        );
        self.permissions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(agent_id, permission);
    }

    /// Revoke capture permission from an agent.
    pub fn revoke_permission(&self, agent_id: &str) -> bool {
        let removed = self
            .permissions
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(agent_id)
            .is_some();
        if removed {
            info!(agent = %agent_id, "Screen capture permission revoked");
        }
        removed
    }

    /// List all current capture permissions.
    pub fn list_permissions(&self) -> Vec<CapturePermission> {
        self.permissions
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Get permission for a specific agent.
    pub fn get_permission(&self, agent_id: &str) -> Option<CapturePermission> {
        self.permissions
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(agent_id)
            .cloned()
    }

    // -----------------------------------------------------------------------
    // Capture history
    // -----------------------------------------------------------------------

    /// Return recent capture history entries.
    pub fn capture_history(&self) -> Vec<CaptureHistoryEntry> {
        self.capture_history
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn record_history(&self, result: &CaptureResult) {
        let entry = CaptureHistoryEntry {
            id: result.id,
            target: result.target.clone(),
            format: result.format,
            width: result.width,
            height: result.height,
            data_size: result.data_size,
            captured_at: result.captured_at,
            requesting_agent: result.requesting_agent.clone(),
        };
        let mut history = self
            .capture_history
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if history.len() >= MAX_HISTORY {
            history.remove(0);
        }
        history.push(entry);
    }

    // -----------------------------------------------------------------------
    // Authorization check
    // -----------------------------------------------------------------------

    /// Verify that an agent is authorized to capture the given target.
    /// Returns `Ok(())` if permitted, or an appropriate `CaptureError`.
    fn check_authorization(
        &self,
        agent_id: &str,
        target: &CaptureTarget,
        compositor: &Compositor,
    ) -> Result<(), CaptureError> {
        // 1. Check secure mode
        let secure = *compositor
            .secure_mode
            .read()
            .unwrap_or_else(|e| e.into_inner());
        if secure {
            warn!(
                agent = %agent_id,
                "Screen capture blocked — secure mode active"
            );
            return Err(CaptureError::SecureModeActive);
        }

        // 2. Check permission exists
        let permissions = self
            .permissions
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let perm = permissions
            .get(agent_id)
            .ok_or_else(|| CaptureError::PermissionDenied(agent_id.to_string()))?;

        // 3. Check expiry
        if let Some(expires) = perm.expires_at {
            if Utc::now() > expires {
                return Err(CaptureError::PermissionExpired(agent_id.to_string()));
            }
        }

        // 4. Check target kind is allowed
        let kind = target_kind(target);
        if !perm.allowed_targets.iter().any(|k| k.matches(target)) {
            return Err(CaptureError::TargetNotAllowed(
                agent_id.to_string(),
                kind,
            ));
        }

        // 5. Rate limit check
        let max_per_min = perm.max_captures_per_minute;
        drop(permissions);
        self.check_rate_limit(agent_id, max_per_min)?;

        Ok(())
    }

    fn check_rate_limit(&self, agent_id: &str, max_per_min: u32) -> Result<(), CaptureError> {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(60);
        let mut rates = self
            .rate_limits
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let entry = rates.entry(agent_id.to_string()).or_insert(RateEntry {
            timestamps: Vec::new(),
        });
        // Prune old entries
        entry.timestamps.retain(|t| *t > cutoff);
        if entry.timestamps.len() as u32 >= max_per_min {
            return Err(CaptureError::RateLimitExceeded(
                agent_id.to_string(),
                max_per_min,
            ));
        }
        entry.timestamps.push(now);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Capture execution
    // -----------------------------------------------------------------------

    /// Capture a screenshot. If `agent_id` is provided, permission and rate
    /// limits are enforced. If `None`, the capture is treated as a system
    /// (privileged) request and only secure-mode is checked.
    pub fn capture(
        &self,
        compositor: &Compositor,
        target: CaptureTarget,
        format: CaptureFormat,
        agent_id: Option<&str>,
    ) -> Result<CaptureResult, CaptureError> {
        // Authorization
        if let Some(aid) = agent_id {
            self.check_authorization(aid, &target, compositor)?;
        } else {
            // System capture — still respect secure mode
            let secure = *compositor
                .secure_mode
                .read()
                .unwrap_or_else(|e| e.into_inner());
            if secure {
                return Err(CaptureError::SecureModeActive);
            }
        }

        // Ensure renderer is up to date
        compositor.render();

        // Extract raw ARGB pixels for the requested target
        let (raw_pixels, width, height) = match &target {
            CaptureTarget::FullScreen => self.capture_full_screen(compositor)?,
            CaptureTarget::Window { surface_id } => {
                self.capture_window(compositor, *surface_id)?
            }
            CaptureTarget::Region {
                x,
                y,
                width,
                height,
            } => self.capture_region(compositor, *x, *y, *width, *height)?,
        };

        // Encode to the requested format
        let data = encode_pixels(&raw_pixels, width, height, format)?;

        let result = CaptureResult {
            id: Uuid::new_v4(),
            target,
            format,
            width,
            height,
            captured_at: Utc::now(),
            data_size: data.len(),
            data,
            requesting_agent: agent_id.map(|s| s.to_string()),
        };

        debug!(
            id = %result.id,
            width = result.width,
            height = result.height,
            format = ?result.format,
            size = result.data_size,
            agent = ?result.requesting_agent,
            "Screen capture completed"
        );

        self.record_history(&result);
        Ok(result)
    }

    /// Capture the full composited output.
    fn capture_full_screen(
        &self,
        compositor: &Compositor,
    ) -> Result<(Vec<u32>, u32, u32), CaptureError> {
        let output = *compositor
            .current_output
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let renderer = compositor
            .renderer
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let fb = renderer.front_buffer();
        Ok((fb.pixels.clone(), output.width, output.height))
    }

    /// Capture a single window's region from the composited output.
    fn capture_window(
        &self,
        compositor: &Compositor,
        surface_id: SurfaceId,
    ) -> Result<(Vec<u32>, u32, u32), CaptureError> {
        // Look up window geometry from the scene graph
        let scene = compositor
            .scene
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let surface = scene
            .get_surface(surface_id)
            .ok_or(CaptureError::WindowNotFound(surface_id))?;
        let geom = surface.geometry;
        drop(scene);

        self.capture_region(compositor, geom.x, geom.y, geom.width, geom.height)
    }

    /// Capture an arbitrary region from the composited output.
    fn capture_region(
        &self,
        compositor: &Compositor,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<(Vec<u32>, u32, u32), CaptureError> {
        if width == 0 || height == 0 {
            return Err(CaptureError::RegionOutOfBounds);
        }

        let output = *compositor
            .current_output
            .read()
            .unwrap_or_else(|e| e.into_inner());

        // Clamp region to screen bounds
        let src_x = x.max(0) as u32;
        let src_y = y.max(0) as u32;
        let clamped_w = width.min(output.width.saturating_sub(src_x));
        let clamped_h = height.min(output.height.saturating_sub(src_y));

        if clamped_w == 0 || clamped_h == 0 {
            return Err(CaptureError::RegionOutOfBounds);
        }

        let renderer = compositor
            .renderer
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let fb = renderer.front_buffer();

        let mut pixels = Vec::with_capacity((clamped_w * clamped_h) as usize);
        for row in src_y..(src_y + clamped_h) {
            for col in src_x..(src_x + clamped_w) {
                let idx = (row * fb.width + col) as usize;
                pixels.push(if idx < fb.pixels.len() {
                    fb.pixels[idx]
                } else {
                    0xFF000000 // black if out-of-bounds
                });
            }
        }

        Ok((pixels, clamped_w, clamped_h))
    }
}

impl Default for ScreenCaptureManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

/// Helper to determine the target kind from a concrete target.
fn target_kind(target: &CaptureTarget) -> CaptureTargetKind {
    match target {
        CaptureTarget::FullScreen => CaptureTargetKind::FullScreen,
        CaptureTarget::Window { .. } => CaptureTargetKind::Window,
        CaptureTarget::Region { .. } => CaptureTargetKind::Region,
    }
}

/// Encode ARGB8888 pixels into the requested format.
fn encode_pixels(
    pixels: &[u32],
    width: u32,
    height: u32,
    format: CaptureFormat,
) -> Result<Vec<u8>, CaptureError> {
    match format {
        CaptureFormat::RawArgb => {
            // Return raw bytes in ARGB8888 layout
            let mut data = Vec::with_capacity(pixels.len() * 4);
            for &px in pixels {
                data.extend_from_slice(&px.to_ne_bytes());
            }
            Ok(data)
        }
        CaptureFormat::Bmp => encode_bmp(pixels, width, height),
        CaptureFormat::Png => encode_png(pixels, width, height),
    }
}

/// Encode as a minimal BMP (BITMAPINFOHEADER, 32-bit BGRA, top-down).
fn encode_bmp(pixels: &[u32], width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    let row_size = width * 4; // 32-bit pixels, already 4-byte aligned
    let pixel_data_size = row_size * height;
    let file_size: u32 = 14 + 40 + pixel_data_size; // BMP header + DIB header + pixels
    let offset: u32 = 14 + 40;

    let mut buf = Vec::with_capacity(file_size as usize);

    // --- BMP file header (14 bytes) ---
    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]); // reserved
    buf.extend_from_slice(&offset.to_le_bytes());

    // --- BITMAPINFOHEADER (40 bytes) ---
    buf.extend_from_slice(&40u32.to_le_bytes()); // header size
    buf.extend_from_slice(&(width as i32).to_le_bytes());
    buf.extend_from_slice(&(-(height as i32)).to_le_bytes()); // negative = top-down
    buf.extend_from_slice(&1u16.to_le_bytes()); // planes
    buf.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    buf.extend_from_slice(&0u32.to_le_bytes()); // compression (BI_RGB)
    buf.extend_from_slice(&pixel_data_size.to_le_bytes()); // image size
    buf.extend_from_slice(&2835u32.to_le_bytes()); // x pixels per meter (~72 dpi)
    buf.extend_from_slice(&2835u32.to_le_bytes()); // y pixels per meter
    buf.extend_from_slice(&0u32.to_le_bytes()); // colors used
    buf.extend_from_slice(&0u32.to_le_bytes()); // important colors

    // --- Pixel data (BGRA from ARGB) ---
    for &px in pixels {
        let a = ((px >> 24) & 0xFF) as u8;
        let r = ((px >> 16) & 0xFF) as u8;
        let g = ((px >> 8) & 0xFF) as u8;
        let b = (px & 0xFF) as u8;
        buf.push(b);
        buf.push(g);
        buf.push(r);
        buf.push(a);
    }

    Ok(buf)
}

/// Encode as a minimal uncompressed PNG.
///
/// We use the simplest valid PNG structure: IHDR + a single uncompressed
/// (stored) IDAT with zlib wrapping + IEND. No external crate required.
fn encode_png(pixels: &[u32], width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    let mut buf = Vec::new();

    // PNG signature
    buf.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // --- IHDR ---
    let mut ihdr_data = Vec::with_capacity(13);
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(8); // bit depth
    ihdr_data.push(6); // color type: RGBA
    ihdr_data.push(0); // compression
    ihdr_data.push(0); // filter
    ihdr_data.push(0); // interlace
    write_png_chunk(&mut buf, b"IHDR", &ihdr_data);

    // --- IDAT (uncompressed deflate in zlib wrapper) ---
    // Build raw image data: each row has a filter byte (0 = None) followed by RGBA pixels
    let row_bytes = 1 + width as usize * 4; // filter byte + RGBA
    let raw_len = row_bytes * height as usize;
    let mut raw = Vec::with_capacity(raw_len);
    for y in 0..height as usize {
        raw.push(0); // filter: None
        for x in 0..width as usize {
            let px = pixels[y * width as usize + x];
            let r = ((px >> 16) & 0xFF) as u8;
            let g = ((px >> 8) & 0xFF) as u8;
            let b = (px & 0xFF) as u8;
            let a = ((px >> 24) & 0xFF) as u8;
            raw.push(r);
            raw.push(g);
            raw.push(b);
            raw.push(a);
        }
    }

    // Wrap raw bytes in zlib stored blocks (no compression)
    let idat_data = zlib_store(&raw);
    write_png_chunk(&mut buf, b"IDAT", &idat_data);

    // --- IEND ---
    write_png_chunk(&mut buf, b"IEND", &[]);

    Ok(buf)
}

/// Write a PNG chunk: length (4) + type (4) + data + CRC-32 (4).
fn write_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);
    let crc = png_crc32(chunk_type, data);
    buf.extend_from_slice(&crc.to_be_bytes());
}

/// CRC-32 used by PNG (same polynomial as zlib/gzip).
fn png_crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// Wrap data in a zlib container using stored (uncompressed) deflate blocks.
/// Max block size for stored deflate is 65535 bytes.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();

    // Zlib header: CM=8 (deflate), CINFO=7 (32K window), FCHECK so header % 31 == 0
    let cmf: u8 = 0x78; // CM=8, CINFO=7
    let flg: u8 = 0x01; // FCHECK=1 (0x7801 % 31 == 0)
    out.push(cmf);
    out.push(flg);

    // Deflate stored blocks
    let max_block = 65535usize;
    let mut offset = 0;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let block_size = remaining.min(max_block);
        let is_last = offset + block_size >= data.len();

        out.push(if is_last { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00 (stored)
        let len = block_size as u16;
        let nlen = !len;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(&data[offset..offset + block_size]);
        offset += block_size;
    }

    // Handle empty data edge case
    if data.is_empty() {
        out.push(0x01); // final empty stored block
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    }

    // Adler-32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());

    out
}

/// Adler-32 checksum as used by zlib.
fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compositor;

    fn setup() -> (Compositor, ScreenCaptureManager) {
        let compositor = Compositor::with_resolution(800, 600);
        let manager = ScreenCaptureManager::new();
        (compositor, manager)
    }

    fn grant_full_access(manager: &ScreenCaptureManager, agent_id: &str) {
        manager.grant_permission(CapturePermission {
            agent_id: agent_id.to_string(),
            allowed_targets: vec![
                CaptureTargetKind::FullScreen,
                CaptureTargetKind::Window,
                CaptureTargetKind::Region,
            ],
            granted_at: Utc::now(),
            expires_at: None,
            max_captures_per_minute: 60,
        });
    }

    // -- Basic capture tests --

    #[test]
    fn test_full_screen_capture_raw() {
        let (compositor, manager) = setup();
        let result = manager
            .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::RawArgb, None)
            .unwrap();
        assert_eq!(result.width, 800);
        assert_eq!(result.height, 600);
        assert_eq!(result.data.len(), 800 * 600 * 4);
        assert_eq!(result.data_size, 800 * 600 * 4);
    }

    #[test]
    fn test_full_screen_capture_bmp() {
        let (compositor, manager) = setup();
        let result = manager
            .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::Bmp, None)
            .unwrap();
        assert_eq!(result.width, 800);
        assert_eq!(result.height, 600);
        // BMP: 14 + 40 header + 800*600*4 pixels
        assert_eq!(result.data.len(), 14 + 40 + 800 * 600 * 4);
        assert_eq!(&result.data[0..2], b"BM");
    }

    #[test]
    fn test_full_screen_capture_png() {
        let (compositor, manager) = setup();
        let result = manager
            .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::Png, None)
            .unwrap();
        assert_eq!(result.width, 800);
        assert_eq!(result.height, 600);
        // PNG signature
        assert_eq!(&result.data[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn test_region_capture() {
        let (compositor, manager) = setup();
        let result = manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: 100,
                    y: 100,
                    width: 200,
                    height: 150,
                },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap();
        assert_eq!(result.width, 200);
        assert_eq!(result.height, 150);
    }

    #[test]
    fn test_region_clamped_to_screen() {
        let (compositor, manager) = setup();
        // Region extends past screen bounds
        let result = manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: 700,
                    y: 500,
                    width: 200,
                    height: 200,
                },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap();
        assert_eq!(result.width, 100); // clamped: 800 - 700
        assert_eq!(result.height, 100); // clamped: 600 - 500
    }

    #[test]
    fn test_region_fully_out_of_bounds() {
        let (compositor, manager) = setup();
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: 900,
                    y: 700,
                    width: 100,
                    height: 100,
                },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::RegionOutOfBounds));
    }

    #[test]
    fn test_region_zero_dimensions() {
        let (compositor, manager) = setup();
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 100,
                },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::RegionOutOfBounds));
    }

    // -- Window capture tests --

    #[test]
    fn test_window_capture() {
        let (compositor, manager) = setup();
        let wid = compositor
            .create_window("Test".to_string(), "test-app".to_string(), false)
            .unwrap();
        compositor.render();
        let result = manager
            .capture(
                &compositor,
                CaptureTarget::Window { surface_id: wid },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap();
        assert!(result.width > 0);
        assert!(result.height > 0);
    }

    #[test]
    fn test_window_capture_not_found() {
        let (compositor, manager) = setup();
        let fake_id = Uuid::new_v4();
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::Window {
                    surface_id: fake_id,
                },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::WindowNotFound(_)));
    }

    // -- Secure mode tests --

    #[test]
    fn test_secure_mode_blocks_capture() {
        let (compositor, manager) = setup();
        compositor.set_secure_mode(true);
        let err = manager
            .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::Png, None)
            .unwrap_err();
        assert!(matches!(err, CaptureError::SecureModeActive));
    }

    #[test]
    fn test_secure_mode_blocks_agent_capture() {
        let (compositor, manager) = setup();
        grant_full_access(&manager, "agent-1");
        compositor.set_secure_mode(true);
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::Png,
                Some("agent-1"),
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::SecureModeActive));
    }

    // -- Permission tests --

    #[test]
    fn test_agent_without_permission_denied() {
        let (compositor, manager) = setup();
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::Png,
                Some("rogue-agent"),
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::PermissionDenied(_)));
    }

    #[test]
    fn test_agent_with_permission_succeeds() {
        let (compositor, manager) = setup();
        grant_full_access(&manager, "good-agent");
        let result = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::Png,
                Some("good-agent"),
            )
            .unwrap();
        assert_eq!(result.requesting_agent.as_deref(), Some("good-agent"));
    }

    #[test]
    fn test_agent_target_not_allowed() {
        let (compositor, manager) = setup();
        manager.grant_permission(CapturePermission {
            agent_id: "limited-agent".to_string(),
            allowed_targets: vec![CaptureTargetKind::Region],
            granted_at: Utc::now(),
            expires_at: None,
            max_captures_per_minute: 60,
        });
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::Png,
                Some("limited-agent"),
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::TargetNotAllowed(_, _)));
    }

    #[test]
    fn test_permission_expired() {
        let (compositor, manager) = setup();
        manager.grant_permission(CapturePermission {
            agent_id: "expired-agent".to_string(),
            allowed_targets: vec![CaptureTargetKind::FullScreen],
            granted_at: Utc::now() - chrono::Duration::hours(2),
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            max_captures_per_minute: 60,
        });
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::Png,
                Some("expired-agent"),
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::PermissionExpired(_)));
    }

    // -- Rate limiting tests --

    #[test]
    fn test_rate_limit_exceeded() {
        let (compositor, manager) = setup();
        manager.grant_permission(CapturePermission {
            agent_id: "fast-agent".to_string(),
            allowed_targets: vec![CaptureTargetKind::FullScreen],
            granted_at: Utc::now(),
            expires_at: None,
            max_captures_per_minute: 3,
        });
        // Use up the rate limit
        for _ in 0..3 {
            manager
                .capture(
                    &compositor,
                    CaptureTarget::FullScreen,
                    CaptureFormat::RawArgb,
                    Some("fast-agent"),
                )
                .unwrap();
        }
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::RawArgb,
                Some("fast-agent"),
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::RateLimitExceeded(_, 3)));
    }

    // -- Permission management tests --

    #[test]
    fn test_grant_and_revoke_permission() {
        let manager = ScreenCaptureManager::new();
        grant_full_access(&manager, "agent-x");
        assert!(manager.get_permission("agent-x").is_some());
        assert_eq!(manager.list_permissions().len(), 1);
        assert!(manager.revoke_permission("agent-x"));
        assert!(manager.get_permission("agent-x").is_none());
        assert_eq!(manager.list_permissions().len(), 0);
    }

    #[test]
    fn test_revoke_nonexistent_returns_false() {
        let manager = ScreenCaptureManager::new();
        assert!(!manager.revoke_permission("ghost"));
    }

    // -- History tests --

    #[test]
    fn test_capture_history_recorded() {
        let (compositor, manager) = setup();
        assert!(manager.capture_history().is_empty());
        manager
            .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::RawArgb, None)
            .unwrap();
        let history = manager.capture_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].width, 800);
        assert_eq!(history[0].height, 600);
    }

    #[test]
    fn test_capture_history_max_entries() {
        let (compositor, manager) = setup();
        for _ in 0..MAX_HISTORY + 10 {
            manager
                .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::RawArgb, None)
                .unwrap();
        }
        assert_eq!(manager.capture_history().len(), MAX_HISTORY);
    }

    // -- Encoding tests --

    #[test]
    fn test_bmp_encoding_structure() {
        let pixels = vec![0xFF_FF_00_00u32; 4]; // 2x2 red
        let bmp = encode_bmp(&pixels, 2, 2).unwrap();
        assert_eq!(&bmp[0..2], b"BM");
        let file_size = u32::from_le_bytes([bmp[2], bmp[3], bmp[4], bmp[5]]);
        assert_eq!(file_size as usize, bmp.len());
        assert_eq!(file_size, 14 + 40 + 2 * 2 * 4);
    }

    #[test]
    fn test_png_encoding_signature() {
        let pixels = vec![0xFF_00_FF_00u32; 4]; // 2x2 green
        let png = encode_png(&pixels, 2, 2).unwrap();
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn test_raw_encoding() {
        let pixels = vec![0xAABBCCDD_u32; 2];
        let raw = encode_pixels(&pixels, 2, 1, CaptureFormat::RawArgb).unwrap();
        assert_eq!(raw.len(), 8);
    }

    // -- Target kind matching tests --

    #[test]
    fn test_target_kind_matches() {
        assert!(CaptureTargetKind::FullScreen.matches(&CaptureTarget::FullScreen));
        assert!(CaptureTargetKind::Window.matches(&CaptureTarget::Window {
            surface_id: Uuid::new_v4()
        }));
        assert!(CaptureTargetKind::Region.matches(&CaptureTarget::Region {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }));
        assert!(!CaptureTargetKind::FullScreen.matches(&CaptureTarget::Region {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }));
    }

    // -- Adler-32 / CRC-32 sanity tests --

    #[test]
    fn test_adler32_empty() {
        assert_eq!(adler32(&[]), 1);
    }

    #[test]
    fn test_adler32_known() {
        // adler32("Wikipedia") = 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn test_crc32_known() {
        // CRC-32 of "IEND" chunk type with no data
        let crc = png_crc32(b"IEND", &[]);
        assert_eq!(crc, 0xAE42_6082);
    }

    // -- Default format test --

    #[test]
    fn test_default_capture_format_is_png() {
        assert_eq!(CaptureFormat::default(), CaptureFormat::Png);
    }

    // -- Negative x/y region test --

    #[test]
    fn test_region_negative_coords_clamped() {
        let (compositor, manager) = setup();
        let result = manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: -50,
                    y: -50,
                    width: 200,
                    height: 200,
                },
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap();
        // Negative coords clamped to 0, so region starts at (0,0)
        assert_eq!(result.width, 200);
        assert_eq!(result.height, 200);
    }

    // -- System capture still respects secure mode --

    #[test]
    fn test_system_capture_respects_secure_mode() {
        let (compositor, manager) = setup();
        compositor.set_secure_mode(true);
        let err = manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::RawArgb,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, CaptureError::SecureModeActive));
    }

    // -- Multiple agents with different permissions --

    #[test]
    fn test_multiple_agent_permissions() {
        let (compositor, manager) = setup();
        manager.grant_permission(CapturePermission {
            agent_id: "agent-a".to_string(),
            allowed_targets: vec![CaptureTargetKind::FullScreen],
            granted_at: Utc::now(),
            expires_at: None,
            max_captures_per_minute: 10,
        });
        manager.grant_permission(CapturePermission {
            agent_id: "agent-b".to_string(),
            allowed_targets: vec![CaptureTargetKind::Region],
            granted_at: Utc::now(),
            expires_at: None,
            max_captures_per_minute: 10,
        });

        // agent-a can fullscreen but not region
        manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::RawArgb,
                Some("agent-a"),
            )
            .unwrap();
        assert!(manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100
                },
                CaptureFormat::RawArgb,
                Some("agent-a"),
            )
            .is_err());

        // agent-b can region but not fullscreen
        manager
            .capture(
                &compositor,
                CaptureTarget::Region {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                },
                CaptureFormat::RawArgb,
                Some("agent-b"),
            )
            .unwrap();
        assert!(manager
            .capture(
                &compositor,
                CaptureTarget::FullScreen,
                CaptureFormat::RawArgb,
                Some("agent-b"),
            )
            .is_err());
    }
}
