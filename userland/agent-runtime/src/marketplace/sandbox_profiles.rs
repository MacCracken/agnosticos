//! Predefined Sandbox Profiles
//!
//! Provides curated sandbox profiles for known application types, including
//! specific profiles for well-known apps like Photis Nadi.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::flutter_agpkg::{LandlockRule, NetworkRule};

// ---------------------------------------------------------------------------
// Sandbox preset enum
// ---------------------------------------------------------------------------

/// Predefined sandbox preset categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxPreset {
    /// Photo/image editing applications.
    PhotoEditor,
    /// Productivity and office applications.
    ProductivityApp,
    /// Web browsers.
    Browser,
    /// Games and entertainment.
    GameApp,
    /// Command-line tools and utilities.
    CliTool,
    /// Custom profile (no preset rules applied).
    Custom,
}

impl std::fmt::Display for SandboxPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PhotoEditor => write!(f, "photo-editor"),
            Self::ProductivityApp => write!(f, "productivity-app"),
            Self::Browser => write!(f, "browser"),
            Self::GameApp => write!(f, "game-app"),
            Self::CliTool => write!(f, "cli-tool"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ---------------------------------------------------------------------------
// Predefined profile
// ---------------------------------------------------------------------------

/// A complete predefined sandbox profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredefinedProfile {
    /// The preset this profile was built from.
    pub preset: SandboxPreset,
    /// Landlock filesystem rules.
    pub landlock_rules: Vec<LandlockRule>,
    /// Seccomp mode: `"basic"`, `"desktop"`, or `"strict"`.
    pub seccomp_mode: String,
    /// Network access rules.
    pub network: NetworkRule,
    /// Maximum memory in megabytes.
    pub max_memory_mb: u64,
    /// Whether the app is allowed to spawn child processes.
    pub allow_process_spawn: bool,
}

// ---------------------------------------------------------------------------
// Photis Nadi specific profile
// ---------------------------------------------------------------------------

/// Build the predefined sandbox profile for **Photis Nadi** (Flutter photo editor
/// with Supabase backend and local Hive DB).
pub fn build_photis_nadi_profile() -> PredefinedProfile {
    PredefinedProfile {
        preset: SandboxPreset::PhotoEditor,
        landlock_rules: vec![
            LandlockRule {
                path: "~/.local/share/photisnadi/".to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/tmp".to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/usr/share/fonts".to_string(),
                access: "ro".to_string(),
            },
        ],
        seccomp_mode: "desktop".to_string(),
        network: NetworkRule {
            enabled: true,
            allowed_hosts: vec![
                "*.supabase.co".to_string(),
                "*.supabase.in".to_string(),
            ],
        },
        max_memory_mb: 512,
        allow_process_spawn: false,
    }
}

// ---------------------------------------------------------------------------
// Generic preset builder
// ---------------------------------------------------------------------------

/// Build a [`PredefinedProfile`] from a [`SandboxPreset`], app name, and data
/// directory path.
pub fn build_profile_for_preset(
    preset: SandboxPreset,
    app_name: &str,
    data_dir: &str,
) -> PredefinedProfile {
    match preset {
        SandboxPreset::PhotoEditor => build_photo_editor_profile(app_name, data_dir),
        SandboxPreset::ProductivityApp => build_productivity_profile(app_name, data_dir),
        SandboxPreset::Browser => build_browser_profile(app_name, data_dir),
        SandboxPreset::GameApp => build_game_profile(app_name, data_dir),
        SandboxPreset::CliTool => build_cli_tool_profile(app_name, data_dir),
        SandboxPreset::Custom => PredefinedProfile {
            preset: SandboxPreset::Custom,
            landlock_rules: vec![LandlockRule {
                path: data_dir.to_string(),
                access: "rw".to_string(),
            }],
            seccomp_mode: "basic".to_string(),
            network: NetworkRule {
                enabled: false,
                allowed_hosts: vec![],
            },
            max_memory_mb: 256,
            allow_process_spawn: false,
        },
    }
}

fn build_photo_editor_profile(_app_name: &str, data_dir: &str) -> PredefinedProfile {
    PredefinedProfile {
        preset: SandboxPreset::PhotoEditor,
        landlock_rules: vec![
            LandlockRule {
                path: data_dir.to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/tmp".to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/usr/share/fonts".to_string(),
                access: "ro".to_string(),
            },
        ],
        seccomp_mode: "desktop".to_string(),
        network: NetworkRule {
            enabled: false,
            allowed_hosts: vec![],
        },
        max_memory_mb: 512,
        allow_process_spawn: false,
    }
}

fn build_productivity_profile(_app_name: &str, data_dir: &str) -> PredefinedProfile {
    PredefinedProfile {
        preset: SandboxPreset::ProductivityApp,
        landlock_rules: vec![
            LandlockRule {
                path: data_dir.to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/tmp".to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/usr/share/fonts".to_string(),
                access: "ro".to_string(),
            },
            LandlockRule {
                path: "~/Documents".to_string(),
                access: "rw".to_string(),
            },
        ],
        seccomp_mode: "desktop".to_string(),
        network: NetworkRule {
            enabled: true,
            allowed_hosts: vec![],
        },
        max_memory_mb: 1024,
        allow_process_spawn: false,
    }
}

fn build_browser_profile(_app_name: &str, data_dir: &str) -> PredefinedProfile {
    PredefinedProfile {
        preset: SandboxPreset::Browser,
        landlock_rules: vec![
            LandlockRule {
                path: data_dir.to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/tmp".to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/usr/share/fonts".to_string(),
                access: "ro".to_string(),
            },
            LandlockRule {
                path: "~/Downloads".to_string(),
                access: "rw".to_string(),
            },
        ],
        seccomp_mode: "desktop".to_string(),
        network: NetworkRule {
            enabled: true,
            allowed_hosts: vec!["*".to_string()],
        },
        max_memory_mb: 2048,
        allow_process_spawn: true,
    }
}

fn build_game_profile(_app_name: &str, data_dir: &str) -> PredefinedProfile {
    PredefinedProfile {
        preset: SandboxPreset::GameApp,
        landlock_rules: vec![
            LandlockRule {
                path: data_dir.to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/tmp".to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/usr/share/fonts".to_string(),
                access: "ro".to_string(),
            },
        ],
        seccomp_mode: "desktop".to_string(),
        network: NetworkRule {
            enabled: true,
            allowed_hosts: vec![],
        },
        max_memory_mb: 4096,
        allow_process_spawn: false,
    }
}

fn build_cli_tool_profile(_app_name: &str, data_dir: &str) -> PredefinedProfile {
    PredefinedProfile {
        preset: SandboxPreset::CliTool,
        landlock_rules: vec![
            LandlockRule {
                path: data_dir.to_string(),
                access: "rw".to_string(),
            },
            LandlockRule {
                path: "/tmp".to_string(),
                access: "rw".to_string(),
            },
        ],
        seccomp_mode: "basic".to_string(),
        network: NetworkRule {
            enabled: false,
            allowed_hosts: vec![],
        },
        max_memory_mb: 256,
        allow_process_spawn: true,
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a [`PredefinedProfile`] for sensibility. Returns `Ok(())` if valid,
/// or an error describing the first problem found.
pub fn validate_profile(profile: &PredefinedProfile) -> Result<()> {
    // Must have at least one landlock rule
    if profile.landlock_rules.is_empty() {
        bail!("profile must have at least one Landlock rule");
    }

    // All access modes must be valid
    for rule in &profile.landlock_rules {
        if rule.access != "ro" && rule.access != "rw" {
            bail!(
                "invalid Landlock access mode '{}' for path '{}' (expected 'ro' or 'rw')",
                rule.access,
                rule.path
            );
        }
    }

    // Paths must not be empty
    for rule in &profile.landlock_rules {
        if rule.path.is_empty() {
            bail!("Landlock rule has an empty path");
        }
    }

    // Seccomp mode must be a known value
    match profile.seccomp_mode.as_str() {
        "basic" | "desktop" | "strict" => {}
        other => bail!("unknown seccomp mode: '{other}' (expected basic, desktop, or strict)"),
    }

    // Memory must be positive
    if profile.max_memory_mb == 0 {
        bail!("max_memory_mb must be greater than zero");
    }

    // If network is disabled, allowed_hosts should be empty
    if !profile.network.enabled && !profile.network.allowed_hosts.is_empty() {
        bail!("network is disabled but allowed_hosts is non-empty");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Photis Nadi profile ---

    #[test]
    fn test_photis_nadi_profile_preset() {
        let profile = build_photis_nadi_profile();
        assert_eq!(profile.preset, SandboxPreset::PhotoEditor);
    }

    #[test]
    fn test_photis_nadi_profile_landlock_rules() {
        let profile = build_photis_nadi_profile();
        assert_eq!(profile.landlock_rules.len(), 3);

        // Hive DB data dir
        assert!(profile
            .landlock_rules
            .iter()
            .any(|r| r.path == "~/.local/share/photisnadi/" && r.access == "rw"));
        // tmp
        assert!(profile
            .landlock_rules
            .iter()
            .any(|r| r.path == "/tmp" && r.access == "rw"));
        // fonts (read-only)
        assert!(profile
            .landlock_rules
            .iter()
            .any(|r| r.path == "/usr/share/fonts" && r.access == "ro"));
    }

    #[test]
    fn test_photis_nadi_profile_network() {
        let profile = build_photis_nadi_profile();
        assert!(profile.network.enabled);
        assert!(profile
            .network
            .allowed_hosts
            .contains(&"*.supabase.co".to_string()));
        assert!(profile
            .network
            .allowed_hosts
            .contains(&"*.supabase.in".to_string()));
        assert_eq!(profile.network.allowed_hosts.len(), 2);
    }

    #[test]
    fn test_photis_nadi_profile_seccomp_and_memory() {
        let profile = build_photis_nadi_profile();
        assert_eq!(profile.seccomp_mode, "desktop");
        assert_eq!(profile.max_memory_mb, 512);
        assert!(!profile.allow_process_spawn);
    }

    #[test]
    fn test_photis_nadi_profile_validates() {
        let profile = build_photis_nadi_profile();
        assert!(validate_profile(&profile).is_ok());
    }

    // --- Preset builder for each type ---

    #[test]
    fn test_photo_editor_preset() {
        let profile =
            build_profile_for_preset(SandboxPreset::PhotoEditor, "test-editor", "/data/editor");
        assert_eq!(profile.preset, SandboxPreset::PhotoEditor);
        assert_eq!(profile.seccomp_mode, "desktop");
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn test_productivity_preset() {
        let profile = build_profile_for_preset(
            SandboxPreset::ProductivityApp,
            "test-office",
            "/data/office",
        );
        assert_eq!(profile.preset, SandboxPreset::ProductivityApp);
        assert!(profile.network.enabled);
        assert!(profile
            .landlock_rules
            .iter()
            .any(|r| r.path == "~/Documents"));
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn test_browser_preset() {
        let profile =
            build_profile_for_preset(SandboxPreset::Browser, "test-browser", "/data/browser");
        assert_eq!(profile.preset, SandboxPreset::Browser);
        assert!(profile.network.enabled);
        assert!(profile.allow_process_spawn);
        assert_eq!(profile.max_memory_mb, 2048);
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn test_game_preset() {
        let profile =
            build_profile_for_preset(SandboxPreset::GameApp, "test-game", "/data/game");
        assert_eq!(profile.preset, SandboxPreset::GameApp);
        assert_eq!(profile.max_memory_mb, 4096);
        assert!(!profile.allow_process_spawn);
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn test_cli_tool_preset() {
        let profile =
            build_profile_for_preset(SandboxPreset::CliTool, "test-cli", "/data/cli");
        assert_eq!(profile.preset, SandboxPreset::CliTool);
        assert_eq!(profile.seccomp_mode, "basic");
        assert!(!profile.network.enabled);
        assert!(profile.allow_process_spawn);
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn test_custom_preset() {
        let profile =
            build_profile_for_preset(SandboxPreset::Custom, "test-custom", "/data/custom");
        assert_eq!(profile.preset, SandboxPreset::Custom);
        assert_eq!(profile.landlock_rules.len(), 1);
        assert!(validate_profile(&profile).is_ok());
    }

    // --- Validation ---

    #[test]
    fn test_validate_catches_empty_landlock_rules() {
        let mut profile = build_photis_nadi_profile();
        profile.landlock_rules.clear();
        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least one"));
    }

    #[test]
    fn test_validate_catches_invalid_access_mode() {
        let mut profile = build_photis_nadi_profile();
        profile.landlock_rules[0].access = "rwx".to_string();
        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid"));
    }

    #[test]
    fn test_validate_catches_empty_path() {
        let mut profile = build_photis_nadi_profile();
        profile.landlock_rules[0].path = String::new();
        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty path"));
    }

    #[test]
    fn test_validate_catches_unknown_seccomp_mode() {
        let mut profile = build_photis_nadi_profile();
        profile.seccomp_mode = "super-permissive".to_string();
        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("seccomp"));
    }

    #[test]
    fn test_validate_catches_zero_memory() {
        let mut profile = build_photis_nadi_profile();
        profile.max_memory_mb = 0;
        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_memory_mb"));
    }

    #[test]
    fn test_validate_catches_disabled_network_with_hosts() {
        let mut profile = build_photis_nadi_profile();
        profile.network.enabled = false;
        // allowed_hosts is still non-empty
        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("allowed_hosts"));
    }

    // --- Serialization ---

    #[test]
    fn test_predefined_profile_serde_roundtrip() {
        let profile = build_photis_nadi_profile();
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let parsed: PredefinedProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.preset, profile.preset);
        assert_eq!(parsed.max_memory_mb, profile.max_memory_mb);
        assert_eq!(parsed.landlock_rules.len(), profile.landlock_rules.len());
        assert_eq!(parsed.network.enabled, profile.network.enabled);
    }
}
