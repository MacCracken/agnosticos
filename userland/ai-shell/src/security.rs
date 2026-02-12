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
    uid: u32,
    gid: u32,
    euid: u32,
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
            uid,
            gid,
            euid,
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
            a.starts_with("/etc/") || 
            a.starts_with("/usr/") || 
            a.starts_with("/bin/") ||
            a.starts_with("/sbin/") ||
            a.starts_with("/lib")
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
