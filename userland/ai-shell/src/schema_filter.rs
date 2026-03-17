//! MCP tool schema filtering for agnoshi.
//!
//! Implements keyword-based relevance filtering so only tool schemas matching
//! the conversation context are sent to the LLM. Reduces cold-request token
//! usage by 60-90% — critical for edge devices with limited resources.

use std::collections::HashMap;

/// A category of MCP tools with associated keywords for relevance matching.
#[derive(Debug, Clone)]
pub struct ToolCategory {
    pub name: &'static str,
    pub tool_prefixes: &'static [&'static str],
    pub keywords: &'static [&'static str],
    pub always_include: bool,
}

/// Canonical tool categories with keyword triggers.
pub const TOOL_CATEGORIES: &[ToolCategory] = &[
    ToolCategory {
        name: "core",
        tool_prefixes: &[
            "agnos_health",
            "agnos_register",
            "agnos_deregister",
            "agnos_agents",
        ],
        keywords: &["health", "status", "agent", "register", "deregister"],
        always_include: true,
    },
    ToolCategory {
        name: "memory",
        tool_prefixes: &["agnos_memory_", "agnos_agent_memory"],
        keywords: &["memory", "remember", "store", "recall", "forget", "kv"],
        always_include: false,
    },
    ToolCategory {
        name: "audit",
        tool_prefixes: &["agnos_audit_", "agnos_traces_"],
        keywords: &["audit", "trace", "log", "chain", "compliance", "forensic"],
        always_include: false,
    },
    ToolCategory {
        name: "llm",
        tool_prefixes: &["agnos_gateway_", "agnos_chat"],
        keywords: &[
            "model",
            "llm",
            "chat",
            "inference",
            "generate",
            "prompt",
            "token",
            "ollama",
        ],
        always_include: false,
    },
    ToolCategory {
        name: "edge",
        tool_prefixes: &["agnos_edge_", "edge_"],
        keywords: &[
            "edge",
            "fleet",
            "node",
            "deploy",
            "ota",
            "update",
            "decommission",
            "iot",
        ],
        always_include: false,
    },
    ToolCategory {
        name: "sandbox",
        tool_prefixes: &["agnos_sandbox_"],
        keywords: &[
            "sandbox",
            "isolat",
            "seccomp",
            "landlock",
            "permission",
            "capability",
        ],
        always_include: false,
    },
    ToolCategory {
        name: "phylax",
        tool_prefixes: &["phylax_"],
        keywords: &[
            "scan",
            "threat",
            "malware",
            "yara",
            "entropy",
            "suspicious",
            "phylax",
            "security",
        ],
        always_include: false,
    },
    ToolCategory {
        name: "webhook",
        tool_prefixes: &["agnos_webhook"],
        keywords: &["webhook", "event", "subscribe", "notify", "callback"],
        always_include: false,
    },
    ToolCategory {
        name: "bridge",
        tool_prefixes: &["agnos_bridge_"],
        keywords: &[
            "bridge",
            "secureyeoman",
            "sy",
            "profile",
            "sync",
            "discover",
        ],
        always_include: false,
    },
    ToolCategory {
        name: "marketplace",
        tool_prefixes: &["marketplace_", "mela_"],
        keywords: &["marketplace", "install", "package", "app", "mela"],
        always_include: false,
    },
];

/// Tool schema with minimal metadata for filtering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of schema filtering.
#[derive(Debug)]
pub struct FilterResult {
    pub schemas: Vec<ToolSchema>,
    pub matched_categories: Vec<String>,
    pub total_available: usize,
    pub filtered_count: usize,
}

/// Filter tool schemas based on conversation context keywords.
///
/// Returns only schemas whose category keywords appear in the input text.
/// Categories marked `always_include` are always returned.
pub fn filter_schemas(input: &str, all_schemas: &[ToolSchema]) -> FilterResult {
    let input_lower = input.to_lowercase();
    let total_available = all_schemas.len();

    // Determine which categories are relevant
    let mut matched_categories = Vec::new();
    let mut relevant_prefixes: Vec<&str> = Vec::new();

    for cat in TOOL_CATEGORIES {
        if cat.always_include || cat.keywords.iter().any(|kw| input_lower.contains(kw)) {
            matched_categories.push(cat.name.to_string());
            relevant_prefixes.extend_from_slice(cat.tool_prefixes);
        }
    }

    // Filter schemas to only those matching relevant prefixes
    let schemas: Vec<ToolSchema> = all_schemas
        .iter()
        .filter(|s| {
            relevant_prefixes
                .iter()
                .any(|prefix| s.name.starts_with(prefix))
        })
        .cloned()
        .collect();

    let filtered_count = schemas.len();

    FilterResult {
        schemas,
        matched_categories,
        total_available,
        filtered_count,
    }
}

/// Cache for recently-used tool schemas to avoid re-filtering on follow-up messages.
#[derive(Debug, Default)]
pub struct SchemaCache {
    /// Categories that have been active in the current conversation.
    active_categories: Vec<String>,
    /// TTL in number of messages before a category expires from the cache.
    ttl: usize,
    /// Counter of messages since each category was last triggered.
    category_age: HashMap<String, usize>,
}

impl SchemaCache {
    pub fn new(ttl: usize) -> Self {
        Self {
            active_categories: Vec::new(),
            ttl,
            category_age: HashMap::new(),
        }
    }

    /// Update the cache with newly matched categories and age out stale ones.
    pub fn update(&mut self, matched: &[String]) {
        // Reset age for matched categories
        for cat in matched {
            self.category_age.insert(cat.clone(), 0);
            if !self.active_categories.contains(cat) {
                self.active_categories.push(cat.clone());
            }
        }

        // Age all categories
        let ttl = self.ttl;
        self.category_age.values_mut().for_each(|age| *age += 1);

        // Remove expired categories
        self.category_age.retain(|_, age| *age <= ttl);
        self.active_categories
            .retain(|cat| self.category_age.contains_key(cat));
    }

    /// Get all currently active category names (matched + cached).
    pub fn active_categories(&self) -> &[String] {
        &self.active_categories
    }

    /// Filter schemas using both fresh keyword matches and cached categories.
    pub fn filter_with_cache(&mut self, input: &str, all_schemas: &[ToolSchema]) -> FilterResult {
        let mut result = filter_schemas(input, all_schemas);

        // Add cached categories that aren't already matched
        let cached_prefixes: Vec<&str> = TOOL_CATEGORIES
            .iter()
            .filter(|cat| {
                self.active_categories.contains(&cat.name.to_string())
                    && !result.matched_categories.contains(&cat.name.to_string())
            })
            .flat_map(|cat| cat.tool_prefixes.iter().copied())
            .collect();

        // Add cached schemas
        for schema in all_schemas {
            if cached_prefixes.iter().any(|p| schema.name.starts_with(p))
                && !result.schemas.iter().any(|s| s.name == schema.name)
            {
                result.schemas.push(schema.clone());
                result.filtered_count += 1;
            }
        }

        // Update cache
        self.update(&result.matched_categories);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schemas() -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "agnos_health".into(),
                description: "Check health".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_agents_list".into(),
                description: "List agents".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_memory_get".into(),
                description: "Get memory".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_memory_set".into(),
                description: "Set memory".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_audit_query".into(),
                description: "Query audit".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_gateway_models".into(),
                description: "List models".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "phylax_scan".into(),
                description: "Scan for threats".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_edge_list".into(),
                description: "List edge nodes".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "agnos_bridge_discover".into(),
                description: "Discover bridge tools".into(),
                input_schema: serde_json::json!({}),
            },
            ToolSchema {
                name: "marketplace_search".into(),
                description: "Search marketplace".into(),
                input_schema: serde_json::json!({}),
            },
        ]
    }

    #[test]
    fn test_core_always_included() {
        let schemas = sample_schemas();
        let result = filter_schemas("hello world", &schemas);
        // Core is always included
        assert!(result.matched_categories.contains(&"core".to_string()));
        assert!(result.schemas.iter().any(|s| s.name == "agnos_health"));
        assert!(result.schemas.iter().any(|s| s.name == "agnos_agents_list"));
        // Non-core should NOT be included for unrelated input
        assert!(!result.schemas.iter().any(|s| s.name == "agnos_memory_get"));
        assert!(!result.schemas.iter().any(|s| s.name == "phylax_scan"));
    }

    #[test]
    fn test_keyword_matching() {
        let schemas = sample_schemas();
        let result = filter_schemas("check the memory store", &schemas);
        assert!(result.matched_categories.contains(&"memory".to_string()));
        assert!(result.schemas.iter().any(|s| s.name == "agnos_memory_get"));
        assert!(result.schemas.iter().any(|s| s.name == "agnos_memory_set"));
    }

    #[test]
    fn test_security_keywords() {
        let schemas = sample_schemas();
        let result = filter_schemas("scan for malware threats", &schemas);
        assert!(result.matched_categories.contains(&"phylax".to_string()));
        assert!(result.schemas.iter().any(|s| s.name == "phylax_scan"));
    }

    #[test]
    fn test_edge_keywords() {
        let schemas = sample_schemas();
        let result = filter_schemas("list the edge fleet nodes", &schemas);
        assert!(result.matched_categories.contains(&"edge".to_string()));
        assert!(result.schemas.iter().any(|s| s.name == "agnos_edge_list"));
    }

    #[test]
    fn test_multiple_categories() {
        let schemas = sample_schemas();
        let result = filter_schemas("scan the edge node for threats", &schemas);
        assert!(result.matched_categories.contains(&"phylax".to_string()));
        assert!(result.matched_categories.contains(&"edge".to_string()));
        assert!(result.schemas.iter().any(|s| s.name == "phylax_scan"));
        assert!(result.schemas.iter().any(|s| s.name == "agnos_edge_list"));
    }

    #[test]
    fn test_bridge_keywords() {
        let schemas = sample_schemas();
        let result = filter_schemas("discover bridge tools from secureyeoman", &schemas);
        assert!(result.matched_categories.contains(&"bridge".to_string()));
        assert!(result
            .schemas
            .iter()
            .any(|s| s.name == "agnos_bridge_discover"));
    }

    #[test]
    fn test_no_duplicates() {
        let schemas = sample_schemas();
        let result = filter_schemas("agent health status", &schemas);
        let names: Vec<&str> = result.schemas.iter().map(|s| s.name.as_str()).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len(), "no duplicate schemas");
    }

    #[test]
    fn test_cache_retains_recent() {
        let schemas = sample_schemas();
        let mut cache = SchemaCache::new(3);

        // First message triggers memory
        let r1 = cache.filter_with_cache("check the memory", &schemas);
        assert!(r1.schemas.iter().any(|s| s.name == "agnos_memory_get"));

        // Second message doesn't mention memory but it should still be cached
        let r2 = cache.filter_with_cache("hello world", &schemas);
        assert!(r2.schemas.iter().any(|s| s.name == "agnos_memory_get"));
    }

    #[test]
    fn test_cache_expires() {
        let schemas = sample_schemas();
        let mut cache = SchemaCache::new(2);

        // Trigger memory category
        cache.filter_with_cache("check memory", &schemas);

        // Age it out: 3 unrelated messages exceed TTL of 2
        cache.filter_with_cache("unrelated 1", &schemas);
        cache.filter_with_cache("unrelated 2", &schemas);
        let r = cache.filter_with_cache("unrelated 3", &schemas);

        // Memory should have expired
        assert!(!r.schemas.iter().any(|s| s.name == "agnos_memory_get"));
    }

    #[test]
    fn test_full_profile_returns_all_on_model_keyword() {
        let schemas = sample_schemas();
        let result = filter_schemas(
            "list all models and check the audit log for the edge fleet",
            &schemas,
        );
        assert!(result.matched_categories.contains(&"llm".to_string()));
        assert!(result.matched_categories.contains(&"audit".to_string()));
        assert!(result.matched_categories.contains(&"edge".to_string()));
    }

    #[test]
    fn test_empty_input() {
        let schemas = sample_schemas();
        let result = filter_schemas("", &schemas);
        // Only core (always_include) should be present
        assert_eq!(result.matched_categories, vec!["core"]);
    }

    #[test]
    fn test_empty_schemas() {
        let result = filter_schemas("check memory", &[]);
        assert_eq!(result.filtered_count, 0);
        assert!(result.schemas.is_empty());
    }
}
