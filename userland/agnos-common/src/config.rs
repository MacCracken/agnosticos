//! Environment profiles for AGNOS
//!
//! Named config profiles (dev/staging/prod) that switch bind addresses,
//! log levels, security strictness, and other settings.

use serde::{Deserialize, Serialize};

/// An environment profile that configures AGNOS runtime behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentProfile {
    /// Profile name (e.g., "dev", "staging", "prod")
    pub name: String,
    /// Bind address for services
    pub bind_address: String,
    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,
    /// Whether to enable debug endpoints
    pub debug_endpoints: bool,
    /// Security strictness (relaxed, standard, strict)
    pub security_mode: SecurityMode,
    /// Whether to auto-approve low-risk operations
    pub auto_approve_low_risk: bool,
    /// LLM Gateway URL
    pub llm_gateway_url: String,
    /// Agent Runtime API URL
    pub runtime_api_url: String,
    /// Maximum concurrent agents
    pub max_concurrent_agents: u32,
    /// Audit log verbosity (minimal, standard, verbose)
    pub audit_verbosity: AuditVerbosity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityMode {
    Relaxed,
    Standard,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditVerbosity {
    Minimal,
    Standard,
    Verbose,
}

impl EnvironmentProfile {
    /// Development profile -- permissive, verbose, localhost
    pub fn dev() -> Self {
        Self {
            name: "dev".to_string(),
            bind_address: "127.0.0.1".to_string(),
            log_level: "debug".to_string(),
            debug_endpoints: true,
            security_mode: SecurityMode::Relaxed,
            auto_approve_low_risk: true,
            llm_gateway_url: "http://127.0.0.1:8088".to_string(),
            runtime_api_url: "http://127.0.0.1:8090".to_string(),
            max_concurrent_agents: 100,
            audit_verbosity: AuditVerbosity::Verbose,
        }
    }

    /// Staging profile -- moderate security, standard logging
    pub fn staging() -> Self {
        Self {
            name: "staging".to_string(),
            bind_address: "127.0.0.1".to_string(),
            log_level: "info".to_string(),
            debug_endpoints: false,
            security_mode: SecurityMode::Standard,
            auto_approve_low_risk: false,
            llm_gateway_url: "http://127.0.0.1:8088".to_string(),
            runtime_api_url: "http://127.0.0.1:8090".to_string(),
            max_concurrent_agents: 50,
            audit_verbosity: AuditVerbosity::Standard,
        }
    }

    /// Production profile -- strict security, minimal debug
    pub fn production() -> Self {
        Self {
            name: "prod".to_string(),
            bind_address: "127.0.0.1".to_string(),
            log_level: "warn".to_string(),
            debug_endpoints: false,
            security_mode: SecurityMode::Strict,
            auto_approve_low_risk: false,
            llm_gateway_url: "http://127.0.0.1:8088".to_string(),
            runtime_api_url: "http://127.0.0.1:8090".to_string(),
            max_concurrent_agents: 200,
            audit_verbosity: AuditVerbosity::Minimal,
        }
    }

    /// Load profile by name
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "dev" | "development" => Some(Self::dev()),
            "staging" | "stage" => Some(Self::staging()),
            "prod" | "production" => Some(Self::production()),
            _ => None,
        }
    }

    /// Load profile from AGNOS_PROFILE env var, defaulting to dev
    pub fn from_env() -> Self {
        let name = std::env::var("AGNOS_PROFILE").unwrap_or_else(|_| "dev".to_string());
        Self::from_name(&name).unwrap_or_else(Self::dev)
    }

    /// Check if this is a production-like profile
    pub fn is_production(&self) -> bool {
        self.security_mode == SecurityMode::Strict
    }
}

impl Default for EnvironmentProfile {
    fn default() -> Self {
        Self::dev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_profile_values() {
        let p = EnvironmentProfile::dev();
        assert_eq!(p.name, "dev");
        assert_eq!(p.bind_address, "127.0.0.1");
        assert_eq!(p.log_level, "debug");
        assert!(p.debug_endpoints);
        assert_eq!(p.security_mode, SecurityMode::Relaxed);
        assert!(p.auto_approve_low_risk);
        assert_eq!(p.max_concurrent_agents, 100);
        assert_eq!(p.audit_verbosity, AuditVerbosity::Verbose);
    }

    #[test]
    fn test_staging_profile_values() {
        let p = EnvironmentProfile::staging();
        assert_eq!(p.name, "staging");
        assert_eq!(p.log_level, "info");
        assert!(!p.debug_endpoints);
        assert_eq!(p.security_mode, SecurityMode::Standard);
        assert!(!p.auto_approve_low_risk);
        assert_eq!(p.max_concurrent_agents, 50);
        assert_eq!(p.audit_verbosity, AuditVerbosity::Standard);
    }

    #[test]
    fn test_production_profile_values() {
        let p = EnvironmentProfile::production();
        assert_eq!(p.name, "prod");
        assert_eq!(p.log_level, "warn");
        assert!(!p.debug_endpoints);
        assert_eq!(p.security_mode, SecurityMode::Strict);
        assert!(!p.auto_approve_low_risk);
        assert_eq!(p.max_concurrent_agents, 200);
        assert_eq!(p.audit_verbosity, AuditVerbosity::Minimal);
    }

    #[test]
    fn test_from_name_dev() {
        let p = EnvironmentProfile::from_name("dev").unwrap();
        assert_eq!(p.name, "dev");
    }

    #[test]
    fn test_from_name_staging() {
        let p = EnvironmentProfile::from_name("staging").unwrap();
        assert_eq!(p.name, "staging");
    }

    #[test]
    fn test_from_name_prod() {
        let p = EnvironmentProfile::from_name("prod").unwrap();
        assert_eq!(p.name, "prod");
    }

    #[test]
    fn test_from_name_aliases() {
        let dev = EnvironmentProfile::from_name("development").unwrap();
        assert_eq!(dev.name, "dev");

        let stage = EnvironmentProfile::from_name("stage").unwrap();
        assert_eq!(stage.name, "staging");

        let prod = EnvironmentProfile::from_name("production").unwrap();
        assert_eq!(prod.name, "prod");
    }

    #[test]
    fn test_from_name_unknown_returns_none() {
        assert!(EnvironmentProfile::from_name("unknown").is_none());
        assert!(EnvironmentProfile::from_name("").is_none());
        assert!(EnvironmentProfile::from_name("test").is_none());
    }

    #[test]
    fn test_default_is_dev() {
        let p = EnvironmentProfile::default();
        assert_eq!(p.name, "dev");
        assert_eq!(p.log_level, "debug");
        assert!(p.debug_endpoints);
    }

    #[test]
    fn test_is_production_true() {
        let p = EnvironmentProfile::production();
        assert!(p.is_production());
    }

    #[test]
    fn test_is_production_false() {
        let dev = EnvironmentProfile::dev();
        assert!(!dev.is_production());

        let staging = EnvironmentProfile::staging();
        assert!(!staging.is_production());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let original = EnvironmentProfile::staging();
        let json = serde_json::to_string(&original).unwrap();
        let deser: EnvironmentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, original.name);
        assert_eq!(deser.bind_address, original.bind_address);
        assert_eq!(deser.log_level, original.log_level);
        assert_eq!(deser.debug_endpoints, original.debug_endpoints);
        assert_eq!(deser.security_mode, original.security_mode);
        assert_eq!(deser.auto_approve_low_risk, original.auto_approve_low_risk);
        assert_eq!(deser.max_concurrent_agents, original.max_concurrent_agents);
        assert_eq!(deser.audit_verbosity, original.audit_verbosity);
    }

    #[test]
    fn test_security_mode_variants() {
        // Serialization roundtrip for each variant
        for mode in [
            SecurityMode::Relaxed,
            SecurityMode::Standard,
            SecurityMode::Strict,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let deser: SecurityMode = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, mode);
        }
        // Verify rename_all = lowercase
        let json = serde_json::to_string(&SecurityMode::Relaxed).unwrap();
        assert_eq!(json, "\"relaxed\"");
    }

    #[test]
    fn test_audit_verbosity_variants() {
        for v in [
            AuditVerbosity::Minimal,
            AuditVerbosity::Standard,
            AuditVerbosity::Verbose,
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let deser: AuditVerbosity = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, v);
        }
        let json = serde_json::to_string(&AuditVerbosity::Minimal).unwrap();
        assert_eq!(json, "\"minimal\"");
    }

    #[test]
    fn test_from_env_default() {
        // When AGNOS_PROFILE is not set, should default to dev
        // (This test relies on the env var not being set in test runner)
        std::env::remove_var("AGNOS_PROFILE");
        let p = EnvironmentProfile::from_env();
        assert_eq!(p.name, "dev");
    }

    #[test]
    fn test_profile_names() {
        assert_eq!(EnvironmentProfile::dev().name, "dev");
        assert_eq!(EnvironmentProfile::staging().name, "staging");
        assert_eq!(EnvironmentProfile::production().name, "prod");
    }

    #[test]
    fn test_profile_urls() {
        let dev = EnvironmentProfile::dev();
        assert_eq!(dev.llm_gateway_url, "http://127.0.0.1:8088");
        assert_eq!(dev.runtime_api_url, "http://127.0.0.1:8090");
    }
}
