//! Agent Activity Dashboard
//!
//! Provides an htop-style TUI view of all registered agents showing
//! status, resource usage, task queue depth, last action, and errors.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Snapshot of a single agent's status for dashboard display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDashboardEntry {
    pub id: String,
    pub name: String,
    pub status: String,
    pub cpu_percent: Option<f32>,
    pub memory_mb: Option<u64>,
    pub task_count: u32,
    pub error_count: u32,
    pub last_action: Option<String>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub uptime_seconds: u64,
}

/// Dashboard state aggregated from the runtime API
#[derive(Debug, Clone, Default)]
pub struct DashboardState {
    pub agents: Vec<AgentDashboardEntry>,
    pub total_agents: usize,
    pub running_agents: usize,
    pub total_cpu: f32,
    pub total_memory_mb: u64,
    pub total_errors: u32,
    pub last_refresh: Option<DateTime<Utc>>,
}

impl DashboardState {
    /// Build dashboard state from a list of agent entries
    pub fn from_entries(entries: Vec<AgentDashboardEntry>) -> Self {
        let total_agents = entries.len();
        let running_agents = entries.iter().filter(|e| e.status == "running").count();
        let total_cpu: f32 = entries.iter().filter_map(|e| e.cpu_percent).sum();
        let total_memory_mb: u64 = entries.iter().filter_map(|e| e.memory_mb).sum();
        let total_errors: u32 = entries.iter().map(|e| e.error_count).sum();

        Self {
            agents: entries,
            total_agents,
            running_agents,
            total_cpu,
            total_memory_mb,
            total_errors,
            last_refresh: Some(Utc::now()),
        }
    }

    /// Format the dashboard as a text table for terminal display
    pub fn render_table(&self) -> String {
        let mut lines = Vec::new();

        // Header
        lines.push(format!(
            "AGNOS Agent Dashboard — {} agents ({} running) | CPU: {:.1}% | Mem: {} MB | Errors: {}",
            self.total_agents,
            self.running_agents,
            self.total_cpu,
            self.total_memory_mb,
            self.total_errors
        ));
        lines.push("\u{2500}".repeat(100));
        lines.push(format!(
            "{:<36} {:<12} {:>6} {:>8} {:>6} {:>6} {:<20}",
            "ID", "STATUS", "CPU%", "MEM(MB)", "TASKS", "ERRS", "LAST ACTION"
        ));
        lines.push("\u{2500}".repeat(100));

        if self.agents.is_empty() {
            lines.push("  (no agents registered)".to_string());
        } else {
            for agent in &self.agents {
                let cpu = agent
                    .cpu_percent
                    .map(|c| format!("{:.1}", c))
                    .unwrap_or_else(|| "\u{2014}".into());
                let mem = agent
                    .memory_mb
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "\u{2014}".into());
                let action = agent.last_action.as_deref().unwrap_or("\u{2014}");
                let action_truncated = if action.len() > 20 {
                    &action[..20]
                } else {
                    action
                };

                lines.push(format!(
                    "{:<36} {:<12} {:>6} {:>8} {:>6} {:>6} {:<20}",
                    if agent.id.len() > 36 {
                        &agent.id[..36]
                    } else {
                        &agent.id
                    },
                    agent.status,
                    cpu,
                    mem,
                    agent.task_count,
                    agent.error_count,
                    action_truncated
                ));
            }
        }

        lines.push("\u{2500}".repeat(100));
        if let Some(ts) = self.last_refresh {
            lines.push(format!("Last refresh: {}", ts.format("%H:%M:%S")));
        }

        lines.join("\n")
    }
}

/// Client for fetching dashboard data from the Agent Runtime API
pub struct DashboardClient {
    endpoint: String,
    client: reqwest::Client,
}

impl DashboardClient {
    pub fn new(endpoint: Option<String>) -> Self {
        let endpoint = endpoint.unwrap_or_else(|| "http://127.0.0.1:8090".to_string());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { endpoint, client }
    }

    /// Fetch current agent list from the runtime API
    pub async fn fetch(&self) -> anyhow::Result<DashboardState> {
        let url = format!("{}/v1/agents", self.endpoint);
        let resp = self.client.get(&url).send().await?;
        let body: serde_json::Value = resp.json().await?;

        let mut entries = Vec::new();
        if let Some(agents) = body["agents"].as_array() {
            for agent in agents {
                entries.push(AgentDashboardEntry {
                    id: agent["id"].as_str().unwrap_or("?").to_string(),
                    name: agent["name"].as_str().unwrap_or("?").to_string(),
                    status: agent["status"].as_str().unwrap_or("unknown").to_string(),
                    cpu_percent: agent["cpu_percent"].as_f64().map(|v| v as f32),
                    memory_mb: agent["memory_mb"].as_u64(),
                    task_count: agent["current_task"].as_str().map(|_| 1).unwrap_or(0),
                    error_count: 0,
                    last_action: agent["current_task"].as_str().map(String::from),
                    last_heartbeat: agent["last_heartbeat"]
                        .as_str()
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    uptime_seconds: 0,
                });
            }
        }

        Ok(DashboardState::from_entries(entries))
    }
}

impl Default for DashboardClient {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        id: &str,
        status: &str,
        cpu: Option<f32>,
        mem: Option<u64>,
        errors: u32,
    ) -> AgentDashboardEntry {
        AgentDashboardEntry {
            id: id.to_string(),
            name: id.to_string(),
            status: status.to_string(),
            cpu_percent: cpu,
            memory_mb: mem,
            task_count: 1,
            error_count: errors,
            last_action: Some("processing".to_string()),
            last_heartbeat: Some(Utc::now()),
            uptime_seconds: 120,
        }
    }

    #[test]
    fn test_from_entries_empty() {
        let state = DashboardState::from_entries(vec![]);
        assert_eq!(state.total_agents, 0);
        assert_eq!(state.running_agents, 0);
        assert_eq!(state.total_cpu, 0.0);
        assert_eq!(state.total_memory_mb, 0);
        assert_eq!(state.total_errors, 0);
        assert!(state.last_refresh.is_some());
    }

    #[test]
    fn test_from_entries_with_data() {
        let entries = vec![
            make_entry("agent-1", "running", Some(25.0), Some(128), 0),
            make_entry("agent-2", "stopped", Some(0.0), Some(64), 2),
        ];
        let state = DashboardState::from_entries(entries);
        assert_eq!(state.total_agents, 2);
        assert_eq!(state.agents.len(), 2);
    }

    #[test]
    fn test_running_count() {
        let entries = vec![
            make_entry("a1", "running", None, None, 0),
            make_entry("a2", "running", None, None, 0),
            make_entry("a3", "stopped", None, None, 0),
        ];
        let state = DashboardState::from_entries(entries);
        assert_eq!(state.running_agents, 2);
    }

    #[test]
    fn test_cpu_aggregation() {
        let entries = vec![
            make_entry("a1", "running", Some(10.5), None, 0),
            make_entry("a2", "running", Some(20.3), None, 0),
            make_entry("a3", "running", None, None, 0), // no CPU data
        ];
        let state = DashboardState::from_entries(entries);
        assert!((state.total_cpu - 30.8).abs() < 0.01);
    }

    #[test]
    fn test_memory_aggregation() {
        let entries = vec![
            make_entry("a1", "running", None, Some(256), 0),
            make_entry("a2", "running", None, Some(512), 0),
        ];
        let state = DashboardState::from_entries(entries);
        assert_eq!(state.total_memory_mb, 768);
    }

    #[test]
    fn test_error_aggregation() {
        let entries = vec![
            make_entry("a1", "running", None, None, 3),
            make_entry("a2", "stopped", None, None, 7),
        ];
        let state = DashboardState::from_entries(entries);
        assert_eq!(state.total_errors, 10);
    }

    #[test]
    fn test_render_table_empty() {
        let state = DashboardState::from_entries(vec![]);
        let table = state.render_table();
        assert!(table.contains("0 agents"));
        assert!(table.contains("(no agents registered)"));
    }

    #[test]
    fn test_render_table_with_agents() {
        let entries = vec![make_entry(
            "agent-alpha",
            "running",
            Some(15.2),
            Some(256),
            1,
        )];
        let state = DashboardState::from_entries(entries);
        let table = state.render_table();
        assert!(table.contains("agent-alpha"));
        assert!(table.contains("running"));
        assert!(table.contains("15.2"));
        assert!(table.contains("256"));
        assert!(table.contains("1 agents"));
    }

    #[test]
    fn test_render_table_truncation() {
        let entry = AgentDashboardEntry {
            id: "a".repeat(50),
            name: "long-name".to_string(),
            status: "running".to_string(),
            cpu_percent: None,
            memory_mb: None,
            task_count: 0,
            error_count: 0,
            last_action: Some("a very long action description that exceeds the limit".to_string()),
            last_heartbeat: None,
            uptime_seconds: 0,
        };
        let state = DashboardState::from_entries(vec![entry]);
        let table = state.render_table();
        // ID truncated to 36 chars
        assert!(!table.contains(&"a".repeat(50)));
        // Action truncated to 20 chars
        assert!(table.contains("a very long action d"));
    }

    #[test]
    fn test_entry_serialization() {
        let entry = make_entry("test-id", "running", Some(50.0), Some(1024), 0);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("running"));
        let deserialized: AgentDashboardEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test-id");
        assert_eq!(deserialized.status, "running");
    }

    #[test]
    fn test_entry_defaults_none() {
        let entry = AgentDashboardEntry {
            id: "x".to_string(),
            name: "x".to_string(),
            status: "idle".to_string(),
            cpu_percent: None,
            memory_mb: None,
            task_count: 0,
            error_count: 0,
            last_action: None,
            last_heartbeat: None,
            uptime_seconds: 0,
        };
        assert!(entry.cpu_percent.is_none());
        assert!(entry.memory_mb.is_none());
        assert!(entry.last_action.is_none());
        assert!(entry.last_heartbeat.is_none());
    }

    #[test]
    fn test_dashboard_client_new() {
        let client = DashboardClient::new(Some("http://10.0.0.1:9090".to_string()));
        assert_eq!(client.endpoint, "http://10.0.0.1:9090");
    }

    #[test]
    fn test_dashboard_client_default() {
        let client = DashboardClient::default();
        assert_eq!(client.endpoint, "http://127.0.0.1:8090");
    }

    #[test]
    fn test_table_header_format() {
        let state = DashboardState::from_entries(vec![]);
        let table = state.render_table();
        assert!(table.contains("AGNOS Agent Dashboard"));
        assert!(table.contains("ID"));
        assert!(table.contains("STATUS"));
        assert!(table.contains("CPU%"));
        assert!(table.contains("MEM(MB)"));
        assert!(table.contains("TASKS"));
        assert!(table.contains("ERRS"));
        assert!(table.contains("LAST ACTION"));
    }

    #[test]
    fn test_multiple_status_types() {
        let entries = vec![
            make_entry("a1", "running", None, None, 0),
            make_entry("a2", "stopped", None, None, 0),
            make_entry("a3", "error", None, None, 0),
            make_entry("a4", "idle", None, None, 0),
        ];
        let state = DashboardState::from_entries(entries);
        assert_eq!(state.total_agents, 4);
        assert_eq!(state.running_agents, 1); // only "running" counts
        let table = state.render_table();
        assert!(table.contains("stopped"));
        assert!(table.contains("error"));
        assert!(table.contains("idle"));
    }

    #[test]
    fn test_last_refresh_set() {
        let state = DashboardState::from_entries(vec![]);
        assert!(state.last_refresh.is_some());
        let table = state.render_table();
        assert!(table.contains("Last refresh:"));
    }

    #[test]
    fn test_dashboard_state_default() {
        let state = DashboardState::default();
        assert_eq!(state.total_agents, 0);
        assert!(state.agents.is_empty());
        assert!(state.last_refresh.is_none());
    }

    #[test]
    fn test_render_table_no_cpu_mem_shows_dash() {
        let entries = vec![make_entry("agent-no-metrics", "running", None, None, 0)];
        let state = DashboardState::from_entries(entries);
        let table = state.render_table();
        // The em-dash character should appear for missing CPU/mem
        assert!(table.contains("\u{2014}"));
    }
}
