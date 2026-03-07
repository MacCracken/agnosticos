//! udev Device Management Interface
//!
//! Userland wrappers for udev device enumeration, monitoring, and rule management.
//! Shells out to `udevadm` (part of systemd/eudev).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Information about a single device as reported by udev / sysfs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Kernel sysfs path (e.g. `/sys/devices/pci0000:00/...`)
    pub syspath: String,
    /// Kernel devpath (relative, e.g. `/devices/pci0000:00/...`)
    pub devpath: String,
    /// Subsystem the device belongs to (e.g. `block`, `net`)
    pub subsystem: String,
    /// Device type within the subsystem (optional)
    pub devtype: Option<String>,
    /// Kernel driver bound to this device (optional)
    pub driver: Option<String>,
    /// Device node path (e.g. `/dev/sda`, optional)
    pub devnode: Option<String>,
    /// All udev properties as key-value pairs
    pub properties: HashMap<String, String>,
}

/// Well-known device subsystems.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceSubsystem {
    Block,
    Net,
    Input,
    Usb,
    Pci,
    Tty,
    Gpu,
    Sound,
    Other(String),
}

impl DeviceSubsystem {
    /// Parse a subsystem string into the enum.
    pub fn parse(s: &str) -> Self {
        match s {
            "block" => Self::Block,
            "net" => Self::Net,
            "input" => Self::Input,
            "usb" => Self::Usb,
            "pci" => Self::Pci,
            "tty" => Self::Tty,
            "drm" => Self::Gpu,
            "sound" | "snd" => Self::Sound,
            other => Self::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for DeviceSubsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Block => write!(f, "block"),
            Self::Net => write!(f, "net"),
            Self::Input => write!(f, "input"),
            Self::Usb => write!(f, "usb"),
            Self::Pci => write!(f, "pci"),
            Self::Tty => write!(f, "tty"),
            Self::Gpu => write!(f, "drm"),
            Self::Sound => write!(f, "sound"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// A udev rule definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UdevRule {
    /// Rule file stem (e.g. `99-agnos-agent`), without `.rules` extension
    pub name: String,
    /// Match conditions (`SUBSYSTEM=="net"`, `ATTR{idVendor}=="1234"`, etc.)
    pub match_attrs: Vec<(String, String)>,
    /// Assignment actions (`MODE="0660"`, `OWNER="agnos"`, `TAG+="systemd"`, etc.)
    pub actions: Vec<(String, String)>,
}

/// Lifecycle events emitted by the kernel / udev.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceEvent {
    Add,
    Remove,
    Change,
    Bind,
    Unbind,
}

impl std::fmt::Display for DeviceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add => write!(f, "add"),
            Self::Remove => write!(f, "remove"),
            Self::Change => write!(f, "change"),
            Self::Bind => write!(f, "bind"),
            Self::Unbind => write!(f, "unbind"),
        }
    }
}

impl DeviceEvent {
    /// Parse a string into a `DeviceEvent`.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "add" => Ok(Self::Add),
            "remove" => Ok(Self::Remove),
            "change" => Ok(Self::Change),
            "bind" => Ok(Self::Bind),
            "unbind" => Ok(Self::Unbind),
            other => Err(SysError::InvalidArgument(format!(
                "Unknown device event: {}",
                other
            ))),
        }
    }
}

/// Configuration for spawning a udev monitor process.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceMonitorConfig {
    /// Only report events for this subsystem (e.g. `"block"`)
    pub subsystem_filter: Option<String>,
    /// Only report events for this devtype within the subsystem
    pub devtype_filter: Option<String>,
}

// ---------------------------------------------------------------------------
// Dangerous / invalid action keys that must be rejected in rules
// ---------------------------------------------------------------------------

/// Action keys that are potentially dangerous in udev rules.
const DANGEROUS_ACTIONS: &[&str] = &["RUN", "PROGRAM", "IMPORT{program}", "IMPORT{builtin}"];

/// Valid udev assignment action keys.
const VALID_ACTION_KEYS: &[&str] = &[
    "MODE",
    "OWNER",
    "GROUP",
    "TAG",
    "TAG+=",
    "SYMLINK",
    "SYMLINK+=",
    "ENV",
    "ATTR",
    "NAME",
    "OPTIONS",
];

/// Valid udev match keys.
const VALID_MATCH_KEYS: &[&str] = &[
    "SUBSYSTEM",
    "KERNEL",
    "DRIVER",
    "ATTR",
    "ATTRS",
    "ACTION",
    "DEVPATH",
    "ENV",
    "TAG",
];

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// List devices, optionally filtered by subsystem.
///
/// Uses `udevadm info --export-db` and parses the output.
pub fn list_devices(subsystem: Option<&str>) -> Result<Vec<DeviceInfo>> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("udevadm")
            .args(["info", "--export-db"])
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run udevadm: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "udevadm info --export-db failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices = parse_export_db(&stdout)?;

        match subsystem {
            Some(sub) => Ok(devices.into_iter().filter(|d| d.subsystem == sub).collect()),
            None => Ok(devices),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = subsystem;
        Err(SysError::NotSupported)
    }
}

/// Get detailed information about a single device by its sysfs path.
pub fn get_device_info(syspath: &str) -> Result<DeviceInfo> {
    #[cfg(target_os = "linux")]
    {
        if syspath.is_empty() {
            return Err(SysError::InvalidArgument("syspath cannot be empty".into()));
        }

        let output = std::process::Command::new("udevadm")
            .args(["info", &format!("--path={}", syspath)])
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run udevadm: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "udevadm info failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_udevadm_info(&stdout)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = syspath;
        Err(SysError::NotSupported)
    }
}

/// Parse the output of `udevadm info` for a single device.
///
/// Expected line prefixes:
/// - `P:` — devpath (sysfs path relative to `/sys`)
/// - `N:` — device node name (relative to `/dev`)
/// - `S:` — symlink (relative to `/dev`)
/// - `E:` — property key=value
pub fn parse_udevadm_info(output: &str) -> Result<DeviceInfo> {
    let mut devpath = String::new();
    let mut devnode: Option<String> = None;
    let mut properties: HashMap<String, String> = HashMap::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(val) = line.strip_prefix("P: ") {
            devpath = val.to_string();
        } else if let Some(val) = line.strip_prefix("N: ") {
            devnode = Some(format!("/dev/{}", val));
        } else if let Some(val) = line.strip_prefix("S: ") {
            // Accumulate symlinks into property
            let existing = properties.entry("DEVLINKS".to_string()).or_default();
            if !existing.is_empty() {
                existing.push(' ');
            }
            existing.push_str(&format!("/dev/{}", val));
        } else if let Some(val) = line.strip_prefix("E: ") {
            if let Some((k, v)) = val.split_once('=') {
                properties.insert(k.to_string(), v.to_string());
            }
        }
    }

    if devpath.is_empty() {
        return Err(SysError::InvalidArgument(
            "udevadm output missing P: (devpath) line".into(),
        ));
    }

    let syspath = format!("/sys{}", devpath);
    let subsystem = properties.get("SUBSYSTEM").cloned().unwrap_or_default();
    let devtype = properties.get("DEVTYPE").cloned();
    let driver = properties.get("DRIVER").cloned();

    // devnode from N: line or DEVNAME property
    let devnode = devnode.or_else(|| properties.get("DEVNAME").cloned());

    Ok(DeviceInfo {
        syspath,
        devpath,
        subsystem,
        devtype,
        driver,
        devnode,
        properties,
    })
}

/// Parse the full `udevadm info --export-db` output into a list of devices.
///
/// Device records are separated by blank lines. Each record uses the same
/// P:/N:/S:/E: prefix format as single-device output.
fn parse_export_db(output: &str) -> Result<Vec<DeviceInfo>> {
    let mut devices = Vec::new();
    let mut current_block = String::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            if !current_block.is_empty() {
                // Only include blocks that have a P: line
                if current_block.contains("\nP: ") || current_block.starts_with("P: ") {
                    if let Ok(dev) = parse_udevadm_info(&current_block) {
                        devices.push(dev);
                    }
                }
                current_block.clear();
            }
        } else {
            if !current_block.is_empty() {
                current_block.push('\n');
            }
            current_block.push_str(line);
        }
    }

    // Handle trailing block without final newline
    if !current_block.is_empty()
        && (current_block.contains("\nP: ") || current_block.starts_with("P: "))
    {
        if let Ok(dev) = parse_udevadm_info(&current_block) {
            devices.push(dev);
        }
    }

    Ok(devices)
}

/// Trigger a udev re-evaluation for a device.
pub fn trigger_device(syspath: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if syspath.is_empty() {
            return Err(SysError::InvalidArgument("syspath cannot be empty".into()));
        }

        let output = std::process::Command::new("udevadm")
            .args(["trigger", "--action=change", syspath])
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run udevadm trigger: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "udevadm trigger failed: {}",
                stderr
            )));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = syspath;
        Err(SysError::NotSupported)
    }
}

/// Render a `UdevRule` to the string content of a `.rules` file.
///
/// This is a pure function with no side effects.
pub fn render_udev_rule(rule: &UdevRule) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Match conditions use `==`
    for (key, value) in &rule.match_attrs {
        parts.push(format!("{}==\"{}\"", key, value));
    }

    // Assignment actions
    for (key, value) in &rule.actions {
        if key.ends_with("+=") {
            // Append-style keys already include the operator
            parts.push(format!("{}\"{}\"", key, value));
        } else {
            parts.push(format!("{}=\"{}\"", key, value));
        }
    }

    let mut output = format!("# AGNOS udev rule: {}\n", rule.name);
    output.push_str(&parts.join(", "));
    output.push('\n');
    output
}

/// Write a udev rule file to the specified rules directory.
///
/// Returns the path to the written file.
pub fn write_udev_rule(rule: &UdevRule, rules_dir: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        validate_rule(rule)?;

        let filename = format!("{}.rules", rule.name);
        let path = rules_dir.join(&filename);

        // Validate the directory exists
        if !rules_dir.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Rules directory does not exist: {}",
                rules_dir.display()
            )));
        }

        let content = render_udev_rule(rule);
        std::fs::write(&path, &content)
            .map_err(|e| SysError::Unknown(format!("Failed to write rule file: {}", e)))?;

        Ok(path)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (rule, rules_dir);
        Err(SysError::NotSupported)
    }
}

/// Remove a udev rule file.
pub fn remove_udev_rule(name: &str, rules_dir: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if name.is_empty() {
            return Err(SysError::InvalidArgument(
                "Rule name cannot be empty".into(),
            ));
        }
        // Prevent path traversal
        if name.contains('/') || name.contains("..") {
            return Err(SysError::InvalidArgument(
                "Rule name contains invalid characters".into(),
            ));
        }

        let filename = format!("{}.rules", name);
        let path = rules_dir.join(&filename);

        if !path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Rule file does not exist: {}",
                path.display()
            )));
        }

        std::fs::remove_file(&path)
            .map_err(|e| SysError::Unknown(format!("Failed to remove rule file: {}", e)))?;

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (name, rules_dir);
        Err(SysError::NotSupported)
    }
}

/// Reload udev rules and trigger re-evaluation.
pub fn reload_udev_rules() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("udevadm")
            .args(["control", "--reload-rules"])
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to reload udev rules: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "udevadm control --reload-rules failed: {}",
                stderr
            )));
        }

        let output = std::process::Command::new("udevadm")
            .arg("trigger")
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to trigger udev: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "udevadm trigger failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Spawn a `udevadm monitor` child process for real-time device events.
///
/// The caller is responsible for reading stdout and waiting on the child.
pub fn monitor_devices(config: &DeviceMonitorConfig) -> Result<std::process::Child> {
    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("udevadm");
        cmd.arg("monitor").arg("--udev").arg("--property");

        if let Some(ref sub) = config.subsystem_filter {
            if let Some(ref devtype) = config.devtype_filter {
                cmd.arg(format!("--subsystem-match={}:{}", sub, devtype));
            } else {
                cmd.arg(format!("--subsystem-match={}", sub));
            }
        }

        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| SysError::Unknown(format!("Failed to spawn udevadm monitor: {}", e)))?;

        Ok(child)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(SysError::NotSupported)
    }
}

/// Validate a udev rule for correctness and safety.
///
/// Rejects rules with:
/// - Empty name or match conditions
/// - Path traversal in name
/// - Dangerous action keys (`RUN`, `PROGRAM`, `IMPORT{program}`)
/// - Unknown match or action keys
pub fn validate_rule(rule: &UdevRule) -> Result<()> {
    // Name validation
    if rule.name.is_empty() {
        return Err(SysError::InvalidArgument(
            "Rule name cannot be empty".into(),
        ));
    }
    if rule.name.len() > 128 {
        return Err(SysError::InvalidArgument(
            "Rule name too long (max 128)".into(),
        ));
    }
    if !rule
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SysError::InvalidArgument(format!(
            "Rule name contains invalid characters: {}",
            rule.name
        )));
    }

    // Must have at least one match condition
    if rule.match_attrs.is_empty() {
        return Err(SysError::InvalidArgument(
            "Rule must have at least one match condition".into(),
        ));
    }

    // Validate match keys
    for (key, value) in &rule.match_attrs {
        // Extract the base key (before any `{...}` qualifier)
        let base_key = key.split('{').next().unwrap_or(key);
        if !VALID_MATCH_KEYS.contains(&base_key) {
            return Err(SysError::InvalidArgument(format!(
                "Invalid match key: {}",
                key
            )));
        }
        if value.is_empty() {
            return Err(SysError::InvalidArgument(format!(
                "Empty value for match key: {}",
                key
            )));
        }
    }

    // Validate action keys — reject dangerous ones
    for (key, _value) in &rule.actions {
        let base_key = key.split('{').next().unwrap_or(key);
        // Strip trailing += for comparison
        let base_key_stripped = base_key.trim_end_matches("+=");

        if DANGEROUS_ACTIONS.contains(&key.as_str()) || DANGEROUS_ACTIONS.contains(&base_key) {
            return Err(SysError::InvalidArgument(format!(
                "Dangerous action key rejected: {}",
                key
            )));
        }

        // Check if it is a valid action key (check both with and without +=)
        if !VALID_ACTION_KEYS.contains(&key.as_str())
            && !VALID_ACTION_KEYS.contains(&base_key)
            && !VALID_ACTION_KEYS.contains(&base_key_stripped)
        {
            return Err(SysError::InvalidArgument(format!(
                "Invalid action key: {}",
                key
            )));
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

    // -- parse_udevadm_info ------------------------------------------------

    #[test]
    fn test_parse_udevadm_info_basic() {
        let output = "\
P: /devices/pci0000:00/0000:00:1f.2/ata1/host0/target0:0:0/0:0:0:0/block/sda
N: sda
S: disk/by-id/ata-VBOX_HARDDISK_VB12345678-abcdefgh
E: SUBSYSTEM=block
E: DEVTYPE=disk
E: DEVNAME=/dev/sda
E: DRIVER=sd
";
        let info = parse_udevadm_info(output).unwrap();
        assert_eq!(
            info.devpath,
            "/devices/pci0000:00/0000:00:1f.2/ata1/host0/target0:0:0/0:0:0:0/block/sda"
        );
        assert_eq!(
            info.syspath,
            "/sys/devices/pci0000:00/0000:00:1f.2/ata1/host0/target0:0:0/0:0:0:0/block/sda"
        );
        assert_eq!(info.subsystem, "block");
        assert_eq!(info.devtype, Some("disk".into()));
        assert_eq!(info.devnode, Some("/dev/sda".into()));
        assert_eq!(info.driver, Some("sd".into()));
    }

    #[test]
    fn test_parse_udevadm_info_devnode_from_n_line() {
        let output = "\
P: /devices/virtual/net/lo
N: lo
E: SUBSYSTEM=net
";
        let info = parse_udevadm_info(output).unwrap();
        // N: line takes priority when DEVNAME is absent
        assert_eq!(info.devnode, Some("/dev/lo".into()));
    }

    #[test]
    fn test_parse_udevadm_info_devnode_from_property() {
        let output = "\
P: /devices/virtual/tty/tty0
E: SUBSYSTEM=tty
E: DEVNAME=/dev/tty0
";
        let info = parse_udevadm_info(output).unwrap();
        assert_eq!(info.devnode, Some("/dev/tty0".into()));
    }

    #[test]
    fn test_parse_udevadm_info_missing_devpath() {
        let output = "E: SUBSYSTEM=block\n";
        let err = parse_udevadm_info(output).unwrap_err();
        assert!(err.to_string().contains("devpath"));
    }

    #[test]
    fn test_parse_udevadm_info_empty_input() {
        let err = parse_udevadm_info("").unwrap_err();
        assert!(err.to_string().contains("devpath"));
    }

    #[test]
    fn test_parse_udevadm_info_symlinks_accumulated() {
        let output = "\
P: /devices/pci0000:00/block/sda
N: sda
S: disk/by-id/ata-VBOX1
S: disk/by-path/pci-0000
E: SUBSYSTEM=block
";
        let info = parse_udevadm_info(output).unwrap();
        let devlinks = info.properties.get("DEVLINKS").unwrap();
        assert!(devlinks.contains("/dev/disk/by-id/ata-VBOX1"));
        assert!(devlinks.contains("/dev/disk/by-path/pci-0000"));
    }

    // -- parse_export_db ---------------------------------------------------

    #[test]
    fn test_parse_export_db_multiple_devices() {
        let output = "\
P: /devices/pci0000:00/block/sda
N: sda
E: SUBSYSTEM=block

P: /devices/virtual/net/lo
E: SUBSYSTEM=net

";
        let devices = parse_export_db(output).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].subsystem, "block");
        assert_eq!(devices[1].subsystem, "net");
    }

    #[test]
    fn test_parse_export_db_trailing_block_no_newline() {
        let output = "P: /devices/virtual/tty/tty0\nE: SUBSYSTEM=tty";
        let devices = parse_export_db(output).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].subsystem, "tty");
    }

    // -- DeviceSubsystem ---------------------------------------------------

    #[test]
    fn test_subsystem_from_str_known() {
        assert_eq!(DeviceSubsystem::parse("block"), DeviceSubsystem::Block);
        assert_eq!(DeviceSubsystem::parse("net"), DeviceSubsystem::Net);
        assert_eq!(DeviceSubsystem::parse("input"), DeviceSubsystem::Input);
        assert_eq!(DeviceSubsystem::parse("usb"), DeviceSubsystem::Usb);
        assert_eq!(DeviceSubsystem::parse("pci"), DeviceSubsystem::Pci);
        assert_eq!(DeviceSubsystem::parse("tty"), DeviceSubsystem::Tty);
        assert_eq!(DeviceSubsystem::parse("drm"), DeviceSubsystem::Gpu);
        assert_eq!(DeviceSubsystem::parse("sound"), DeviceSubsystem::Sound);
        assert_eq!(DeviceSubsystem::parse("snd"), DeviceSubsystem::Sound);
    }

    #[test]
    fn test_subsystem_from_str_other() {
        assert_eq!(
            DeviceSubsystem::parse("thunderbolt"),
            DeviceSubsystem::Other("thunderbolt".into())
        );
    }

    #[test]
    fn test_subsystem_display_roundtrip() {
        let subs = [
            DeviceSubsystem::Block,
            DeviceSubsystem::Net,
            DeviceSubsystem::Gpu,
        ];
        for sub in &subs {
            let s = sub.to_string();
            assert!(!s.is_empty());
        }
        assert_eq!(DeviceSubsystem::Other("nvme".into()).to_string(), "nvme");
    }

    // -- DeviceEvent -------------------------------------------------------

    #[test]
    fn test_device_event_display() {
        assert_eq!(DeviceEvent::Add.to_string(), "add");
        assert_eq!(DeviceEvent::Remove.to_string(), "remove");
        assert_eq!(DeviceEvent::Change.to_string(), "change");
        assert_eq!(DeviceEvent::Bind.to_string(), "bind");
        assert_eq!(DeviceEvent::Unbind.to_string(), "unbind");
    }

    #[test]
    fn test_device_event_from_str() {
        assert_eq!(DeviceEvent::parse("add").unwrap(), DeviceEvent::Add);
        assert_eq!(DeviceEvent::parse("remove").unwrap(), DeviceEvent::Remove);
        assert!(DeviceEvent::parse("invalid").is_err());
    }

    // -- render_udev_rule --------------------------------------------------

    #[test]
    fn test_render_udev_rule_basic() {
        let rule = UdevRule {
            name: "99-agnos-usb".to_string(),
            match_attrs: vec![
                ("SUBSYSTEM".into(), "usb".into()),
                ("ATTR{idVendor}".into(), "1234".into()),
            ],
            actions: vec![
                ("MODE".into(), "0660".into()),
                ("GROUP".into(), "agnos".into()),
            ],
        };
        let rendered = render_udev_rule(&rule);
        assert!(rendered.contains("# AGNOS udev rule: 99-agnos-usb"));
        assert!(rendered.contains("SUBSYSTEM==\"usb\""));
        assert!(rendered.contains("ATTR{idVendor}==\"1234\""));
        assert!(rendered.contains("MODE=\"0660\""));
        assert!(rendered.contains("GROUP=\"agnos\""));
    }

    #[test]
    fn test_render_udev_rule_append_operator() {
        let rule = UdevRule {
            name: "99-test".to_string(),
            match_attrs: vec![("SUBSYSTEM".into(), "net".into())],
            actions: vec![("TAG+=".into(), "systemd".into())],
        };
        let rendered = render_udev_rule(&rule);
        assert!(rendered.contains("TAG+=\"systemd\""));
    }

    // -- validate_rule -----------------------------------------------------

    #[test]
    fn test_validate_rule_valid() {
        let rule = UdevRule {
            name: "99-agnos-test".to_string(),
            match_attrs: vec![("SUBSYSTEM".into(), "block".into())],
            actions: vec![("MODE".into(), "0660".into())],
        };
        assert!(validate_rule(&rule).is_ok());
    }

    #[test]
    fn test_validate_rule_empty_name() {
        let rule = UdevRule {
            name: String::new(),
            match_attrs: vec![("SUBSYSTEM".into(), "block".into())],
            actions: vec![],
        };
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_validate_rule_dangerous_run() {
        let rule = UdevRule {
            name: "99-bad".to_string(),
            match_attrs: vec![("SUBSYSTEM".into(), "usb".into())],
            actions: vec![("RUN".into(), "/bin/malicious".into())],
        };
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.to_string().contains("Dangerous"));
    }

    #[test]
    fn test_validate_rule_dangerous_program() {
        let rule = UdevRule {
            name: "99-bad".to_string(),
            match_attrs: vec![("SUBSYSTEM".into(), "usb".into())],
            actions: vec![("PROGRAM".into(), "/bin/exploit".into())],
        };
        assert!(validate_rule(&rule).is_err());
    }

    #[test]
    fn test_validate_rule_no_match_attrs() {
        let rule = UdevRule {
            name: "99-empty".to_string(),
            match_attrs: vec![],
            actions: vec![("MODE".into(), "0660".into())],
        };
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.to_string().contains("match condition"));
    }

    #[test]
    fn test_validate_rule_invalid_match_key() {
        let rule = UdevRule {
            name: "99-bad-key".to_string(),
            match_attrs: vec![("BOGUS_KEY".into(), "value".into())],
            actions: vec![],
        };
        assert!(validate_rule(&rule).is_err());
    }

    #[test]
    fn test_validate_rule_empty_match_value() {
        let rule = UdevRule {
            name: "99-empty-val".to_string(),
            match_attrs: vec![("SUBSYSTEM".into(), "".into())],
            actions: vec![],
        };
        let err = validate_rule(&rule).unwrap_err();
        assert!(err.to_string().contains("Empty value"));
    }

    #[test]
    fn test_validate_rule_name_path_traversal() {
        let rule = UdevRule {
            name: "../etc/passwd".to_string(),
            match_attrs: vec![("SUBSYSTEM".into(), "block".into())],
            actions: vec![],
        };
        assert!(validate_rule(&rule).is_err());
    }

    // -- DeviceInfo serialization ------------------------------------------

    #[test]
    fn test_device_info_serialize_deserialize() {
        let info = DeviceInfo {
            syspath: "/sys/devices/pci0000:00/block/sda".into(),
            devpath: "/devices/pci0000:00/block/sda".into(),
            subsystem: "block".into(),
            devtype: Some("disk".into()),
            driver: None,
            devnode: Some("/dev/sda".into()),
            properties: {
                let mut m = HashMap::new();
                m.insert("ID_SERIAL".into(), "VBOX123".into());
                m
            },
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }

    #[test]
    fn test_udev_rule_serialize_deserialize() {
        let rule = UdevRule {
            name: "99-test".into(),
            match_attrs: vec![("SUBSYSTEM".into(), "net".into())],
            actions: vec![("MODE".into(), "0644".into())],
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: UdevRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule, back);
    }

    // -- DeviceMonitorConfig -----------------------------------------------

    #[test]
    fn test_device_monitor_config_default() {
        let config = DeviceMonitorConfig::default();
        assert!(config.subsystem_filter.is_none());
        assert!(config.devtype_filter.is_none());
    }
}
