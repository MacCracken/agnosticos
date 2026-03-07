//! Flutter App Packaging Tool (`agpkg pack-flutter`)
//!
//! Takes a Flutter build directory and produces a `.agnos-agent` tarball
//! ready for marketplace installation.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use super::flutter_packaging::WaylandRequirement;
use super::{MarketplaceCategory, MarketplaceManifest, PublisherInfo};

// ---------------------------------------------------------------------------
// Build directory validation
// ---------------------------------------------------------------------------

/// Represents a validated Flutter build output directory.
#[derive(Debug, Clone)]
pub struct FlutterBuildDir {
    /// Root path of the build directory.
    pub root: PathBuf,
    /// Path to the engine shared library (relative to root).
    pub engine_lib: PathBuf,
    /// Path to the AOT binary (relative to root).
    pub aot_binary: Option<PathBuf>,
    /// Path to the flutter_assets directory (relative to root).
    pub assets_dir: PathBuf,
}

impl FlutterBuildDir {
    /// Validate a Flutter build output directory and return a [`FlutterBuildDir`]
    /// if the structure is correct.
    pub fn validate(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            bail!("build directory does not exist: {}", root.display());
        }

        // Check for engine shared library
        let engine_candidates = [
            PathBuf::from("lib/libflutter_engine.so"),
            PathBuf::from("lib/libflutter_linux_gtk.so"),
        ];
        let engine_lib = engine_candidates
            .iter()
            .find(|p| root.join(p).is_file())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing Flutter engine shared library: expected lib/libflutter_engine.so or lib/libflutter_linux_gtk.so"
                )
            })?;

        // Check for AOT binary in root or lib/
        let aot_binary = find_aot_binary(root);

        // Check for flutter_assets directory
        let assets_dir = PathBuf::from("flutter_assets");
        if !root.join(&assets_dir).is_dir() {
            bail!(
                "missing flutter_assets/ directory in build output: {}",
                root.display()
            );
        }

        Ok(Self {
            root: root.to_path_buf(),
            engine_lib,
            aot_binary,
            assets_dir,
        })
    }
}

/// Look for an AOT binary in root or lib/ directory. Returns the relative
/// path if found.
fn find_aot_binary(root: &Path) -> Option<PathBuf> {
    // Check root for common AOT binary names
    for name in &["app.so", "libapp.so"] {
        let candidate = PathBuf::from(name);
        if root.join(&candidate).is_file() {
            return Some(candidate);
        }
    }
    // Check lib/ for AOT binaries
    for name in &["app.so", "libapp.so"] {
        let candidate = PathBuf::from("lib").join(name);
        if root.join(&candidate).is_file() {
            return Some(candidate);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pack configuration
// ---------------------------------------------------------------------------

/// Configuration for packing a Flutter app into a `.agnos-agent` bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFlutterConfig {
    /// Application name (lowercase, alphanumeric + hyphens).
    pub app_name: String,
    /// Semantic version (e.g. `"1.0.0"`).
    pub version: String,
    /// Publisher name.
    pub publisher: String,
    /// Short description.
    pub description: String,
    /// Marketplace category (defaults to DesktopApp).
    #[serde(default = "default_category")]
    pub category: MarketplaceCategory,
    /// Wayland protocols required by the app.
    #[serde(default)]
    pub wayland_requirements: Vec<WaylandRequirement>,
    /// Whether the app needs network access.
    #[serde(default)]
    pub network_access: bool,
    /// Custom data directory (defaults to `~/.local/share/<app_name>/`).
    #[serde(default)]
    pub data_dir: Option<String>,
}

fn default_category() -> MarketplaceCategory {
    MarketplaceCategory::DesktopApp
}

// ---------------------------------------------------------------------------
// Sandbox profile types
// ---------------------------------------------------------------------------

/// A Landlock filesystem access rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LandlockRule {
    /// Filesystem path pattern.
    pub path: String,
    /// Access mode: `"ro"` for read-only, `"rw"` for read-write.
    pub access: String,
}

/// Network access rule for the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkRule {
    /// Whether network access is enabled.
    pub enabled: bool,
    /// Allowed hostnames / patterns (empty if network disabled).
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

/// Sandbox profile written as `sandbox.json` in the agent bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProfile {
    /// Landlock filesystem rules.
    pub landlock_paths: Vec<LandlockRule>,
    /// Seccomp mode: `"basic"`, `"desktop"`, or `"custom"`.
    pub seccomp_mode: String,
    /// Network access rule.
    pub network: NetworkRule,
}

// ---------------------------------------------------------------------------
// Pack function
// ---------------------------------------------------------------------------

/// Pack a Flutter build directory into a `.agnos-agent` tarball.
///
/// The resulting tarball contains:
/// - `bin/flutter_engine.so` — the engine shared library
/// - `bin/<app_name>` — the AOT binary (if present)
/// - `assets/flutter_assets/` — dart assets
/// - `manifest.json` — marketplace manifest
/// - `sandbox.json` — sandbox profile
pub fn pack_flutter_app(
    build_dir: &Path,
    config: &PackFlutterConfig,
    output_dir: &Path,
) -> Result<PathBuf> {
    // Validate the build directory
    let validated = FlutterBuildDir::validate(build_dir)
        .context("failed to validate Flutter build directory")?;

    // Generate manifest
    let manifest = generate_manifest(config);
    let manifest_json =
        serde_json::to_string_pretty(&manifest).context("failed to serialize manifest")?;

    // Generate sandbox profile
    let sandbox = generate_sandbox_profile(config);
    let sandbox_json =
        serde_json::to_string_pretty(&sandbox).context("failed to serialize sandbox profile")?;

    // Create output directory if needed
    std::fs::create_dir_all(output_dir)
        .context("failed to create output directory")?;

    // Build tarball
    let tarball_name = format!("{}-{}.agnos-agent", config.app_name, config.version);
    let tarball_path = output_dir.join(&tarball_name);

    let file = std::fs::File::create(&tarball_path)
        .with_context(|| format!("failed to create tarball: {}", tarball_path.display()))?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar_builder = tar::Builder::new(enc);

    // Add engine shared library as bin/flutter_engine.so
    let engine_src = validated.root.join(&validated.engine_lib);
    tar_builder
        .append_path_with_name(&engine_src, "bin/flutter_engine.so")
        .context("failed to add engine library to tarball")?;

    // Add AOT binary as bin/<app_name>
    if let Some(ref aot) = validated.aot_binary {
        let aot_src = validated.root.join(aot);
        let aot_dest = format!("bin/{}", config.app_name);
        tar_builder
            .append_path_with_name(&aot_src, &aot_dest)
            .context("failed to add AOT binary to tarball")?;
    }

    // Add flutter_assets/ directory as assets/flutter_assets/
    let assets_src = validated.root.join(&validated.assets_dir);
    add_dir_to_tar(&mut tar_builder, &assets_src, "assets/flutter_assets")
        .context("failed to add flutter_assets to tarball")?;

    // Add manifest.json
    add_bytes_to_tar(&mut tar_builder, "manifest.json", manifest_json.as_bytes())
        .context("failed to add manifest.json to tarball")?;

    // Add sandbox.json
    add_bytes_to_tar(&mut tar_builder, "sandbox.json", sandbox_json.as_bytes())
        .context("failed to add sandbox.json to tarball")?;

    // Finalize
    let enc = tar_builder
        .into_inner()
        .context("failed to finalize tarball")?;
    enc.finish().context("failed to finish gzip compression")?;

    Ok(tarball_path)
}

// ---------------------------------------------------------------------------
// Manifest generation
// ---------------------------------------------------------------------------

/// Generate a [`MarketplaceManifest`] from the pack config.
pub fn generate_manifest(config: &PackFlutterConfig) -> MarketplaceManifest {
    use agnos_common::AgentManifest;

    MarketplaceManifest {
        agent: AgentManifest {
            name: config.app_name.clone(),
            description: config.description.clone(),
            version: config.version.clone(),
            author: config.publisher.clone(),
            ..Default::default()
        },
        publisher: PublisherInfo {
            name: config.publisher.clone(),
            key_id: String::new(), // Populated during signing
            homepage: String::new(),
        },
        category: config.category,
        runtime: "flutter".to_string(),
        screenshots: vec![],
        changelog: String::new(),
        min_agnos_version: String::new(),
        dependencies: HashMap::new(),
        tags: vec!["flutter".to_string(), "desktop".to_string()],
    }
}

/// Generate a [`SandboxProfile`] from the pack config.
pub fn generate_sandbox_profile(config: &PackFlutterConfig) -> SandboxProfile {
    let data_dir = config
        .data_dir
        .clone()
        .unwrap_or_else(|| format!("~/.local/share/{}/", config.app_name));

    let mut landlock_paths = vec![
        LandlockRule {
            path: data_dir,
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
    ];

    // Desktop apps also need read access to the Wayland socket directory
    if !config.wayland_requirements.is_empty() {
        landlock_paths.push(LandlockRule {
            path: "/run/user/".to_string(),
            access: "ro".to_string(),
        });
    }

    let network = if config.network_access {
        NetworkRule {
            enabled: true,
            allowed_hosts: vec!["*".to_string()],
        }
    } else {
        NetworkRule {
            enabled: false,
            allowed_hosts: vec![],
        }
    };

    SandboxProfile {
        landlock_paths,
        seccomp_mode: "desktop".to_string(),
        network,
    }
}

// ---------------------------------------------------------------------------
// Tar helpers
// ---------------------------------------------------------------------------

/// Add a byte slice as a file entry in the tarball.
fn add_bytes_to_tar<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    data: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, data)?;
    Ok(())
}

/// Recursively add a directory to the tarball under the given prefix.
fn add_dir_to_tar<W: Write>(
    builder: &mut tar::Builder<W>,
    src_dir: &Path,
    tar_prefix: &str,
) -> Result<()> {
    if !src_dir.is_dir() {
        bail!("not a directory: {}", src_dir.display());
    }

    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let rel_name = entry.file_name();
        let tar_path = format!("{}/{}", tar_prefix, rel_name.to_string_lossy());

        if file_type.is_file() {
            builder.append_path_with_name(entry.path(), &tar_path)?;
        } else if file_type.is_dir() {
            add_dir_to_tar(builder, &entry.path(), &tar_path)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a valid Flutter build directory structure in a temp dir.
    fn create_valid_build_dir(tmp: &TempDir) -> PathBuf {
        let root = tmp.path().join("build");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(root.join("flutter_assets")).unwrap();

        // Engine shared library
        fs::write(root.join("lib/libflutter_engine.so"), b"fake engine").unwrap();

        // AOT binary
        fs::write(root.join("lib/app.so"), b"fake aot").unwrap();

        // Flutter assets
        fs::write(
            root.join("flutter_assets/AssetManifest.json"),
            b"{}",
        )
        .unwrap();
        fs::write(
            root.join("flutter_assets/FontManifest.json"),
            b"[]",
        )
        .unwrap();

        root
    }

    fn sample_config() -> PackFlutterConfig {
        PackFlutterConfig {
            app_name: "my-flutter-app".to_string(),
            version: "1.0.0".to_string(),
            publisher: "Test Publisher".to_string(),
            description: "A test Flutter application".to_string(),
            category: MarketplaceCategory::DesktopApp,
            wayland_requirements: vec![WaylandRequirement::Core, WaylandRequirement::XdgShell],
            network_access: false,
            data_dir: None,
        }
    }

    // --- Build directory validation ---

    #[test]
    fn test_validate_valid_build_dir() {
        let tmp = TempDir::new().unwrap();
        let root = create_valid_build_dir(&tmp);
        let result = FlutterBuildDir::validate(&root);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let build = result.unwrap();
        assert_eq!(build.engine_lib, PathBuf::from("lib/libflutter_engine.so"));
        assert!(build.aot_binary.is_some());
        assert_eq!(build.assets_dir, PathBuf::from("flutter_assets"));
    }

    #[test]
    fn test_validate_build_dir_missing_engine() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("build");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(root.join("flutter_assets")).unwrap();
        // No engine library

        let result = FlutterBuildDir::validate(&root);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing Flutter engine"), "got: {err}");
    }

    #[test]
    fn test_validate_build_dir_missing_assets() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("build");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("lib/libflutter_engine.so"), b"engine").unwrap();
        // No flutter_assets directory

        let result = FlutterBuildDir::validate(&root);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("flutter_assets"), "got: {err}");
    }

    #[test]
    fn test_validate_build_dir_nonexistent() {
        let result = FlutterBuildDir::validate(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"), "got: {err}");
    }

    #[test]
    fn test_validate_build_dir_gtk_engine() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("build");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(root.join("flutter_assets")).unwrap();
        fs::write(root.join("lib/libflutter_linux_gtk.so"), b"gtk engine").unwrap();

        let result = FlutterBuildDir::validate(&root);
        assert!(result.is_ok());
        let build = result.unwrap();
        assert_eq!(
            build.engine_lib,
            PathBuf::from("lib/libflutter_linux_gtk.so")
        );
    }

    #[test]
    fn test_validate_build_dir_no_aot_is_ok() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("build");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(root.join("flutter_assets")).unwrap();
        fs::write(root.join("lib/libflutter_engine.so"), b"engine").unwrap();
        // No AOT binary — should still succeed

        let result = FlutterBuildDir::validate(&root);
        assert!(result.is_ok());
        assert!(result.unwrap().aot_binary.is_none());
    }

    // --- Pack creates tarball ---

    #[test]
    fn test_pack_creates_tarball() {
        let tmp = TempDir::new().unwrap();
        let build_root = create_valid_build_dir(&tmp);
        let output_dir = tmp.path().join("output");

        let config = sample_config();
        let result = pack_flutter_app(&build_root, &config, &output_dir);
        assert!(result.is_ok(), "pack failed: {:?}", result.err());

        let tarball_path = result.unwrap();
        assert!(tarball_path.exists());
        assert!(tarball_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".agnos-agent"));
        assert_eq!(
            tarball_path.file_name().unwrap().to_string_lossy(),
            "my-flutter-app-1.0.0.agnos-agent"
        );
    }

    #[test]
    fn test_pack_tarball_contents() {
        let tmp = TempDir::new().unwrap();
        let build_root = create_valid_build_dir(&tmp);
        let output_dir = tmp.path().join("output");

        let config = sample_config();
        let tarball_path = pack_flutter_app(&build_root, &config, &output_dir).unwrap();

        // Extract and verify contents
        let file = fs::File::open(&tarball_path).unwrap();
        let dec = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(dec);

        let entries: Vec<String> = archive
            .entries()
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(
            entries.iter().any(|e| e == "bin/flutter_engine.so"),
            "missing bin/flutter_engine.so in {entries:?}"
        );
        assert!(
            entries.iter().any(|e| e == "manifest.json"),
            "missing manifest.json in {entries:?}"
        );
        assert!(
            entries.iter().any(|e| e == "sandbox.json"),
            "missing sandbox.json in {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|e| e.starts_with("assets/flutter_assets/")),
            "missing assets/flutter_assets/ in {entries:?}"
        );
    }

    // --- Manifest generation ---

    #[test]
    fn test_generate_manifest_content() {
        let config = sample_config();
        let manifest = generate_manifest(&config);
        assert_eq!(manifest.agent.name, "my-flutter-app");
        assert_eq!(manifest.agent.version, "1.0.0");
        assert_eq!(manifest.agent.description, "A test Flutter application");
        assert_eq!(manifest.publisher.name, "Test Publisher");
        assert_eq!(manifest.category, MarketplaceCategory::DesktopApp);
        assert_eq!(manifest.runtime, "flutter");
        assert!(manifest.tags.contains(&"flutter".to_string()));
    }

    #[test]
    fn test_manifest_serializes_to_json() {
        let config = sample_config();
        let manifest = generate_manifest(&config);
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("my-flutter-app"));
        assert!(json.contains("flutter"));
        // Round-trip
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["runtime"], "flutter");
    }

    // --- Sandbox profile generation ---

    #[test]
    fn test_sandbox_profile_no_network() {
        let config = sample_config();
        let sandbox = generate_sandbox_profile(&config);

        assert_eq!(sandbox.seccomp_mode, "desktop");
        assert!(!sandbox.network.enabled);
        assert!(sandbox.network.allowed_hosts.is_empty());

        // Should have data dir, /tmp, /usr/share/fonts, and /run/user/
        assert!(sandbox
            .landlock_paths
            .iter()
            .any(|r| r.path.contains("my-flutter-app") && r.access == "rw"));
        assert!(sandbox
            .landlock_paths
            .iter()
            .any(|r| r.path == "/tmp" && r.access == "rw"));
        assert!(sandbox
            .landlock_paths
            .iter()
            .any(|r| r.path == "/usr/share/fonts" && r.access == "ro"));
    }

    #[test]
    fn test_sandbox_profile_with_network() {
        let mut config = sample_config();
        config.network_access = true;
        let sandbox = generate_sandbox_profile(&config);

        assert!(sandbox.network.enabled);
        assert!(!sandbox.network.allowed_hosts.is_empty());
    }

    #[test]
    fn test_sandbox_profile_custom_data_dir() {
        let mut config = sample_config();
        config.data_dir = Some("/custom/data".to_string());
        let sandbox = generate_sandbox_profile(&config);

        assert!(sandbox
            .landlock_paths
            .iter()
            .any(|r| r.path == "/custom/data" && r.access == "rw"));
    }

    #[test]
    fn test_sandbox_profile_serialization_roundtrip() {
        let config = sample_config();
        let sandbox = generate_sandbox_profile(&config);
        let json = serde_json::to_string_pretty(&sandbox).unwrap();
        let parsed: SandboxProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.seccomp_mode, sandbox.seccomp_mode);
        assert_eq!(parsed.network.enabled, sandbox.network.enabled);
        assert_eq!(parsed.landlock_paths.len(), sandbox.landlock_paths.len());
    }

    // --- Config defaults ---

    #[test]
    fn test_config_default_category() {
        let json = r#"{
            "app_name": "test",
            "version": "0.1.0",
            "publisher": "pub",
            "description": "desc"
        }"#;
        let config: PackFlutterConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.category, MarketplaceCategory::DesktopApp);
        assert!(!config.network_access);
        assert!(config.data_dir.is_none());
    }
}
