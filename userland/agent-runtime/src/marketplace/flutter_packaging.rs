//! Flutter App Packaging Specification for `.agnos-agent` bundles.
//!
//! Defines the manifest format, package layout, launch configuration, and
//! validation logic for shipping Flutter-based desktop applications as
//! first-class AGNOS agents.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Wayland requirement enum
// ---------------------------------------------------------------------------

/// Wayland protocols / capabilities that a Flutter app may require.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WaylandRequirement {
    Core,
    XdgShell,
    XdgDecoration,
    TextInputV3,
    DataDevice,
    Viewporter,
    FractionalScale,
    XWayland,
}

impl std::fmt::Display for WaylandRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::XdgShell => write!(f, "xdg_shell"),
            Self::XdgDecoration => write!(f, "xdg_decoration"),
            Self::TextInputV3 => write!(f, "text_input_v3"),
            Self::DataDevice => write!(f, "data_device"),
            Self::Viewporter => write!(f, "viewporter"),
            Self::FractionalScale => write!(f, "fractional_scale"),
            Self::XWayland => write!(f, "xwayland"),
        }
    }
}

// ---------------------------------------------------------------------------
// Display backend
// ---------------------------------------------------------------------------

/// Display backend used to launch a Flutter application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayBackend {
    Wayland,
    XWayland,
}

impl std::fmt::Display for DisplayBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wayland => write!(f, "wayland"),
            Self::XWayland => write!(f, "xwayland"),
        }
    }
}

// ---------------------------------------------------------------------------
// Flutter app manifest
// ---------------------------------------------------------------------------

/// Manifest embedded in a `.agnos-agent` bundle describing a Flutter application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterAppManifest {
    /// Runtime type — always `"flutter"` for Flutter apps.
    pub runtime: String,
    /// Flutter engine version (e.g. `"3.22.0"`).
    pub engine_version: String,
    /// Dart SDK version used to compile the app.
    pub dart_version: String,
    /// Wayland protocols required by the application.
    pub wayland_protocols: Vec<WaylandRequirement>,
    /// Named platform channels used by the app (e.g. `"flutter/textinput"`).
    pub platform_channels: Vec<String>,
    /// Whether the binary was AOT-compiled (true for release builds).
    pub aot_compiled: bool,
}

// ---------------------------------------------------------------------------
// Package layout
// ---------------------------------------------------------------------------

/// Documents the on-disk layout inside a `.agnos-agent` tarball for Flutter apps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterPackageLayout {
    /// Path to the Flutter engine shared library.
    pub engine_path: String,
    /// Path to the AOT application binary.
    pub app_binary_path: String,
    /// Path to the Flutter asset bundle directory.
    pub assets_path: String,
    /// Path to the JSON manifest.
    pub manifest_path: String,
    /// Path to the sandbox profile.
    pub sandbox_path: String,
}

impl FlutterPackageLayout {
    /// Construct the canonical package layout for a given app name.
    pub fn for_app(app_name: &str) -> Self {
        Self {
            engine_path: "bin/flutter_engine.so".to_string(),
            app_binary_path: format!("bin/{app_name}"),
            assets_path: "assets/flutter_assets/".to_string(),
            manifest_path: "manifest.json".to_string(),
            sandbox_path: "sandbox.json".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Launch configuration
// ---------------------------------------------------------------------------

/// Runtime launch configuration for a Flutter application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterLaunchConfig {
    /// Application name.
    pub app_name: String,
    /// Display backend to use.
    pub display_backend: DisplayBackend,
    /// Compositor socket path (e.g. `"wayland-0"`).
    pub compositor_socket: String,
    /// Optional theme channel for dynamic theming.
    pub theme_channel: Option<String>,
    /// Whether accessibility services are enabled.
    pub accessibility_enabled: bool,
    /// Display scale factor.
    pub scale_factor: f64,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a [`FlutterAppManifest`], returning a list of human-readable errors.
/// An empty vec means the manifest is valid.
pub fn validate_flutter_manifest(manifest: &FlutterAppManifest) -> Vec<String> {
    let mut errors = Vec::new();

    // runtime must be "flutter"
    if manifest.runtime != "flutter" {
        errors.push(format!(
            "runtime must be 'flutter', got '{}'",
            manifest.runtime
        ));
    }

    // engine_version must be present and look like semver (x.y.z)
    if manifest.engine_version.is_empty() {
        errors.push("engine_version is required".to_string());
    } else if !is_dotted_version(&manifest.engine_version) {
        errors.push(format!(
            "engine_version '{}' is not a valid version (expected x.y.z)",
            manifest.engine_version
        ));
    }

    // dart_version must be present and look like semver
    if manifest.dart_version.is_empty() {
        errors.push("dart_version is required".to_string());
    } else if !is_dotted_version(&manifest.dart_version) {
        errors.push(format!(
            "dart_version '{}' is not a valid version (expected x.y.z)",
            manifest.dart_version
        ));
    }

    // wayland_protocols must include at least Core and XdgShell
    if !manifest
        .wayland_protocols
        .contains(&WaylandRequirement::Core)
    {
        errors.push("wayland_protocols must include Core".to_string());
    }
    if !manifest
        .wayland_protocols
        .contains(&WaylandRequirement::XdgShell)
    {
        errors.push("wayland_protocols must include XdgShell".to_string());
    }

    errors
}

/// Check whether a string looks like a dotted version `x.y.z` where each part is numeric.
fn is_dotted_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

// ---------------------------------------------------------------------------
// Backend determination
// ---------------------------------------------------------------------------

/// Determine the best display backend for a Flutter app given the compositor's
/// advertised capabilities. Returns [`DisplayBackend::Wayland`] when every
/// requirement is met, otherwise falls back to [`DisplayBackend::XWayland`].
pub fn determine_backend(
    manifest: &FlutterAppManifest,
    compositor_capabilities: &[WaylandRequirement],
) -> DisplayBackend {
    let all_met = manifest
        .wayland_protocols
        .iter()
        .all(|req| compositor_capabilities.contains(req));

    if all_met {
        DisplayBackend::Wayland
    } else {
        DisplayBackend::XWayland
    }
}

// ---------------------------------------------------------------------------
// Launch config builder
// ---------------------------------------------------------------------------

/// Build a [`FlutterLaunchConfig`] from a manifest and runtime parameters.
pub fn build_launch_config(
    manifest: &FlutterAppManifest,
    app_name: &str,
    compositor_socket: &str,
    scale: f64,
) -> FlutterLaunchConfig {
    let backend = if manifest
        .wayland_protocols
        .contains(&WaylandRequirement::XWayland)
    {
        DisplayBackend::XWayland
    } else {
        DisplayBackend::Wayland
    };

    FlutterLaunchConfig {
        app_name: app_name.to_string(),
        display_backend: backend,
        compositor_socket: compositor_socket.to_string(),
        theme_channel: if manifest
            .platform_channels
            .iter()
            .any(|c| c.contains("theme"))
        {
            Some("flutter/theme".to_string())
        } else {
            None
        },
        accessibility_enabled: manifest
            .platform_channels
            .iter()
            .any(|c| c.contains("accessibility")),
        scale_factor: scale,
    }
}

// ---------------------------------------------------------------------------
// Environment variables
// ---------------------------------------------------------------------------

/// Build the environment variable map used to launch a Flutter application.
pub fn build_env_vars(config: &FlutterLaunchConfig) -> HashMap<String, String> {
    let mut env = HashMap::new();

    match config.display_backend {
        DisplayBackend::Wayland => {
            env.insert("GDK_BACKEND".to_string(), "wayland".to_string());
            env.insert(
                "WAYLAND_DISPLAY".to_string(),
                config.compositor_socket.clone(),
            );
            env.insert(
                "FLUTTER_ENGINE_SWITCH_GL".to_string(),
                "wayland".to_string(),
            );
        }
        DisplayBackend::XWayland => {
            env.insert("GDK_BACKEND".to_string(), "x11".to_string());
            env.insert("DISPLAY".to_string(), ":0".to_string());
            env.insert("FLUTTER_ENGINE_SWITCH_GL".to_string(), "x11".to_string());
        }
    }

    env.insert(
        "FLUTTER_FORCE_SCALE_FACTOR".to_string(),
        config.scale_factor.to_string(),
    );

    if config.accessibility_enabled {
        env.insert("FLUTTER_ACCESSIBILITY".to_string(), "true".to_string());
    }

    if let Some(ref theme) = config.theme_channel {
        env.insert("FLUTTER_THEME_CHANNEL".to_string(), theme.clone());
    }

    env.insert("AGNOS_APP_NAME".to_string(), config.app_name.clone());

    env
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> FlutterAppManifest {
        FlutterAppManifest {
            runtime: "flutter".to_string(),
            engine_version: "3.22.0".to_string(),
            dart_version: "3.4.0".to_string(),
            wayland_protocols: vec![
                WaylandRequirement::Core,
                WaylandRequirement::XdgShell,
                WaylandRequirement::XdgDecoration,
                WaylandRequirement::TextInputV3,
            ],
            platform_channels: vec!["flutter/textinput".to_string(), "flutter/theme".to_string()],
            aot_compiled: true,
        }
    }

    // --- manifest validation ---

    #[test]
    fn test_validate_valid_manifest() {
        let errors = validate_flutter_manifest(&valid_manifest());
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_validate_missing_engine_version() {
        let mut m = valid_manifest();
        m.engine_version = String::new();
        let errors = validate_flutter_manifest(&m);
        assert!(errors
            .iter()
            .any(|e| e.contains("engine_version is required")));
    }

    #[test]
    fn test_validate_bad_engine_version() {
        let mut m = valid_manifest();
        m.engine_version = "not-a-version".to_string();
        let errors = validate_flutter_manifest(&m);
        assert!(errors
            .iter()
            .any(|e| e.contains("engine_version") && e.contains("not a valid")));
    }

    #[test]
    fn test_validate_missing_dart_version() {
        let mut m = valid_manifest();
        m.dart_version = String::new();
        let errors = validate_flutter_manifest(&m);
        assert!(errors
            .iter()
            .any(|e| e.contains("dart_version is required")));
    }

    #[test]
    fn test_validate_bad_dart_version() {
        let mut m = valid_manifest();
        m.dart_version = "abc".to_string();
        let errors = validate_flutter_manifest(&m);
        assert!(errors.iter().any(|e| e.contains("dart_version")));
    }

    #[test]
    fn test_validate_wrong_runtime() {
        let mut m = valid_manifest();
        m.runtime = "native".to_string();
        let errors = validate_flutter_manifest(&m);
        assert!(errors
            .iter()
            .any(|e| e.contains("runtime must be 'flutter'")));
    }

    #[test]
    fn test_validate_missing_core_protocol() {
        let mut m = valid_manifest();
        m.wayland_protocols = vec![WaylandRequirement::XdgShell];
        let errors = validate_flutter_manifest(&m);
        assert!(errors.iter().any(|e| e.contains("Core")));
    }

    #[test]
    fn test_validate_missing_xdg_shell_protocol() {
        let mut m = valid_manifest();
        m.wayland_protocols = vec![WaylandRequirement::Core];
        let errors = validate_flutter_manifest(&m);
        assert!(errors.iter().any(|e| e.contains("XdgShell")));
    }

    // --- backend determination ---

    #[test]
    fn test_determine_backend_all_met() {
        let m = valid_manifest();
        let caps = vec![
            WaylandRequirement::Core,
            WaylandRequirement::XdgShell,
            WaylandRequirement::XdgDecoration,
            WaylandRequirement::TextInputV3,
            WaylandRequirement::Viewporter,
        ];
        assert_eq!(determine_backend(&m, &caps), DisplayBackend::Wayland);
    }

    #[test]
    fn test_determine_backend_missing_requirement() {
        let m = valid_manifest();
        // Only Core — missing XdgShell, XdgDecoration, TextInputV3
        let caps = vec![WaylandRequirement::Core];
        assert_eq!(determine_backend(&m, &caps), DisplayBackend::XWayland);
    }

    #[test]
    fn test_determine_backend_empty_capabilities() {
        let m = valid_manifest();
        assert_eq!(determine_backend(&m, &[]), DisplayBackend::XWayland);
    }

    #[test]
    fn test_determine_backend_empty_requirements() {
        let m = FlutterAppManifest {
            wayland_protocols: vec![],
            ..valid_manifest()
        };
        // No requirements — always satisfied
        assert_eq!(determine_backend(&m, &[]), DisplayBackend::Wayland);
    }

    // --- launch config ---

    #[test]
    fn test_build_launch_config_wayland() {
        let m = valid_manifest();
        let cfg = build_launch_config(&m, "my-app", "wayland-0", 1.5);
        assert_eq!(cfg.app_name, "my-app");
        assert_eq!(cfg.display_backend, DisplayBackend::Wayland);
        assert_eq!(cfg.compositor_socket, "wayland-0");
        assert_eq!(cfg.scale_factor, 1.5);
        assert_eq!(cfg.theme_channel, Some("flutter/theme".to_string()));
    }

    #[test]
    fn test_build_launch_config_xwayland() {
        let mut m = valid_manifest();
        m.wayland_protocols.push(WaylandRequirement::XWayland);
        let cfg = build_launch_config(&m, "x-app", "wayland-1", 2.0);
        assert_eq!(cfg.display_backend, DisplayBackend::XWayland);
    }

    // --- env vars ---

    #[test]
    fn test_env_vars_wayland() {
        let cfg = FlutterLaunchConfig {
            app_name: "test-app".to_string(),
            display_backend: DisplayBackend::Wayland,
            compositor_socket: "wayland-0".to_string(),
            theme_channel: Some("flutter/theme".to_string()),
            accessibility_enabled: true,
            scale_factor: 1.25,
        };
        let env = build_env_vars(&cfg);
        assert_eq!(env.get("GDK_BACKEND").unwrap(), "wayland");
        assert_eq!(env.get("WAYLAND_DISPLAY").unwrap(), "wayland-0");
        assert_eq!(env.get("FLUTTER_ENGINE_SWITCH_GL").unwrap(), "wayland");
        assert_eq!(env.get("FLUTTER_FORCE_SCALE_FACTOR").unwrap(), "1.25");
        assert_eq!(env.get("FLUTTER_ACCESSIBILITY").unwrap(), "true");
        assert_eq!(env.get("FLUTTER_THEME_CHANNEL").unwrap(), "flutter/theme");
        assert_eq!(env.get("AGNOS_APP_NAME").unwrap(), "test-app");
    }

    #[test]
    fn test_env_vars_xwayland() {
        let cfg = FlutterLaunchConfig {
            app_name: "x-app".to_string(),
            display_backend: DisplayBackend::XWayland,
            compositor_socket: "wayland-0".to_string(),
            theme_channel: None,
            accessibility_enabled: false,
            scale_factor: 1.0,
        };
        let env = build_env_vars(&cfg);
        assert_eq!(env.get("GDK_BACKEND").unwrap(), "x11");
        assert_eq!(env.get("DISPLAY").unwrap(), ":0");
        assert_eq!(env.get("FLUTTER_ENGINE_SWITCH_GL").unwrap(), "x11");
        assert!(env.get("FLUTTER_ACCESSIBILITY").is_none());
        assert!(env.get("FLUTTER_THEME_CHANNEL").is_none());
    }

    // --- package layout ---

    #[test]
    fn test_package_layout_for_app() {
        let layout = FlutterPackageLayout::for_app("my-flutter-app");
        assert_eq!(layout.engine_path, "bin/flutter_engine.so");
        assert_eq!(layout.app_binary_path, "bin/my-flutter-app");
        assert_eq!(layout.assets_path, "assets/flutter_assets/");
        assert_eq!(layout.manifest_path, "manifest.json");
        assert_eq!(layout.sandbox_path, "sandbox.json");
    }

    // --- display backend serialization ---

    #[test]
    fn test_display_backend_display() {
        assert_eq!(DisplayBackend::Wayland.to_string(), "wayland");
        assert_eq!(DisplayBackend::XWayland.to_string(), "xwayland");
    }

    #[test]
    fn test_display_backend_serde_roundtrip() {
        let json = serde_json::to_string(&DisplayBackend::Wayland).unwrap();
        let parsed: DisplayBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DisplayBackend::Wayland);

        let json = serde_json::to_string(&DisplayBackend::XWayland).unwrap();
        let parsed: DisplayBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DisplayBackend::XWayland);
    }

    // --- wayland requirement display ---

    #[test]
    fn test_wayland_requirement_display() {
        assert_eq!(WaylandRequirement::Core.to_string(), "core");
        assert_eq!(
            WaylandRequirement::FractionalScale.to_string(),
            "fractional_scale"
        );
        assert_eq!(WaylandRequirement::XWayland.to_string(), "xwayland");
    }

    // --- dotted version helper ---

    #[test]
    fn test_is_dotted_version() {
        assert!(is_dotted_version("3.22.0"));
        assert!(is_dotted_version("0.0.1"));
        assert!(!is_dotted_version("3.22"));
        assert!(!is_dotted_version("abc"));
        assert!(!is_dotted_version("3.22.0.1"));
        assert!(!is_dotted_version(""));
    }
}
