//! Multi-domain agent grouping HUD widget for the AGNOS desktop environment.
//!
//! Reads the agent list from `http://localhost:8090/v1/agents`, groups each
//! agent by its `domain` metadata field, and exposes a filterable view that
//! the compositor can render as domain tabs/chips with per-domain agent cards.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

/// A single agent entry as shown inside a domain group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainAgentEntry {
    pub agent_id: Uuid,
    pub name: String,
    pub domain: String,
    pub status: String,
    pub current_task: Option<String>,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

/// One rendered domain group containing its member agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainGroup {
    /// The domain label (e.g. `"security"`, `"trading"`, `"media"`).
    pub domain: String,
    /// Agents belonging to this domain in the current filtered view.
    pub agents: Vec<DomainAgentEntry>,
    /// Pre-computed count of agents in a running/active state.
    pub active_count: usize,
}

/// Render output produced by [`DomainFilterWidget::render`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainFilterRenderData {
    /// Ordered list of domain groups (sorted alphabetically by domain name).
    pub groups: Vec<DomainGroup>,
    /// All known domain names (for rendering filter tabs/chips in the UI).
    pub all_domains: Vec<String>,
    /// Currently active domain filter; `None` means all domains are shown.
    pub active_filter: Option<String>,
    /// Total number of agents across all visible groups.
    pub total_visible: usize,
    /// Whether the last `update()` call succeeded.
    pub last_fetch_ok: bool,
}

/// Internal agent record stored before grouping.
#[derive(Debug, Clone)]
struct RawAgent {
    agent_id: Uuid,
    name: String,
    domain: String,
    status: String,
    current_task: Option<String>,
    last_heartbeat: Option<DateTime<Utc>>,
}

/// HUD widget that groups agents by domain and supports filtering.
///
/// # Example
/// ```rust,no_run
/// use desktop_environment::hud::domain_filter::DomainFilterWidget;
/// use std::time::Duration;
///
/// let widget = DomainFilterWidget::new();
/// let handle = widget.start_polling(Duration::from_secs(5));
///
/// // Switch to showing only the "security" domain
/// widget.set_filter(Some("security".to_string()));
///
/// let render = widget.render();
/// for group in &render.groups {
///     println!("{}: {} agent(s)", group.domain, group.agents.len());
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DomainFilterWidget {
    agents: Arc<RwLock<Vec<RawAgent>>>,
    active_filter: Arc<RwLock<Option<String>>>,
    last_fetch_ok: Arc<RwLock<bool>>,
}

impl Default for DomainFilterWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainFilterWidget {
    /// Create a new widget with no agents and no active filter.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(Vec::new())),
            active_filter: Arc::new(RwLock::new(None)),
            last_fetch_ok: Arc::new(RwLock::new(false)),
        }
    }

    /// Set or clear the active domain filter.
    ///
    /// Pass `None` to show all domains; pass `Some("security")` etc. to narrow.
    pub fn set_filter(&self, domain: Option<String>) {
        *self
            .active_filter
            .write()
            .unwrap_or_else(|e| e.into_inner()) = domain;
    }

    /// Return the current domain filter.
    pub fn active_filter(&self) -> Option<String> {
        self.active_filter
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Produce display-ready grouped data for the compositor.
    pub fn render(&self) -> DomainFilterRenderData {
        let agents = self
            .agents
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let filter = self
            .active_filter
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let last_fetch_ok = *self.last_fetch_ok.read().unwrap_or_else(|e| e.into_inner());

        // Collect all known domains (for tab rendering) before applying filter.
        let mut all_domains: Vec<String> = {
            let mut seen: HashMap<String, ()> = HashMap::new();
            for a in &agents {
                seen.entry(a.domain.clone()).or_default();
            }
            let mut v: Vec<String> = seen.into_keys().collect();
            v.sort();
            v
        };
        all_domains.dedup();

        // Apply optional domain filter.
        let visible_agents: Vec<&RawAgent> = agents
            .iter()
            .filter(|a| filter.as_deref().map(|f| a.domain == f).unwrap_or(true))
            .collect();

        // Group by domain.
        let mut grouped: HashMap<String, Vec<DomainAgentEntry>> = HashMap::new();
        for a in &visible_agents {
            grouped
                .entry(a.domain.clone())
                .or_default()
                .push(DomainAgentEntry {
                    agent_id: a.agent_id,
                    name: a.name.clone(),
                    domain: a.domain.clone(),
                    status: a.status.clone(),
                    current_task: a.current_task.clone(),
                    last_heartbeat: a.last_heartbeat,
                });
        }

        let total_visible = visible_agents.len();

        let mut groups: Vec<DomainGroup> = grouped
            .into_iter()
            .map(|(domain, agents)| {
                let active_count = agents
                    .iter()
                    .filter(|a| matches!(a.status.as_str(), "running" | "active" | "Acting"))
                    .count();
                DomainGroup {
                    domain,
                    agents,
                    active_count,
                }
            })
            .collect();

        groups.sort_by(|a, b| a.domain.cmp(&b.domain));

        DomainFilterRenderData {
            groups,
            all_domains,
            active_filter: filter,
            total_visible,
            last_fetch_ok,
        }
    }

    /// Fetch agent data from daimon and rebuild internal state.
    pub async fn update(&self) {
        match Self::fetch_agents().await {
            Ok(raw) => {
                let mut agents = self.agents.write().unwrap_or_else(|e| e.into_inner());
                *agents = raw;
                *self
                    .last_fetch_ok
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = true;
                debug!(
                    "Domain filter HUD: {} agent(s) across domains",
                    agents.len()
                );
            }
            Err(e) => {
                warn!("Domain filter HUD update failed: {}", e);
                *self
                    .last_fetch_ok
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = false;
            }
        }
    }

    /// Spawn a background task that calls [`update`](Self::update) on `interval`.
    pub fn start_polling(&self, interval: std::time::Duration) -> tokio::task::JoinHandle<()> {
        let widget = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                widget.update().await;
            }
        })
    }

    /// Directly inject agent data (useful for tests or offline mode).
    pub fn set_agents(&self, entries: Vec<DomainAgentEntry>) {
        let raw: Vec<RawAgent> = entries
            .into_iter()
            .map(|e| RawAgent {
                agent_id: e.agent_id,
                name: e.name,
                domain: e.domain,
                status: e.status,
                current_task: e.current_task,
                last_heartbeat: e.last_heartbeat,
            })
            .collect();
        let mut agents = self.agents.write().unwrap_or_else(|e| e.into_inner());
        *agents = raw;
        *self
            .last_fetch_ok
            .write()
            .unwrap_or_else(|e| e.into_inner()) = true;
    }

    // --- private helpers ---

    async fn fetch_agents() -> Result<Vec<RawAgent>, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        let resp = client.get("http://localhost:8090/v1/agents").send().await?;

        if !resp.status().is_success() {
            return Err(format!("API returned {}", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        Self::parse_agents(&body)
    }

    fn parse_agents(
        json: &serde_json::Value,
    ) -> Result<Vec<RawAgent>, Box<dyn std::error::Error + Send + Sync>> {
        let arr = json["agents"].as_array().cloned().unwrap_or_default();

        let agents = arr
            .iter()
            .map(|a| {
                let id_str = a["id"].as_str().unwrap_or_default();
                let agent_id = Uuid::parse_str(id_str).unwrap_or_else(|_| Uuid::new_v4());

                // Domain comes from the agent metadata or a top-level field.
                let domain = a["domain"]
                    .as_str()
                    .or_else(|| a["metadata"]["domain"].as_str())
                    .unwrap_or("general")
                    .to_string();

                let last_heartbeat = a["last_heartbeat"]
                    .as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                let current_task = a["current_task"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);

                RawAgent {
                    agent_id,
                    name: a["name"].as_str().unwrap_or("unknown").to_string(),
                    domain,
                    status: a["status"].as_str().unwrap_or("unknown").to_string(),
                    current_task,
                    last_heartbeat,
                }
            })
            .collect();

        Ok(agents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(id: &str, name: &str, domain: &str, status: &str) -> DomainAgentEntry {
        DomainAgentEntry {
            agent_id: Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()),
            name: name.to_string(),
            domain: domain.to_string(),
            status: status.to_string(),
            current_task: None,
            last_heartbeat: None,
        }
    }

    #[test]
    fn test_render_no_filter() {
        let widget = DomainFilterWidget::new();
        widget.set_agents(vec![
            make_agent(
                "00000000-0000-0000-0000-000000000001",
                "sentinel",
                "security",
                "running",
            ),
            make_agent(
                "00000000-0000-0000-0000-000000000002",
                "trader",
                "trading",
                "idle",
            ),
            make_agent(
                "00000000-0000-0000-0000-000000000003",
                "analyst",
                "security",
                "idle",
            ),
        ]);

        let data = widget.render();
        assert_eq!(data.total_visible, 3);
        assert_eq!(data.all_domains, vec!["security", "trading"]);
        assert!(data.active_filter.is_none());
        assert_eq!(data.groups.len(), 2);

        let sec = data.groups.iter().find(|g| g.domain == "security").unwrap();
        assert_eq!(sec.agents.len(), 2);
        assert_eq!(sec.active_count, 1);
    }

    #[test]
    fn test_render_with_filter() {
        let widget = DomainFilterWidget::new();
        widget.set_agents(vec![
            make_agent(
                "00000000-0000-0000-0000-000000000001",
                "sentinel",
                "security",
                "running",
            ),
            make_agent(
                "00000000-0000-0000-0000-000000000002",
                "trader",
                "trading",
                "idle",
            ),
        ]);
        widget.set_filter(Some("trading".to_string()));

        let data = widget.render();
        assert_eq!(data.total_visible, 1);
        assert_eq!(data.active_filter, Some("trading".to_string()));
        assert_eq!(data.groups.len(), 1);
        assert_eq!(data.groups[0].domain, "trading");
        // All domains are still reported for tab rendering.
        assert_eq!(data.all_domains.len(), 2);
    }

    #[test]
    fn test_set_filter_none_shows_all() {
        let widget = DomainFilterWidget::new();
        widget.set_filter(Some("media".to_string()));
        widget.set_filter(None);
        assert!(widget.active_filter().is_none());
    }

    #[test]
    fn test_parse_agents_json() {
        let json = serde_json::json!({
            "agents": [
                {
                    "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                    "name": "watcher",
                    "status": "running",
                    "domain": "security",
                    "current_task": "scanning",
                    "last_heartbeat": "2026-03-18T12:00:00Z"
                },
                {
                    "id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                    "name": "packer",
                    "status": "idle",
                    "metadata": { "domain": "build" }
                }
            ]
        });

        let agents = DomainFilterWidget::parse_agents(&json).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].domain, "security");
        assert_eq!(agents[0].current_task, Some("scanning".to_string()));
        assert!(agents[0].last_heartbeat.is_some());
        // Domain from metadata fallback
        assert_eq!(agents[1].domain, "build");
        assert!(agents[1].current_task.is_none());
    }

    #[test]
    fn test_parse_agents_missing_domain_defaults_to_general() {
        let json = serde_json::json!({
            "agents": [
                { "id": "cccccccc-cccc-cccc-cccc-cccccccccccc", "name": "orphan", "status": "idle" }
            ]
        });
        let agents = DomainFilterWidget::parse_agents(&json).unwrap();
        assert_eq!(agents[0].domain, "general");
    }

    #[test]
    fn test_render_empty_widget() {
        let widget = DomainFilterWidget::new();
        let data = widget.render();
        assert!(data.groups.is_empty());
        assert!(data.all_domains.is_empty());
        assert_eq!(data.total_visible, 0);
        assert!(!data.last_fetch_ok);
    }

    #[test]
    fn test_groups_sorted_alphabetically() {
        let widget = DomainFilterWidget::new();
        widget.set_agents(vec![
            make_agent(
                "00000000-0000-0000-0000-000000000001",
                "z-agent",
                "zzz",
                "idle",
            ),
            make_agent(
                "00000000-0000-0000-0000-000000000002",
                "a-agent",
                "aaa",
                "idle",
            ),
            make_agent(
                "00000000-0000-0000-0000-000000000003",
                "m-agent",
                "mmm",
                "idle",
            ),
        ]);
        let data = widget.render();
        let names: Vec<&str> = data.groups.iter().map(|g| g.domain.as_str()).collect();
        assert_eq!(names, vec!["aaa", "mmm", "zzz"]);
    }
}
