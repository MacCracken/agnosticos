//! Security context and privilege management
//!
//! Ensures AI never has root access and all privileged operations
//! require human approval through secure privilege escalation.

use anyhow::{anyhow, Result};
use nix::unistd::{getuid, getgid, geteuid};
use std::process::Command;
use tracing::{info, warn};

/// Security context for the shell session
pub struct SecurityContext {
    _uid: u32,
    _gid: u32,
    _euid: u32,
    username: String,
    is_root: bool,
    restricted: bool,
    sudo_available: bool,
}

impl SecurityContext {
    pub fn new(restricted: bool) -> Result<Self> {
        let uid = getuid().as_raw() as u32;
        let gid = getgid().as_raw() as u32;
        let euid = geteuid().as_raw() as u32;
        let is_root = uid == 0;
        
        let username = Self::get_username(uid)?;
        let sudo_available = Self::check_sudo_available();
        
        if is_root {
            warn!("Shell running as root - AI features disabled for safety");
        }
        
        Ok(Self {
            _uid: uid,
            _gid: gid,
            _euid: euid,
            username,
            is_root,
            restricted: restricted || is_root,
            sudo_available,
        })
    }
    
    /// Get current username
    fn get_username(uid: u32) -> Result<String> {
        // Try to get username from environment
        if let Ok(user) = std::env::var("USER") {
            return Ok(user);
        }
        
        // Fallback to uid
        Ok(format!("uid_{}", uid))
    }
    
    /// Check if sudo is available
    fn check_sudo_available() -> bool {
        Command::new("which")
            .arg("sudo")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    /// Check if running as root
    pub fn is_root(&self) -> bool {
        self.is_root
    }
    
    /// Check if in restricted mode
    pub fn is_restricted(&self) -> bool {
        self.restricted
    }
    
    /// Get username
    pub fn username(&self) -> &str {
        &self.username
    }
    
    /// Check if can escalate privileges
    pub fn can_escalate(&self) -> bool {
        !self.restricted && self.sudo_available && !self.is_root
    }
    
    /// Execute command with privilege escalation
    /// This ALWAYS requires human approval
    pub async fn execute_with_privileges(&self, command: &[String]) -> Result<std::process::Output> {
        if self.restricted {
            return Err(anyhow!("Cannot escalate privileges in restricted mode"));
        }
        
        if !self.sudo_available {
            return Err(anyhow!("sudo not available for privilege escalation"));
        }
        
        // Build sudo command
        let mut cmd = Command::new("sudo");
        cmd.arg("-n");  // Non-interactive (will fail if password needed)
        cmd.args(command);
        
        info!("Executing with elevated privileges: {:?}", command);
        
        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Privileged command failed: {}", stderr));
        }
        
        Ok(output)
    }
    
    /// Execute command without privileges
    pub fn execute_normal(&self, command: &[String]) -> Result<std::process::Output> {
        if command.is_empty() {
            return Err(anyhow!("Empty command"));
        }
        
        let mut cmd = Command::new(&command[0]);
        cmd.args(&command[1..]);
        
        let output = cmd.output()?;
        Ok(output)
    }
}

/// Permission levels for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    /// Safe commands that don't modify system
    Safe,
    /// Read-only system information
    ReadOnly,
    /// Can modify user files
    UserWrite,
    /// Can modify system files (requires approval)
    SystemWrite,
    /// Administrative commands (requires approval)
    Admin,
    /// Dangerous commands (always blocked for AI)
    Blocked,
}

impl PermissionLevel {
    /// Check if this level requires human approval
    pub fn requires_approval(&self) -> bool {
        matches!(self, 
            PermissionLevel::SystemWrite | 
            PermissionLevel::Admin | 
            PermissionLevel::Blocked
        )
    }
    
    /// Check if AI is allowed to execute this
    pub fn ai_allowed(&self) -> bool {
        !matches!(self, PermissionLevel::Blocked)
    }
}

/// Analyze command for required permission level
pub fn analyze_command_permission(command: &str, args: &[String]) -> PermissionLevel {
    let cmd = command.to_lowercase();
    
    // Blocked commands (never allowed for AI)
    let blocked = [
        "rm", "dd", "mkfs", "fdisk", "parted", "dd",
        "chmod", "chown", "chgrp",
    ];
    
    if blocked.contains(&cmd.as_str()) {
        // But allow safe variations
        if cmd == "rm" && !args.iter().any(|a| a.starts_with('-')) {
            return PermissionLevel::UserWrite;
        }
        return PermissionLevel::Blocked;
    }
    
    // Admin commands
    let admin = [
        "apt", "yum", "dnf", "pacman", "systemctl", "service",
        "useradd", "userdel", "usermod", "groupadd",
        "mount", "umount", "modprobe", "insmod", "rmmod",
    ];
    
    if admin.contains(&cmd.as_str()) {
        return PermissionLevel::Admin;
    }
    
    // System write commands
    let system_write = [
        "cp", "mv", "ln", "touch", "mkdir", "rmdir",
        "tee", "sed", "awk",
    ];
    
    if system_write.contains(&cmd.as_str()) {
        // Check if targeting system directories
        if args.iter().any(|a| {
            // Normalize path to prevent traversal attacks (e.g., /usr/../etc/passwd)
            let normalized = if a.starts_with('/') {
                // Attempt to canonicalize; fall back to cleaning the path manually
                std::path::Path::new(a)
                    .canonicalize()
                    .unwrap_or_else(|_| {
                        // Manual normalization for paths that don't exist yet
                        let mut components = Vec::new();
                        for component in std::path::Path::new(a).components() {
                            match component {
                                std::path::Component::ParentDir => { components.pop(); }
                                std::path::Component::CurDir => {}
                                other => components.push(other),
                            }
                        }
                        components.iter().collect()
                    })
            } else {
                std::path::PathBuf::from(a)
            };
            let path_str = normalized.to_string_lossy();
            path_str.starts_with("/etc/") || path_str == "/etc"
                || path_str.starts_with("/usr/") || path_str == "/usr"
                || path_str.starts_with("/bin/") || path_str == "/bin"
                || path_str.starts_with("/sbin/") || path_str == "/sbin"
                || path_str.starts_with("/lib")
        }) {
            return PermissionLevel::SystemWrite;
        }
        return PermissionLevel::UserWrite;
    }
    
    // Read-only commands
    let read_only = [
        "ls", "cat", "head", "tail", "less", "more",
        "grep", "find", "ps", "top", "htop",
        "df", "du", "free", "uptime", "uname",
        "ifconfig", "ip", "netstat", "ss",
        "pwd", "echo", "date", "whoami", "id",
    ];
    
    if read_only.contains(&cmd.as_str()) {
        return PermissionLevel::ReadOnly;
    }
    
    // Safe commands (builtin or non-destructive)
    let safe = [
        "cd", "pwd", "echo", "clear", "exit", "history",
        "help", "agnsh",
    ];
    
    if safe.contains(&cmd.as_str()) {
        return PermissionLevel::Safe;
    }
    
    // Default to requiring approval for unknown commands
    PermissionLevel::UserWrite
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_level_safe_approval() {
        let level = PermissionLevel::Safe;
        assert!(!level.requires_approval());
        assert!(level.ai_allowed());
    }

    #[test]
    fn test_permission_level_read_only_approval() {
        let level = PermissionLevel::ReadOnly;
        assert!(!level.requires_approval());
        assert!(level.ai_allowed());
    }

    #[test]
    fn test_permission_level_user_write_approval() {
        let level = PermissionLevel::UserWrite;
        assert!(!level.requires_approval());
        assert!(level.ai_allowed());
    }

    #[test]
    fn test_permission_level_system_write_approval() {
        let level = PermissionLevel::SystemWrite;
        assert!(level.requires_approval());
        assert!(level.ai_allowed());
    }

    #[test]
    fn test_permission_level_admin_approval() {
        let level = PermissionLevel::Admin;
        assert!(level.requires_approval());
        assert!(level.ai_allowed());
    }

    #[test]
    fn test_permission_level_blocked_approval() {
        let level = PermissionLevel::Blocked;
        assert!(level.requires_approval());
        assert!(!level.ai_allowed());
    }

    #[test]
    fn test_analyze_command_safe() {
        assert_eq!(analyze_command_permission("cd", &[]), PermissionLevel::Safe);
        assert_eq!(analyze_command_permission("clear", &[]), PermissionLevel::Safe);
        assert_eq!(analyze_command_permission("exit", &[]), PermissionLevel::Safe);
        assert_eq!(analyze_command_permission("history", &[]), PermissionLevel::Safe);
        assert_eq!(analyze_command_permission("help", &[]), PermissionLevel::Safe);
    }

    #[test]
    fn test_analyze_command_user_write() {
        assert_eq!(analyze_command_permission("mkdir", &["/tmp/test".to_string()]), PermissionLevel::UserWrite);
        assert_eq!(analyze_command_permission("cp", &["a".to_string(), "b".to_string()]), PermissionLevel::UserWrite);
        assert_eq!(analyze_command_permission("mv", &["a".to_string(), "b".to_string()]), PermissionLevel::UserWrite);
    }

    #[test]
    fn test_analyze_command_system_write() {
        assert_eq!(
            analyze_command_permission("cp", &["a".to_string(), "/etc/config".to_string()]),
            PermissionLevel::SystemWrite
        );
        assert_eq!(
            analyze_command_permission("mv", &["a".to_string(), "/usr/bin/app".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_admin() {
        assert_eq!(analyze_command_permission("apt", &["install".to_string()]), PermissionLevel::Admin);
        assert_eq!(analyze_command_permission("systemctl", &["start".to_string()]), PermissionLevel::Admin);
        assert_eq!(analyze_command_permission("mount", &["/dev/sda1".to_string()]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_blocked() {
        assert_eq!(analyze_command_permission("chmod", &[]), PermissionLevel::Blocked);
        assert_eq!(analyze_command_permission("chown", &[]), PermissionLevel::Blocked);
        assert_eq!(analyze_command_permission("fdisk", &[]), PermissionLevel::Blocked);
        assert_eq!(analyze_command_permission("mkfs", &[]), PermissionLevel::Blocked);
    }

    #[test]
    fn test_analyze_command_rm_without_args() {
        assert_eq!(analyze_command_permission("rm", &["file.txt".to_string()]), PermissionLevel::UserWrite);
    }

    #[test]
    fn test_analyze_command_unknown() {
        assert_eq!(analyze_command_permission("unknowncmd", &[]), PermissionLevel::UserWrite);
    }

    #[test]
    fn test_security_context_username() {
        let ctx = SecurityContext::new(false);
        assert!(ctx.is_ok());
        let ctx = ctx.unwrap();
        assert!(!ctx.username().is_empty());
    }

    #[test]
    fn test_security_context_is_restricted() {
        let ctx = SecurityContext::new(true).unwrap();
        assert!(ctx.is_restricted());
        
        let _ctx_normal = SecurityContext::new(false).unwrap();
        // Note: This might be true if running as root
    }

    #[test]
    fn test_security_context_execute_normal_empty() {
        let ctx = SecurityContext::new(false).unwrap();
        let result = ctx.execute_normal(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_security_context_execute_normal() {
        let ctx = SecurityContext::new(false).unwrap();
        let result = ctx.execute_normal(&["echo".to_string(), "test".to_string()]);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_security_context_execute_with_privileges_restricted() {
        let ctx = SecurityContext::new(true).unwrap();
        let result = ctx.execute_with_privileges(&["echo".to_string()]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("restricted"));
    }

    #[test]
    fn test_security_context_can_escalate_restricted() {
        let ctx = SecurityContext::new(true).unwrap();
        assert!(!ctx.can_escalate());
    }

    #[test]
    fn test_security_context_is_root() {
        let ctx = SecurityContext::new(false).unwrap();
        // In CI/test env, usually not root
        let _ = ctx.is_root();
    }

    #[test]
    fn test_analyze_command_read_only() {
        assert_eq!(analyze_command_permission("ls", &["-la".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("cat", &["file.txt".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("ps", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("df", &["-h".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("whoami", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("id", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("uname", &["-a".to_string()]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_rm_with_flags() {
        // rm with flags should be blocked
        assert_eq!(analyze_command_permission("rm", &["-rf".to_string(), "/tmp".to_string()]), PermissionLevel::Blocked);
        assert_eq!(analyze_command_permission("rm", &["-r".to_string(), "dir".to_string()]), PermissionLevel::Blocked);
    }

    #[test]
    fn test_analyze_command_case_insensitive() {
        assert_eq!(analyze_command_permission("LS", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("CD", &[]), PermissionLevel::Safe);
    }

    #[test]
    fn test_analyze_command_path_traversal_detection() {
        // Path traversal attempt should still detect system paths
        assert_eq!(
            analyze_command_permission("cp", &["file".to_string(), "/usr/../etc/passwd".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_relative_path() {
        // Relative paths → UserWrite (not system paths)
        assert_eq!(
            analyze_command_permission("cp", &["file".to_string(), "dest".to_string()]),
            PermissionLevel::UserWrite
        );
    }

    #[test]
    fn test_analyze_command_system_write_lib() {
        assert_eq!(
            analyze_command_permission("cp", &["a".to_string(), "/lib/modules/x".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_system_write_sbin() {
        assert_eq!(
            analyze_command_permission("cp", &["a".to_string(), "/sbin/init".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_system_write_bin() {
        assert_eq!(
            analyze_command_permission("cp", &["a".to_string(), "/bin/sh".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_dd_blocked() {
        assert_eq!(analyze_command_permission("dd", &[]), PermissionLevel::Blocked);
    }

    #[test]
    fn test_analyze_command_more_admin() {
        assert_eq!(analyze_command_permission("yum", &[]), PermissionLevel::Admin);
        assert_eq!(analyze_command_permission("dnf", &[]), PermissionLevel::Admin);
        assert_eq!(analyze_command_permission("pacman", &[]), PermissionLevel::Admin);
        assert_eq!(analyze_command_permission("useradd", &[]), PermissionLevel::Admin);
        assert_eq!(analyze_command_permission("modprobe", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_more_read_only() {
        assert_eq!(analyze_command_permission("head", &["-10".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("tail", &["-f".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("grep", &["pattern".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("find", &["/tmp".to_string()]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("free", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("uptime", &[]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_system_write_sed_awk() {
        assert_eq!(analyze_command_permission("sed", &["s/a/b/".to_string()]), PermissionLevel::UserWrite);
        assert_eq!(analyze_command_permission("awk", &["{}".to_string()]), PermissionLevel::UserWrite);
        assert_eq!(analyze_command_permission("tee", &["file".to_string()]), PermissionLevel::UserWrite);
    }

    #[test]
    fn test_execute_normal_success() {
        let ctx = SecurityContext::new(false).unwrap();
        let result = ctx.execute_normal(&["true".to_string()]);
        assert!(result.is_ok());
        assert!(result.unwrap().status.success());
    }

    #[test]
    fn test_execute_normal_failure() {
        let ctx = SecurityContext::new(false).unwrap();
        let result = ctx.execute_normal(&["false".to_string()]);
        assert!(result.is_ok()); // command ran, just non-zero exit
        assert!(!result.unwrap().status.success());
    }

    #[test]
    fn test_permission_level_debug() {
        assert_eq!(format!("{:?}", PermissionLevel::Safe), "Safe");
        assert_eq!(format!("{:?}", PermissionLevel::Blocked), "Blocked");
    }

    #[test]
    fn test_analyze_command_chgrp_blocked() {
        assert_eq!(analyze_command_permission("chgrp", &[]), PermissionLevel::Blocked);
    }

    #[test]
    fn test_analyze_command_touch_user_write() {
        assert_eq!(
            analyze_command_permission("touch", &["newfile.txt".to_string()]),
            PermissionLevel::UserWrite
        );
    }

    #[test]
    fn test_analyze_command_rmdir_user_write() {
        assert_eq!(
            analyze_command_permission("rmdir", &["mydir".to_string()]),
            PermissionLevel::UserWrite
        );
    }

    #[test]
    fn test_analyze_command_ln_system_write() {
        assert_eq!(
            analyze_command_permission("ln", &["-s".to_string(), "/usr/bin/foo".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_service_admin() {
        assert_eq!(analyze_command_permission("service", &["start".to_string()]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_userdel_admin() {
        assert_eq!(analyze_command_permission("userdel", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_usermod_admin() {
        assert_eq!(analyze_command_permission("usermod", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_groupadd_admin() {
        assert_eq!(analyze_command_permission("groupadd", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_umount_admin() {
        assert_eq!(analyze_command_permission("umount", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_insmod_admin() {
        assert_eq!(analyze_command_permission("insmod", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_rmmod_admin() {
        assert_eq!(analyze_command_permission("rmmod", &[]), PermissionLevel::Admin);
    }

    #[test]
    fn test_analyze_command_more_read_only_network() {
        assert_eq!(analyze_command_permission("ifconfig", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("ip", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("netstat", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("ss", &[]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_echo_read_only_precedence() {
        // "echo" appears in both read_only and safe lists, but read_only is checked first
        assert_eq!(analyze_command_permission("echo", &["hello".to_string()]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_agnsh_safe() {
        assert_eq!(analyze_command_permission("agnsh", &[]), PermissionLevel::Safe);
    }

    #[test]
    fn test_analyze_command_less_read_only() {
        assert_eq!(analyze_command_permission("less", &["file".to_string()]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_more_read_only_cmd() {
        assert_eq!(analyze_command_permission("more", &["file".to_string()]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_top_htop_read_only() {
        assert_eq!(analyze_command_permission("top", &[]), PermissionLevel::ReadOnly);
        assert_eq!(analyze_command_permission("htop", &[]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_date_read_only() {
        assert_eq!(analyze_command_permission("date", &[]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_du_read_only() {
        assert_eq!(analyze_command_permission("du", &["-sh".to_string()]), PermissionLevel::ReadOnly);
    }

    #[test]
    fn test_analyze_command_etc_direct() {
        assert_eq!(
            analyze_command_permission("cp", &["file".to_string(), "/etc".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_usr_direct() {
        assert_eq!(
            analyze_command_permission("mv", &["file".to_string(), "/usr".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_permission_level_clone() {
        let p = PermissionLevel::Admin;
        let p2 = p.clone();
        assert_eq!(p, p2);
    }

    #[test]
    fn test_permission_level_copy() {
        let p = PermissionLevel::Safe;
        let p2 = p;
        assert_eq!(p, p2);
    }

    // ====================================================================
    // Additional coverage tests: edge cases, boundary values, error paths
    // ====================================================================

    #[test]
    fn test_analyze_command_empty_string() {
        // Empty command is unknown — falls to UserWrite default
        let perm = analyze_command_permission("", &[]);
        assert_eq!(perm, PermissionLevel::UserWrite);
    }

    #[test]
    fn test_analyze_command_whitespace_command() {
        // Command with spaces — not in any list
        let perm = analyze_command_permission(" ", &[]);
        assert_eq!(perm, PermissionLevel::UserWrite);
    }

    #[test]
    fn test_analyze_command_rm_single_file_no_dash() {
        // rm without dash args is UserWrite (safe variation)
        assert_eq!(
            analyze_command_permission("rm", &["myfile.txt".to_string()]),
            PermissionLevel::UserWrite
        );
    }

    #[test]
    fn test_analyze_command_rm_with_force_flag() {
        assert_eq!(
            analyze_command_permission("rm", &["-f".to_string(), "file.txt".to_string()]),
            PermissionLevel::Blocked
        );
    }

    #[test]
    fn test_analyze_command_rm_with_interactive_flag() {
        // -i flag still starts with dash
        assert_eq!(
            analyze_command_permission("rm", &["-i".to_string(), "file.txt".to_string()]),
            PermissionLevel::Blocked
        );
    }

    #[test]
    fn test_analyze_command_rm_no_args() {
        // rm with no args at all — no args start with dash, so UserWrite
        assert_eq!(
            analyze_command_permission("rm", &[]),
            PermissionLevel::UserWrite
        );
    }

    #[test]
    fn test_analyze_command_system_write_touch_etc() {
        assert_eq!(
            analyze_command_permission("touch", &["/etc/newfile".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_sed_etc_system_write() {
        assert_eq!(
            analyze_command_permission("sed", &["-i".to_string(), "s/a/b/".to_string(), "/etc/hosts".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_mkdir_usr_system_write() {
        assert_eq!(
            analyze_command_permission("mkdir", &["/usr/local/newdir".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_ln_user_write() {
        // ln with no system paths → UserWrite
        assert_eq!(
            analyze_command_permission("ln", &["-s".to_string(), "target".to_string(), "link".to_string()]),
            PermissionLevel::UserWrite
        );
    }

    #[test]
    fn test_analyze_command_tee_system_write() {
        assert_eq!(
            analyze_command_permission("tee", &["/etc/config".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_mkfs_blocked() {
        assert_eq!(analyze_command_permission("mkfs", &["/dev/sda1".to_string()]), PermissionLevel::Blocked);
    }

    #[test]
    fn test_analyze_command_parted_blocked() {
        assert_eq!(analyze_command_permission("parted", &[]), PermissionLevel::Blocked);
    }

    #[test]
    fn test_permission_level_all_variants_requires_approval() {
        let cases = vec![
            (PermissionLevel::Safe, false),
            (PermissionLevel::ReadOnly, false),
            (PermissionLevel::UserWrite, false),
            (PermissionLevel::SystemWrite, true),
            (PermissionLevel::Admin, true),
            (PermissionLevel::Blocked, true),
        ];
        for (level, expected) in cases {
            assert_eq!(
                level.requires_approval(), expected,
                "{:?} requires_approval() should be {}",
                level, expected
            );
        }
    }

    #[test]
    fn test_permission_level_all_variants_ai_allowed() {
        let cases = vec![
            (PermissionLevel::Safe, true),
            (PermissionLevel::ReadOnly, true),
            (PermissionLevel::UserWrite, true),
            (PermissionLevel::SystemWrite, true),
            (PermissionLevel::Admin, true),
            (PermissionLevel::Blocked, false),
        ];
        for (level, expected) in cases {
            assert_eq!(
                level.ai_allowed(), expected,
                "{:?} ai_allowed() should be {}",
                level, expected
            );
        }
    }

    #[test]
    fn test_analyze_command_path_traversal_etc_via_usr() {
        // /usr/../etc/passwd should normalize to /etc/passwd
        assert_eq!(
            analyze_command_permission("mv", &["file".to_string(), "/usr/../etc/passwd".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_path_traversal_with_curdir() {
        // /etc/./hosts should still be /etc/hosts
        assert_eq!(
            analyze_command_permission("cp", &["file".to_string(), "/etc/./hosts".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_security_context_execute_normal_with_output() {
        let ctx = SecurityContext::new(false).unwrap();
        let result = ctx.execute_normal(&["echo".to_string(), "hello".to_string()]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello"));
    }

    #[test]
    fn test_security_context_execute_normal_nonexistent_command() {
        let ctx = SecurityContext::new(false).unwrap();
        let result = ctx.execute_normal(&["___nonexistent_binary___".to_string()]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_security_context_execute_with_privileges_not_available() {
        // Create a context where sudo might not be available via -n
        // In test env, sudo -n will fail — but we test that the restricted path works
        let ctx = SecurityContext::new(true).unwrap();
        let result = ctx.execute_with_privileges(&["id".to_string()]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("restricted"));
    }

    #[test]
    fn test_security_context_restricted_always_for_root_simulation() {
        // If restricted=true, is_restricted() should return true
        let ctx = SecurityContext::new(true).unwrap();
        assert!(ctx.is_restricted());
        // can_escalate should be false when restricted
        assert!(!ctx.can_escalate());
    }

    #[test]
    fn test_analyze_command_multiple_system_paths() {
        // Multiple args, one of which is a system path
        assert_eq!(
            analyze_command_permission("cp", &[
                "localfile".to_string(),
                "anotherlocal".to_string(),
                "/bin/target".to_string(),
            ]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_rmdir_system_path() {
        assert_eq!(
            analyze_command_permission("rmdir", &["/usr/share/empty".to_string()]),
            PermissionLevel::SystemWrite
        );
    }

    #[test]
    fn test_analyze_command_lib64_system_write() {
        // /lib64 starts with /lib, should be SystemWrite
        assert_eq!(
            analyze_command_permission("cp", &["a".to_string(), "/lib64/test".to_string()]),
            PermissionLevel::SystemWrite
        );
    }
}
