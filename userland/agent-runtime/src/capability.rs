//! Agent Capability Negotiation
//!
//! Provides a registry for agents to advertise capabilities and for consumers
//! to discover which agents can perform specific tasks. Supports schema-based
//! matching so that callers can find agents whose input/output contracts are
//! compatible with their needs.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use agnos_common::AgentId;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single capability that an agent can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Machine-readable name (e.g. "port_scan", "log_analysis").
    pub name: String,
    /// Semver version of this capability's contract.
    pub version: String,
    /// Human-readable description of what the capability does.
    pub description: String,
    /// Optional JSON Schema describing expected input.
    pub input_schema: Option<serde_json::Value>,
    /// Optional JSON Schema describing produced output.
    pub output_schema: Option<serde_json::Value>,
}

impl Capability {
    /// Create a new capability with the given name and version.
    pub fn new(name: impl Into<String>, version: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            input_schema: None,
            output_schema: None,
        }
    }

    /// Attach an input schema.
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }

    /// Attach an output schema.
    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }
}

/// An agent that provides one or more capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProvider {
    /// The agent offering these capabilities.
    pub agent_id: AgentId,
    /// Capabilities this agent provides.
    pub capabilities: Vec<Capability>,
    /// When the agent registered its capabilities.
    pub registered_at: DateTime<Utc>,
}

/// Query for finding agents by capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityQuery {
    /// Name of the capability to search for.
    pub name: String,
    /// If set, only match providers with this exact version.
    pub required_version: Option<String>,
    /// If set, used for schema compatibility checking.
    pub input: Option<serde_json::Value>,
}

impl CapabilityQuery {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required_version: None,
            input: None,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.required_version = Some(version.into());
        self
    }

    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = Some(input);
        self
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Central registry for agent capability advertisement and discovery.
#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    /// agent_id → provider record
    providers: HashMap<AgentId, CapabilityProvider>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register capabilities for an agent (replaces any previous registration).
    pub fn register(&mut self, agent_id: AgentId, capabilities: Vec<Capability>) {
        let provider = CapabilityProvider {
            agent_id,
            capabilities,
            registered_at: Utc::now(),
        };
        self.providers.insert(agent_id, provider);
    }

    /// Remove all capabilities for an agent.
    pub fn unregister(&mut self, agent_id: &AgentId) {
        self.providers.remove(agent_id);
    }

    /// Find all providers that offer a capability with the given name.
    pub fn find_capability(&self, name: &str) -> Vec<&CapabilityProvider> {
        self.providers
            .values()
            .filter(|p| p.capabilities.iter().any(|c| c.name == name))
            .collect()
    }

    /// Find the best-matching provider for a capability, considering schema compatibility.
    ///
    /// If `input` is provided and a provider has an `input_schema`, we check that the
    /// input's top-level keys are a subset of the schema's `properties` (basic check).
    /// Returns the first compatible provider, or falls back to any provider with the name.
    pub fn find_best_match(&self, name: &str, input: &serde_json::Value) -> Option<&CapabilityProvider> {
        let candidates = self.find_capability(name);
        if candidates.is_empty() {
            return None;
        }

        // Try to find a schema-compatible provider
        for provider in &candidates {
            for cap in &provider.capabilities {
                if cap.name != name {
                    continue;
                }
                if let Some(schema) = &cap.input_schema {
                    if is_schema_compatible(schema, input) {
                        return Some(provider);
                    }
                } else {
                    // No schema constraint — compatible by default
                    return Some(provider);
                }
            }
        }

        // Fallback: return first candidate regardless
        candidates.into_iter().next()
    }

    /// Get a map of capability_name → list of agent IDs that provide it.
    pub fn all_capabilities(&self) -> Vec<(String, Vec<AgentId>)> {
        let mut map: HashMap<String, Vec<AgentId>> = HashMap::new();
        for provider in self.providers.values() {
            for cap in &provider.capabilities {
                map.entry(cap.name.clone())
                    .or_default()
                    .push(provider.agent_id);
            }
        }
        map.into_iter().collect()
    }

    /// Get the capabilities of a specific agent.
    pub fn agent_capabilities(&self, agent_id: &AgentId) -> Option<&Vec<Capability>> {
        self.providers.get(agent_id).map(|p| &p.capabilities)
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Basic schema compatibility check.
///
/// If the schema has a `properties` object, verify that every top-level key
/// in `input` exists in the schema properties. This is intentionally simple;
/// full JSON Schema validation is out of scope for the built-in registry.
fn is_schema_compatible(schema: &serde_json::Value, input: &serde_json::Value) -> bool {
    let schema_props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(props) => props,
        None => return true, // no properties defined — accept anything
    };

    let input_obj = match input.as_object() {
        Some(obj) => obj,
        None => return true, // non-object input — can't validate fields
    };

    // Every key in input should exist in schema properties
    for key in input_obj.keys() {
        if !schema_props.contains_key(key) {
            return false;
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cap(name: &str, version: &str) -> Capability {
        Capability::new(name, version, format!("{} capability", name))
    }

    #[test]
    fn test_capability_new() {
        let cap = Capability::new("scan", "1.0.0", "Scan ports");
        assert_eq!(cap.name, "scan");
        assert_eq!(cap.version, "1.0.0");
        assert!(cap.input_schema.is_none());
        assert!(cap.output_schema.is_none());
    }

    #[test]
    fn test_capability_with_schemas() {
        let cap = Capability::new("analyze", "2.0.0", "Analyze data")
            .with_input_schema(serde_json::json!({"type": "object"}))
            .with_output_schema(serde_json::json!({"type": "array"}));
        assert!(cap.input_schema.is_some());
        assert!(cap.output_schema.is_some());
    }

    #[test]
    fn test_capability_serialization() {
        let cap = make_cap("test", "1.0.0");
        let json = serde_json::to_string(&cap).unwrap();
        let deser: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
        assert_eq!(deser.version, "1.0.0");
    }

    #[test]
    fn test_capability_query_new() {
        let q = CapabilityQuery::new("port_scan");
        assert_eq!(q.name, "port_scan");
        assert!(q.required_version.is_none());
        assert!(q.input.is_none());
    }

    #[test]
    fn test_capability_query_with_version() {
        let q = CapabilityQuery::new("scan").with_version("2.0.0");
        assert_eq!(q.required_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn test_capability_query_with_input() {
        let q = CapabilityQuery::new("scan")
            .with_input(serde_json::json!({"host": "127.0.0.1"}));
        assert!(q.input.is_some());
    }

    #[test]
    fn test_registry_new_empty() {
        let reg = CapabilityRegistry::new();
        assert!(reg.all_capabilities().is_empty());
    }

    #[test]
    fn test_registry_register_and_find() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();

        reg.register(agent, vec![
            make_cap("port_scan", "1.0.0"),
            make_cap("ping", "1.0.0"),
        ]);

        let providers = reg.find_capability("port_scan");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].agent_id, agent);

        let providers = reg.find_capability("ping");
        assert_eq!(providers.len(), 1);

        let providers = reg.find_capability("nonexistent");
        assert!(providers.is_empty());
    }

    #[test]
    fn test_registry_unregister() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();

        reg.register(agent, vec![make_cap("scan", "1.0.0")]);
        assert_eq!(reg.find_capability("scan").len(), 1);

        reg.unregister(&agent);
        assert!(reg.find_capability("scan").is_empty());
    }

    #[test]
    fn test_registry_agent_capabilities() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();

        reg.register(agent, vec![
            make_cap("cap_a", "1.0.0"),
            make_cap("cap_b", "2.0.0"),
        ]);

        let caps = reg.agent_capabilities(&agent).unwrap();
        assert_eq!(caps.len(), 2);

        let unknown = AgentId::new();
        assert!(reg.agent_capabilities(&unknown).is_none());
    }

    #[test]
    fn test_registry_all_capabilities() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        reg.register(a1, vec![make_cap("scan", "1.0.0"), make_cap("ping", "1.0.0")]);
        reg.register(a2, vec![make_cap("scan", "2.0.0"), make_cap("analyze", "1.0.0")]);

        let all = reg.all_capabilities();
        // Should have 3 unique capability names: scan, ping, analyze
        let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"scan"));
        assert!(names.contains(&"ping"));
        assert!(names.contains(&"analyze"));

        // "scan" should be provided by 2 agents
        let scan_providers = all.iter().find(|(n, _)| n == "scan").unwrap();
        assert_eq!(scan_providers.1.len(), 2);
    }

    #[test]
    fn test_registry_multiple_providers_same_capability() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        reg.register(a1, vec![make_cap("log_monitor", "1.0.0")]);
        reg.register(a2, vec![make_cap("log_monitor", "1.0.0")]);
        reg.register(a3, vec![make_cap("log_monitor", "2.0.0")]);

        let providers = reg.find_capability("log_monitor");
        assert_eq!(providers.len(), 3);
    }

    #[test]
    fn test_registry_find_best_match_no_schema() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();
        reg.register(agent, vec![make_cap("scan", "1.0.0")]);

        let result = reg.find_best_match("scan", &serde_json::json!({"host": "localhost"}));
        assert!(result.is_some());
        assert_eq!(result.unwrap().agent_id, agent);
    }

    #[test]
    fn test_registry_find_best_match_with_compatible_schema() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();

        let cap = Capability::new("scan", "1.0.0", "Port scanner")
            .with_input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "host": {"type": "string"},
                    "port": {"type": "integer"}
                }
            }));
        reg.register(agent, vec![cap]);

        // Compatible input (keys are subset of schema properties)
        let result = reg.find_best_match("scan", &serde_json::json!({"host": "localhost"}));
        assert!(result.is_some());
    }

    #[test]
    fn test_registry_find_best_match_incompatible_schema() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();

        let cap = Capability::new("scan", "1.0.0", "Port scanner")
            .with_input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "host": {"type": "string"}
                }
            }));
        reg.register(agent, vec![cap]);

        // Input has key "unknown_field" not in schema
        let result = reg.find_best_match("scan", &serde_json::json!({"unknown_field": 42}));
        // Falls back to first candidate
        assert!(result.is_some());
    }

    #[test]
    fn test_registry_find_best_match_not_found() {
        let reg = CapabilityRegistry::new();
        let result = reg.find_best_match("nonexistent", &serde_json::json!({}));
        assert!(result.is_none());
    }

    #[test]
    fn test_registry_register_replaces_previous() {
        let mut reg = CapabilityRegistry::new();
        let agent = AgentId::new();

        reg.register(agent, vec![make_cap("old_cap", "1.0.0")]);
        assert_eq!(reg.agent_capabilities(&agent).unwrap().len(), 1);

        // Re-register with different capabilities
        reg.register(agent, vec![make_cap("new_cap", "2.0.0")]);
        let caps = reg.agent_capabilities(&agent).unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "new_cap");

        // old_cap should no longer be findable
        assert!(reg.find_capability("old_cap").is_empty());
    }

    #[test]
    fn test_registry_default() {
        let reg = CapabilityRegistry::default();
        assert!(reg.all_capabilities().is_empty());
    }

    #[test]
    fn test_schema_compatible_no_properties() {
        let schema = serde_json::json!({"type": "object"});
        assert!(is_schema_compatible(&schema, &serde_json::json!({"any": "key"})));
    }

    #[test]
    fn test_schema_compatible_matching_keys() {
        let schema = serde_json::json!({
            "properties": {"host": {}, "port": {}}
        });
        assert!(is_schema_compatible(&schema, &serde_json::json!({"host": "x"})));
        assert!(is_schema_compatible(&schema, &serde_json::json!({"host": "x", "port": 80})));
    }

    #[test]
    fn test_schema_incompatible_extra_key() {
        let schema = serde_json::json!({
            "properties": {"host": {}}
        });
        assert!(!is_schema_compatible(&schema, &serde_json::json!({"host": "x", "extra": 1})));
    }

    #[test]
    fn test_schema_compatible_non_object_input() {
        let schema = serde_json::json!({"properties": {"a": {}}});
        // Non-object inputs pass (can't validate)
        assert!(is_schema_compatible(&schema, &serde_json::json!("string")));
        assert!(is_schema_compatible(&schema, &serde_json::json!(42)));
    }

    #[test]
    fn test_capability_provider_serialization() {
        let provider = CapabilityProvider {
            agent_id: AgentId::new(),
            capabilities: vec![make_cap("test", "1.0.0")],
            registered_at: Utc::now(),
        };
        let json = serde_json::to_string(&provider).unwrap();
        let deser: CapabilityProvider = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.agent_id, provider.agent_id);
        assert_eq!(deser.capabilities.len(), 1);
    }

    #[test]
    fn test_capability_query_serialization() {
        let q = CapabilityQuery::new("scan")
            .with_version("1.0.0")
            .with_input(serde_json::json!({"host": "localhost"}));
        let json = serde_json::to_string(&q).unwrap();
        let deser: CapabilityQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "scan");
        assert_eq!(deser.required_version, Some("1.0.0".to_string()));
    }
}
