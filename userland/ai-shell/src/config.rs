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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ShellConfig::default();
        assert!(config.ai_enabled);
        assert_eq!(config.history_size, 10000);
        assert_eq!(config.output_format, "auto");
        assert!(!config.auto_approve_low_risk);
        assert_eq!(config.approval_timeout, 300);
        assert!(config.show_explanations);
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn test_default_config_mode() {
        let config = ShellConfig::default();
        assert!(matches!(config.default_mode, Mode::AiAssisted));
    }

    #[test]
    fn test_default_config_paths() {
        let config = ShellConfig::default();
        assert!(config.history_file.to_string_lossy().contains(".agnsh_history"));
        assert!(config.audit_log.to_string_lossy().contains(".agnsh_audit"));
    }

    #[test]
    fn test_default_config_llm_endpoint() {
        let config = ShellConfig::default();
        assert!(config.llm_endpoint.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = ShellConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("ai_enabled"));
        assert!(toml_str.contains("history_size"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            default_mode = "AiAssisted"
            history_file = "/tmp/history"
            history_size = 5000
            output_format = "json"
            ai_enabled = false
            auto_approve_low_risk = true
            approval_timeout = 60
            llm_endpoint = "http://localhost:11434"
            audit_log = "/tmp/audit.log"
            show_explanations = false
            theme = "dark"
        "#;
        let config: ShellConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.ai_enabled);
        assert!(config.auto_approve_low_risk);
        assert_eq!(config.history_size, 5000);
        assert_eq!(config.output_format, "json");
        assert_eq!(config.approval_timeout, 60);
        assert!(config.llm_endpoint.is_some());
        assert!(!config.show_explanations);
        assert_eq!(config.theme, "dark");
    }

    #[tokio::test]
    async fn test_config_save_and_load() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("agnos_test_config.toml");
        
        let config = ShellConfig {
            ai_enabled: false,
            approval_timeout: 120,
            theme: "light".to_string(),
            ..Default::default()
        };
        
        config.save(&config_path).await.unwrap();
        assert!(config_path.exists());
        
        let loaded = ShellConfig::from_file(config_path.clone()).await.unwrap();
        assert!(!loaded.ai_enabled);
        assert_eq!(loaded.approval_timeout, 120);
        assert_eq!(loaded.theme, "light");
        
        let _ = std::fs::remove_file(&config_path);
    }

    #[tokio::test]
    async fn test_config_from_nonexistent_file() {
        let dir = std::env::temp_dir().join("agnos_config_new_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("new_config.toml");
        let _ = std::fs::remove_file(&path);

        // Loading from a nonexistent path should create a default config
        let config = ShellConfig::from_file(path.clone()).await.unwrap();
        assert!(config.ai_enabled); // default value
        assert!(path.exists()); // file was created

        let _ = std::fs::remove_file(&path);
    }
}
