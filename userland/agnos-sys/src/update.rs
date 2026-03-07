//! A/B System Update Manager
//!
//! Provides Rust APIs for A/B partition-based system updates with rollback
//! capability, replacing the legacy `scripts/agnos-update.sh` bash script.
//!
//! Features:
//! - A/B slot management for atomic, rollback-safe updates
//! - SHA-256 verification of update manifests and written data
//! - CalVer version comparison (YYYY.D.M format)
//! - Delta update support (compressed patches from a prior version)
//! - Persistent update state tracking
//! - Boot-count based automatic rollback detection
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Represents an A/B update slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateSlot {
    A,
    B,
}

impl UpdateSlot {
    /// Return the opposite slot.
    pub fn other(&self) -> UpdateSlot {
        match self {
            UpdateSlot::A => UpdateSlot::B,
            UpdateSlot::B => UpdateSlot::A,
        }
    }

    /// Return the partition suffix string (`"a"` or `"b"`).
    pub fn partition_suffix(&self) -> &'static str {
        match self {
            UpdateSlot::A => "a",
            UpdateSlot::B => "b",
        }
    }
}

impl fmt::Display for UpdateSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateSlot::A => write!(f, "A"),
            UpdateSlot::B => write!(f, "B"),
        }
    }
}

/// Release channel for updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
    Custom(String),
}

impl fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateChannel::Stable => write!(f, "stable"),
            UpdateChannel::Beta => write!(f, "beta"),
            UpdateChannel::Nightly => write!(f, "nightly"),
            UpdateChannel::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// A single file within an update manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFile {
    /// Relative path within the update image.
    pub path: String,
    /// SHA-256 hex digest of the file.
    pub sha256: String,
    /// Size of the (uncompressed) file in bytes.
    pub size_bytes: u64,
    /// If this is a delta, the source version it patches from.
    pub delta_from: Option<String>,
    /// Whether the file is zstd-compressed.
    pub compressed: bool,
}

/// Manifest describing an available update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    /// Target version string (CalVer YYYY.D.M).
    pub version: String,
    /// Release channel.
    pub channel: UpdateChannel,
    /// Files included in this update.
    pub files: Vec<UpdateFile>,
    /// ISO-8601 release date.
    pub release_date: String,
    /// Minimum installed version required to apply this update.
    pub min_version: Option<String>,
    /// Human-readable changelog.
    pub changelog: Option<String>,
    /// SHA-256 hex digest covering the canonical manifest body (excluding
    /// this field itself).
    pub sha256_digest: String,
}

/// Phases of an in-progress update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdatePhase {
    Downloading,
    Verifying,
    Applying,
    Finalizing,
    RollingBack,
    Complete,
    Failed,
}

impl fmt::Display for UpdatePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            UpdatePhase::Downloading => "Downloading",
            UpdatePhase::Verifying => "Verifying",
            UpdatePhase::Applying => "Applying",
            UpdatePhase::Finalizing => "Finalizing",
            UpdatePhase::RollingBack => "Rolling back",
            UpdatePhase::Complete => "Complete",
            UpdatePhase::Failed => "Failed",
        };
        write!(f, "{}", label)
    }
}

/// Progress information for an active update operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    /// Current phase.
    pub phase: UpdatePhase,
    /// Completion percentage (0-100).
    pub percent: u8,
    /// Human-readable status message.
    pub message: String,
}

impl UpdateProgress {
    /// Create a new progress value, clamping percent to 0..=100.
    pub fn new(phase: UpdatePhase, percent: u8, message: impl Into<String>) -> Self {
        Self {
            phase,
            percent: percent.min(100),
            message: message.into(),
        }
    }
}

/// Persistent update state stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateState {
    /// Slot that is currently booted.
    pub current_slot: UpdateSlot,
    /// Installed version on the current slot.
    pub current_version: String,
    /// Version string of a pending (written but not yet confirmed) update.
    pub pending_update: Option<String>,
    /// Timestamp of the last successful update.
    pub last_update: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether the previous slot contains a valid rollback image.
    pub rollback_available: bool,
    /// Number of boots since the last update was applied.
    pub boot_count_since_update: u32,
}

/// Configuration for the update subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Base URL of the update server (or path to a local manifest).
    pub update_url: String,
    /// Block device for slot A.
    pub slot_a_device: PathBuf,
    /// Block device for slot B.
    pub slot_b_device: PathBuf,
    /// Path to the persisted `UpdateState` JSON file.
    pub state_file: PathBuf,
    /// Directory used for backups before updates.
    pub backup_dir: PathBuf,
    /// Maximum retry count for download/apply failures.
    pub max_retries: u32,
    /// Whether to re-read and verify the slot after writing.
    pub verify_after_apply: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            update_url: "https://updates.agnos.org".into(),
            slot_a_device: PathBuf::from("/dev/disk/by-partlabel/rootfs_a"),
            slot_b_device: PathBuf::from("/dev/disk/by-partlabel/rootfs_b"),
            state_file: PathBuf::from("/var/lib/agnos/update-state.json"),
            backup_dir: PathBuf::from("/var/lib/agnos/backups"),
            max_retries: 3,
            verify_after_apply: true,
        }
    }
}

impl UpdateConfig {
    /// Return the device path for the given slot.
    pub fn device_for_slot(&self, slot: UpdateSlot) -> &Path {
        match slot {
            UpdateSlot::A => &self.slot_a_device,
            UpdateSlot::B => &self.slot_b_device,
        }
    }
}

// ---------------------------------------------------------------------------
// Version helpers
// ---------------------------------------------------------------------------

/// Validate that `version` conforms to CalVer `YYYY.D.M` (all numeric parts).
pub fn validate_version(version: &str) -> Result<()> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err(SysError::InvalidArgument(format!(
            "Version must have exactly 3 parts (YYYY.D.M), got: {version}"
        )));
    }
    for (i, label) in [(0, "year"), (1, "day"), (2, "month")] {
        parts[i].parse::<u32>().map_err(|_| {
            SysError::InvalidArgument(format!(
                "Version {label} component is not a number: '{}'",
                parts[i]
            ))
        })?;
    }
    let year: u32 = parts[0].parse().map_err(|_| {
        SysError::InvalidArgument(format!("Version year is not a number: '{}'", parts[0]))
    })?;
    if !(2024..=2100).contains(&year) {
        return Err(SysError::InvalidArgument(format!(
            "Version year out of range (2024-2100): {year}"
        )));
    }
    let month: u32 = parts[2].parse().map_err(|_| {
        SysError::InvalidArgument(format!("Version month is not a number: '{}'", parts[2]))
    })?;
    if !(1..=12).contains(&month) {
        return Err(SysError::InvalidArgument(format!(
            "Version month out of range (1-12): {month}"
        )));
    }
    Ok(())
}

/// Compare two CalVer version strings (`YYYY.D.M`).
///
/// Returns `Ordering::Less` when `a` is older than `b`.  If either string
/// is malformed the comparison falls back to lexicographic ordering.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let p: Vec<&str> = v.split('.').collect();
        if p.len() != 3 {
            return None;
        }
        Some((p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?))
    };

    match (parse(a), parse(b)) {
        (Some(av), Some(bv)) => av.cmp(&bv),
        _ => a.cmp(b),
    }
}

// ---------------------------------------------------------------------------
// Slot detection
// ---------------------------------------------------------------------------

/// Determine the currently booted slot by inspecting `/proc/cmdline` for a
/// `agnos.slot=` parameter.  Falls back to reading the state file.
#[cfg(target_os = "linux")]
pub fn get_current_slot() -> Result<UpdateSlot> {
    // Try kernel command line first.
    if let Ok(cmdline) = std::fs::read_to_string("/proc/cmdline") {
        for token in cmdline.split_whitespace() {
            if let Some(val) = token.strip_prefix("agnos.slot=") {
                return match val {
                    "a" | "A" => Ok(UpdateSlot::A),
                    "b" | "B" => Ok(UpdateSlot::B),
                    _ => Err(SysError::InvalidArgument(format!(
                        "Unknown slot in cmdline: {val}"
                    ))),
                };
            }
        }
    }

    // Fallback: read state file at the default location.
    let default_state = PathBuf::from("/var/lib/agnos/update-state.json");
    if default_state.exists() {
        let data = std::fs::read_to_string(&default_state)
            .map_err(|e| SysError::Unknown(format!("Failed to read state file: {e}")))?;
        let state: UpdateState = serde_json::from_str(&data)
            .map_err(|e| SysError::InvalidArgument(format!("Malformed state file: {e}")))?;
        return Ok(state.current_slot);
    }

    // Default to slot A.
    Ok(UpdateSlot::A)
}

#[cfg(not(target_os = "linux"))]
pub fn get_current_slot() -> Result<UpdateSlot> {
    Err(SysError::NotSupported)
}

// ---------------------------------------------------------------------------
// State persistence
// ---------------------------------------------------------------------------

/// Load persisted update state from the config's `state_file`.
pub fn get_update_state(config: &UpdateConfig) -> Result<UpdateState> {
    let data = std::fs::read_to_string(&config.state_file).map_err(|e| {
        SysError::Unknown(format!(
            "Failed to read update state from {}: {e}",
            config.state_file.display()
        ))
    })?;
    serde_json::from_str(&data)
        .map_err(|e| SysError::InvalidArgument(format!("Malformed update state: {e}")))
}

/// Persist `state` as JSON to `path`.
pub fn save_update_state(state: &UpdateState, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SysError::Unknown(format!(
                "Failed to create state directory {}: {e}",
                parent.display()
            ))
        })?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| SysError::Unknown(format!("Failed to serialize update state: {e}")))?;
    std::fs::write(path, json).map_err(|e| {
        SysError::Unknown(format!(
            "Failed to write update state to {}: {e}",
            path.display()
        ))
    })
}

// ---------------------------------------------------------------------------
// Manifest handling
// ---------------------------------------------------------------------------

/// Parse a JSON string into an `UpdateManifest`.
pub fn parse_update_manifest(json: &str) -> Result<UpdateManifest> {
    serde_json::from_str(json)
        .map_err(|e| SysError::InvalidArgument(format!("Failed to parse update manifest: {e}")))
}

/// Verify the integrity of a manifest: check that the SHA-256 digest field
/// matches a digest computed over the canonical content (version + channel +
/// files + release_date + min_version + changelog).
pub fn verify_manifest(manifest: &UpdateManifest) -> Result<()> {
    if manifest.version.is_empty() {
        return Err(SysError::InvalidArgument(
            "Manifest version must not be empty".into(),
        ));
    }
    if manifest.files.is_empty() {
        return Err(SysError::InvalidArgument(
            "Manifest must contain at least one file".into(),
        ));
    }
    if manifest.release_date.is_empty() {
        return Err(SysError::InvalidArgument(
            "Manifest release_date must not be empty".into(),
        ));
    }
    if manifest.sha256_digest.is_empty() {
        return Err(SysError::InvalidArgument(
            "Manifest sha256_digest must not be empty".into(),
        ));
    }

    validate_version(&manifest.version)?;

    // Verify per-file hashes are well-formed hex (64 chars).
    for file in &manifest.files {
        if file.sha256.len() != 64 || !file.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SysError::InvalidArgument(format!(
                "File '{}' has invalid sha256 (expected 64 hex chars): '{}'",
                file.path, file.sha256
            )));
        }
    }

    // Compute canonical digest.
    let canonical = compute_manifest_digest(manifest);
    if canonical != manifest.sha256_digest {
        return Err(SysError::InvalidArgument(format!(
            "Manifest digest mismatch: expected {}, computed {}",
            manifest.sha256_digest, canonical
        )));
    }

    Ok(())
}

/// Compute the SHA-256 digest of the canonical manifest content.  This
/// covers version, channel, files, release_date, min_version, and changelog
/// but deliberately excludes the `sha256_digest` field itself.
fn compute_manifest_digest(manifest: &UpdateManifest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest.version.as_bytes());
    hasher.update(manifest.channel.to_string().as_bytes());
    for f in &manifest.files {
        hasher.update(f.path.as_bytes());
        hasher.update(f.sha256.as_bytes());
        hasher.update(f.size_bytes.to_le_bytes());
        hasher.update(if f.compressed { &[1u8] } else { &[0u8] });
        if let Some(ref delta) = f.delta_from {
            hasher.update(delta.as_bytes());
        }
    }
    hasher.update(manifest.release_date.as_bytes());
    if let Some(ref mv) = manifest.min_version {
        hasher.update(mv.as_bytes());
    }
    if let Some(ref cl) = manifest.changelog {
        hasher.update(cl.as_bytes());
    }
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Update check
// ---------------------------------------------------------------------------

/// Check the update server (or a local manifest file) for a newer version.
///
/// If `config.update_url` starts with `file://` or `/`, it is treated as a
/// local path to a manifest JSON file.  Otherwise a network fetch is
/// attempted (currently shells out to `curl`).
#[cfg(target_os = "linux")]
pub fn check_for_update(
    config: &UpdateConfig,
    current_version: &str,
) -> Result<Option<UpdateManifest>> {
    let json = if config.update_url.starts_with("file://") {
        let path = config.update_url.trim_start_matches("file://");
        std::fs::read_to_string(path)
            .map_err(|e| SysError::Unknown(format!("Failed to read local manifest {path}: {e}")))?
    } else if config.update_url.starts_with('/') {
        std::fs::read_to_string(&config.update_url).map_err(|e| {
            SysError::Unknown(format!(
                "Failed to read local manifest {}: {e}",
                config.update_url
            ))
        })?
    } else {
        let url = format!("{}/versions.json", config.update_url);
        let output = std::process::Command::new("curl")
            .args(["-fsSL", "--proto", "=https", "--max-redirs", "3", &url])
            .output()
            .map_err(|e| SysError::Unknown(format!("curl failed: {e}")))?;
        if !output.status.success() {
            return Err(SysError::Unknown(format!(
                "curl returned non-zero: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let manifest = parse_update_manifest(&json)?;
    if compare_versions(&manifest.version, current_version) == Ordering::Greater {
        Ok(Some(manifest))
    } else {
        Ok(None)
    }
}

#[cfg(not(target_os = "linux"))]
pub fn check_for_update(
    _config: &UpdateConfig,
    _current_version: &str,
) -> Result<Option<UpdateManifest>> {
    Err(SysError::NotSupported)
}

// ---------------------------------------------------------------------------
// Apply / verify / switch / rollback
// ---------------------------------------------------------------------------

/// Apply an update to the inactive slot.
///
/// On a real system this writes the update image to the block device of the
/// inactive slot.  Here we shell out to `dd` to write the first file listed
/// in the manifest.
#[cfg(target_os = "linux")]
pub fn apply_update(config: &UpdateConfig, manifest: &UpdateManifest) -> Result<()> {
    let current = get_current_slot().unwrap_or(UpdateSlot::A);
    let target_slot = current.other();
    let target_dev = config.device_for_slot(target_slot);

    if manifest.files.is_empty() {
        return Err(SysError::InvalidArgument(
            "Manifest contains no files to apply".into(),
        ));
    }

    let image_path = &manifest.files[0].path;

    // Validate image path: must be absolute, within the staging dir, and contain no traversal.
    // Use canonical path comparison to prevent prefix tricks (e.g. /var/lib/agnos/updates-evil/).
    let staging_dir = Path::new("/var/lib/agnos/updates");
    let image = Path::new(image_path);
    if !image.is_absolute() || image_path.contains("..") {
        return Err(SysError::InvalidArgument(format!(
            "Update image path must be absolute without traversal, got: {image_path}"
        )));
    }
    // Canonicalize both paths to resolve symlinks and verify containment.
    // If staging dir doesn't exist yet, fall back to string prefix check with trailing slash.
    let in_staging = if let (Ok(canon_staging), Ok(canon_image)) =
        (staging_dir.canonicalize(), image.canonicalize())
    {
        canon_image.starts_with(&canon_staging)
    } else {
        // Fallback: ensure the staging dir prefix includes a path separator
        let prefix = format!("{}/", staging_dir.display());
        image_path.starts_with(&prefix)
    };
    if !in_staging {
        return Err(SysError::InvalidArgument(format!(
            "Update image path must be within {}, got: {image_path}",
            staging_dir.display()
        )));
    }

    let status = std::process::Command::new("dd")
        .args([
            &format!("if={image_path}"),
            &format!("of={}", target_dev.display()),
            "bs=4M",
            "conv=fsync",
            "status=progress",
        ])
        .status()
        .map_err(|e| SysError::Unknown(format!("dd failed to start: {e}")))?;

    if !status.success() {
        return Err(SysError::Unknown(
            "dd returned non-zero while writing update image".into(),
        ));
    }

    // Persist pending state.
    let mut state = get_update_state(config).unwrap_or(UpdateState {
        current_slot: current,
        current_version: String::new(),
        pending_update: None,
        last_update: None,
        rollback_available: false,
        boot_count_since_update: 0,
    });
    state.pending_update = Some(manifest.version.clone());
    state.rollback_available = true;
    save_update_state(&state, &config.state_file)?;

    if config.verify_after_apply && !verify_update(config, manifest)? {
        return Err(SysError::Unknown("Post-apply verification failed".into()));
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_update(_config: &UpdateConfig, _manifest: &UpdateManifest) -> Result<()> {
    Err(SysError::NotSupported)
}

/// SHA-256 verify the data written to the inactive slot against the manifest.
#[cfg(target_os = "linux")]
pub fn verify_update(config: &UpdateConfig, manifest: &UpdateManifest) -> Result<bool> {
    if manifest.files.is_empty() {
        return Err(SysError::InvalidArgument(
            "Manifest contains no files to verify".into(),
        ));
    }

    let current = get_current_slot().unwrap_or(UpdateSlot::A);
    let target_dev = config.device_for_slot(current.other());

    let expected = &manifest.files[0].sha256;
    let size = manifest.files[0].size_bytes;

    // Read the written data using 4M blocks for performance, limiting to
    // the exact byte count via `count` and `iflag=count_bytes`.
    let output = std::process::Command::new("dd")
        .args([
            &format!("if={}", target_dev.display()),
            "bs=4M",
            &format!("count={size}"),
            "iflag=count_bytes",
        ])
        .output()
        .map_err(|e| SysError::Unknown(format!("dd read failed: {e}")))?;

    if !output.status.success() {
        return Err(SysError::Unknown("dd read returned non-zero".into()));
    }

    let mut hasher = Sha256::new();
    // Only hash the exact number of bytes requested (dd may read a full block).
    let data = if (output.stdout.len() as u64) > size {
        &output.stdout[..size as usize]
    } else {
        &output.stdout
    };
    hasher.update(data);
    let actual = hex::encode(hasher.finalize());

    Ok(actual == *expected)
}

#[cfg(not(target_os = "linux"))]
pub fn verify_update(_config: &UpdateConfig, _manifest: &UpdateManifest) -> Result<bool> {
    Err(SysError::NotSupported)
}

/// Mark a slot as active for the next boot.
///
/// Writes the slot marker file for argonaut to read at next boot.
/// Falls back to direct EFI variable if `efibootmgr` is available.
#[cfg(target_os = "linux")]
pub fn switch_active_slot(config: &UpdateConfig, slot: UpdateSlot) -> Result<()> {
    // Write slot marker for argonaut init system.
    let marker = PathBuf::from("/var/lib/agnos/next-slot");
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&marker, slot.partition_suffix())
        .map_err(|e| SysError::Unknown(format!("Failed to write slot marker: {e}")))?;

    // If efibootmgr is available, also set the EFI boot entry.
    let _ = std::process::Command::new("efibootmgr")
        .args([
            "--bootnext",
            &format!("AGNOS-{}", slot.partition_suffix().to_uppercase()),
        ])
        .status();

    // Update persistent state.
    let mut state = get_update_state(config).unwrap_or(UpdateState {
        current_slot: slot.other(),
        current_version: String::new(),
        pending_update: None,
        last_update: None,
        rollback_available: true,
        boot_count_since_update: 0,
    });
    state.current_slot = slot;
    state.boot_count_since_update = 0;
    save_update_state(&state, &config.state_file)?;

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn switch_active_slot(_config: &UpdateConfig, _slot: UpdateSlot) -> Result<()> {
    Err(SysError::NotSupported)
}

/// Rollback to the previous slot.
#[cfg(target_os = "linux")]
pub fn rollback(config: &UpdateConfig) -> Result<()> {
    let state = get_update_state(config)?;
    if !state.rollback_available {
        return Err(SysError::InvalidArgument(
            "No rollback image available".into(),
        ));
    }
    let previous = state.current_slot.other();

    // Stop services before switching (argonaut service management).
    for service in &["daimon", "hoosh"] {
        if let Err(e) = std::process::Command::new("argonaut")
            .args(["stop", service])
            .status()
        {
            tracing::warn!("Failed to stop {} before rollback: {}", service, e);
        }
    }

    switch_active_slot(config, previous)?;

    // Clear pending update.
    let mut new_state = get_update_state(config).unwrap_or(state.clone());
    new_state.pending_update = None;
    new_state.rollback_available = false;
    save_update_state(&new_state, &config.state_file)?;

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn rollback(_config: &UpdateConfig) -> Result<()> {
    Err(SysError::NotSupported)
}

/// Mark the current boot as successful: increment the boot counter and
/// clear any pending update flag.
pub fn mark_boot_successful(config: &UpdateConfig) -> Result<()> {
    let mut state = get_update_state(config)?;
    state.boot_count_since_update = state.boot_count_since_update.saturating_add(1);
    if state.pending_update.is_some() {
        state.pending_update = None;
        state.last_update = Some(chrono::Utc::now());
    }
    save_update_state(&state, &config.state_file)
}

/// Pure function: returns `true` if the system has exceeded the maximum
/// allowed boot attempts since an update, indicating an automatic rollback
/// should be triggered.
pub fn needs_rollback(state: &UpdateState, max_boot_attempts: u32) -> bool {
    state.pending_update.is_some() && state.boot_count_since_update >= max_boot_attempts
}

// ---------------------------------------------------------------------------
// Helper: build a test manifest with a valid digest
// ---------------------------------------------------------------------------

/// Build a minimal valid manifest (for use in tests and examples).
///
/// Computes the `sha256_digest` automatically so that `verify_manifest`
/// passes.
#[doc(hidden)]
pub fn build_test_manifest(version: &str, channel: UpdateChannel) -> UpdateManifest {
    let file_hash = "a".repeat(64); // valid hex
    let mut manifest = UpdateManifest {
        version: version.to_string(),
        channel,
        files: vec![UpdateFile {
            path: "rootfs.img".into(),
            sha256: file_hash,
            size_bytes: 1024,
            delta_from: None,
            compressed: false,
        }],
        release_date: "2026-03-06T00:00:00Z".into(),
        min_version: None,
        changelog: Some("Test release".into()),
        sha256_digest: String::new(),
    };
    manifest.sha256_digest = compute_manifest_digest(&manifest);
    manifest
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    // -- Slot operations --

    #[test]
    fn test_slot_other() {
        assert_eq!(UpdateSlot::A.other(), UpdateSlot::B);
        assert_eq!(UpdateSlot::B.other(), UpdateSlot::A);
    }

    #[test]
    fn test_slot_other_roundtrip() {
        assert_eq!(UpdateSlot::A.other().other(), UpdateSlot::A);
        assert_eq!(UpdateSlot::B.other().other(), UpdateSlot::B);
    }

    #[test]
    fn test_slot_partition_suffix() {
        assert_eq!(UpdateSlot::A.partition_suffix(), "a");
        assert_eq!(UpdateSlot::B.partition_suffix(), "b");
    }

    #[test]
    fn test_slot_display() {
        assert_eq!(format!("{}", UpdateSlot::A), "A");
        assert_eq!(format!("{}", UpdateSlot::B), "B");
    }

    // -- Version comparison --

    #[test]
    fn test_compare_versions_equal() {
        assert_eq!(compare_versions("2026.3.5", "2026.3.5"), Ordering::Equal);
    }

    #[test]
    fn test_compare_versions_less() {
        assert_eq!(compare_versions("2026.3.5", "2026.4.5"), Ordering::Less);
    }

    #[test]
    fn test_compare_versions_greater() {
        assert_eq!(
            compare_versions("2027.1.1", "2026.12.12"),
            Ordering::Greater
        );
    }

    #[test]
    fn test_compare_versions_month_difference() {
        assert_eq!(compare_versions("2026.3.5", "2026.3.6"), Ordering::Less);
    }

    #[test]
    fn test_compare_versions_day_difference() {
        assert_eq!(compare_versions("2026.1.5", "2026.3.5"), Ordering::Less);
    }

    #[test]
    fn test_compare_versions_malformed_fallback() {
        // Falls back to lexicographic.
        assert_eq!(compare_versions("abc", "def"), Ordering::Less);
    }

    // -- Version validation --

    #[test]
    fn test_validate_version_valid() {
        assert!(validate_version("2026.3.5").is_ok());
        assert!(validate_version("2026.15.12").is_ok());
    }

    #[test]
    fn test_validate_version_too_few_parts() {
        assert!(validate_version("2026.3").is_err());
    }

    #[test]
    fn test_validate_version_too_many_parts() {
        assert!(validate_version("2026.3.5.1").is_err());
    }

    #[test]
    fn test_validate_version_non_numeric() {
        assert!(validate_version("2026.x.5").is_err());
    }

    #[test]
    fn test_validate_version_year_out_of_range() {
        assert!(validate_version("2023.1.1").is_err());
    }

    #[test]
    fn test_validate_version_month_out_of_range() {
        assert!(validate_version("2026.3.13").is_err());
        assert!(validate_version("2026.3.0").is_err());
    }

    // -- Manifest parsing --

    #[test]
    fn test_parse_update_manifest_valid() {
        let manifest = build_test_manifest("2026.4.5", UpdateChannel::Stable);
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed = parse_update_manifest(&json).unwrap();
        assert_eq!(parsed.version, "2026.4.5");
        assert_eq!(parsed.files.len(), 1);
    }

    #[test]
    fn test_parse_update_manifest_invalid_json() {
        assert!(parse_update_manifest("not json").is_err());
    }

    #[test]
    fn test_verify_manifest_valid() {
        let manifest = build_test_manifest("2026.4.5", UpdateChannel::Stable);
        assert!(verify_manifest(&manifest).is_ok());
    }

    #[test]
    fn test_verify_manifest_bad_digest() {
        let mut manifest = build_test_manifest("2026.4.5", UpdateChannel::Stable);
        manifest.sha256_digest = "0".repeat(64);
        assert!(verify_manifest(&manifest).is_err());
    }

    #[test]
    fn test_verify_manifest_empty_version() {
        let mut manifest = build_test_manifest("2026.4.5", UpdateChannel::Stable);
        manifest.version = String::new();
        assert!(verify_manifest(&manifest).is_err());
    }

    #[test]
    fn test_verify_manifest_empty_files() {
        let mut manifest = build_test_manifest("2026.4.5", UpdateChannel::Stable);
        manifest.files.clear();
        // recompute digest so we test the "empty files" check, not digest mismatch
        manifest.sha256_digest = compute_manifest_digest(&manifest);
        assert!(verify_manifest(&manifest).is_err());
    }

    #[test]
    fn test_verify_manifest_bad_file_hash() {
        let mut manifest = build_test_manifest("2026.4.5", UpdateChannel::Stable);
        manifest.files[0].sha256 = "tooshort".into();
        manifest.sha256_digest = compute_manifest_digest(&manifest);
        assert!(verify_manifest(&manifest).is_err());
    }

    // -- State serialization --

    #[test]
    fn test_state_roundtrip() {
        let path = std::env::temp_dir().join("agnos-test-update-state-roundtrip.json");
        let state = UpdateState {
            current_slot: UpdateSlot::B,
            current_version: "2026.3.5".into(),
            pending_update: Some("2026.4.5".into()),
            last_update: Some(chrono::Utc::now()),
            rollback_available: true,
            boot_count_since_update: 2,
        };
        save_update_state(&state, &path).unwrap();
        let config = UpdateConfig {
            state_file: path,
            ..UpdateConfig::default()
        };
        let loaded = get_update_state(&config).unwrap();
        assert_eq!(loaded.current_slot, UpdateSlot::B);
        assert_eq!(loaded.current_version, "2026.3.5");
        assert_eq!(loaded.pending_update, Some("2026.4.5".into()));
        assert!(loaded.rollback_available);
        assert_eq!(loaded.boot_count_since_update, 2);
    }

    #[test]
    fn test_get_update_state_missing_file() {
        let config = UpdateConfig {
            state_file: PathBuf::from("/tmp/does-not-exist-agnos-test-state.json"),
            ..UpdateConfig::default()
        };
        assert!(get_update_state(&config).is_err());
    }

    // -- Rollback logic --

    #[test]
    fn test_needs_rollback_true() {
        let state = UpdateState {
            current_slot: UpdateSlot::A,
            current_version: "2026.3.5".into(),
            pending_update: Some("2026.4.5".into()),
            last_update: None,
            rollback_available: true,
            boot_count_since_update: 3,
        };
        assert!(needs_rollback(&state, 3));
    }

    #[test]
    fn test_needs_rollback_false_no_pending() {
        let state = UpdateState {
            current_slot: UpdateSlot::A,
            current_version: "2026.3.5".into(),
            pending_update: None,
            last_update: None,
            rollback_available: true,
            boot_count_since_update: 5,
        };
        assert!(!needs_rollback(&state, 3));
    }

    #[test]
    fn test_needs_rollback_false_low_count() {
        let state = UpdateState {
            current_slot: UpdateSlot::A,
            current_version: "2026.3.5".into(),
            pending_update: Some("2026.4.5".into()),
            last_update: None,
            rollback_available: true,
            boot_count_since_update: 1,
        };
        assert!(!needs_rollback(&state, 3));
    }

    // -- Phase display --

    #[test]
    fn test_phase_display() {
        assert_eq!(format!("{}", UpdatePhase::Downloading), "Downloading");
        assert_eq!(format!("{}", UpdatePhase::Verifying), "Verifying");
        assert_eq!(format!("{}", UpdatePhase::Applying), "Applying");
        assert_eq!(format!("{}", UpdatePhase::Finalizing), "Finalizing");
        assert_eq!(format!("{}", UpdatePhase::RollingBack), "Rolling back");
        assert_eq!(format!("{}", UpdatePhase::Complete), "Complete");
        assert_eq!(format!("{}", UpdatePhase::Failed), "Failed");
    }

    // -- Channel types --

    #[test]
    fn test_channel_display() {
        assert_eq!(format!("{}", UpdateChannel::Stable), "stable");
        assert_eq!(format!("{}", UpdateChannel::Beta), "beta");
        assert_eq!(format!("{}", UpdateChannel::Nightly), "nightly");
        assert_eq!(
            format!("{}", UpdateChannel::Custom("canary".into())),
            "custom:canary"
        );
    }

    #[test]
    fn test_channel_serde_roundtrip() {
        let channels = vec![
            UpdateChannel::Stable,
            UpdateChannel::Beta,
            UpdateChannel::Nightly,
            UpdateChannel::Custom("edge".into()),
        ];
        for ch in &channels {
            let json = serde_json::to_string(ch).unwrap();
            let parsed: UpdateChannel = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, ch);
        }
    }

    // -- Progress tracking --

    #[test]
    fn test_progress_clamps_percent() {
        let p = UpdateProgress::new(UpdatePhase::Downloading, 150, "test");
        assert_eq!(p.percent, 100);
    }

    #[test]
    fn test_progress_normal() {
        let p = UpdateProgress::new(UpdatePhase::Applying, 42, "writing image");
        assert_eq!(p.phase, UpdatePhase::Applying);
        assert_eq!(p.percent, 42);
        assert_eq!(p.message, "writing image");
    }

    // -- Config --

    #[test]
    fn test_config_device_for_slot() {
        let config = UpdateConfig::default();
        assert!(config
            .device_for_slot(UpdateSlot::A)
            .to_str()
            .unwrap()
            .contains("rootfs_a"));
        assert!(config
            .device_for_slot(UpdateSlot::B)
            .to_str()
            .unwrap()
            .contains("rootfs_b"));
    }

    // -- Manifest with delta --

    #[test]
    fn test_manifest_with_delta_from() {
        let file_hash = "b".repeat(64);
        let mut manifest = UpdateManifest {
            version: "2026.4.5".into(),
            channel: UpdateChannel::Stable,
            files: vec![UpdateFile {
                path: "rootfs.delta".into(),
                sha256: file_hash,
                size_bytes: 512,
                delta_from: Some("2026.3.5".into()),
                compressed: true,
            }],
            release_date: "2026-03-06T00:00:00Z".into(),
            min_version: Some("2026.3.5".into()),
            changelog: None,
            sha256_digest: String::new(),
        };
        manifest.sha256_digest = compute_manifest_digest(&manifest);
        assert!(verify_manifest(&manifest).is_ok());
        assert!(manifest.files[0].delta_from.is_some());
        assert!(manifest.files[0].compressed);
    }

    // -- Edge cases --

    #[test]
    fn test_slot_eq_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(UpdateSlot::A);
        set.insert(UpdateSlot::B);
        set.insert(UpdateSlot::A); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_compare_versions_year_boundary() {
        assert_eq!(compare_versions("2026.31.12", "2027.1.1"), Ordering::Less);
    }
}
