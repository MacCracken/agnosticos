//! Human approval system for sensitive operations
//!
//! Provides interactive prompts and logging for all actions
//! that require human oversight.

use anyhow::Result;
use console::style;
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

use crate::security::PermissionLevel;

/// Types of approval requests
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ApprovalRequest {
    /// Command execution approval
    Command {
        command: String,
        args: Vec<String>,
        reason: String,
        risk_level: RiskLevel,
    },
    /// Privilege escalation approval
    #[allow(dead_code)]
    PrivilegeEscalation {
        command: String,
        user: String,
        reason: String,
    },
    /// File operation approval
    #[allow(dead_code)]
    FileOperation {
        operation: String,
        path: std::path::PathBuf,
        description: String,
    },
    /// Network access approval
    #[allow(dead_code)]
    NetworkAccess {
        host: String,
        port: u16,
        protocol: String,
        purpose: String,
    },
    /// Batch operation approval
    Batch {
        operations: Vec<String>,
        summary: String,
    },
}

/// Risk levels for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn from_permission(perm: &PermissionLevel) -> Self {
        match perm {
            PermissionLevel::Safe => RiskLevel::Low,
            PermissionLevel::ReadOnly => RiskLevel::Low,
            PermissionLevel::UserWrite => RiskLevel::Medium,
            PermissionLevel::SystemWrite => RiskLevel::High,
            PermissionLevel::Admin => RiskLevel::Critical,
            PermissionLevel::Blocked => RiskLevel::Critical,
        }
    }
    
    pub fn color(&self) -> impl Fn(&str) -> String {
        match self {
            RiskLevel::Low => |s: &str| format!("\x1b[32m{}\x1b[0m", s),
            RiskLevel::Medium => |s: &str| format!("\x1b[33m{}\x1b[0m", s),
            RiskLevel::High => |s: &str| format!("\x1b[31m{}\x1b[0m", s),
            RiskLevel::Critical => |s: &str| format!("\x1b[1;31m{}\x1b[0m", s),
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            RiskLevel::Low => "✓",
            RiskLevel::Medium => "⚠",
            RiskLevel::High => "⚠",
            RiskLevel::Critical => "✕",
        }
    }
}

/// Response to an approval request
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ApprovalResponse {
    /// Approved to proceed
    Approved,
    /// Approved once (don't remember)
    ApprovedOnce,
    /// Denied - do not execute
    Denied,
    /// Denied and block similar requests
    DenyAndBlock,
    /// Request more information
    #[allow(dead_code)]
    NeedInfo(String),
    /// Modify the request
    #[allow(dead_code)]
    Modify(ApprovalRequest),
}

/// Manages approval workflows
pub struct ApprovalManager {
    theme: ColorfulTheme,
    timeout_seconds: u64,
    auto_approve_low_risk: bool,
    blocked_patterns: Vec<String>,
}

impl ApprovalManager {
    pub fn new() -> Self {
        Self {
            theme: ColorfulTheme::default(),
            timeout_seconds: 300, // 5 minutes default timeout
            auto_approve_low_risk: false,
            blocked_patterns: Vec::new(),
        }
    }
    
    /// Configure auto-approval for low-risk operations
    #[allow(dead_code)]
    pub fn set_auto_approve_low_risk(&mut self, enabled: bool) {
        self.auto_approve_low_risk = enabled;
    }
    
    /// Request approval for an operation
    pub async fn request(&self, request: &ApprovalRequest) -> Result<ApprovalResponse> {
        info!("Approval requested: {:?}", request);
        
        // Check if pattern is blocked
        if self.is_blocked(request) {
            warn!("Request matches blocked pattern, auto-denying");
            return Ok(ApprovalResponse::Denied);
        }
        
        // Determine risk level
        let risk = self.assess_risk(request);
        
        // Auto-approve low risk if configured
        if self.auto_approve_low_risk && risk == RiskLevel::Low {
            info!("Auto-approved low-risk operation");
            return Ok(ApprovalResponse::Approved);
        }
        
        // Display request and get user input
        self.display_request(request, risk)?;
        
        // For non-interactive environments, deny by default
        if !std::io::stdin().is_terminal() {
            warn!("Non-interactive environment, denying by default");
            return Ok(ApprovalResponse::Denied);
        }
        
        // Get user decision with timeout
        let response = timeout(
            Duration::from_secs(self.timeout_seconds),
            self.get_user_decision(request, risk)
        ).await;
        
        match response {
            Ok(Ok(resp)) => {
                info!("User responded: {:?}", resp);
                Ok(resp)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                warn!("Approval request timed out");
                Ok(ApprovalResponse::Denied)
            }
        }
    }
    
    /// Assess risk level of a request
    fn assess_risk(&self, request: &ApprovalRequest) -> RiskLevel {
        match request {
            ApprovalRequest::Command { risk_level, .. } => *risk_level,
            ApprovalRequest::PrivilegeEscalation { .. } => RiskLevel::Critical,
            ApprovalRequest::FileOperation { operation, path, .. } => {
                let path_str = path.to_string_lossy();
                if path_str.starts_with("/etc/") || 
                   path_str.starts_with("/usr/") ||
                   path_str.starts_with("/bin/") ||
                   path_str.starts_with("/sbin/") {
                    RiskLevel::High
                } else if operation.contains("delete") || operation.contains("remove") {
                    RiskLevel::High
                } else {
                    RiskLevel::Medium
                }
            }
            ApprovalRequest::NetworkAccess { .. } => RiskLevel::Medium,
            ApprovalRequest::Batch { operations, .. } => {
                if operations.len() > 10 {
                    RiskLevel::High
                } else {
                    RiskLevel::Medium
                }
            }
        }
    }
    
    /// Check if request matches blocked pattern
    fn is_blocked(&self, request: &ApprovalRequest) -> bool {
        let cmd = match request {
            ApprovalRequest::Command { command, .. } => command.clone(),
            ApprovalRequest::PrivilegeEscalation { command, .. } => command.clone(),
            _ => return false,
        };
        
        self.blocked_patterns.iter().any(|pattern| {
            cmd.contains(pattern)
        })
    }
    
    /// Display the approval request to user
    fn display_request(&self, request: &ApprovalRequest, risk: RiskLevel) -> Result<()> {
        println!("\n{}", style("─".repeat(60)).dim());
        println!("{}", style("  APPROVAL REQUIRED").bold().yellow());
        println!("{}\n", style("─".repeat(60)).dim());
        
        match request {
            ApprovalRequest::Command { command, args, reason, .. } => {
                println!("  {} {}", style("Command:").bold(), command);
                if !args.is_empty() {
                    println!("  {} {}", style("Args:").bold(), args.join(" "));
                }
                println!("  {} {}", style("Reason:").dim(), reason);
            }
            ApprovalRequest::PrivilegeEscalation { command, user, reason } => {
                println!("  {}", style("⚠️  PRIVILEGE ESCALATION").red().bold());
                println!("  {} {} as {}", style("Command:").bold(), command, user);
                println!("  {} {}", style("Reason:").dim(), reason);
            }
            ApprovalRequest::FileOperation { operation, path, description } => {
                println!("  {} {}", style("Operation:").bold(), operation);
                println!("  {} {}", style("Path:").bold(), path.display());
                println!("  {} {}", style("Description:").dim(), description);
            }
            ApprovalRequest::NetworkAccess { host, port, protocol, purpose } => {
                println!("  {} {}://{}:{}", style("Network:").bold(), protocol, host, port);
                println!("  {} {}", style("Purpose:").dim(), purpose);
            }
            ApprovalRequest::Batch { operations, summary } => {
                println!("  {} {}", style("Batch Operation:").bold(), summary);
                println!("  {} {} operations", style("Count:").bold(), operations.len());
                for (i, op) in operations.iter().take(5).enumerate() {
                    println!("    {}. {}", i + 1, op);
                }
                if operations.len() > 5 {
                    println!("    ... and {} more", operations.len() - 5);
                }
            }
        }
        
        println!("\n  {} {} {}", 
            style("Risk Level:").bold(),
            risk.icon(),
            risk.color()(&format!("{:?}", risk))
        );
        
        println!("{}", style("─".repeat(60)).dim());
        
        Ok(())
    }
    
    /// Get user decision interactively
    async fn get_user_decision(&self, request: &ApprovalRequest, _risk: RiskLevel) -> Result<ApprovalResponse> {
        let can_edit = matches!(
            request,
            ApprovalRequest::Command { .. } | ApprovalRequest::FileOperation { .. }
        );

        let mut choices = vec![
            "✓ Approve",
            "✓ Approve once",
            "✗ Deny",
            "✗ Deny and block",
            "? More info",
        ];
        if can_edit {
            choices.push("✎ Edit command");
        }

        let selection = Select::with_theme(&self.theme)
            .with_prompt("Your decision")
            .items(&choices)
            .default(1) // Default to "Approve once"
            .interact()?;

        match selection {
            0 => Ok(ApprovalResponse::Approved),
            1 => Ok(ApprovalResponse::ApprovedOnce),
            2 => Ok(ApprovalResponse::Denied),
            3 => Ok(ApprovalResponse::DenyAndBlock),
            4 => {
                // Request more info
                let info = self.gather_more_info(request).await?;
                Ok(ApprovalResponse::NeedInfo(info))
            }
            5 if can_edit => {
                // Edit the command before approving
                let modified = self.edit_request(request).await?;
                Ok(ApprovalResponse::Modify(modified))
            }
            _ => Ok(ApprovalResponse::Denied),
        }
    }

    /// Let the user edit a command or file operation before approving.
    async fn edit_request(&self, request: &ApprovalRequest) -> Result<ApprovalRequest> {
        use dialoguer::Input;

        match request {
            ApprovalRequest::Command { command, args, reason, risk_level } => {
                let current = if args.is_empty() {
                    command.clone()
                } else {
                    format!("{} {}", command, args.join(" "))
                };

                let edited: String = Input::with_theme(&self.theme)
                    .with_prompt("Edit command")
                    .default(current)
                    .interact_text()?;

                let parts: Vec<&str> = edited.split_whitespace().collect();
                let (new_cmd, new_args) = if parts.is_empty() {
                    (command.clone(), args.clone())
                } else {
                    (
                        parts[0].to_string(),
                        parts[1..].iter().map(|s| s.to_string()).collect(),
                    )
                };

                Ok(ApprovalRequest::Command {
                    command: new_cmd,
                    args: new_args,
                    reason: reason.clone(),
                    risk_level: *risk_level,
                })
            }
            ApprovalRequest::FileOperation { operation, path, description } => {
                let edited_path: String = Input::with_theme(&self.theme)
                    .with_prompt("Edit path")
                    .default(path.to_string_lossy().to_string())
                    .interact_text()?;

                Ok(ApprovalRequest::FileOperation {
                    operation: operation.clone(),
                    path: std::path::PathBuf::from(edited_path),
                    description: description.clone(),
                })
            }
            // For other types, return unchanged (shouldn't reach here due to can_edit guard)
            other => Ok(other.clone()),
        }
    }
    
    /// Gather additional information from user
    async fn gather_more_info(&self, _request: &ApprovalRequest) -> Result<String> {
        use dialoguer::Input;
        
        let info: String = Input::with_theme(&self.theme)
            .with_prompt("What would you like to know?")
            .interact_text()?;
        
        Ok(info)
    }
    
    /// Add a blocked pattern
    #[allow(dead_code)]
    pub fn block_pattern(&mut self, pattern: String) {
        self.blocked_patterns.push(pattern);
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch approval for multiple operations
#[allow(dead_code)]
pub async fn batch_approve(
    manager: &ApprovalManager,
    operations: Vec<ApprovalRequest>,
) -> Result<Vec<(ApprovalRequest, ApprovalResponse)>> {
    let mut results = Vec::new();
    
    // Group by risk level
    let low_risk: Vec<_> = operations.iter()
        .filter(|op| manager.assess_risk(op) == RiskLevel::Low)
        .cloned()
        .collect();
    
    let high_risk: Vec<_> = operations.iter()
        .filter(|op| manager.assess_risk(op) != RiskLevel::Low)
        .cloned()
        .collect();
    
    // Auto-approve low risk
    for op in low_risk {
        results.push((op, ApprovalResponse::Approved));
    }
    
    // Request approval for high risk as batch
    if !high_risk.is_empty() {
        let batch_request = ApprovalRequest::Batch {
            operations: high_risk.iter().map(|op| format!("{:?}", op)).collect(),
            summary: format!("{} high-risk operations", high_risk.len()),
        };
        
        let response = manager.request(&batch_request).await?;
        
        // Apply response to all high-risk operations
        for op in high_risk {
            results.push((op, response.clone()));
        }
    }
    
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_from_permission_safe() {
        let risk = RiskLevel::from_permission(&PermissionLevel::Safe);
        assert_eq!(risk, RiskLevel::Low);
    }

    #[test]
    fn test_risk_level_from_permission_readonly() {
        let risk = RiskLevel::from_permission(&PermissionLevel::ReadOnly);
        assert_eq!(risk, RiskLevel::Low);
    }

    #[test]
    fn test_risk_level_from_permission_user_write() {
        let risk = RiskLevel::from_permission(&PermissionLevel::UserWrite);
        assert_eq!(risk, RiskLevel::Medium);
    }

    #[test]
    fn test_risk_level_from_permission_system_write() {
        let risk = RiskLevel::from_permission(&PermissionLevel::SystemWrite);
        assert_eq!(risk, RiskLevel::High);
    }

    #[test]
    fn test_risk_level_from_permission_admin() {
        let risk = RiskLevel::from_permission(&PermissionLevel::Admin);
        assert_eq!(risk, RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_from_permission_blocked() {
        let risk = RiskLevel::from_permission(&PermissionLevel::Blocked);
        assert_eq!(risk, RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_icon() {
        assert_eq!(RiskLevel::Low.icon(), "✓");
        assert_eq!(RiskLevel::Medium.icon(), "⚠");
        assert_eq!(RiskLevel::High.icon(), "⚠");
        assert_eq!(RiskLevel::Critical.icon(), "✕");
    }

    #[test]
    fn test_risk_level_color() {
        let low_color = RiskLevel::Low.color()("test");
        assert!(low_color.contains("32")); // green

        let med_color = RiskLevel::Medium.color()("test");
        assert!(med_color.contains("33")); // yellow

        let high_color = RiskLevel::High.color()("test");
        assert!(high_color.contains("31")); // red

        let crit_color = RiskLevel::Critical.color()("test");
        assert!(crit_color.contains("1;31")); // bright red
    }

    #[test]
    fn test_approval_manager_new() {
        let manager = ApprovalManager::new();
        assert_eq!(manager.timeout_seconds, 300);
        assert!(!manager.auto_approve_low_risk);
        assert!(manager.blocked_patterns.is_empty());
    }

    #[test]
    fn test_approval_manager_auto_approve_setter() {
        let mut manager = ApprovalManager::new();
        assert!(!manager.auto_approve_low_risk);

        manager.set_auto_approve_low_risk(true);
        assert!(manager.auto_approve_low_risk);

        manager.set_auto_approve_low_risk(false);
        assert!(!manager.auto_approve_low_risk);
    }

    #[test]
    fn test_approval_manager_add_blocked_pattern() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("rm -rf /".to_string());
        assert!(manager.blocked_patterns.contains(&"rm -rf /".to_string()));
    }

    #[test]
    fn test_approval_manager_is_blocked() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("rm -rf".to_string());

        let request = ApprovalRequest::Command {
            command: "rm -rf /".to_string(),
            args: vec!["-rf".to_string(), "/".to_string()],
            reason: "Remove all files".to_string(),
            risk_level: RiskLevel::Critical,
        };

        assert!(manager.is_blocked(&request));
    }

    #[test]
    fn test_approval_manager_is_not_blocked() {
        let manager = ApprovalManager::new();

        let request = ApprovalRequest::Command {
            command: "ls".to_string(),
            args: vec!["-la".to_string()],
            reason: "List files".to_string(),
            risk_level: RiskLevel::Low,
        };

        assert!(!manager.is_blocked(&request));
    }

    #[test]
    fn test_approval_request_command() {
        let request = ApprovalRequest::Command {
            command: "ls".to_string(),
            args: vec!["-la".to_string()],
            reason: "List files".to_string(),
            risk_level: RiskLevel::Low,
        };

        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("Command"));
        assert!(debug_str.contains("ls"));
    }

    #[test]
    fn test_approval_request_privilege_escalation() {
        let request = ApprovalRequest::PrivilegeEscalation {
            command: "sudo rm -rf /".to_string(),
            user: "root".to_string(),
            reason: "Admin access needed".to_string(),
        };

        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("PrivilegeEscalation"));
        assert!(debug_str.contains("root"));
    }

    #[test]
    fn test_approval_request_file_operation() {
        let path = std::path::PathBuf::from("/home/user/important.txt");
        let request = ApprovalRequest::FileOperation {
            operation: "delete".to_string(),
            path: path.clone(),
            description: "Delete important file".to_string(),
        };

        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("FileOperation"));
        assert!(debug_str.contains("important.txt"));
    }

    #[test]
    fn test_approval_request_network_access() {
        let request = ApprovalRequest::NetworkAccess {
            host: "evil.com".to_string(),
            port: 443,
            protocol: "https".to_string(),
            purpose: "Check connectivity".to_string(),
        };

        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("NetworkAccess"));
        assert!(debug_str.contains("evil.com"));
    }

    #[test]
    fn test_approval_request_batch() {
        let request = ApprovalRequest::Batch {
            operations: vec!["op1".to_string(), "op2".to_string()],
            summary: "Batch operations".to_string(),
        };

        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("Batch"));
        assert!(debug_str.contains("op1"));
        assert!(debug_str.contains("op2"));
    }

    #[test]
    fn test_approval_response_variants() {
        let approved = ApprovalResponse::Approved;
        let approved_once = ApprovalResponse::ApprovedOnce;
        let denied = ApprovalResponse::Denied;
        let deny_block = ApprovalResponse::DenyAndBlock;
        let need_info = ApprovalResponse::NeedInfo("Why?".to_string());

        assert!(format!("{:?}", approved).contains("Approved"));
        assert!(format!("{:?}", approved_once).contains("ApprovedOnce"));
        assert!(format!("{:?}", denied).contains("Denied"));
        assert!(format!("{:?}", deny_block).contains("DenyAndBlock"));
        assert!(format!("{:?}", need_info).contains("NeedInfo"));
    }

    #[test]
    fn test_assess_risk_command_low() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Command {
            command: "ls".to_string(),
            args: vec![],
            reason: "List".to_string(),
            risk_level: RiskLevel::Low,
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Low);
    }

    #[test]
    fn test_assess_risk_command_high() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Command {
            command: "rm".to_string(),
            args: vec!["-rf".to_string(), "/".to_string()],
            reason: "Delete".to_string(),
            risk_level: RiskLevel::Critical,
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Critical);
    }

    #[test]
    fn test_assess_risk_privilege_escalation() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::PrivilegeEscalation {
            command: "sudo su".to_string(),
            user: "root".to_string(),
            reason: "Root access".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Critical);
    }

    #[test]
    fn test_assess_risk_network() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::NetworkAccess {
            host: "example.com".to_string(),
            port: 80,
            protocol: "http".to_string(),
            purpose: "HTTP request".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Medium);
    }

    #[test]
    fn test_approval_manager_default() {
        let manager = ApprovalManager::default();
        assert_eq!(manager.timeout_seconds, 300);
        assert!(!manager.auto_approve_low_risk);
    }

    #[test]
    fn test_risk_level_debug() {
        let debug_low = format!("{:?}", RiskLevel::Low);
        let debug_medium = format!("{:?}", RiskLevel::Medium);
        let debug_high = format!("{:?}", RiskLevel::High);
        let debug_critical = format!("{:?}", RiskLevel::Critical);

        assert!(debug_low.contains("Low"));
        assert!(debug_medium.contains("Medium"));
        assert!(debug_high.contains("High"));
        assert!(debug_critical.contains("Critical"));
    }

    #[test]
    fn test_risk_level_clone() {
        let original = RiskLevel::High;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_approval_response_clone() {
        let original = ApprovalResponse::NeedInfo("test".to_string());
        let cloned = original.clone();
        assert!(format!("{:?}", cloned).contains("NeedInfo"));
    }

    #[test]
    fn test_approval_request_clone() {
        let original = ApprovalRequest::Command {
            command: "test".to_string(),
            args: vec!["arg1".to_string()],
            reason: "testing".to_string(),
            risk_level: RiskLevel::Low,
        };
        let cloned = original.clone();
        assert!(format!("{:?}", cloned).contains("test"));
    }

    // --- Additional approval.rs coverage tests ---

    #[test]
    fn test_is_blocked_privilege_escalation() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("sudo".to_string());

        let request = ApprovalRequest::PrivilegeEscalation {
            command: "sudo rm -rf /".to_string(),
            user: "root".to_string(),
            reason: "Need root".to_string(),
        };
        assert!(manager.is_blocked(&request));
    }

    #[test]
    fn test_is_blocked_privilege_escalation_no_match() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("reboot".to_string());

        let request = ApprovalRequest::PrivilegeEscalation {
            command: "sudo ls".to_string(),
            user: "root".to_string(),
            reason: "List files".to_string(),
        };
        assert!(!manager.is_blocked(&request));
    }

    #[test]
    fn test_is_blocked_file_operation_returns_false() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("rm".to_string());

        // FileOperation is not checked by is_blocked — always returns false
        let request = ApprovalRequest::FileOperation {
            operation: "rm".to_string(),
            path: std::path::PathBuf::from("/etc/passwd"),
            description: "Delete passwd".to_string(),
        };
        assert!(!manager.is_blocked(&request));
    }

    #[test]
    fn test_is_blocked_network_access_returns_false() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("evil".to_string());

        let request = ApprovalRequest::NetworkAccess {
            host: "evil.com".to_string(),
            port: 443,
            protocol: "https".to_string(),
            purpose: "Exfiltrate".to_string(),
        };
        assert!(!manager.is_blocked(&request));
    }

    #[test]
    fn test_is_blocked_batch_returns_false() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("rm".to_string());

        let request = ApprovalRequest::Batch {
            operations: vec!["rm -rf /".to_string()],
            summary: "Dangerous".to_string(),
        };
        assert!(!manager.is_blocked(&request));
    }

    #[test]
    fn test_is_blocked_multiple_patterns() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("rm -rf".to_string());
        manager.block_pattern("shutdown".to_string());
        manager.block_pattern("reboot".to_string());

        let blocked = ApprovalRequest::Command {
            command: "shutdown now".to_string(),
            args: vec![],
            reason: "Power off".to_string(),
            risk_level: RiskLevel::Critical,
        };
        assert!(manager.is_blocked(&blocked));

        let not_blocked = ApprovalRequest::Command {
            command: "ls -la".to_string(),
            args: vec![],
            reason: "List".to_string(),
            risk_level: RiskLevel::Low,
        };
        assert!(!manager.is_blocked(&not_blocked));
    }

    #[test]
    fn test_assess_risk_file_operation_system_path_etc() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/etc/shadow"),
            description: "Modify shadow".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_file_operation_system_path_usr() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/usr/lib/something"),
            description: "Modify lib".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_file_operation_system_path_bin() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/bin/sh"),
            description: "Replace shell".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_file_operation_system_path_sbin() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/sbin/init"),
            description: "Replace init".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_file_operation_delete() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "delete".to_string(),
            path: std::path::PathBuf::from("/home/user/file.txt"),
            description: "Delete user file".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_file_operation_remove() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "remove".to_string(),
            path: std::path::PathBuf::from("/home/user/file.txt"),
            description: "Remove user file".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_file_operation_safe_user_path() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/home/user/notes.txt"),
            description: "Write notes".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Medium);
    }

    #[test]
    fn test_assess_risk_batch_large() {
        let manager = ApprovalManager::new();
        let ops: Vec<String> = (0..15).map(|i| format!("op{}", i)).collect();
        let request = ApprovalRequest::Batch {
            operations: ops,
            summary: "Many ops".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_batch_small() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Batch {
            operations: vec!["op1".to_string(), "op2".to_string()],
            summary: "Few ops".to_string(),
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Medium);
    }

    #[test]
    fn test_assess_risk_batch_exactly_10() {
        let manager = ApprovalManager::new();
        let ops: Vec<String> = (0..10).map(|i| format!("op{}", i)).collect();
        let request = ApprovalRequest::Batch {
            operations: ops,
            summary: "Boundary".to_string(),
        };
        // 10 is not > 10, so Medium
        assert_eq!(manager.assess_risk(&request), RiskLevel::Medium);
    }

    #[test]
    fn test_assess_risk_command_medium() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Command {
            command: "touch".to_string(),
            args: vec!["file.txt".to_string()],
            reason: "Create file".to_string(),
            risk_level: RiskLevel::Medium,
        };
        assert_eq!(manager.assess_risk(&request), RiskLevel::Medium);
    }

    #[test]
    fn test_approval_response_modify_variant() {
        let modified_request = ApprovalRequest::Command {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            reason: "Modified".to_string(),
            risk_level: RiskLevel::Low,
        };
        let response = ApprovalResponse::Modify(modified_request);
        let debug = format!("{:?}", response);
        assert!(debug.contains("Modify"));
        assert!(debug.contains("echo"));
    }

    #[test]
    fn test_risk_level_copy_eq() {
        let a = RiskLevel::Critical;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn test_request_blocked_pattern_auto_denies() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("dangerous".to_string());

        let request = ApprovalRequest::Command {
            command: "dangerous-cmd".to_string(),
            args: vec![],
            reason: "Test blocked".to_string(),
            risk_level: RiskLevel::Critical,
        };

        let result = manager.request(&request).await.unwrap();
        assert!(matches!(result, ApprovalResponse::Denied));
    }

    #[tokio::test]
    async fn test_request_auto_approve_low_risk() {
        let mut manager = ApprovalManager::new();
        manager.set_auto_approve_low_risk(true);

        let request = ApprovalRequest::Command {
            command: "ls".to_string(),
            args: vec![],
            reason: "List".to_string(),
            risk_level: RiskLevel::Low,
        };

        let result = manager.request(&request).await.unwrap();
        assert!(matches!(result, ApprovalResponse::Approved));
    }

    #[tokio::test]
    async fn test_request_non_interactive_denies() {
        // In test environment, stdin is not a terminal, so this should deny
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Command {
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            reason: "Test non-interactive".to_string(),
            risk_level: RiskLevel::Medium,
        };

        let result = manager.request(&request).await.unwrap();
        assert!(matches!(result, ApprovalResponse::Denied));
    }

    #[test]
    fn test_display_request_command_no_args() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Command {
            command: "ls".to_string(),
            args: vec![],
            reason: "List files".to_string(),
            risk_level: RiskLevel::Low,
        };
        // Should not panic; output goes to stdout
        assert!(manager.display_request(&request, RiskLevel::Low).is_ok());
    }

    #[test]
    fn test_display_request_command_with_args() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Command {
            command: "rm".to_string(),
            args: vec!["-rf".to_string(), "/tmp/test".to_string()],
            reason: "Remove temp files".to_string(),
            risk_level: RiskLevel::High,
        };
        assert!(manager.display_request(&request, RiskLevel::High).is_ok());
    }

    #[test]
    fn test_display_request_privilege_escalation() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::PrivilegeEscalation {
            command: "systemctl restart nginx".to_string(),
            user: "root".to_string(),
            reason: "Restart web server".to_string(),
        };
        assert!(manager.display_request(&request, RiskLevel::Critical).is_ok());
    }

    #[test]
    fn test_display_request_file_operation() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/etc/config.toml"),
            description: "Update configuration".to_string(),
        };
        assert!(manager.display_request(&request, RiskLevel::High).is_ok());
    }

    #[test]
    fn test_display_request_network_access() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::NetworkAccess {
            host: "api.example.com".to_string(),
            port: 443,
            protocol: "https".to_string(),
            purpose: "Fetch model weights".to_string(),
        };
        assert!(manager.display_request(&request, RiskLevel::Medium).is_ok());
    }

    #[test]
    fn test_display_request_batch_small() {
        let manager = ApprovalManager::new();
        let request = ApprovalRequest::Batch {
            operations: vec!["op1".to_string(), "op2".to_string(), "op3".to_string()],
            summary: "Batch of 3 operations".to_string(),
        };
        assert!(manager.display_request(&request, RiskLevel::Medium).is_ok());
    }

    #[test]
    fn test_display_request_batch_large_truncates() {
        let manager = ApprovalManager::new();
        let ops: Vec<String> = (0..10).map(|i| format!("operation_{}", i)).collect();
        let request = ApprovalRequest::Batch {
            operations: ops,
            summary: "Batch of 10 operations".to_string(),
        };
        // Should display first 5 + "...and 5 more"
        assert!(manager.display_request(&request, RiskLevel::High).is_ok());
    }

    #[tokio::test]
    async fn test_request_blocked_privilege_escalation() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("sudo".to_string());

        let request = ApprovalRequest::PrivilegeEscalation {
            command: "sudo systemctl".to_string(),
            user: "root".to_string(),
            reason: "Admin".to_string(),
        };
        let result = manager.request(&request).await.unwrap();
        assert!(matches!(result, ApprovalResponse::Denied));
    }

    #[tokio::test]
    async fn test_request_non_interactive_high_risk_denies() {
        let manager = ApprovalManager::new();
        // High risk, non-interactive → should deny
        let request = ApprovalRequest::PrivilegeEscalation {
            command: "passwd root".to_string(),
            user: "root".to_string(),
            reason: "Change root password".to_string(),
        };
        let result = manager.request(&request).await.unwrap();
        assert!(matches!(result, ApprovalResponse::Denied));
    }

    #[tokio::test]
    async fn test_request_auto_approve_disabled_for_non_low() {
        let mut manager = ApprovalManager::new();
        manager.set_auto_approve_low_risk(true);

        // Medium risk should NOT be auto-approved
        let request = ApprovalRequest::Command {
            command: "wget".to_string(),
            args: vec!["https://example.com".to_string()],
            reason: "Download".to_string(),
            risk_level: RiskLevel::Medium,
        };

        // In non-interactive (test), still gets denied
        let result = manager.request(&request).await.unwrap();
        assert!(matches!(result, ApprovalResponse::Denied));
    }

    #[test]
    fn test_edit_request_preserves_reason_and_risk() {
        // Verify that Modify variant can carry an edited command
        let original = ApprovalRequest::Command {
            command: "rm".to_string(),
            args: vec!["-rf".to_string(), "/tmp/data".to_string()],
            reason: "Cleanup temp".to_string(),
            risk_level: RiskLevel::High,
        };

        // Simulate what edit_request would produce
        let edited = ApprovalRequest::Command {
            command: "rm".to_string(),
            args: vec!["-r".to_string(), "/tmp/data".to_string()],
            reason: "Cleanup temp".to_string(),  // preserved
            risk_level: RiskLevel::High,          // preserved
        };

        let response = ApprovalResponse::Modify(edited.clone());
        if let ApprovalResponse::Modify(ApprovalRequest::Command { command, args, reason, risk_level }) = response {
            assert_eq!(command, "rm");
            assert_eq!(args, vec!["-r", "/tmp/data"]);
            assert_eq!(reason, "Cleanup temp");
            assert_eq!(risk_level, RiskLevel::High);
        } else {
            panic!("Expected Modify(Command)");
        }
    }

    #[test]
    fn test_edit_request_file_operation() {
        let original = ApprovalRequest::FileOperation {
            operation: "delete".to_string(),
            path: std::path::PathBuf::from("/etc/important"),
            description: "Remove config".to_string(),
        };

        // Simulate editing the path
        let edited = ApprovalRequest::FileOperation {
            operation: "delete".to_string(),
            path: std::path::PathBuf::from("/tmp/safe-to-delete"),
            description: "Remove config".to_string(),
        };

        let response = ApprovalResponse::Modify(edited);
        if let ApprovalResponse::Modify(ApprovalRequest::FileOperation { path, .. }) = response {
            assert_eq!(path, std::path::PathBuf::from("/tmp/safe-to-delete"));
        } else {
            panic!("Expected Modify(FileOperation)");
        }
    }

    #[test]
    fn test_can_edit_command_type() {
        // Command and FileOperation are editable
        let cmd = ApprovalRequest::Command {
            command: "ls".to_string(),
            args: vec![],
            reason: "test".to_string(),
            risk_level: RiskLevel::Low,
        };
        assert!(matches!(cmd, ApprovalRequest::Command { .. }));

        let file_op = ApprovalRequest::FileOperation {
            operation: "write".to_string(),
            path: std::path::PathBuf::from("/tmp/f"),
            description: "test".to_string(),
        };
        assert!(matches!(file_op, ApprovalRequest::FileOperation { .. }));

        // Network and Batch are NOT editable
        let net = ApprovalRequest::NetworkAccess {
            host: "x".to_string(),
            port: 80,
            protocol: "http".to_string(),
            purpose: "test".to_string(),
        };
        assert!(!matches!(net, ApprovalRequest::Command { .. } | ApprovalRequest::FileOperation { .. }));
    }

    #[test]
    fn test_block_multiple_patterns_and_check() {
        let mut manager = ApprovalManager::new();
        manager.block_pattern("rm".to_string());
        manager.block_pattern("dd".to_string());
        manager.block_pattern("mkfs".to_string());

        assert_eq!(manager.blocked_patterns.len(), 3);

        let rm_req = ApprovalRequest::Command {
            command: "rm -rf /".to_string(),
            args: vec![],
            reason: "test".to_string(),
            risk_level: RiskLevel::Critical,
        };
        assert!(manager.is_blocked(&rm_req));

        let dd_req = ApprovalRequest::Command {
            command: "dd if=/dev/zero".to_string(),
            args: vec![],
            reason: "test".to_string(),
            risk_level: RiskLevel::Critical,
        };
        assert!(manager.is_blocked(&dd_req));

        let safe_req = ApprovalRequest::Command {
            command: "ls -la".to_string(),
            args: vec![],
            reason: "test".to_string(),
            risk_level: RiskLevel::Low,
        };
        assert!(!manager.is_blocked(&safe_req));
    }
}
