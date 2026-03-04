//! Mandatory Access Control (MAC) Interface
//!
//! Auto-detects the active Linux Security Module (SELinux or AppArmor) and
//! provides per-agent-type MAC profile management.
//!
//! On non-Linux platforms, `detect_mac_system()` returns `MacSystem::None`
//! and all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Which MAC system is active on this kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacSystem {
    SELinux,
    AppArmor,
    None,
}

impl std::fmt::Display for MacSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MacSystem::SELinux => write!(f, "SELinux"),
            MacSystem::AppArmor => write!(f, "AppArmor"),
            MacSystem::None => write!(f, "None"),
        }
    }
}

/// SELinux enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SELinuxMode {
    Enforcing,
    Permissive,
    Disabled,
}

impl std::fmt::Display for SELinuxMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SELinuxMode::Enforcing => write!(f, "Enforcing"),
            SELinuxMode::Permissive => write!(f, "Permissive"),
            SELinuxMode::Disabled => write!(f, "Disabled"),
        }
    }
}

/// AppArmor profile enforcement state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppArmorProfileState {
    Enforce,
    Complain,
    Unconfined,
}

impl std::fmt::Display for AppArmorProfileState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppArmorProfileState::Enforce => write!(f, "enforce"),
            AppArmorProfileState::Complain => write!(f, "complain"),
            AppArmorProfileState::Unconfined => write!(f, "unconfined"),
        }
    }
}

/// MAC profile for a specific agent type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMacProfile {
    /// Agent type this profile applies to (User, Service, System)
    pub agent_type: String,
    /// SELinux security context (e.g., `system_u:system_r:agnos_agent_user_t:s0`)
    pub selinux_context: Option<String>,
    /// AppArmor profile name (e.g., `agnos-agent-user`)
    pub apparmor_profile: Option<String>,
}

impl AgentMacProfile {
    /// Create a new profile for the given agent type.
    pub fn new(agent_type: impl Into<String>) -> Self {
        let agent_type = agent_type.into();
        let lower = agent_type.to_lowercase();
        Self {
            selinux_context: Some(format!(
                "system_u:system_r:agnos_agent_{}_t:s0",
                lower
            )),
            apparmor_profile: Some(format!("agnos-agent-{}", lower)),
            agent_type,
        }
    }

    /// Validate that the profile has the required fields for the given MAC system.
    pub fn validate(&self, mac_system: MacSystem) -> Result<()> {
        if self.agent_type.is_empty() {
            return Err(SysError::InvalidArgument("Agent type cannot be empty".into()));
        }
        match mac_system {
            MacSystem::SELinux => {
                let ctx = self.selinux_context.as_deref().unwrap_or("");
                if ctx.is_empty() {
                    return Err(SysError::InvalidArgument(
                        "SELinux context required but not set".into(),
                    ));
                }
                // SELinux context format: user:role:type:level
                if ctx.split(':').count() < 4 {
                    return Err(SysError::InvalidArgument(format!(
                        "Invalid SELinux context format (expected user:role:type:level): {}",
                        ctx
                    )));
                }
            }
            MacSystem::AppArmor => {
                let profile = self.apparmor_profile.as_deref().unwrap_or("");
                if profile.is_empty() {
                    return Err(SysError::InvalidArgument(
                        "AppArmor profile name required but not set".into(),
                    ));
                }
                if profile.contains('/') || profile.contains('\0') {
                    return Err(SysError::InvalidArgument(format!(
                        "Invalid AppArmor profile name: {}",
                        profile
                    )));
                }
            }
            MacSystem::None => {}
        }
        Ok(())
    }
}

/// Detect which MAC system is active on this kernel.
///
/// Reads `/sys/kernel/security/lsm` to determine the active LSMs.
/// Returns `MacSystem::SELinux` if SELinux is present, `MacSystem::AppArmor` if
/// AppArmor is present, or `MacSystem::None` if neither is active.
pub fn detect_mac_system() -> MacSystem {
    #[cfg(target_os = "linux")]
    {
        let lsm_path = "/sys/kernel/security/lsm";
        match std::fs::read_to_string(lsm_path) {
            Ok(contents) => {
                let lower = contents.to_lowercase();
                // Check SELinux first (higher priority if both are listed)
                if lower.contains("selinux") {
                    tracing::debug!("Detected MAC system: SELinux");
                    return MacSystem::SELinux;
                }
                if lower.contains("apparmor") {
                    tracing::debug!("Detected MAC system: AppArmor");
                    return MacSystem::AppArmor;
                }
                tracing::debug!("No supported MAC system found in: {}", contents.trim());
                MacSystem::None
            }
            Err(e) => {
                tracing::debug!("Cannot read {}: {} (MAC detection unavailable)", lsm_path, e);
                MacSystem::None
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        MacSystem::None
    }
}

/// Get the current SELinux enforcement mode.
pub fn get_selinux_mode() -> Result<SELinuxMode> {
    #[cfg(target_os = "linux")]
    {
        let enforce_path = "/sys/fs/selinux/enforce";
        if !Path::new(enforce_path).exists() {
            return Ok(SELinuxMode::Disabled);
        }
        let val = std::fs::read_to_string(enforce_path)
            .map_err(|e| SysError::Unknown(format!("Failed to read {}: {}", enforce_path, e)))?;
        match val.trim() {
            "1" => Ok(SELinuxMode::Enforcing),
            "0" => Ok(SELinuxMode::Permissive),
            _ => Ok(SELinuxMode::Disabled),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Set the SELinux enforcement mode (requires CAP_MAC_ADMIN).
pub fn set_selinux_mode(mode: SELinuxMode) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let enforce_path = "/sys/fs/selinux/enforce";
        if !Path::new(enforce_path).exists() {
            return Err(SysError::NotSupported);
        }
        let val = match mode {
            SELinuxMode::Enforcing => "1",
            SELinuxMode::Permissive => "0",
            SELinuxMode::Disabled => {
                return Err(SysError::InvalidArgument(
                    "Cannot disable SELinux at runtime; use kernel boot parameter".into(),
                ));
            }
        };
        std::fs::write(enforce_path, val)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => SysError::PermissionDenied,
                _ => SysError::Unknown(format!("Failed to write {}: {}", enforce_path, e)),
            })?;
        tracing::info!("SELinux mode set to {}", mode);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = mode;
        Err(SysError::NotSupported)
    }
}

/// Get the current SELinux security context of this process.
pub fn get_current_selinux_context() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let path = "/proc/self/attr/current";
        if !Path::new(path).exists() {
            return Err(SysError::NotSupported);
        }
        let ctx = std::fs::read_to_string(path)
            .map_err(|e| SysError::Unknown(format!("Failed to read {}: {}", path, e)))?;
        Ok(ctx.trim_end_matches('\0').trim().to_string())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Set the SELinux security context for this process.
///
/// If `on_exec` is true, the context will be applied on the next `exec()` call
/// (writes to `/proc/self/attr/exec`). Otherwise, applies immediately
/// (writes to `/proc/self/attr/current`).
pub fn set_selinux_context(context: &str, on_exec: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if context.is_empty() {
            return Err(SysError::InvalidArgument("SELinux context cannot be empty".into()));
        }
        if context.split(':').count() < 4 {
            return Err(SysError::InvalidArgument(format!(
                "Invalid SELinux context format: {}",
                context
            )));
        }

        let path = if on_exec {
            "/proc/self/attr/exec"
        } else {
            "/proc/self/attr/current"
        };

        if !Path::new(path).exists() {
            return Err(SysError::NotSupported);
        }

        std::fs::write(path, context).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => SysError::PermissionDenied,
            _ => SysError::Unknown(format!("Failed to write SELinux context to {}: {}", path, e)),
        })?;

        tracing::debug!(
            "Set SELinux context to {} (on_exec={})",
            context,
            on_exec
        );
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (context, on_exec);
        Err(SysError::NotSupported)
    }
}

/// Load an SELinux policy module from a .pp file.
///
/// Shells out to `semodule -i <path>`. Requires root.
pub fn load_selinux_module(module_path: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if !module_path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "SELinux module file not found: {}",
                module_path.display()
            )));
        }

        let output = std::process::Command::new("semodule")
            .arg("-i")
            .arg(module_path)
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run semodule: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "semodule -i failed: {}",
                stderr.trim()
            )));
        }

        tracing::info!("Loaded SELinux module: {}", module_path.display());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = module_path;
        Err(SysError::NotSupported)
    }
}

/// Remove an SELinux policy module by name.
///
/// Shells out to `semodule -r <name>`. Requires root.
pub fn remove_selinux_module(module_name: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if module_name.is_empty() {
            return Err(SysError::InvalidArgument("Module name cannot be empty".into()));
        }

        let output = std::process::Command::new("semodule")
            .arg("-r")
            .arg(module_name)
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run semodule: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SysError::Unknown(format!(
                "semodule -r failed: {}",
                stderr.trim()
            )));
        }

        tracing::info!("Removed SELinux module: {}", module_name);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = module_name;
        Err(SysError::NotSupported)
    }
}

/// Load an AppArmor profile from a file path.
///
/// Writes the profile content to `/sys/kernel/security/apparmor/.load`.
pub fn load_apparmor_profile(profile_path: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if !profile_path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "AppArmor profile not found: {}",
                profile_path.display()
            )));
        }

        let load_path = "/sys/kernel/security/apparmor/.load";
        if !Path::new(load_path).exists() {
            return Err(SysError::NotSupported);
        }

        let profile_content = std::fs::read(profile_path)
            .map_err(|e| SysError::Unknown(format!("Failed to read profile: {}", e)))?;

        std::fs::write(load_path, &profile_content).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => SysError::PermissionDenied,
            _ => SysError::Unknown(format!("Failed to load AppArmor profile: {}", e)),
        })?;

        tracing::info!("Loaded AppArmor profile from: {}", profile_path.display());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = profile_path;
        Err(SysError::NotSupported)
    }
}

/// Change the AppArmor profile of the current process.
///
/// Writes to `/proc/self/attr/current` with the `changeprofile <name>` command.
pub fn apparmor_change_profile(profile_name: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if profile_name.is_empty() {
            return Err(SysError::InvalidArgument(
                "AppArmor profile name cannot be empty".into(),
            ));
        }
        if profile_name.contains('/') || profile_name.contains('\0') {
            return Err(SysError::InvalidArgument(format!(
                "Invalid AppArmor profile name: {}",
                profile_name
            )));
        }

        let attr_path = "/proc/self/attr/current";
        if !Path::new(attr_path).exists() {
            return Err(SysError::NotSupported);
        }

        let command = format!("changeprofile {}", profile_name);
        std::fs::write(attr_path, &command).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => SysError::PermissionDenied,
            _ => SysError::Unknown(format!("AppArmor changeprofile failed: {}", e)),
        })?;

        tracing::debug!("Changed AppArmor profile to: {}", profile_name);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = profile_name;
        Err(SysError::NotSupported)
    }
}

/// Return default MAC profiles for the three standard AGNOS agent types.
pub fn default_agent_profiles() -> Vec<AgentMacProfile> {
    vec![
        AgentMacProfile::new("User"),
        AgentMacProfile::new("Service"),
        AgentMacProfile::new("System"),
    ]
}

/// Auto-detect the active MAC system and apply the appropriate profile.
///
/// Finds the matching profile for `agent_type` from the provided list.
/// If no MAC system is active, logs a warning and returns Ok.
pub fn apply_agent_mac_profile(
    agent_type: &str,
    profiles: &[AgentMacProfile],
) -> Result<()> {
    let mac_system = detect_mac_system();

    if mac_system == MacSystem::None {
        tracing::warn!("No MAC system active — skipping MAC profile application for agent type '{}'", agent_type);
        return Ok(());
    }

    let profile = profiles
        .iter()
        .find(|p| p.agent_type.eq_ignore_ascii_case(agent_type))
        .ok_or_else(|| {
            SysError::InvalidArgument(format!(
                "No MAC profile found for agent type '{}'",
                agent_type
            ))
        })?;

    profile.validate(mac_system)?;

    match mac_system {
        MacSystem::SELinux => {
            let context = profile.selinux_context.as_deref().unwrap_or("");
            tracing::info!(
                "Applying SELinux context '{}' for agent type '{}'",
                context,
                agent_type
            );
            set_selinux_context(context, true)?;
        }
        MacSystem::AppArmor => {
            let profile_name = profile.apparmor_profile.as_deref().unwrap_or("");
            tracing::info!(
                "Applying AppArmor profile '{}' for agent type '{}'",
                profile_name,
                agent_type
            );
            apparmor_change_profile(profile_name)?;
        }
        MacSystem::None => unreachable!(),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_system_display() {
        assert_eq!(MacSystem::SELinux.to_string(), "SELinux");
        assert_eq!(MacSystem::AppArmor.to_string(), "AppArmor");
        assert_eq!(MacSystem::None.to_string(), "None");
    }

    #[test]
    fn test_selinux_mode_display() {
        assert_eq!(SELinuxMode::Enforcing.to_string(), "Enforcing");
        assert_eq!(SELinuxMode::Permissive.to_string(), "Permissive");
        assert_eq!(SELinuxMode::Disabled.to_string(), "Disabled");
    }

    #[test]
    fn test_apparmor_profile_state_display() {
        assert_eq!(AppArmorProfileState::Enforce.to_string(), "enforce");
        assert_eq!(AppArmorProfileState::Complain.to_string(), "complain");
        assert_eq!(AppArmorProfileState::Unconfined.to_string(), "unconfined");
    }

    #[test]
    fn test_agent_mac_profile_new() {
        let profile = AgentMacProfile::new("User");
        assert_eq!(profile.agent_type, "User");
        assert_eq!(
            profile.selinux_context.as_deref(),
            Some("system_u:system_r:agnos_agent_user_t:s0")
        );
        assert_eq!(
            profile.apparmor_profile.as_deref(),
            Some("agnos-agent-user")
        );
    }

    #[test]
    fn test_agent_mac_profile_new_service() {
        let profile = AgentMacProfile::new("Service");
        assert_eq!(
            profile.selinux_context.as_deref(),
            Some("system_u:system_r:agnos_agent_service_t:s0")
        );
        assert_eq!(
            profile.apparmor_profile.as_deref(),
            Some("agnos-agent-service")
        );
    }

    #[test]
    fn test_agent_mac_profile_validate_selinux_ok() {
        let profile = AgentMacProfile::new("User");
        assert!(profile.validate(MacSystem::SELinux).is_ok());
    }

    #[test]
    fn test_agent_mac_profile_validate_selinux_bad_context() {
        let profile = AgentMacProfile {
            agent_type: "User".to_string(),
            selinux_context: Some("bad_context".to_string()),
            apparmor_profile: None,
        };
        assert!(profile.validate(MacSystem::SELinux).is_err());
    }

    #[test]
    fn test_agent_mac_profile_validate_selinux_missing() {
        let profile = AgentMacProfile {
            agent_type: "User".to_string(),
            selinux_context: None,
            apparmor_profile: None,
        };
        assert!(profile.validate(MacSystem::SELinux).is_err());
    }

    #[test]
    fn test_agent_mac_profile_validate_apparmor_ok() {
        let profile = AgentMacProfile::new("User");
        assert!(profile.validate(MacSystem::AppArmor).is_ok());
    }

    #[test]
    fn test_agent_mac_profile_validate_apparmor_bad_name() {
        let profile = AgentMacProfile {
            agent_type: "User".to_string(),
            selinux_context: None,
            apparmor_profile: Some("bad/name".to_string()),
        };
        assert!(profile.validate(MacSystem::AppArmor).is_err());
    }

    #[test]
    fn test_agent_mac_profile_validate_apparmor_missing() {
        let profile = AgentMacProfile {
            agent_type: "User".to_string(),
            selinux_context: None,
            apparmor_profile: None,
        };
        assert!(profile.validate(MacSystem::AppArmor).is_err());
    }

    #[test]
    fn test_agent_mac_profile_validate_none() {
        let profile = AgentMacProfile::new("User");
        assert!(profile.validate(MacSystem::None).is_ok());
    }

    #[test]
    fn test_agent_mac_profile_validate_empty_type() {
        let profile = AgentMacProfile {
            agent_type: String::new(),
            selinux_context: None,
            apparmor_profile: None,
        };
        assert!(profile.validate(MacSystem::None).is_err());
    }

    #[test]
    fn test_default_agent_profiles() {
        let profiles = default_agent_profiles();
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[0].agent_type, "User");
        assert_eq!(profiles[1].agent_type, "Service");
        assert_eq!(profiles[2].agent_type, "System");
    }

    #[test]
    fn test_detect_mac_system() {
        // This is platform-dependent; just verify it doesn't crash
        let system = detect_mac_system();
        // On most dev machines it will be AppArmor or None
        assert!(matches!(
            system,
            MacSystem::SELinux | MacSystem::AppArmor | MacSystem::None
        ));
    }

    #[test]
    fn test_set_selinux_context_validation() {
        // Empty context
        let result = set_selinux_context("", false);
        assert!(result.is_err());

        // Bad format (not enough components)
        let result = set_selinux_context("user:role", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_apparmor_change_profile_validation() {
        // Empty name
        let result = apparmor_change_profile("");
        assert!(result.is_err());

        // Name with slash
        let result = apparmor_change_profile("bad/name");
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_agent_mac_profile_no_match() {
        let profiles = default_agent_profiles();
        // If no MAC system active, should warn and succeed
        let result = apply_agent_mac_profile("NonExistent", &profiles);
        let mac = detect_mac_system();
        if mac == MacSystem::None {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_agent_mac_profile_serialization() {
        let profile = AgentMacProfile::new("User");
        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: AgentMacProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_type, "User");
        assert_eq!(deserialized.selinux_context, profile.selinux_context);
        assert_eq!(deserialized.apparmor_profile, profile.apparmor_profile);
    }

    #[test]
    #[ignore = "Requires SELinux active and CAP_MAC_ADMIN"]
    fn test_get_selinux_mode_live() {
        let mode = get_selinux_mode().unwrap();
        assert!(matches!(
            mode,
            SELinuxMode::Enforcing | SELinuxMode::Permissive | SELinuxMode::Disabled
        ));
    }

    #[test]
    #[ignore = "Requires SELinux active and root"]
    fn test_get_current_selinux_context_live() {
        let ctx = get_current_selinux_context().unwrap();
        assert!(!ctx.is_empty());
    }
}
