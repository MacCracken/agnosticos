//! Agent Marketplace
//!
//! Distribution, discovery, trust, and installation of agents and desktop apps.
//! Implements the marketplace architecture defined in ADR-015.

pub mod flutter_agpkg;
pub mod flutter_packaging;
pub mod local_registry;
pub mod ratings;
pub mod remote_client;
pub mod sandbox_profiles;
pub mod transparency;
pub mod trust;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use agnos_common::AgentManifest;

// ---------------------------------------------------------------------------
// Marketplace categories
// ---------------------------------------------------------------------------

/// Category for marketplace discovery and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketplaceCategory {
    Utility,
    Productivity,
    Security,
    DevTool,
    DesktopApp,
    System,
}

impl MarketplaceCategory {
    pub fn all() -> &'static [MarketplaceCategory] {
        &[
            Self::Utility,
            Self::Productivity,
            Self::Security,
            Self::DevTool,
            Self::DesktopApp,
            Self::System,
        ]
    }
}

impl std::fmt::Display for MarketplaceCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Utility => write!(f, "utility"),
            Self::Productivity => write!(f, "productivity"),
            Self::Security => write!(f, "security"),
            Self::DevTool => write!(f, "devtool"),
            Self::DesktopApp => write!(f, "desktop-app"),
            Self::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for MarketplaceCategory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "utility" => Ok(Self::Utility),
            "productivity" => Ok(Self::Productivity),
            "security" => Ok(Self::Security),
            "devtool" | "dev-tool" => Ok(Self::DevTool),
            "desktop-app" | "desktopapp" => Ok(Self::DesktopApp),
            "system" => Ok(Self::System),
            _ => Err(anyhow::anyhow!("Unknown marketplace category: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// Publisher info
// ---------------------------------------------------------------------------

/// Identity of a package publisher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherInfo {
    /// Human-readable publisher name.
    pub name: String,
    /// Ed25519 public key identifier (hex-encoded first 8 bytes).
    pub key_id: String,
    /// Optional homepage URL.
    #[serde(default)]
    pub homepage: String,
}

// ---------------------------------------------------------------------------
// Marketplace manifest
// ---------------------------------------------------------------------------

/// Extended manifest combining the base `AgentManifest` with marketplace metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceManifest {
    /// Core agent manifest (name, permissions, sandbox, etc.).
    #[serde(flatten)]
    pub agent: AgentManifest,
    /// Publisher identity.
    pub publisher: PublisherInfo,
    /// Marketplace category.
    pub category: MarketplaceCategory,
    /// Optional runtime type (e.g., "native", "wasm", "flutter").
    #[serde(default)]
    pub runtime: String,
    /// Screenshot URLs or relative paths.
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// Changelog for this version.
    #[serde(default)]
    pub changelog: String,
    /// Minimum AGNOS version required.
    #[serde(default)]
    pub min_agnos_version: String,
    /// Agent dependencies (name → version constraint).
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    /// Tags for discovery.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl MarketplaceManifest {
    /// Validate the manifest, returning a list of validation errors (empty = valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.agent.name.is_empty() {
            errors.push("name is required".to_string());
        }
        if self.agent.name.len() > 128 {
            errors.push("name must be 128 characters or fewer".to_string());
        }
        // Name must be lowercase alphanumeric + hyphens
        if !self.agent.name.is_empty()
            && !self
                .agent
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            errors
                .push("name must contain only lowercase letters, digits, and hyphens".to_string());
        }
        if self.agent.version.is_empty() {
            errors.push("version is required".to_string());
        }
        if !self.agent.version.is_empty() && !is_valid_semver(&self.agent.version) {
            errors.push(format!(
                "version '{}' is not valid semver",
                self.agent.version
            ));
        }
        if self.agent.description.is_empty() {
            errors.push("description is required".to_string());
        }
        if self.publisher.name.is_empty() {
            errors.push("publisher.name is required".to_string());
        }
        if self.publisher.key_id.is_empty() {
            errors.push("publisher.key_id is required".to_string());
        }

        errors
    }

    /// Fully-qualified package name: `publisher/agent-name`.
    pub fn qualified_name(&self) -> String {
        format!(
            "{}/{}",
            self.publisher.name.to_lowercase().replace(' ', "-"),
            self.agent.name
        )
    }
}

// ---------------------------------------------------------------------------
// Dependency resolution
// ---------------------------------------------------------------------------

/// A node in the dependency graph.
#[derive(Debug, Clone)]
pub struct DepNode {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
}

/// Directed acyclic graph for dependency resolution.
pub struct DependencyGraph {
    nodes: HashMap<String, DepNode>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Add a package to the graph.
    pub fn add(&mut self, node: DepNode) {
        self.nodes.insert(node.name.clone(), node);
    }

    /// Detect circular dependencies. Returns the cycle path if found.
    pub fn detect_cycle(&self) -> Option<Vec<String>> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();
        let mut path = Vec::new();

        for name in self.nodes.keys() {
            if !visited.contains(name.as_str()) {
                if let Some(cycle) = self.dfs_cycle(name, &mut visited, &mut stack, &mut path) {
                    return Some(cycle);
                }
            }
        }
        None
    }

    fn dfs_cycle<'a>(
        &'a self,
        name: &str,
        visited: &mut std::collections::HashSet<&'a str>,
        stack: &mut std::collections::HashSet<&'a str>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        let node = self.nodes.get(name)?;
        let name_ref: &str = &node.name;
        visited.insert(name_ref);
        stack.insert(name_ref);
        path.push(name.to_string());

        for dep in &node.dependencies {
            if let Some(dep_node) = self.nodes.get(dep.as_str()) {
                let dep_ref: &str = &dep_node.name;
                if !visited.contains(dep_ref) {
                    if let Some(cycle) = self.dfs_cycle(dep, visited, stack, path) {
                        return Some(cycle);
                    }
                } else if stack.contains(dep_ref) {
                    path.push(dep.clone());
                    return Some(path.clone());
                }
            }
        }

        stack.remove(name_ref);
        path.pop();
        None
    }

    /// Topological sort — returns install order. Errors on cycle.
    pub fn resolve(&self) -> anyhow::Result<Vec<String>> {
        if let Some(cycle) = self.detect_cycle() {
            return Err(anyhow::anyhow!(
                "Circular dependency detected: {}",
                cycle.join(" -> ")
            ));
        }

        // Build in-degree map and reverse dependency map (dep → dependents) for O(n+e) sort.
        // If A depends on B, B must come first. In-degree of A increases per dependency.
        let mut in_deg: HashMap<&str, usize> = HashMap::new();
        let mut reverse_deps: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in self.nodes.values() {
            in_deg.entry(&node.name).or_insert(0);
            for dep in &node.dependencies {
                if self.nodes.contains_key(dep.as_str()) {
                    in_deg.entry(dep.as_str()).or_insert(0);
                    *in_deg.entry(&node.name).or_insert(0) += 1;
                    reverse_deps
                        .entry(dep.as_str())
                        .or_default()
                        .push(&node.name);
                }
            }
        }

        let mut queue_sorted: Vec<&str> = in_deg
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();
        queue_sorted.sort();
        let mut queue: std::collections::VecDeque<&str> = queue_sorted.into_iter().collect();

        let mut order = Vec::with_capacity(self.nodes.len());

        while let Some(name) = queue.pop_front() {
            order.push(name.to_string());
            // O(1) lookup of dependents instead of O(n) scan
            if let Some(dependents) = reverse_deps.get(name) {
                for &dependent in dependents {
                    if let Some(deg) = in_deg.get_mut(dependent) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }

        Ok(order)
    }

    /// Check if all dependencies are satisfiable (exist in the graph).
    pub fn check_missing(&self) -> Vec<(String, String)> {
        let mut missing = Vec::new();
        for node in self.nodes.values() {
            for dep in &node.dependencies {
                if !self.nodes.contains_key(dep.as_str()) {
                    missing.push((node.name.clone(), dep.clone()));
                }
            }
        }
        missing
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Basic semver validation (major.minor.patch with optional pre-release).
fn is_valid_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.splitn(2, '-').collect();
    let version_part = parts[0];
    let segments: Vec<&str> = version_part.split('.').collect();
    if segments.len() != 3 {
        return false;
    }
    segments.iter().all(|s| s.parse::<u64>().is_ok())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_category_display() {
        assert_eq!(MarketplaceCategory::Utility.to_string(), "utility");
        assert_eq!(MarketplaceCategory::DesktopApp.to_string(), "desktop-app");
        assert_eq!(MarketplaceCategory::Security.to_string(), "security");
    }

    #[test]
    fn test_marketplace_category_from_str() {
        assert_eq!(
            "utility".parse::<MarketplaceCategory>().unwrap(),
            MarketplaceCategory::Utility
        );
        assert_eq!(
            "desktop-app".parse::<MarketplaceCategory>().unwrap(),
            MarketplaceCategory::DesktopApp
        );
        assert_eq!(
            "devtool".parse::<MarketplaceCategory>().unwrap(),
            MarketplaceCategory::DevTool
        );
        assert_eq!(
            "dev-tool".parse::<MarketplaceCategory>().unwrap(),
            MarketplaceCategory::DevTool
        );
        assert!("unknown".parse::<MarketplaceCategory>().is_err());
    }

    #[test]
    fn test_marketplace_category_all() {
        let all = MarketplaceCategory::all();
        assert_eq!(all.len(), 6);
    }

    #[test]
    fn test_marketplace_category_case_insensitive() {
        assert_eq!(
            "UTILITY".parse::<MarketplaceCategory>().unwrap(),
            MarketplaceCategory::Utility
        );
        assert_eq!(
            "Security".parse::<MarketplaceCategory>().unwrap(),
            MarketplaceCategory::Security
        );
    }

    fn sample_manifest() -> MarketplaceManifest {
        MarketplaceManifest {
            agent: AgentManifest {
                name: "test-agent".to_string(),
                description: "A test agent".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            },
            publisher: PublisherInfo {
                name: "Test Publisher".to_string(),
                key_id: "abc12345".to_string(),
                homepage: "https://example.com".to_string(),
            },
            category: MarketplaceCategory::Utility,
            runtime: "native".to_string(),
            screenshots: vec![],
            changelog: "Initial release".to_string(),
            min_agnos_version: "2026.3.6".to_string(),
            dependencies: HashMap::new(),
            tags: vec!["test".to_string()],
        }
    }

    #[test]
    fn test_manifest_validate_valid() {
        let m = sample_manifest();
        assert!(m.validate().is_empty());
    }

    #[test]
    fn test_manifest_validate_missing_name() {
        let mut m = sample_manifest();
        m.agent.name = String::new();
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("name is required")));
    }

    #[test]
    fn test_manifest_validate_long_name() {
        let mut m = sample_manifest();
        m.agent.name = "a".repeat(200);
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("128 characters")));
    }

    #[test]
    fn test_manifest_validate_invalid_name_chars() {
        let mut m = sample_manifest();
        m.agent.name = "Test Agent!".to_string();
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("lowercase")));
    }

    #[test]
    fn test_manifest_validate_missing_version() {
        let mut m = sample_manifest();
        m.agent.version = String::new();
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("version is required")));
    }

    #[test]
    fn test_manifest_validate_invalid_semver() {
        let mut m = sample_manifest();
        m.agent.version = "not-a-version".to_string();
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("semver")));
    }

    #[test]
    fn test_manifest_validate_missing_description() {
        let mut m = sample_manifest();
        m.agent.description = String::new();
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("description is required")));
    }

    #[test]
    fn test_manifest_validate_missing_publisher() {
        let mut m = sample_manifest();
        m.publisher.name = String::new();
        m.publisher.key_id = String::new();
        let errors = m.validate();
        assert!(errors.iter().any(|e| e.contains("publisher.name")));
        assert!(errors.iter().any(|e| e.contains("publisher.key_id")));
    }

    #[test]
    fn test_manifest_qualified_name() {
        let m = sample_manifest();
        assert_eq!(m.qualified_name(), "test-publisher/test-agent");
    }

    #[test]
    fn test_manifest_serialization() {
        let m = sample_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let parsed: MarketplaceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent.name, "test-agent");
        assert_eq!(parsed.category, MarketplaceCategory::Utility);
        assert_eq!(parsed.publisher.key_id, "abc12345");
    }

    #[test]
    fn test_semver_validation() {
        assert!(is_valid_semver("1.0.0"));
        assert!(is_valid_semver("0.1.0"));
        assert!(is_valid_semver("2026.3.6"));
        assert!(is_valid_semver("1.0.0-alpha"));
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("abc"));
        assert!(!is_valid_semver("1.0.0.0"));
    }

    #[test]
    fn test_dependency_graph_empty() {
        let graph = DependencyGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
        assert!(graph.resolve().unwrap().is_empty());
    }

    #[test]
    fn test_dependency_graph_single_node() {
        let mut graph = DependencyGraph::new();
        graph.add(DepNode {
            name: "agent-a".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec![],
        });
        let order = graph.resolve().unwrap();
        assert_eq!(order, vec!["agent-a"]);
    }

    #[test]
    fn test_dependency_graph_linear_chain() {
        let mut graph = DependencyGraph::new();
        graph.add(DepNode {
            name: "c".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["b".to_string()],
        });
        graph.add(DepNode {
            name: "b".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["a".to_string()],
        });
        graph.add(DepNode {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec![],
        });
        let order = graph.resolve().unwrap();
        // a must come before b, b before c
        let pos_a = order.iter().position(|n| n == "a").unwrap();
        let pos_b = order.iter().position(|n| n == "b").unwrap();
        let pos_c = order.iter().position(|n| n == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_dependency_graph_diamond() {
        let mut graph = DependencyGraph::new();
        graph.add(DepNode {
            name: "d".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["b".to_string(), "c".to_string()],
        });
        graph.add(DepNode {
            name: "b".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["a".to_string()],
        });
        graph.add(DepNode {
            name: "c".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["a".to_string()],
        });
        graph.add(DepNode {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec![],
        });
        let order = graph.resolve().unwrap();
        assert_eq!(order.len(), 4);
        let pos_a = order.iter().position(|n| n == "a").unwrap();
        let pos_d = order.iter().position(|n| n == "d").unwrap();
        assert!(pos_a < pos_d);
    }

    #[test]
    fn test_dependency_graph_cycle_detection() {
        let mut graph = DependencyGraph::new();
        graph.add(DepNode {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["b".to_string()],
        });
        graph.add(DepNode {
            name: "b".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["a".to_string()],
        });
        assert!(graph.detect_cycle().is_some());
        assert!(graph.resolve().is_err());
    }

    #[test]
    fn test_dependency_graph_missing_deps() {
        let mut graph = DependencyGraph::new();
        graph.add(DepNode {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec!["missing-dep".to_string()],
        });
        let missing = graph.check_missing();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], ("a".to_string(), "missing-dep".to_string()));
    }

    #[test]
    fn test_dependency_graph_no_missing() {
        let mut graph = DependencyGraph::new();
        graph.add(DepNode {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            dependencies: vec![],
        });
        assert!(graph.check_missing().is_empty());
    }

    #[test]
    fn test_publisher_info_serialization() {
        let p = PublisherInfo {
            name: "AGNOS Team".to_string(),
            key_id: "deadbeef".to_string(),
            homepage: "https://agnos.org".to_string(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: PublisherInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "AGNOS Team");
        assert_eq!(parsed.key_id, "deadbeef");
    }
}
