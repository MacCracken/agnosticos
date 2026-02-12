//! Shell configuration

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::mode::Mode;

/// Shell configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Default operating mode
    pub default_mode: Mode,
    /// History file path
    pub history_file: PathBuf,
    /// Maximum history entries
    pub history_size: usize,
    /// Output format style
    pub output_format: String,
    /// Enable AI by default
    pub ai_enabled: bool,
    /// Auto-approve low-risk commands
    pub auto_approve_low_risk: bool,
    /// Timeout for approval requests (seconds)
    pub approval_timeout: u64,
    /// LLM endpoint
    pub llm_endpoint: Option<String>,
    /// Audit log file
    pub audit_log: PathBuf,
    /// Show explanations for commands
    pub show_explanations: bool,
    /// Theme
    pub theme: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        
        Self {
            default_mode: Mode::AiAssisted,
            history_file: home.join(".agnsh_history"),
            history_size: 10000,
            output_format: "auto".to_string(),
            ai_enabled: true,
            auto_approve_low_risk: false,
            approval_timeout: 300,
            llm_endpoint: None,
            audit_log: home.join(".agnsh_audit.log"),
            show_explanations: true,
            theme: "default".to_string(),
        }
    }
}

impl ShellConfig {
    /// Load config from file
    pub async fn from_file(path: PathBuf) -> Result<Self> {
        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            let config: ShellConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config
            let config = ShellConfig::default();
            config.save(&path).await?;
            Ok(config)
        }
    }
    
    /// Save config to file
    pub async fn save(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, content).await?;
        Ok(())
    }
}
