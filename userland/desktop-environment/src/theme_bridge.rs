//! High-contrast theme synchronisation bridge: propagates AGNOS
//! `HighContrastTheme` to Flutter's `ThemeData` via a platform channel protocol.

use serde::{Deserialize, Serialize};

use crate::accessibility::HighContrastTheme;

// ============================================================================
// Flutter theme data
// ============================================================================

/// A serialisable representation of Flutter's `ThemeData`, transmitted over the
/// platform channel so the Flutter UI can adopt the AGNOS theme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlutterThemeData {
    pub brightness: String,
    pub primary_color: String,
    pub background_color: String,
    pub surface_color: String,
    pub error_color: String,
    pub on_primary: String,
    pub on_background: String,
    pub on_surface: String,
    pub on_error: String,
    pub font_size_multiplier: f32,
    pub use_material3: bool,
}

// ============================================================================
// Platform channel message
// ============================================================================

/// A message in Flutter's platform channel JSON format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformChannelMessage {
    pub channel: String,
    pub method: String,
    pub args: serde_json::Value,
}

// ============================================================================
// Theme overrides
// ============================================================================

/// Optional overrides that can be layered on top of a `FlutterThemeData`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeOverrides {
    pub brightness: Option<String>,
    pub primary_color: Option<String>,
    pub background_color: Option<String>,
    pub surface_color: Option<String>,
    pub error_color: Option<String>,
    pub on_primary: Option<String>,
    pub on_background: Option<String>,
    pub on_surface: Option<String>,
    pub on_error: Option<String>,
    pub font_size_multiplier: Option<f32>,
    pub use_material3: Option<bool>,
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a 0xAARRGGBB `u32` colour to a `"#RRGGBB"` hex string.
pub fn color_u32_to_hex(color: u32) -> String {
    let r = (color >> 16) & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = color & 0xFF;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

/// Parse a `"#RRGGBB"` hex string to `0xFF_RRGGBB`.
pub fn color_hex_to_u32(hex: &str) -> Result<u32, String> {
    let hex = hex.strip_prefix('#').ok_or_else(|| {
        format!("Expected '#' prefix in color string: {}", hex)
    })?;
    if hex.len() != 6 {
        return Err(format!(
            "Expected 6 hex digits after '#', got {}: {}",
            hex.len(),
            hex
        ));
    }
    let rgb = u32::from_str_radix(hex, 16)
        .map_err(|e| format!("Invalid hex color: {}", e))?;
    Ok(0xFF000000 | rgb)
}

// ============================================================================
// Theme bridge
// ============================================================================

/// Converts AGNOS `HighContrastTheme` to Flutter theme structures and platform
/// channel messages.
pub struct ThemeBridge;

impl ThemeBridge {
    /// Convert an AGNOS high-contrast theme to a Flutter `ThemeData` representation.
    pub fn convert_to_flutter(theme: &HighContrastTheme) -> FlutterThemeData {
        let bg = color_u32_to_hex(theme.background);
        let fg = color_u32_to_hex(theme.foreground);
        let accent = color_u32_to_hex(theme.accent);
        let error = color_u32_to_hex(theme.error);

        // Determine brightness from the background luminance.
        let bg_r = (theme.background >> 16) & 0xFF;
        let bg_g = (theme.background >> 8) & 0xFF;
        let bg_b = theme.background & 0xFF;
        let luminance = 0.299 * bg_r as f64 + 0.587 * bg_g as f64 + 0.114 * bg_b as f64;
        let brightness = if luminance > 127.5 { "light" } else { "dark" };

        // For high-contrast themes, on-* colours are the contrasting colour.
        let on_primary = fg.clone();
        let on_background = fg.clone();
        let on_surface = fg.clone();
        // on_error: use background (white/black) for readability against error colour.
        let on_error = bg.clone();

        FlutterThemeData {
            brightness: brightness.to_string(),
            primary_color: accent,
            background_color: bg.clone(),
            surface_color: bg,
            error_color: error,
            on_primary,
            on_background,
            on_surface,
            on_error,
            font_size_multiplier: theme.font_scale,
            use_material3: true,
        }
    }

    /// Wrap a `FlutterThemeData` in a Flutter platform channel message.
    pub fn create_platform_message(theme_data: &FlutterThemeData) -> PlatformChannelMessage {
        PlatformChannelMessage {
            channel: "agnos/theme".to_string(),
            method: "setTheme".to_string(),
            args: serde_json::to_value(theme_data).expect("FlutterThemeData is always serialisable"),
        }
    }

    /// Apply optional overrides on top of a base theme.
    pub fn apply_theme_override(
        base: &FlutterThemeData,
        overrides: &ThemeOverrides,
    ) -> FlutterThemeData {
        FlutterThemeData {
            brightness: overrides
                .brightness
                .clone()
                .unwrap_or_else(|| base.brightness.clone()),
            primary_color: overrides
                .primary_color
                .clone()
                .unwrap_or_else(|| base.primary_color.clone()),
            background_color: overrides
                .background_color
                .clone()
                .unwrap_or_else(|| base.background_color.clone()),
            surface_color: overrides
                .surface_color
                .clone()
                .unwrap_or_else(|| base.surface_color.clone()),
            error_color: overrides
                .error_color
                .clone()
                .unwrap_or_else(|| base.error_color.clone()),
            on_primary: overrides
                .on_primary
                .clone()
                .unwrap_or_else(|| base.on_primary.clone()),
            on_background: overrides
                .on_background
                .clone()
                .unwrap_or_else(|| base.on_background.clone()),
            on_surface: overrides
                .on_surface
                .clone()
                .unwrap_or_else(|| base.on_surface.clone()),
            on_error: overrides
                .on_error
                .clone()
                .unwrap_or_else(|| base.on_error.clone()),
            font_size_multiplier: overrides
                .font_size_multiplier
                .unwrap_or(base.font_size_multiplier),
            use_material3: overrides
                .use_material3
                .unwrap_or(base.use_material3),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- color_u32_to_hex ---------------------------------------------------

    #[test]
    fn test_color_u32_to_hex_white() {
        assert_eq!(color_u32_to_hex(0xFFFFFFFF), "#FFFFFF");
    }

    #[test]
    fn test_color_u32_to_hex_black() {
        assert_eq!(color_u32_to_hex(0xFF000000), "#000000");
    }

    #[test]
    fn test_color_u32_to_hex_red() {
        assert_eq!(color_u32_to_hex(0xFFFF0000), "#FF0000");
    }

    #[test]
    fn test_color_u32_to_hex_ignores_alpha() {
        // Different alpha, same RGB — should produce the same hex.
        assert_eq!(color_u32_to_hex(0x00FF8800), "#FF8800");
        assert_eq!(color_u32_to_hex(0xFFFF8800), "#FF8800");
    }

    // -- color_hex_to_u32 ---------------------------------------------------

    #[test]
    fn test_color_hex_to_u32_white() {
        assert_eq!(color_hex_to_u32("#FFFFFF").unwrap(), 0xFFFFFFFF);
    }

    #[test]
    fn test_color_hex_to_u32_black() {
        assert_eq!(color_hex_to_u32("#000000").unwrap(), 0xFF000000);
    }

    #[test]
    fn test_color_hex_to_u32_blue() {
        assert_eq!(color_hex_to_u32("#0000FF").unwrap(), 0xFF0000FF);
    }

    #[test]
    fn test_color_hex_to_u32_missing_hash() {
        assert!(color_hex_to_u32("FFFFFF").is_err());
    }

    #[test]
    fn test_color_hex_to_u32_too_short() {
        assert!(color_hex_to_u32("#FFF").is_err());
    }

    #[test]
    fn test_color_hex_to_u32_invalid_chars() {
        assert!(color_hex_to_u32("#GGGGGG").is_err());
    }

    // -- roundtrip ----------------------------------------------------------

    #[test]
    fn test_color_roundtrip() {
        let original = 0xFF12AB34;
        let hex = color_u32_to_hex(original);
        let back = color_hex_to_u32(&hex).unwrap();
        assert_eq!(back, original);
    }

    // -- theme conversion ---------------------------------------------------

    #[test]
    fn test_convert_light_high_contrast() {
        let theme = HighContrastTheme::default_high_contrast();
        let flutter = ThemeBridge::convert_to_flutter(&theme);
        assert_eq!(flutter.brightness, "light");
        assert_eq!(flutter.background_color, "#FFFFFF");
        assert_eq!(flutter.on_background, "#000000");
        assert_eq!(flutter.primary_color, "#0000FF"); // accent = blue
        assert_eq!(flutter.error_color, "#FF0000");
        assert_eq!(flutter.font_size_multiplier, 1.25);
        assert!(flutter.use_material3);
    }

    #[test]
    fn test_convert_dark_high_contrast() {
        let theme = HighContrastTheme::default_dark_high_contrast();
        let flutter = ThemeBridge::convert_to_flutter(&theme);
        assert_eq!(flutter.brightness, "dark");
        assert_eq!(flutter.background_color, "#000000");
        assert_eq!(flutter.on_background, "#FFFFFF");
        assert_eq!(flutter.primary_color, "#00CCFF"); // accent = cyan
    }

    // -- platform message ---------------------------------------------------

    #[test]
    fn test_platform_message_channel() {
        let theme = HighContrastTheme::default_high_contrast();
        let flutter = ThemeBridge::convert_to_flutter(&theme);
        let msg = ThemeBridge::create_platform_message(&flutter);
        assert_eq!(msg.channel, "agnos/theme");
        assert_eq!(msg.method, "setTheme");
        // args should be a JSON object with all FlutterThemeData fields
        assert!(msg.args.is_object());
        let obj = msg.args.as_object().unwrap();
        assert!(obj.contains_key("brightness"));
        assert!(obj.contains_key("primary_color"));
        assert!(obj.contains_key("font_size_multiplier"));
    }

    #[test]
    fn test_platform_message_roundtrip_json() {
        let theme = HighContrastTheme::default_high_contrast();
        let flutter = ThemeBridge::convert_to_flutter(&theme);
        let msg = ThemeBridge::create_platform_message(&flutter);
        // Deserialise the args back to FlutterThemeData
        let restored: FlutterThemeData =
            serde_json::from_value(msg.args).expect("should deserialise");
        assert_eq!(restored, flutter);
    }

    // -- theme overrides ----------------------------------------------------

    #[test]
    fn test_apply_empty_overrides() {
        let theme = HighContrastTheme::default_high_contrast();
        let base = ThemeBridge::convert_to_flutter(&theme);
        let overrides = ThemeOverrides::default();
        let result = ThemeBridge::apply_theme_override(&base, &overrides);
        assert_eq!(result, base);
    }

    #[test]
    fn test_apply_partial_overrides() {
        let theme = HighContrastTheme::default_high_contrast();
        let base = ThemeBridge::convert_to_flutter(&theme);
        let overrides = ThemeOverrides {
            primary_color: Some("#FF00FF".to_string()),
            font_size_multiplier: Some(2.0),
            ..Default::default()
        };
        let result = ThemeBridge::apply_theme_override(&base, &overrides);
        assert_eq!(result.primary_color, "#FF00FF");
        assert_eq!(result.font_size_multiplier, 2.0);
        // Unchanged fields should be preserved
        assert_eq!(result.brightness, base.brightness);
        assert_eq!(result.background_color, base.background_color);
    }

    #[test]
    fn test_apply_full_overrides() {
        let theme = HighContrastTheme::default_high_contrast();
        let base = ThemeBridge::convert_to_flutter(&theme);
        let overrides = ThemeOverrides {
            brightness: Some("dark".to_string()),
            primary_color: Some("#111111".to_string()),
            background_color: Some("#222222".to_string()),
            surface_color: Some("#333333".to_string()),
            error_color: Some("#444444".to_string()),
            on_primary: Some("#555555".to_string()),
            on_background: Some("#666666".to_string()),
            on_surface: Some("#777777".to_string()),
            on_error: Some("#888888".to_string()),
            font_size_multiplier: Some(3.0),
            use_material3: Some(false),
        };
        let result = ThemeBridge::apply_theme_override(&base, &overrides);
        assert_eq!(result.brightness, "dark");
        assert_eq!(result.primary_color, "#111111");
        assert_eq!(result.background_color, "#222222");
        assert_eq!(result.font_size_multiplier, 3.0);
        assert!(!result.use_material3);
    }
}
