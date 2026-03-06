//! Bootloader Configuration Management
//!
//! Supports both systemd-boot and GRUB2 bootloaders. Auto-detects which
//! bootloader is installed and provides a unified interface for reading/writing
//! boot configuration, managing boot entries, and modifying kernel command lines.
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Detected bootloader type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bootloader {
    SystemdBoot,
    Grub2,
    Unknown,
}

impl std::fmt::Display for Bootloader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Bootloader::SystemdBoot => write!(f, "systemd-boot"),
            Bootloader::Grub2 => write!(f, "GRUB2"),
            Bootloader::Unknown => write!(f, "unknown"),
        }
    }
}

/// A single boot entry (kernel + initrd + options).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootEntry {
    /// Unique identifier (filename stem for systemd-boot, menuentry index/id for GRUB2)
    pub id: String,
    /// Human-readable title shown in the boot menu
    pub title: String,
    /// Path to the kernel image (e.g. `/boot/vmlinuz-6.6.0-agnos`)
    pub linux: PathBuf,
    /// Path to the initramfs image
    pub initrd: Option<PathBuf>,
    /// Kernel command line options
    pub options: String,
    /// Whether this entry is the current default
    pub is_default: bool,
    /// Kernel version string (parsed from the entry or filename)
    pub version: Option<String>,
}

/// Aggregate boot configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootConfig {
    /// Detected bootloader
    pub bootloader: Bootloader,
    /// Boot menu timeout in seconds
    pub timeout_secs: u32,
    /// ID of the default boot entry
    pub default_entry: Option<String>,
    /// All available boot entries
    pub entries: Vec<BootEntry>,
}

// ---------------------------------------------------------------------------
// Well-known paths
// ---------------------------------------------------------------------------

const SYSTEMD_BOOT_DIR: &str = "/boot/efi/EFI/systemd";
const SYSTEMD_LOADER_CONF: &str = "/boot/loader/loader.conf";
const SYSTEMD_ENTRIES_DIR: &str = "/boot/loader/entries";

const GRUB_CFG: &str = "/boot/grub/grub.cfg";
const GRUB_DEFAULT_FILE: &str = "/etc/default/grub";

/// Kernel command-line tokens that must never appear (security-sensitive).
const DANGEROUS_CMDLINE_TOKENS: &[&str] = &[
    "init=/bin/sh",
    "init=/bin/bash",
    "init=/bin/dash",
    "init=/sbin/init.debug",
    "single",
    "emergency",
    "rd.break",
    "debug_shell",
    "systemd.debug-shell",
];

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect the installed bootloader by probing well-known paths.
///
/// Checks for systemd-boot first (presence of `/boot/efi/EFI/systemd/`),
/// then GRUB2 (`/boot/grub/grub.cfg`). Returns `Bootloader::Unknown` if
/// neither is found.
pub fn detect_bootloader() -> Result<Bootloader> {
    #[cfg(target_os = "linux")]
    {
        if Path::new(SYSTEMD_BOOT_DIR).is_dir() {
            return Ok(Bootloader::SystemdBoot);
        }
        if Path::new(GRUB_CFG).is_file() {
            return Ok(Bootloader::Grub2);
        }
        Ok(Bootloader::Unknown)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

// ---------------------------------------------------------------------------
// Reading configuration
// ---------------------------------------------------------------------------

/// Read the full boot configuration (bootloader type, timeout, default entry, and all entries).
pub fn read_boot_config() -> Result<BootConfig> {
    #[cfg(target_os = "linux")]
    {
        let bootloader = detect_bootloader()?;
        match bootloader {
            Bootloader::SystemdBoot => read_systemd_boot_config(),
            Bootloader::Grub2 => read_grub2_config(),
            Bootloader::Unknown => Err(SysError::Unknown(
                "No supported bootloader detected".into(),
            )),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// List all available boot entries for the detected bootloader.
pub fn list_boot_entries() -> Result<Vec<BootEntry>> {
    let config = read_boot_config()?;
    Ok(config.entries)
}

/// Get the current default boot entry.
pub fn get_default_entry() -> Result<BootEntry> {
    let config = read_boot_config()?;
    config
        .entries
        .into_iter()
        .find(|e| e.is_default)
        .or(None)
        .ok_or_else(|| SysError::Unknown("No default boot entry found".into()))
}

/// Set the default boot entry by its ID.
pub fn set_default_entry(id: &str) -> Result<()> {
    validate_entry_id(id)?;

    #[cfg(target_os = "linux")]
    {
        let bootloader = detect_bootloader()?;
        match bootloader {
            Bootloader::SystemdBoot => set_systemd_boot_default(id),
            Bootloader::Grub2 => set_grub2_default(id),
            Bootloader::Unknown => Err(SysError::Unknown(
                "No supported bootloader detected".into(),
            )),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = id;
        Err(SysError::NotSupported)
    }
}

/// Modify the kernel command line for a specific boot entry.
pub fn set_kernel_cmdline(entry_id: &str, options: &str) -> Result<()> {
    validate_entry_id(entry_id)?;
    validate_kernel_cmdline(options)?;

    #[cfg(target_os = "linux")]
    {
        let bootloader = detect_bootloader()?;
        match bootloader {
            Bootloader::SystemdBoot => set_systemd_boot_cmdline(entry_id, options),
            Bootloader::Grub2 => set_grub2_cmdline(entry_id, options),
            Bootloader::Unknown => Err(SysError::Unknown(
                "No supported bootloader detected".into(),
            )),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (entry_id, options);
        Err(SysError::NotSupported)
    }
}

/// Set the boot menu timeout in seconds (capped at 300).
pub fn set_timeout(seconds: u32) -> Result<()> {
    if seconds > 300 {
        return Err(SysError::InvalidArgument(format!(
            "Timeout {} exceeds maximum of 300 seconds",
            seconds
        )));
    }

    #[cfg(target_os = "linux")]
    {
        let bootloader = detect_bootloader()?;
        match bootloader {
            Bootloader::SystemdBoot => set_systemd_boot_timeout(seconds),
            Bootloader::Grub2 => set_grub2_timeout(seconds),
            Bootloader::Unknown => Err(SysError::Unknown(
                "No supported bootloader detected".into(),
            )),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = seconds;
        Err(SysError::NotSupported)
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Validate kernel command-line parameters.
///
/// Rejects dangerous options such as `init=/bin/sh`, `single`, `rd.break`,
/// and `debug_shell` that could bypass normal boot security.
pub fn validate_kernel_cmdline(options: &str) -> Result<()> {
    if options.len() > 4096 {
        return Err(SysError::InvalidArgument(
            "Kernel command line exceeds 4096-character limit".into(),
        ));
    }

    let lower = options.to_lowercase();
    for token in DANGEROUS_CMDLINE_TOKENS {
        if lower.contains(&token.to_lowercase()) {
            return Err(SysError::InvalidArgument(format!(
                "Kernel command line contains dangerous parameter: {}",
                token
            )));
        }
    }

    // Reject non-printable / non-ASCII characters
    if options.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Err(SysError::InvalidArgument(
            "Kernel command line contains non-printable or non-ASCII characters".into(),
        ));
    }

    Ok(())
}

/// Validate that a boot-entry ID is well-formed.
fn validate_entry_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(SysError::InvalidArgument("Boot entry ID cannot be empty".into()));
    }
    if id.len() > 256 {
        return Err(SysError::InvalidArgument("Boot entry ID too long (max 256)".into()));
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err(SysError::InvalidArgument(format!(
            "Boot entry ID contains invalid characters: {}",
            id
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// systemd-boot implementation
// ---------------------------------------------------------------------------

/// Parse `/boot/loader/loader.conf` and `/boot/loader/entries/*.conf`.
#[cfg(target_os = "linux")]
fn read_systemd_boot_config() -> Result<BootConfig> {
    let (timeout, default_id) = parse_loader_conf(Path::new(SYSTEMD_LOADER_CONF))?;
    let entries = parse_systemd_boot_entries(Path::new(SYSTEMD_ENTRIES_DIR), default_id.as_deref())?;

    Ok(BootConfig {
        bootloader: Bootloader::SystemdBoot,
        timeout_secs: timeout,
        default_entry: default_id,
        entries,
    })
}

/// Parse `loader.conf` for `timeout` and `default` directives.
#[cfg(target_os = "linux")]
fn parse_loader_conf(path: &Path) -> Result<(u32, Option<String>)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", path.display(), e))
    })?;

    let mut timeout: u32 = 5; // sensible default
    let mut default_id: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(val) = line.strip_prefix("timeout") {
            let val = val.trim();
            if let Ok(t) = val.parse::<u32>() {
                timeout = t;
            }
        } else if let Some(val) = line.strip_prefix("default") {
            let val = val.trim().trim_end_matches(".conf");
            if !val.is_empty() {
                default_id = Some(val.to_string());
            }
        }
    }

    Ok((timeout, default_id))
}

/// Parse all `*.conf` files in the systemd-boot entries directory.
#[cfg(target_os = "linux")]
fn parse_systemd_boot_entries(dir: &Path, default_id: Option<&str>) -> Result<Vec<BootEntry>> {
    let mut entries = Vec::new();

    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        SysError::Unknown(format!("Failed to read entries dir {}: {}", dir.display(), e))
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| SysError::Unknown(format!("Dir entry error: {}", e)))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("conf") {
            continue;
        }

        if let Ok(boot_entry) = parse_systemd_boot_entry(&path, default_id) {
            entries.push(boot_entry);
        }
    }

    // Sort by ID for deterministic output
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(entries)
}

/// Parse a single systemd-boot entry file.
#[cfg(target_os = "linux")]
fn parse_systemd_boot_entry(path: &Path, default_id: Option<&str>) -> Result<BootEntry> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        SysError::Unknown(format!("Failed to read entry {}: {}", path.display(), e))
    })?;

    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut title = String::new();
    let mut linux = PathBuf::new();
    let mut initrd: Option<PathBuf> = None;
    let mut options = String::new();
    let mut version: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(val) = line.strip_prefix("title") {
            title = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("linux") {
            linux = PathBuf::from(val.trim());
        } else if let Some(val) = line.strip_prefix("initrd") {
            initrd = Some(PathBuf::from(val.trim()));
        } else if let Some(val) = line.strip_prefix("options") {
            options = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("version") {
            version = Some(val.trim().to_string());
        }
    }

    let is_default = default_id.is_some_and(|d| d == id);

    Ok(BootEntry {
        id,
        title,
        linux,
        initrd,
        options,
        is_default,
        version,
    })
}

#[cfg(target_os = "linux")]
fn set_systemd_boot_default(id: &str) -> Result<()> {
    let conf_path = Path::new(SYSTEMD_LOADER_CONF);
    let content = std::fs::read_to_string(conf_path).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", conf_path.display(), e))
    })?;

    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("default") {
            lines.push(format!("default {}.conf", id));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("default {}.conf", id));
    }

    let new_content = lines.join("\n") + "\n";
    std::fs::write(conf_path, new_content).map_err(|e| {
        SysError::Unknown(format!("Failed to write {}: {}", conf_path.display(), e))
    })?;

    tracing::info!("Set systemd-boot default entry to: {}", id);
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_systemd_boot_cmdline(entry_id: &str, options: &str) -> Result<()> {
    let entry_path = Path::new(SYSTEMD_ENTRIES_DIR).join(format!("{}.conf", entry_id));
    if !entry_path.exists() {
        return Err(SysError::InvalidArgument(format!(
            "Boot entry not found: {}",
            entry_id
        )));
    }

    let content = std::fs::read_to_string(&entry_path).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", entry_path.display(), e))
    })?;

    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("options") {
            lines.push(format!("options {}", options));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("options {}", options));
    }

    let new_content = lines.join("\n") + "\n";
    std::fs::write(&entry_path, new_content).map_err(|e| {
        SysError::Unknown(format!("Failed to write {}: {}", entry_path.display(), e))
    })?;

    tracing::info!("Set kernel cmdline for {} to: {}", entry_id, options);
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_systemd_boot_timeout(seconds: u32) -> Result<()> {
    let conf_path = Path::new(SYSTEMD_LOADER_CONF);
    let content = std::fs::read_to_string(conf_path).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", conf_path.display(), e))
    })?;

    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("timeout") {
            lines.push(format!("timeout {}", seconds));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("timeout {}", seconds));
    }

    let new_content = lines.join("\n") + "\n";
    std::fs::write(conf_path, new_content).map_err(|e| {
        SysError::Unknown(format!("Failed to write {}: {}", conf_path.display(), e))
    })?;

    tracing::info!("Set systemd-boot timeout to: {}s", seconds);
    Ok(())
}

// ---------------------------------------------------------------------------
// GRUB2 implementation
// ---------------------------------------------------------------------------

/// Read GRUB2 configuration by parsing `grub.cfg` and `/etc/default/grub`.
#[cfg(target_os = "linux")]
fn read_grub2_config() -> Result<BootConfig> {
    let timeout = parse_grub_default_timeout()?;
    let (entries, default_id) = parse_grub_cfg(Path::new(GRUB_CFG))?;

    Ok(BootConfig {
        bootloader: Bootloader::Grub2,
        timeout_secs: timeout,
        default_entry: default_id.clone(),
        entries,
    })
}

/// Parse `GRUB_TIMEOUT` from `/etc/default/grub`.
#[cfg(target_os = "linux")]
fn parse_grub_default_timeout() -> Result<u32> {
    let content = std::fs::read_to_string(GRUB_DEFAULT_FILE).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", GRUB_DEFAULT_FILE, e))
    })?;

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("GRUB_TIMEOUT=") {
            let val = val.trim().trim_matches('"');
            if let Ok(t) = val.parse::<u32>() {
                return Ok(t);
            }
        }
    }

    Ok(5) // default
}

/// Parse `grub.cfg` for menuentry blocks.
///
/// Extracts title, linux, initrd, and options from each menuentry.
#[cfg(target_os = "linux")]
fn parse_grub_cfg(path: &Path) -> Result<(Vec<BootEntry>, Option<String>)> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", path.display(), e))
    })?;

    let mut entries = Vec::new();
    let mut default_entry: Option<String> = None;
    let mut entry_index: usize = 0;

    // Parse `set default="..."` directive
    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("set default=\"") {
            if let Some(val) = val.strip_suffix('"') {
                default_entry = Some(val.to_string());
            }
        }
    }

    // Parse menuentry blocks
    let mut in_entry = false;
    let mut title = String::new();
    let mut linux = PathBuf::new();
    let mut initrd: Option<PathBuf> = None;
    let mut options = String::new();

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("menuentry ") {
            in_entry = true;
            // Extract title from: menuentry 'Title' ... {
            title = line
                .split('\'')
                .nth(1)
                .or_else(|| line.split('"').nth(1))
                .unwrap_or("Unknown")
                .to_string();
            linux = PathBuf::new();
            initrd = None;
            options = String::new();
        } else if in_entry && line == "}" {
            let id = entry_index.to_string();
            let is_default = default_entry
                .as_ref()
                .map_or(entry_index == 0, |d| d == &id || d == &title);

            let version = extract_version_from_path(&linux);

            entries.push(BootEntry {
                id,
                title: title.clone(),
                linux: linux.clone(),
                initrd: initrd.clone(),
                options: options.clone(),
                is_default,
                version,
            });
            entry_index += 1;
            in_entry = false;
        } else if in_entry {
            if let Some(rest) = line.strip_prefix("linux") {
                // "linux /boot/vmlinuz-... root=UUID=... quiet"
                // or "linux16 ..."
                let rest = rest.trim_start_matches("16").trim();
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if !parts.is_empty() {
                    linux = PathBuf::from(parts[0]);
                }
                if parts.len() > 1 {
                    options = parts[1].to_string();
                }
            } else if let Some(rest) = line.strip_prefix("initrd") {
                let rest = rest.trim_start_matches("16").trim();
                if !rest.is_empty() {
                    initrd = Some(PathBuf::from(rest));
                }
            }
        }
    }

    Ok((entries, default_entry))
}

/// Try to extract a kernel version from a path like `/boot/vmlinuz-6.6.0-agnos`.
fn extract_version_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_str()?;
    let version = filename.strip_prefix("vmlinuz-")?;
    Some(version.to_string())
}

#[cfg(target_os = "linux")]
fn set_grub2_default(id: &str) -> Result<()> {
    let output = std::process::Command::new("grub-set-default")
        .arg(id)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run grub-set-default: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "grub-set-default failed: {}",
            stderr.trim()
        )));
    }

    tracing::info!("Set GRUB2 default entry to: {}", id);
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_grub2_cmdline(entry_id: &str, options: &str) -> Result<()> {
    // GRUB2 kernel cmdline is typically set via /etc/default/grub
    // For per-entry modification, we update GRUB_CMDLINE_LINUX_DEFAULT
    // and regenerate grub.cfg.
    let content = std::fs::read_to_string(GRUB_DEFAULT_FILE).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", GRUB_DEFAULT_FILE, e))
    })?;

    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("GRUB_CMDLINE_LINUX_DEFAULT=") {
            lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", options));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", options));
    }

    let new_content = lines.join("\n") + "\n";
    std::fs::write(GRUB_DEFAULT_FILE, new_content).map_err(|e| {
        SysError::Unknown(format!("Failed to write {}: {}", GRUB_DEFAULT_FILE, e))
    })?;

    // Regenerate grub.cfg
    let output = std::process::Command::new("grub-mkconfig")
        .arg("-o")
        .arg(GRUB_CFG)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run grub-mkconfig: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "grub-mkconfig failed: {}",
            stderr.trim()
        )));
    }

    tracing::info!(
        "Set GRUB2 kernel cmdline for entry {} and regenerated grub.cfg",
        entry_id
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_grub2_timeout(seconds: u32) -> Result<()> {
    let content = std::fs::read_to_string(GRUB_DEFAULT_FILE).map_err(|e| {
        SysError::Unknown(format!("Failed to read {}: {}", GRUB_DEFAULT_FILE, e))
    })?;

    let mut lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("GRUB_TIMEOUT=") {
            lines.push(format!("GRUB_TIMEOUT={}", seconds));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("GRUB_TIMEOUT={}", seconds));
    }

    let new_content = lines.join("\n") + "\n";
    std::fs::write(GRUB_DEFAULT_FILE, new_content).map_err(|e| {
        SysError::Unknown(format!("Failed to write {}: {}", GRUB_DEFAULT_FILE, e))
    })?;

    tracing::info!("Set GRUB2 timeout to: {}s", seconds);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Bootloader enum tests --

    #[test]
    fn test_bootloader_display() {
        assert_eq!(Bootloader::SystemdBoot.to_string(), "systemd-boot");
        assert_eq!(Bootloader::Grub2.to_string(), "GRUB2");
        assert_eq!(Bootloader::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_bootloader_equality() {
        assert_eq!(Bootloader::SystemdBoot, Bootloader::SystemdBoot);
        assert_ne!(Bootloader::SystemdBoot, Bootloader::Grub2);
        assert_ne!(Bootloader::Grub2, Bootloader::Unknown);
    }

    #[test]
    fn test_bootloader_clone() {
        let b = Bootloader::Grub2;
        let b2 = b;
        assert_eq!(b, b2);
    }

    // -- Validation tests --

    #[test]
    fn test_validate_cmdline_safe() {
        assert!(validate_kernel_cmdline("root=UUID=abc quiet splash").is_ok());
    }

    #[test]
    fn test_validate_cmdline_empty() {
        assert!(validate_kernel_cmdline("").is_ok());
    }

    #[test]
    fn test_validate_cmdline_rejects_init_bin_sh() {
        let result = validate_kernel_cmdline("root=UUID=abc init=/bin/sh");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dangerous"), "Error: {}", err);
    }

    #[test]
    fn test_validate_cmdline_rejects_init_bin_bash() {
        let result = validate_kernel_cmdline("root=UUID=abc init=/bin/bash quiet");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cmdline_rejects_single() {
        assert!(validate_kernel_cmdline("root=UUID=abc single").is_err());
    }

    #[test]
    fn test_validate_cmdline_rejects_rd_break() {
        assert!(validate_kernel_cmdline("rd.break root=UUID=abc").is_err());
    }

    #[test]
    fn test_validate_cmdline_rejects_debug_shell() {
        assert!(validate_kernel_cmdline("systemd.debug-shell quiet").is_err());
    }

    #[test]
    fn test_validate_cmdline_rejects_emergency() {
        assert!(validate_kernel_cmdline("emergency").is_err());
    }

    #[test]
    fn test_validate_cmdline_rejects_too_long() {
        let long_line = "a".repeat(4097);
        let result = validate_kernel_cmdline(&long_line);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("4096"));
    }

    #[test]
    fn test_validate_cmdline_rejects_non_ascii() {
        let result = validate_kernel_cmdline("root=UUID=abc quiet\x00hidden");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-printable"));
    }

    #[test]
    fn test_validate_cmdline_case_insensitive_dangerous() {
        // The check should be case-insensitive
        assert!(validate_kernel_cmdline("INIT=/BIN/SH").is_err());
    }

    // -- Entry ID validation --

    #[test]
    fn test_validate_entry_id_valid() {
        assert!(validate_entry_id("agnos-6.6.0").is_ok());
        assert!(validate_entry_id("entry_1").is_ok());
        assert!(validate_entry_id("0").is_ok());
    }

    #[test]
    fn test_validate_entry_id_empty() {
        assert!(validate_entry_id("").is_err());
    }

    #[test]
    fn test_validate_entry_id_too_long() {
        let long_id = "a".repeat(257);
        assert!(validate_entry_id(&long_id).is_err());
    }

    #[test]
    fn test_validate_entry_id_invalid_chars() {
        assert!(validate_entry_id("entry id with spaces").is_err());
        assert!(validate_entry_id("entry;drop").is_err());
    }

    // -- Timeout validation --

    #[test]
    fn test_set_timeout_rejects_over_300() {
        let result = set_timeout(301);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("300"));
    }

    #[test]
    fn test_set_timeout_boundary_300_accepted() {
        // 300 should not fail validation (will fail on actual write since no bootloader,
        // but should pass the bounds check)
        let result = set_timeout(300);
        // On non-linux or without a real bootloader, this may fail for other reasons,
        // but not the bounds check
        if let Err(e) = result {
            assert!(
                !e.to_string().contains("300 exceeds"),
                "300 should be accepted: {}",
                e
            );
        }
    }

    // -- Version extraction --

    #[test]
    fn test_extract_version_from_path() {
        let p = PathBuf::from("/boot/vmlinuz-6.6.0-agnos");
        assert_eq!(
            extract_version_from_path(&p),
            Some("6.6.0-agnos".to_string())
        );
    }

    #[test]
    fn test_extract_version_no_prefix() {
        let p = PathBuf::from("/boot/bzImage");
        assert_eq!(extract_version_from_path(&p), None);
    }

    // -- Serialization round-trips --

    #[test]
    fn test_bootloader_serde_roundtrip() {
        let b = Bootloader::SystemdBoot;
        let json = serde_json::to_string(&b).unwrap();
        let b2: Bootloader = serde_json::from_str(&json).unwrap();
        assert_eq!(b, b2);
    }

    #[test]
    fn test_boot_entry_serde_roundtrip() {
        let entry = BootEntry {
            id: "agnos-6.6.0".to_string(),
            title: "AGNOS 6.6.0".to_string(),
            linux: PathBuf::from("/boot/vmlinuz-6.6.0-agnos"),
            initrd: Some(PathBuf::from("/boot/initramfs-6.6.0-agnos.img")),
            options: "root=UUID=abc quiet splash".to_string(),
            is_default: true,
            version: Some("6.6.0-agnos".to_string()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let entry2: BootEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, entry2);
    }

    #[test]
    fn test_boot_config_serde_roundtrip() {
        let config = BootConfig {
            bootloader: Bootloader::Grub2,
            timeout_secs: 10,
            default_entry: Some("0".to_string()),
            entries: vec![BootEntry {
                id: "0".to_string(),
                title: "AGNOS".to_string(),
                linux: PathBuf::from("/boot/vmlinuz"),
                initrd: None,
                options: "quiet".to_string(),
                is_default: true,
                version: None,
            }],
        };
        let json = serde_json::to_string(&config).unwrap();
        let config2: BootConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.bootloader, config2.bootloader);
        assert_eq!(config.timeout_secs, config2.timeout_secs);
        assert_eq!(config.default_entry, config2.default_entry);
        assert_eq!(config.entries.len(), config2.entries.len());
        assert_eq!(config.entries[0], config2.entries[0]);
    }

    // -- Detection (runs on the host; result depends on environment) --

    #[test]
    fn test_detect_bootloader_returns_ok() {
        // Should never panic — returns Ok(Unknown) if neither bootloader is present
        let result = detect_bootloader();
        assert!(result.is_ok());
    }

    // -- Dangerous tokens coverage --

    #[test]
    fn test_all_dangerous_tokens_rejected() {
        for token in DANGEROUS_CMDLINE_TOKENS {
            let result = validate_kernel_cmdline(token);
            assert!(
                result.is_err(),
                "Expected rejection for dangerous token: {}",
                token
            );
        }
    }
}
