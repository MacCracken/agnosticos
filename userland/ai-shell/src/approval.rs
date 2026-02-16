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
