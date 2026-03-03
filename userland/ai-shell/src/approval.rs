//! Human approval system for sensitive operations
//!
//! Provides interactive prompts and logging for all actions
//! that require human oversight.

use anyhow::{anyhow, Result};
use console::{style, Style};
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect, Select};
use std::io::IsTerminal;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

use crate::security::PermissionLevel;

/// Types of approval requests
#[derive(Debug, Clone)]
pub enum ApprovalRequest {
    /// Command execution approval
    Command {
        command: String,
        args: Vec<String>,
        reason: String,
        risk_level: RiskLevel,
    },
    /// Privilege escalation approval
    PrivilegeEscalation {
        command: String,
        user: String,
        reason: String,
    },
    /// File operation approval
    FileOperation {
        operation: String,
        path: std::path::PathBuf,
        description: String,
    },
    /// Network access approval
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
    NeedInfo(String),
    /// Modify the request
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
    async fn get_user_decision(&self, request: &ApprovalRequest, risk: RiskLevel) -> Result<ApprovalResponse> {
        let choices = vec![
            "✓ Approve",
            "✓ Approve once",
            "✗ Deny",
            "✗ Deny and block",
            "? More info",
        ];
        
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
            _ => Ok(ApprovalResponse::Denied),
        }
    }
    
    /// Gather additional information from user
    async fn gather_more_info(&self, request: &ApprovalRequest) -> Result<String> {
        use dialoguer::Input;
        
        let info: String = Input::with_theme(&self.theme)
            .with_prompt("What would you like to know?")
            .interact_text()?;
        
        Ok(info)
    }
    
    /// Add a blocked pattern
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
}
