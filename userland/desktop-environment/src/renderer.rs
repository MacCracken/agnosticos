//! Software Framebuffer Renderer
//!
//! Provides a pixel-buffer rendering pipeline for the AGNOS desktop compositor.
//! Supports window decorations, damage tracking, layer compositing, and
//! text rendering via a built-in bitmap font.
//!
//! This renderer targets a linear ARGB8888 framebuffer suitable for
//! DRM/KMS scanout or Wayland SHM buffer submission.

use std::collections::HashMap;

use tracing::debug;

use crate::accessibility::HighContrastTheme;
use crate::compositor::{Rectangle, SurfaceId, WindowState};

// ---------------------------------------------------------------------------
// Color & Pixel Types
// ---------------------------------------------------------------------------

/// ARGB8888 pixel: `[alpha, red, green, blue]` packed into a u32.
pub type Pixel = u32;

/// Construct an ARGB pixel from components.
pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Pixel {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Extract ARGB components from a pixel.
pub const fn decompose(p: Pixel) -> (u8, u8, u8, u8) {
    (
        ((p >> 24) & 0xFF) as u8,
        ((p >> 16) & 0xFF) as u8,
        ((p >> 8) & 0xFF) as u8,
        (p & 0xFF) as u8,
    )
}

/// Alpha-blend `src` over `dst` (pre-multiplied alpha).
pub fn blend(src: Pixel, dst: Pixel) -> Pixel {
    let (sa, sr, sg, sb) = decompose(src);
    let (_, dr, dg, db) = decompose(dst);

    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }

    let inv_alpha = 255 - sa as u32;
    let r = sr as u32 + (dr as u32 * inv_alpha / 255);
    let g = sg as u32 + (dg as u32 * inv_alpha / 255);
    let b = sb as u32 + (db as u32 * inv_alpha / 255);
    let a = sa as u32 + (255u32.saturating_sub(sa as u32)); // result alpha

    argb(
        a.min(255) as u8,
        r.min(255) as u8,
        g.min(255) as u8,
        b.min(255) as u8,
    )
}

// Well-known colors
pub const COLOR_TRANSPARENT: Pixel = argb(0, 0, 0, 0);
pub const COLOR_BLACK: Pixel = argb(255, 0, 0, 0);
pub const COLOR_WHITE: Pixel = argb(255, 255, 255, 255);
pub const COLOR_BG_DARK: Pixel = argb(255, 30, 30, 36);
pub const COLOR_TITLEBAR: Pixel = argb(255, 48, 48, 60);
pub const COLOR_TITLEBAR_ACTIVE: Pixel = argb(255, 70, 70, 120);
pub const COLOR_BORDER: Pixel = argb(255, 80, 80, 100);
pub const COLOR_CLOSE_BTN: Pixel = argb(255, 220, 60, 60);
pub const COLOR_MINIMIZE_BTN: Pixel = argb(255, 220, 180, 40);
pub const COLOR_MAXIMIZE_BTN: Pixel = argb(255, 60, 180, 60);
pub const COLOR_PANEL_BG: Pixel = argb(230, 20, 20, 28);
pub const COLOR_HUD_BG: Pixel = argb(200, 20, 20, 30);
pub const COLOR_ACCENT: Pixel = argb(255, 100, 120, 255);
pub const COLOR_TEXT: Pixel = argb(255, 220, 220, 230);
pub const COLOR_TEXT_DIM: Pixel = argb(255, 140, 140, 160);

// ---------------------------------------------------------------------------
// Framebuffer
// ---------------------------------------------------------------------------

/// A linear pixel buffer.
#[derive(Clone)]
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Pixel>,
}

impl std::fmt::Debug for Framebuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Framebuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixel_count", &self.pixels.len())
            .finish()
    }
}

impl Framebuffer {
    /// Create a framebuffer filled with a solid color.
    pub fn new(width: u32, height: u32, fill: Pixel) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; (width * height) as usize],
        }
    }

    /// Get a pixel at (x, y). Returns None if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> Option<Pixel> {
        if x < self.width && y < self.height {
            Some(self.pixels[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    /// Set a pixel at (x, y). No-op if out of bounds.
    pub fn set(&mut self, x: u32, y: u32, color: Pixel) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    /// Blend a pixel onto the buffer at (x, y).
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: Pixel) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.pixels[idx] = blend(color, self.pixels[idx]);
        }
    }

    /// Fill a rectangle with a solid color.
    pub fn fill_rect(&mut self, rect: &Rectangle, color: Pixel) {
        let x_start = rect.x.max(0) as u32;
        let y_start = rect.y.max(0) as u32;
        let x_end = rect.x.saturating_add(rect.width as i32).max(0) as u32;
        let x_end = x_end.min(self.width);
        let y_end = rect.y.saturating_add(rect.height as i32).max(0) as u32;
        let y_end = y_end.min(self.height);

        if x_start >= x_end || y_start >= y_end {
            return;
        }

        for y in y_start..y_end {
            let row_start = (y * self.width + x_start) as usize;
            let row_end = (y * self.width + x_end) as usize;
            if row_end <= self.pixels.len() {
                self.pixels[row_start..row_end].fill(color);
            }
        }
    }

    /// Draw a 1px border around a rectangle.
    pub fn draw_rect_outline(&mut self, rect: &Rectangle, color: Pixel) {
        let x0 = rect.x.max(0) as u32;
        let y0 = rect.y.max(0) as u32;
        let right = rect.x + rect.width as i32 - 1;
        let bottom = rect.y + rect.height as i32 - 1;
        if right < 0 || bottom < 0 {
            return;
        }
        let x1 = (right as u32).min(self.width.saturating_sub(1));
        let y1 = (bottom as u32).min(self.height.saturating_sub(1));
        if x0 > x1 || y0 > y1 {
            return;
        }

        // Top and bottom edges
        for x in x0..=x1 {
            self.set(x, y0, color);
            self.set(x, y1, color);
        }
        // Left and right edges
        for y in y0..=y1 {
            self.set(x0, y, color);
            self.set(x1, y, color);
        }
    }

    /// Blit another framebuffer onto this one at offset (dx, dy).
    /// Pre-clips source ranges to avoid per-pixel bounds checks.
    pub fn blit(&mut self, src: &Framebuffer, dx: i32, dy: i32) {
        // Compute visible source row/col range upfront
        let sy_start = if dy < 0 { (-dy) as u32 } else { 0 };
        let sy_end = src.height.min(((self.height as i32) - dy).max(0) as u32);
        let sx_start = if dx < 0 { (-dx) as u32 } else { 0 };
        let sx_end = src.width.min(((self.width as i32) - dx).max(0) as u32);

        for sy in sy_start..sy_end {
            let ty = (dy + sy as i32) as u32;
            for sx in sx_start..sx_end {
                let tx = (dx + sx as i32) as u32;
                let pixel = src.pixels[(sy * src.width + sx) as usize];
                self.blend_pixel(tx, ty, pixel);
            }
        }
    }

    /// Blit with clipping to a damage region.
    /// Pre-clips source ranges to avoid per-pixel bounds checks.
    pub fn blit_clipped(&mut self, src: &Framebuffer, dx: i32, dy: i32, clip: &Rectangle) {
        let cx0 = clip.x.max(0);
        let cy0 = clip.y.max(0);
        let cx1 = (clip.x + clip.width as i32).min(self.width as i32);
        let cy1 = (clip.y + clip.height as i32).min(self.height as i32);

        let sy_start = if dy < cy0 { (cy0 - dy) as u32 } else { 0 };
        let sy_end = src.height.min((cy1 - dy).max(0) as u32);
        let sx_start = if dx < cx0 { (cx0 - dx) as u32 } else { 0 };
        let sx_end = src.width.min((cx1 - dx).max(0) as u32);

        for sy in sy_start..sy_end {
            let ty = (dy + sy as i32) as u32;
            for sx in sx_start..sx_end {
                let tx = (dx + sx as i32) as u32;
                let pixel = src.pixels[(sy * src.width + sx) as usize];
                self.blend_pixel(tx, ty, pixel);
            }
        }
    }

    /// Clear to a solid color.
    pub fn clear(&mut self, color: Pixel) {
        self.pixels.fill(color);
    }

    /// Get raw byte slice (for DRM/KMS scanout or SHM buffer).
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: Pixel is u32 (4 bytes, no padding). Vec<u32> is a contiguous
        // array, so reinterpreting as &[u8] of len * 4 bytes is valid.
        // The resulting slice borrows self, preventing drop or reallocation.
        // The checked_mul guards against theoretical overflow on 32-bit platforms.
        let byte_len = self
            .pixels
            .len()
            .checked_mul(4)
            .expect("framebuffer byte length overflow");
        unsafe { std::slice::from_raw_parts(self.pixels.as_ptr() as *const u8, byte_len) }
    }
}

// ---------------------------------------------------------------------------
// Damage Tracking
// ---------------------------------------------------------------------------

/// Tracks dirty regions of the screen for efficient partial redraws.
#[derive(Debug, Clone)]
pub struct DamageTracker {
    /// Dirty rectangles accumulated since last flush.
    regions: Vec<Rectangle>,
    /// Screen dimensions for bounding.
    screen_width: u32,
    screen_height: u32,
    /// If true, the entire screen is dirty.
    full_damage: bool,
}

impl DamageTracker {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            regions: Vec::new(),
            screen_width: width,
            screen_height: height,
            full_damage: true, // First frame is always full
        }
    }

    /// Mark a rectangle as damaged (needs redraw).
    pub fn add_damage(&mut self, rect: Rectangle) {
        if self.full_damage {
            return; // Already fully damaged
        }
        self.regions.push(rect);
    }

    /// Mark the entire screen as damaged.
    pub fn damage_full(&mut self) {
        self.full_damage = true;
        self.regions.clear();
    }

    /// Get the bounding box of all damage, or the full screen if fully damaged.
    pub fn get_damage_bounds(&self) -> Rectangle {
        if self.full_damage || self.regions.is_empty() {
            return Rectangle {
                x: 0,
                y: 0,
                width: self.screen_width,
                height: self.screen_height,
            };
        }

        let mut x_min = i32::MAX;
        let mut y_min = i32::MAX;
        let mut x_max = i32::MIN;
        let mut y_max = i32::MIN;

        for r in &self.regions {
            x_min = x_min.min(r.x);
            y_min = y_min.min(r.y);
            x_max = x_max.max(r.x + r.width as i32);
            y_max = y_max.max(r.y + r.height as i32);
        }

        Rectangle {
            x: x_min.max(0),
            y: y_min.max(0),
            width: (x_max - x_min.max(0)).max(0) as u32,
            height: (y_max - y_min.max(0)).max(0) as u32,
        }
    }

    /// Flush damage — returns the damage bounds and resets the tracker.
    pub fn flush(&mut self) -> Rectangle {
        let bounds = self.get_damage_bounds();
        self.regions.clear();
        self.full_damage = false;
        bounds
    }

    /// Check if any damage exists.
    pub fn has_damage(&self) -> bool {
        self.full_damage || !self.regions.is_empty()
    }

    /// Number of damage regions.
    pub fn region_count(&self) -> usize {
        if self.full_damage {
            1
        } else {
            self.regions.len()
        }
    }
}

// ---------------------------------------------------------------------------
// Bitmap Font (5x7 fixed-width)
// ---------------------------------------------------------------------------

/// Render a character from a built-in 5x7 bitmap font.
/// Returns true if the character was rendered.
pub fn draw_char(fb: &mut Framebuffer, ch: char, x: u32, y: u32, color: Pixel) -> bool {
    let glyph = match get_glyph(ch) {
        Some(g) => g,
        None => return false,
    };

    for (row, &bits) in glyph.iter().enumerate() {
        for col in 0..5u32 {
            if bits & (1 << (4 - col)) != 0 {
                fb.blend_pixel(x + col, y + row as u32, color);
            }
        }
    }
    true
}

/// Draw a string using the bitmap font. Returns the width in pixels drawn.
pub fn draw_text(fb: &mut Framebuffer, text: &str, x: u32, y: u32, color: Pixel) -> u32 {
    let mut cx = x;
    for ch in text.chars() {
        if draw_char(fb, ch, cx, y, color) {
            cx += 6; // 5px glyph + 1px spacing
        } else {
            cx += 6; // space for unknown chars
        }
    }
    cx - x
}

/// Calculate text width in pixels.
pub fn text_width(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    (text.chars().count() as u32) * 6 - 1 // 5px + 1px gap, minus trailing gap
}

/// Get the 5x7 glyph bitmap for a character. Each u8 represents a row
/// with the 5 most significant bits being the pixel columns.
fn get_glyph(ch: char) -> Option<&'static [u8; 7]> {
    // Minimal ASCII subset covering A-Z, a-z, 0-9, common punctuation
    match ch {
        ' ' => Some(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        '!' => Some(&[0x04, 0x04, 0x04, 0x04, 0x00, 0x04, 0x00]),
        '.' => Some(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00]),
        ',' => Some(&[0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08]),
        ':' => Some(&[0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x00]),
        '-' => Some(&[0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00]),
        '_' => Some(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F]),
        '/' => Some(&[0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00]),
        '(' => Some(&[0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02]),
        ')' => Some(&[0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08]),
        '%' => Some(&[0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03]),
        '0' => Some(&[0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E]),
        '1' => Some(&[0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        '2' => Some(&[0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F]),
        '3' => Some(&[0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E]),
        '4' => Some(&[0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02]),
        '5' => Some(&[0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E]),
        '6' => Some(&[0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E]),
        '7' => Some(&[0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
        '8' => Some(&[0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E]),
        '9' => Some(&[0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C]),
        'A' | 'a' => Some(&[0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        'B' | 'b' => Some(&[0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E]),
        'C' | 'c' => Some(&[0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E]),
        'D' | 'd' => Some(&[0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E]),
        'E' | 'e' => Some(&[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F]),
        'F' | 'f' => Some(&[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10]),
        'G' | 'g' => Some(&[0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F]),
        'H' | 'h' => Some(&[0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        'I' | 'i' => Some(&[0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        'J' | 'j' => Some(&[0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C]),
        'K' | 'k' => Some(&[0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11]),
        'L' | 'l' => Some(&[0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F]),
        'M' | 'm' => Some(&[0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11]),
        'N' | 'n' => Some(&[0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11]),
        'O' | 'o' => Some(&[0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        'P' | 'p' => Some(&[0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10]),
        'Q' | 'q' => Some(&[0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D]),
        'R' | 'r' => Some(&[0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11]),
        'S' | 's' => Some(&[0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E]),
        'T' | 't' => Some(&[0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        'U' | 'u' => Some(&[0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        'V' | 'v' => Some(&[0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04]),
        'W' | 'w' => Some(&[0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11]),
        'X' | 'x' => Some(&[0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11]),
        'Y' | 'y' => Some(&[0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04]),
        'Z' | 'z' => Some(&[0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F]),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Window Decoration
// ---------------------------------------------------------------------------

/// Title bar height in pixels.
pub const TITLEBAR_HEIGHT: u32 = 24;
/// Window border width in pixels.
pub const BORDER_WIDTH: u32 = 1;
/// Button diameter in title bar.
pub const BUTTON_SIZE: u32 = 12;

/// Decoration hit-test result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecorationHit {
    /// Title bar (for dragging).
    TitleBar,
    /// Close button.
    CloseButton,
    /// Minimize button.
    MinimizeButton,
    /// Maximize button.
    MaximizeButton,
    /// Client area (pass through to window content).
    ClientArea,
    /// Border (for resizing).
    Border(ResizeEdge),
    /// Outside the window.
    Outside,
}

/// Which edge/corner is being resized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Render server-side decorations for a window.
pub fn render_decorations(
    fb: &mut Framebuffer,
    rect: &Rectangle,
    title: &str,
    is_active: bool,
    state: &WindowState,
) {
    if *state == WindowState::Fullscreen {
        return; // No decorations in fullscreen
    }

    let titlebar_color = if is_active {
        COLOR_TITLEBAR_ACTIVE
    } else {
        COLOR_TITLEBAR
    };

    // Title bar background
    let titlebar_rect = Rectangle {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: TITLEBAR_HEIGHT,
    };
    fb.fill_rect(&titlebar_rect, titlebar_color);

    // Title text (centered vertically in title bar)
    let text_y = rect.y as u32 + (TITLEBAR_HEIGHT - 7) / 2;
    let max_title_width = rect.width.saturating_sub(80); // Leave room for buttons
    let display_title = if text_width(title) > max_title_width {
        let max_chars = (max_title_width / 6) as usize;
        if max_chars > 2 {
            format!("{}…", &title[..max_chars - 1])
        } else {
            title.to_string()
        }
    } else {
        title.to_string()
    };
    draw_text(fb, &display_title, rect.x as u32 + 8, text_y, COLOR_TEXT);

    // Window control buttons (right-aligned)
    let btn_y = rect.y as u32 + (TITLEBAR_HEIGHT - BUTTON_SIZE) / 2;
    let close_x = rect.x as u32 + rect.width - BUTTON_SIZE - 6;
    let max_x = close_x - BUTTON_SIZE - 4;
    let min_x = max_x - BUTTON_SIZE - 4;

    // Close button (red circle area)
    let close_rect = Rectangle {
        x: close_x as i32,
        y: btn_y as i32,
        width: BUTTON_SIZE,
        height: BUTTON_SIZE,
    };
    fb.fill_rect(&close_rect, COLOR_CLOSE_BTN);

    // Maximize button (green)
    let max_rect = Rectangle {
        x: max_x as i32,
        y: btn_y as i32,
        width: BUTTON_SIZE,
        height: BUTTON_SIZE,
    };
    fb.fill_rect(&max_rect, COLOR_MAXIMIZE_BTN);

    // Minimize button (yellow)
    let min_rect = Rectangle {
        x: min_x as i32,
        y: btn_y as i32,
        width: BUTTON_SIZE,
        height: BUTTON_SIZE,
    };
    fb.fill_rect(&min_rect, COLOR_MINIMIZE_BTN);

    // Window border
    fb.draw_rect_outline(rect, COLOR_BORDER);
}

/// Hit-test a point against window decorations.
pub fn decoration_hit_test(rect: &Rectangle, x: i32, y: i32, state: &WindowState) -> DecorationHit {
    // Check if point is inside the window at all
    if x < rect.x
        || y < rect.y
        || x >= rect.x + rect.width as i32
        || y >= rect.y + rect.height as i32
    {
        return DecorationHit::Outside;
    }

    if *state == WindowState::Fullscreen {
        return DecorationHit::ClientArea;
    }

    let local_x = x - rect.x;
    let local_y = y - rect.y;

    // Title bar region
    if local_y < TITLEBAR_HEIGHT as i32 {
        // Check buttons (right-aligned)
        let close_x = rect.width as i32 - BUTTON_SIZE as i32 - 6;
        let max_x = close_x - BUTTON_SIZE as i32 - 4;
        let min_x = max_x - BUTTON_SIZE as i32 - 4;

        if local_x >= close_x && local_x < close_x + BUTTON_SIZE as i32 {
            return DecorationHit::CloseButton;
        }
        if local_x >= max_x && local_x < max_x + BUTTON_SIZE as i32 {
            return DecorationHit::MaximizeButton;
        }
        if local_x >= min_x && local_x < min_x + BUTTON_SIZE as i32 {
            return DecorationHit::MinimizeButton;
        }

        return DecorationHit::TitleBar;
    }

    // Border region (1px edges)
    if local_x < BORDER_WIDTH as i32
        || local_x >= rect.width as i32 - BORDER_WIDTH as i32
        || local_y >= rect.height as i32 - BORDER_WIDTH as i32
    {
        let on_left = local_x < BORDER_WIDTH as i32;
        let on_right = local_x >= rect.width as i32 - BORDER_WIDTH as i32;
        let on_top = local_y < BORDER_WIDTH as i32;
        let on_bottom = local_y >= rect.height as i32 - BORDER_WIDTH as i32;

        let edge = match (on_left, on_right, on_top, on_bottom) {
            (true, _, true, _) => ResizeEdge::TopLeft,
            (true, _, _, true) => ResizeEdge::BottomLeft,
            (_, true, true, _) => ResizeEdge::TopRight,
            (_, true, _, true) => ResizeEdge::BottomRight,
            (true, _, _, _) => ResizeEdge::Left,
            (_, true, _, _) => ResizeEdge::Right,
            (_, _, true, _) => ResizeEdge::Top,
            (_, _, _, true) => ResizeEdge::Bottom,
            _ => return DecorationHit::ClientArea,
        };
        return DecorationHit::Border(edge);
    }

    DecorationHit::ClientArea
}

// ---------------------------------------------------------------------------
// Scene Graph — Layer-based compositing
// ---------------------------------------------------------------------------

/// Z-order layers for compositing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Desktop wallpaper / background.
    Background = 0,
    /// Normal application windows.
    Normal = 1,
    /// Floating/always-on-top windows.
    Floating = 2,
    /// Desktop panel (taskbar).
    Panel = 3,
    /// HUD overlay (agent status).
    Overlay = 4,
    /// Notifications and popups.
    Notification = 5,
}

/// A renderable surface in the scene graph.
#[derive(Debug, Clone)]
pub struct SceneSurface {
    pub id: SurfaceId,
    pub layer: Layer,
    pub geometry: Rectangle,
    pub visible: bool,
    pub opacity: f32,
    pub title: String,
    pub is_active: bool,
    pub window_state: WindowState,
}

/// Scene graph managing all renderable surfaces.
#[derive(Debug)]
pub struct SceneGraph {
    surfaces: HashMap<SurfaceId, SceneSurface>,
    /// Cached z-ordered list (rebuilt on change).
    z_order: Vec<SurfaceId>,
    dirty: bool,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self {
            surfaces: HashMap::new(),
            z_order: Vec::new(),
            dirty: true,
        }
    }

    /// Add or update a surface.
    pub fn add_surface(&mut self, surface: SceneSurface) {
        self.surfaces.insert(surface.id, surface);
        self.dirty = true;
    }

    /// Remove a surface.
    pub fn remove_surface(&mut self, id: SurfaceId) -> Option<SceneSurface> {
        let removed = self.surfaces.remove(&id);
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    /// Get a surface by ID.
    pub fn get_surface(&self, id: SurfaceId) -> Option<&SceneSurface> {
        self.surfaces.get(&id)
    }

    /// Get a mutable surface by ID.
    pub fn get_surface_mut(&mut self, id: SurfaceId) -> Option<&mut SceneSurface> {
        self.surfaces.get_mut(&id)
    }

    /// Get surfaces in z-order (bottom to top) for rendering.
    pub fn surfaces_in_order(&mut self) -> Vec<&SceneSurface> {
        if self.dirty {
            self.rebuild_z_order();
        }
        self.z_order
            .iter()
            .filter_map(|id| self.surfaces.get(id))
            .filter(|s| s.visible)
            .collect()
    }

    /// Find the topmost surface at a given point (for input routing).
    pub fn surface_at(&mut self, x: i32, y: i32) -> Option<SurfaceId> {
        if self.dirty {
            self.rebuild_z_order();
        }

        // Iterate in reverse z-order (top to bottom)
        for id in self.z_order.iter().rev() {
            if let Some(surface) = self.surfaces.get(id) {
                if !surface.visible {
                    continue;
                }
                let r = &surface.geometry;
                if x >= r.x && y >= r.y && x < r.x + r.width as i32 && y < r.y + r.height as i32 {
                    return Some(*id);
                }
            }
        }
        None
    }

    /// Move a surface to the front of its layer.
    pub fn raise_surface(&mut self, id: SurfaceId) {
        // Mark dirty so z_order is rebuilt with this surface last in its layer group
        if self.surfaces.contains_key(&id) {
            self.dirty = true;
        }
    }

    /// Count visible surfaces.
    pub fn visible_count(&self) -> usize {
        self.surfaces.values().filter(|s| s.visible).count()
    }

    /// Total surface count.
    pub fn total_count(&self) -> usize {
        self.surfaces.len()
    }

    fn rebuild_z_order(&mut self) {
        let mut ordered: Vec<_> = self.surfaces.values().collect();
        ordered.sort_by(|a, b| a.layer.cmp(&b.layer));
        self.z_order = ordered.iter().map(|s| s.id).collect();
        self.dirty = false;
        debug!("Rebuilt scene z-order: {} surfaces", self.z_order.len());
    }
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Desktop Renderer
// ---------------------------------------------------------------------------

/// High-level renderer that composites the scene into a framebuffer.
#[derive(Debug)]
pub struct DesktopRenderer {
    /// Front buffer (displayed).
    pub front: Framebuffer,
    /// Back buffer (being rendered).
    pub back: Framebuffer,
    /// Damage tracker.
    pub damage: DamageTracker,
    /// Per-window content buffers.
    window_buffers: HashMap<SurfaceId, Framebuffer>,
    /// Optional high-contrast accessibility theme.
    pub high_contrast: Option<HighContrastTheme>,
}

impl DesktopRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            front: Framebuffer::new(width, height, COLOR_BG_DARK),
            back: Framebuffer::new(width, height, COLOR_BG_DARK),
            damage: DamageTracker::new(width, height),
            window_buffers: HashMap::new(),
            high_contrast: None,
        }
    }

    /// Set or clear the high-contrast accessibility theme.
    pub fn set_high_contrast(&mut self, theme: Option<HighContrastTheme>) {
        self.high_contrast = theme;
    }

    /// Submit a window's content buffer.
    pub fn submit_buffer(&mut self, id: SurfaceId, buffer: Framebuffer) {
        self.window_buffers.insert(id, buffer);
    }

    /// Remove a window's buffer.
    pub fn remove_buffer(&mut self, id: SurfaceId) {
        self.window_buffers.remove(&id);
    }

    /// Composite the scene into the back buffer, then swap.
    pub fn render_frame(&mut self, scene: &mut SceneGraph) {
        // Clear back buffer
        self.back.clear(COLOR_BG_DARK);

        // Collect surface snapshots for rendering. We need owned data because
        // render_decorations borrows self.back mutably (can't hold scene refs).
        let surfaces: Vec<SceneSurface> = scene
            .surfaces_in_order()
            .iter()
            .map(|s| (*s).clone())
            .collect();

        for surface in &surfaces {
            match surface.layer {
                Layer::Background => {
                    // Background just fills with dark color (already done by clear)
                }
                Layer::Normal | Layer::Floating => {
                    // Render window decorations
                    render_decorations(
                        &mut self.back,
                        &surface.geometry,
                        &surface.title,
                        surface.is_active,
                        &surface.window_state,
                    );

                    // Render window content if available
                    if let Some(buf) = self.window_buffers.get(&surface.id) {
                        let content_x = surface.geometry.x + BORDER_WIDTH as i32;
                        let content_y = surface.geometry.y + TITLEBAR_HEIGHT as i32;
                        self.back.blit(buf, content_x, content_y);
                    }
                }
                Layer::Panel => {
                    // Panel background
                    self.back.fill_rect(&surface.geometry, COLOR_PANEL_BG);
                    draw_text(
                        &mut self.back,
                        &surface.title,
                        surface.geometry.x as u32 + 8,
                        surface.geometry.y as u32 + 6,
                        COLOR_TEXT,
                    );
                }
                Layer::Overlay => {
                    // Semi-transparent overlay
                    self.back.fill_rect(&surface.geometry, COLOR_HUD_BG);
                    draw_text(
                        &mut self.back,
                        &surface.title,
                        surface.geometry.x as u32 + 4,
                        surface.geometry.y as u32 + 4,
                        COLOR_ACCENT,
                    );
                }
                Layer::Notification => {
                    self.back.fill_rect(&surface.geometry, COLOR_PANEL_BG);
                    self.back.draw_rect_outline(&surface.geometry, COLOR_ACCENT);
                    draw_text(
                        &mut self.back,
                        &surface.title,
                        surface.geometry.x as u32 + 8,
                        surface.geometry.y as u32 + 8,
                        COLOR_TEXT,
                    );
                }
            }
        }

        // Swap buffers
        std::mem::swap(&mut self.front, &mut self.back);
        self.damage.damage_full();
    }

    /// Get the current front buffer for display.
    pub fn front_buffer(&self) -> &Framebuffer {
        &self.front
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_argb_roundtrip() {
        let p = argb(255, 128, 64, 32);
        let (a, r, g, b) = decompose(p);
        assert_eq!((a, r, g, b), (255, 128, 64, 32));
    }

    #[test]
    fn test_argb_black_white() {
        assert_eq!(decompose(COLOR_BLACK), (255, 0, 0, 0));
        assert_eq!(decompose(COLOR_WHITE), (255, 255, 255, 255));
    }

    #[test]
    fn test_blend_opaque_over() {
        let result = blend(COLOR_WHITE, COLOR_BLACK);
        assert_eq!(result, COLOR_WHITE);
    }

    #[test]
    fn test_blend_transparent_over() {
        let result = blend(COLOR_TRANSPARENT, COLOR_BLACK);
        assert_eq!(result, COLOR_BLACK);
    }

    #[test]
    fn test_blend_semi_transparent() {
        let src = argb(128, 255, 0, 0);
        let dst = argb(255, 0, 0, 255);
        let result = blend(src, dst);
        let (a, r, _g, b) = decompose(result);
        assert_eq!(a, 255);
        assert!(r > 128); // Red should be mixed
        assert!(b > 0 && b < 255); // Blue should be partially visible
    }

    #[test]
    fn test_framebuffer_new() {
        let fb = Framebuffer::new(100, 50, COLOR_BLACK);
        assert_eq!(fb.width, 100);
        assert_eq!(fb.height, 50);
        assert_eq!(fb.pixels.len(), 5000);
        assert_eq!(fb.get(0, 0), Some(COLOR_BLACK));
    }

    #[test]
    fn test_framebuffer_set_get() {
        let mut fb = Framebuffer::new(10, 10, COLOR_BLACK);
        fb.set(5, 5, COLOR_WHITE);
        assert_eq!(fb.get(5, 5), Some(COLOR_WHITE));
        assert_eq!(fb.get(0, 0), Some(COLOR_BLACK));
    }

    #[test]
    fn test_framebuffer_out_of_bounds() {
        let mut fb = Framebuffer::new(10, 10, COLOR_BLACK);
        assert_eq!(fb.get(10, 10), None);
        fb.set(10, 10, COLOR_WHITE); // Should not panic
    }

    #[test]
    fn test_framebuffer_fill_rect() {
        let mut fb = Framebuffer::new(100, 100, COLOR_BLACK);
        let rect = Rectangle {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        };
        fb.fill_rect(&rect, COLOR_WHITE);
        assert_eq!(fb.get(10, 10), Some(COLOR_WHITE));
        assert_eq!(fb.get(29, 29), Some(COLOR_WHITE));
        assert_eq!(fb.get(30, 30), Some(COLOR_BLACK));
        assert_eq!(fb.get(9, 9), Some(COLOR_BLACK));
    }

    #[test]
    fn test_framebuffer_draw_rect_outline() {
        let mut fb = Framebuffer::new(100, 100, COLOR_BLACK);
        let rect = Rectangle {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        };
        fb.draw_rect_outline(&rect, COLOR_WHITE);
        assert_eq!(fb.get(10, 10), Some(COLOR_WHITE)); // top-left corner
        assert_eq!(fb.get(15, 10), Some(COLOR_WHITE)); // top edge
        assert_eq!(fb.get(15, 15), Some(COLOR_BLACK)); // interior
    }

    #[test]
    fn test_framebuffer_blit() {
        let mut dst = Framebuffer::new(100, 100, COLOR_BLACK);
        let src = Framebuffer::new(10, 10, COLOR_WHITE);
        dst.blit(&src, 5, 5);
        assert_eq!(dst.get(5, 5), Some(COLOR_WHITE));
        assert_eq!(dst.get(14, 14), Some(COLOR_WHITE));
        assert_eq!(dst.get(4, 4), Some(COLOR_BLACK));
    }

    #[test]
    fn test_framebuffer_blit_negative_offset() {
        let mut dst = Framebuffer::new(20, 20, COLOR_BLACK);
        let src = Framebuffer::new(10, 10, COLOR_WHITE);
        dst.blit(&src, -5, -5);
        assert_eq!(dst.get(0, 0), Some(COLOR_WHITE));
        assert_eq!(dst.get(4, 4), Some(COLOR_WHITE));
        assert_eq!(dst.get(5, 5), Some(COLOR_BLACK));
    }

    #[test]
    fn test_framebuffer_clear() {
        let mut fb = Framebuffer::new(10, 10, COLOR_BLACK);
        fb.set(5, 5, COLOR_WHITE);
        fb.clear(COLOR_BLACK);
        assert_eq!(fb.get(5, 5), Some(COLOR_BLACK));
    }

    #[test]
    fn test_framebuffer_as_bytes() {
        let fb = Framebuffer::new(2, 2, COLOR_BLACK);
        let bytes = fb.as_bytes();
        assert_eq!(bytes.len(), 16); // 4 pixels * 4 bytes
    }

    #[test]
    fn test_framebuffer_clone() {
        let fb = Framebuffer::new(10, 10, COLOR_WHITE);
        let fb2 = fb.clone();
        assert_eq!(fb.pixels, fb2.pixels);
    }

    #[test]
    fn test_damage_tracker_new() {
        let dt = DamageTracker::new(1920, 1080);
        assert!(dt.has_damage()); // First frame always damaged
        assert!(dt.full_damage);
    }

    #[test]
    fn test_damage_tracker_flush() {
        let mut dt = DamageTracker::new(1920, 1080);
        let bounds = dt.flush();
        assert_eq!(bounds.width, 1920);
        assert_eq!(bounds.height, 1080);
        assert!(!dt.has_damage());
    }

    #[test]
    fn test_damage_tracker_add_damage() {
        let mut dt = DamageTracker::new(1920, 1080);
        dt.flush(); // Clear initial full damage
        assert!(!dt.has_damage());

        dt.add_damage(Rectangle {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        });
        assert!(dt.has_damage());
        assert_eq!(dt.region_count(), 1);

        let bounds = dt.get_damage_bounds();
        assert_eq!(bounds.x, 10);
        assert_eq!(bounds.y, 20);
        assert_eq!(bounds.width, 100);
        assert_eq!(bounds.height, 50);
    }

    #[test]
    fn test_damage_tracker_multiple_regions() {
        let mut dt = DamageTracker::new(1920, 1080);
        dt.flush();

        dt.add_damage(Rectangle {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        });
        dt.add_damage(Rectangle {
            x: 500,
            y: 500,
            width: 200,
            height: 200,
        });
        assert_eq!(dt.region_count(), 2);

        let bounds = dt.get_damage_bounds();
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.width, 700);
        assert_eq!(bounds.height, 700);
    }

    #[test]
    fn test_damage_tracker_full_damage() {
        let mut dt = DamageTracker::new(1920, 1080);
        dt.flush();
        dt.damage_full();
        assert!(dt.has_damage());
        assert_eq!(dt.region_count(), 1);
    }

    #[test]
    fn test_draw_char() {
        let mut fb = Framebuffer::new(20, 20, COLOR_BLACK);
        assert!(draw_char(&mut fb, 'A', 0, 0, COLOR_WHITE));
        // At least some pixels should be white
        let white_count = fb.pixels.iter().filter(|&&p| p == COLOR_WHITE).count();
        assert!(white_count > 5);
    }

    #[test]
    fn test_draw_char_unknown() {
        let mut fb = Framebuffer::new(20, 20, COLOR_BLACK);
        assert!(!draw_char(&mut fb, '★', 0, 0, COLOR_WHITE));
    }

    #[test]
    fn test_draw_text() {
        let mut fb = Framebuffer::new(200, 20, COLOR_BLACK);
        let width = draw_text(&mut fb, "Hello", 0, 0, COLOR_WHITE);
        assert!(width > 0);
        assert_eq!(width, 5 * 6); // 5 chars * 6px each
    }

    #[test]
    fn test_text_width() {
        assert_eq!(text_width(""), 0);
        assert_eq!(text_width("A"), 5); // 5px for single char (no trailing gap)
        assert_eq!(text_width("AB"), 11); // 5 + 1 + 5
    }

    #[test]
    fn test_decoration_hit_test_outside() {
        let rect = Rectangle {
            x: 100,
            y: 100,
            width: 200,
            height: 150,
        };
        assert_eq!(
            decoration_hit_test(&rect, 50, 50, &WindowState::Normal),
            DecorationHit::Outside
        );
    }

    #[test]
    fn test_decoration_hit_test_titlebar() {
        let rect = Rectangle {
            x: 100,
            y: 100,
            width: 200,
            height: 150,
        };
        assert_eq!(
            decoration_hit_test(&rect, 150, 110, &WindowState::Normal),
            DecorationHit::TitleBar
        );
    }

    #[test]
    fn test_decoration_hit_test_client_area() {
        let rect = Rectangle {
            x: 100,
            y: 100,
            width: 200,
            height: 150,
        };
        assert_eq!(
            decoration_hit_test(&rect, 150, 140, &WindowState::Normal),
            DecorationHit::ClientArea
        );
    }

    #[test]
    fn test_decoration_hit_test_fullscreen() {
        let rect = Rectangle {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        // Everything is client area in fullscreen
        assert_eq!(
            decoration_hit_test(&rect, 100, 10, &WindowState::Fullscreen),
            DecorationHit::ClientArea
        );
    }

    #[test]
    fn test_decoration_hit_test_close_button() {
        let rect = Rectangle {
            x: 100,
            y: 100,
            width: 200,
            height: 150,
        };
        let close_x = 100 + 200 - BUTTON_SIZE as i32 - 6;
        let btn_y = 100 + (TITLEBAR_HEIGHT as i32 - BUTTON_SIZE as i32) / 2;
        assert_eq!(
            decoration_hit_test(&rect, close_x + 2, btn_y + 2, &WindowState::Normal),
            DecorationHit::CloseButton
        );
    }

    #[test]
    fn test_decoration_hit_test_border_bottom() {
        let rect = Rectangle {
            x: 100,
            y: 100,
            width: 200,
            height: 150,
        };
        assert_eq!(
            decoration_hit_test(&rect, 150, 249, &WindowState::Normal),
            DecorationHit::Border(ResizeEdge::Bottom)
        );
    }

    #[test]
    fn test_render_decorations_normal() {
        let mut fb = Framebuffer::new(400, 300, COLOR_BG_DARK);
        let rect = Rectangle {
            x: 10,
            y: 10,
            width: 200,
            height: 150,
        };
        render_decorations(&mut fb, &rect, "Test Window", true, &WindowState::Normal);
        // Title bar should have active color
        assert_ne!(fb.get(15, 15), Some(COLOR_BG_DARK));
    }

    #[test]
    fn test_render_decorations_fullscreen_noop() {
        let mut fb = Framebuffer::new(400, 300, COLOR_BG_DARK);
        let rect = Rectangle {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        render_decorations(&mut fb, &rect, "FS", true, &WindowState::Fullscreen);
        // Should not draw decorations — pixel at (0,0) remains background
        assert_eq!(fb.get(0, 0), Some(COLOR_BG_DARK));
    }

    #[test]
    fn test_scene_graph_add_remove() {
        let mut sg = SceneGraph::new();
        let id = Uuid::new_v4();
        sg.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            visible: true,
            opacity: 1.0,
            title: "Test".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });
        assert_eq!(sg.total_count(), 1);
        assert_eq!(sg.visible_count(), 1);

        sg.remove_surface(id);
        assert_eq!(sg.total_count(), 0);
    }

    #[test]
    fn test_scene_graph_z_order() {
        let mut sg = SceneGraph::new();

        let overlay_id = Uuid::new_v4();
        let normal_id = Uuid::new_v4();

        sg.add_surface(SceneSurface {
            id: overlay_id,
            layer: Layer::Overlay,
            geometry: Rectangle::default(),
            visible: true,
            opacity: 1.0,
            title: "Overlay".to_string(),
            is_active: false,
            window_state: WindowState::Normal,
        });
        sg.add_surface(SceneSurface {
            id: normal_id,
            layer: Layer::Normal,
            geometry: Rectangle::default(),
            visible: true,
            opacity: 1.0,
            title: "Window".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });

        let ordered = sg.surfaces_in_order();
        assert_eq!(ordered.len(), 2);
        // Normal should come before Overlay
        assert_eq!(ordered[0].layer, Layer::Normal);
        assert_eq!(ordered[1].layer, Layer::Overlay);
    }

    #[test]
    fn test_scene_graph_surface_at() {
        let mut sg = SceneGraph::new();
        let id = Uuid::new_v4();
        sg.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle {
                x: 50,
                y: 50,
                width: 100,
                height: 100,
            },
            visible: true,
            opacity: 1.0,
            title: "W".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });

        assert_eq!(sg.surface_at(75, 75), Some(id));
        assert_eq!(sg.surface_at(0, 0), None);
        assert_eq!(sg.surface_at(150, 150), None);
    }

    #[test]
    fn test_scene_graph_hidden_surface() {
        let mut sg = SceneGraph::new();
        let id = Uuid::new_v4();
        sg.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            visible: false,
            opacity: 1.0,
            title: "Hidden".to_string(),
            is_active: false,
            window_state: WindowState::Minimized,
        });

        assert_eq!(sg.visible_count(), 0);
        assert_eq!(sg.total_count(), 1);
        assert_eq!(sg.surface_at(50, 50), None);
    }

    #[test]
    fn test_scene_graph_overlapping_surfaces() {
        let mut sg = SceneGraph::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        sg.add_surface(SceneSurface {
            id: id1,
            layer: Layer::Normal,
            geometry: Rectangle {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            visible: true,
            opacity: 1.0,
            title: "Back".to_string(),
            is_active: false,
            window_state: WindowState::Normal,
        });
        sg.add_surface(SceneSurface {
            id: id2,
            layer: Layer::Floating,
            geometry: Rectangle {
                x: 50,
                y: 50,
                width: 100,
                height: 100,
            },
            visible: true,
            opacity: 1.0,
            title: "Front".to_string(),
            is_active: true,
            window_state: WindowState::Floating,
        });

        // Overlap region should hit the topmost (Floating)
        assert_eq!(sg.surface_at(75, 75), Some(id2));
        // Non-overlap region of back window
        assert_eq!(sg.surface_at(10, 10), Some(id1));
    }

    #[test]
    fn test_desktop_renderer_new() {
        let renderer = DesktopRenderer::new(1920, 1080);
        assert_eq!(renderer.front.width, 1920);
        assert_eq!(renderer.front.height, 1080);
    }

    #[test]
    fn test_desktop_renderer_render_frame() {
        let mut renderer = DesktopRenderer::new(640, 480);
        let mut scene = SceneGraph::new();

        let id = Uuid::new_v4();
        scene.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle {
                x: 50,
                y: 50,
                width: 200,
                height: 150,
            },
            visible: true,
            opacity: 1.0,
            title: "Test Window".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });

        renderer.render_frame(&mut scene);
        let front = renderer.front_buffer();
        // The title bar region should not be background color
        assert_ne!(front.get(60, 55), Some(COLOR_BG_DARK));
    }

    #[test]
    fn test_desktop_renderer_submit_buffer() {
        let mut renderer = DesktopRenderer::new(640, 480);
        let id = Uuid::new_v4();
        let buf = Framebuffer::new(100, 100, COLOR_WHITE);
        renderer.submit_buffer(id, buf);

        let mut scene = SceneGraph::new();
        scene.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle {
                x: 0,
                y: 0,
                width: 102,
                height: 125,
            },
            visible: true,
            opacity: 1.0,
            title: "W".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });

        renderer.render_frame(&mut scene);
        // Content area should have white pixels
        let content_y = TITLEBAR_HEIGHT + 1;
        let pixel = renderer.front_buffer().get(2, content_y);
        assert_eq!(pixel, Some(COLOR_WHITE));
    }

    #[test]
    fn test_layer_ordering() {
        assert!(Layer::Background < Layer::Normal);
        assert!(Layer::Normal < Layer::Floating);
        assert!(Layer::Floating < Layer::Panel);
        assert!(Layer::Panel < Layer::Overlay);
        assert!(Layer::Overlay < Layer::Notification);
    }

    #[test]
    fn test_resize_edge_variants() {
        let edges = [
            ResizeEdge::Top,
            ResizeEdge::Bottom,
            ResizeEdge::Left,
            ResizeEdge::Right,
            ResizeEdge::TopLeft,
            ResizeEdge::TopRight,
            ResizeEdge::BottomLeft,
            ResizeEdge::BottomRight,
        ];
        assert_eq!(edges.len(), 8);
    }

    #[test]
    fn test_framebuffer_fill_rect_clipped() {
        let mut fb = Framebuffer::new(10, 10, COLOR_BLACK);
        // Rectangle extends beyond framebuffer
        let rect = Rectangle {
            x: 5,
            y: 5,
            width: 100,
            height: 100,
        };
        fb.fill_rect(&rect, COLOR_WHITE);
        assert_eq!(fb.get(5, 5), Some(COLOR_WHITE));
        assert_eq!(fb.get(9, 9), Some(COLOR_WHITE));
    }

    #[test]
    fn test_blit_clipped() {
        let mut dst = Framebuffer::new(100, 100, COLOR_BLACK);
        let src = Framebuffer::new(50, 50, COLOR_WHITE);
        let clip = Rectangle {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        };
        dst.blit_clipped(&src, 0, 0, &clip);
        assert_eq!(dst.get(10, 10), Some(COLOR_WHITE));
        assert_eq!(dst.get(29, 29), Some(COLOR_WHITE));
        assert_eq!(dst.get(5, 5), Some(COLOR_BLACK)); // Outside clip
        assert_eq!(dst.get(30, 30), Some(COLOR_BLACK)); // Outside clip
    }

    #[test]
    fn test_scene_graph_raise_surface() {
        let mut sg = SceneGraph::new();
        let id = Uuid::new_v4();
        sg.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle::default(),
            visible: true,
            opacity: 1.0,
            title: "W".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });
        sg.raise_surface(id);
        assert!(sg.dirty);
    }

    #[test]
    fn test_scene_graph_get_surface() {
        let mut sg = SceneGraph::new();
        let id = Uuid::new_v4();
        sg.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle::default(),
            visible: true,
            opacity: 1.0,
            title: "W".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });
        assert!(sg.get_surface(id).is_some());
        assert!(sg.get_surface(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_scene_graph_get_surface_mut() {
        let mut sg = SceneGraph::new();
        let id = Uuid::new_v4();
        sg.add_surface(SceneSurface {
            id,
            layer: Layer::Normal,
            geometry: Rectangle::default(),
            visible: true,
            opacity: 1.0,
            title: "Original".to_string(),
            is_active: true,
            window_state: WindowState::Normal,
        });

        if let Some(s) = sg.get_surface_mut(id) {
            s.title = "Modified".to_string();
        }
        assert_eq!(sg.get_surface(id).unwrap().title, "Modified");
    }

    #[test]
    fn test_all_glyphs_defined() {
        // Verify all expected characters have glyphs
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 !.,-_/:()%".chars() {
            assert!(get_glyph(ch).is_some(), "Missing glyph for '{}'", ch);
        }
    }

    #[test]
    fn test_glyph_lowercase_maps_to_uppercase() {
        for ch in 'a'..='z' {
            let upper = ch.to_ascii_uppercase();
            assert_eq!(
                get_glyph(ch),
                get_glyph(upper),
                "Lowercase '{}' should map to uppercase '{}'",
                ch,
                upper
            );
        }
    }
}
