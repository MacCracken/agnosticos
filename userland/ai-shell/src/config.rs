//! Shell configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// User-defined command aliases
    #[serde(default)]
    pub aliases: HashMap<String, String>,
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
            aliases: HashMap::new(),
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
        assert!(config
            .history_file
            .to_string_lossy()
            .contains(".agnsh_history"));
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

    #[test]
    fn test_config_round_trip_toml() {
        let config = ShellConfig {
            llm_endpoint: Some("http://localhost:11434".to_string()),
            theme: "monokai".to_string(),
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: ShellConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            parsed.llm_endpoint,
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(parsed.theme, "monokai");
        assert_eq!(parsed.history_size, config.history_size);
        assert_eq!(parsed.ai_enabled, config.ai_enabled);
    }

    #[test]
    fn test_config_deserialization_minimal() {
        // Test that all fields are required in deserialization
        let toml_str = r#"
            default_mode = "Human"
            history_file = "/tmp/h"
            history_size = 100
            output_format = "plain"
            ai_enabled = true
            auto_approve_low_risk = false
            approval_timeout = 10
            audit_log = "/tmp/a.log"
            show_explanations = true
            theme = "minimal"
        "#;
        let config: ShellConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.history_size, 100);
        assert!(config.llm_endpoint.is_none());
        assert_eq!(config.theme, "minimal");
    }

    #[test]
    fn test_config_serialization_contains_all_fields() {
        let config = ShellConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("default_mode"));
        assert!(toml_str.contains("history_file"));
        assert!(toml_str.contains("history_size"));
        assert!(toml_str.contains("output_format"));
        assert!(toml_str.contains("ai_enabled"));
        assert!(toml_str.contains("auto_approve_low_risk"));
        assert!(toml_str.contains("approval_timeout"));
        assert!(toml_str.contains("audit_log"));
        assert!(toml_str.contains("show_explanations"));
        assert!(toml_str.contains("theme"));
    }

    #[test]
    fn test_config_clone() {
        let config = ShellConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.history_size, config.history_size);
        assert_eq!(cloned.ai_enabled, config.ai_enabled);
        assert_eq!(cloned.theme, config.theme);
    }

    #[test]
    fn test_config_debug() {
        let config = ShellConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ShellConfig"));
    }
}
