use super::types::{McpToolDescription, McpToolManifest};

fn json_schema_object(properties: serde_json::Value, required: Vec<&str>) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// Helper macro to construct an `McpToolDescription` concisely.
macro_rules! tool {
    ($name:expr, $desc:expr, $props:expr, $req:expr) => {
        McpToolDescription {
            name: $name.to_string(),
            description: $desc.to_string(),
            input_schema: json_schema_object($props, $req),
        }
    };
    ($name:expr, $desc:expr) => {
        McpToolDescription {
            name: $name.to_string(),
            description: $desc.to_string(),
            input_schema: json_schema_object(serde_json::json!({}), vec![]),
        }
    };
}

/// Build the static MCP tool manifest for the agent runtime.
pub fn build_tool_manifest() -> McpToolManifest {
    use serde_json::json;

    let tools = vec![
        // ----- AGNOS core runtime tools (10) -----
        tool!("agnos_health", "Check agent runtime health status"),
        tool!("agnos_list_agents", "List all registered agents"),
        tool!(
            "agnos_get_agent",
            "Get details for a specific agent by ID",
            json!({"agent_id": {"type": "string", "description": "UUID of the agent"}}),
            vec!["agent_id"]
        ),
        tool!(
            "agnos_register_agent",
            "Register a new agent with the runtime",
            json!({
                "name": {"type": "string", "description": "Agent name"},
                "capabilities": {"type": "array", "items": {"type": "string"}, "description": "Agent capabilities"},
                "metadata": {"type": "object", "description": "Additional key-value metadata"}
            }),
            vec!["name"]
        ),
        tool!(
            "agnos_deregister_agent",
            "Deregister an agent by ID",
            json!({"agent_id": {"type": "string", "description": "UUID of the agent to deregister"}}),
            vec!["agent_id"]
        ),
        tool!(
            "agnos_heartbeat",
            "Send a heartbeat for an agent",
            json!({
                "agent_id": {"type": "string", "description": "UUID of the agent"},
                "status": {"type": "string", "description": "Optional status update"},
                "current_task": {"type": "string", "description": "Optional current task description"}
            }),
            vec!["agent_id"]
        ),
        tool!("agnos_get_metrics", "Get agent runtime metrics"),
        tool!(
            "agnos_forward_audit",
            "Forward an audit event to the runtime",
            json!({
                "action": {"type": "string", "description": "Audit action name"},
                "agent": {"type": "string", "description": "Optional agent name or ID"},
                "details": {"type": "object", "description": "Arbitrary event details"},
                "outcome": {"type": "string", "description": "Event outcome (e.g. success, failure)"},
                "source": {"type": "string", "description": "Source identifier for the audit event"}
            }),
            vec!["action", "source"]
        ),
        tool!(
            "agnos_memory_get",
            "Get a memory value for an agent by key",
            json!({
                "agent_id": {"type": "string", "description": "UUID of the agent"},
                "key": {"type": "string", "description": "Memory key to retrieve"}
            }),
            vec!["agent_id", "key"]
        ),
        tool!(
            "agnos_memory_set",
            "Set a memory value for an agent by key",
            json!({
                "agent_id": {"type": "string", "description": "UUID of the agent"},
                "key": {"type": "string", "description": "Memory key to set"},
                "value": {"description": "Value to store (any JSON value)"}
            }),
            vec!["agent_id", "key", "value"]
        ),
        // ----- Delta code hosting tools (5) -----
        tool!(
            "delta_create_repository",
            "Create a git repository in Delta",
            json!({
                "name": {"type": "string", "description": "Repository name"},
                "description": {"type": "string", "description": "Repository description"},
                "visibility": {"type": "string", "description": "Visibility: public or private"}
            }),
            vec!["name"]
        ),
        tool!(
            "delta_list_repositories",
            "List git repositories",
            json!({
                "owner": {"type": "string", "description": "Filter by owner"},
                "limit": {"type": "integer", "description": "Max results to return"}
            }),
            vec![]
        ),
        tool!(
            "delta_pull_request",
            "Manage pull requests (list, create, merge, close)",
            json!({
                "action": {"type": "string", "description": "Action: list, create, merge, close"},
                "repo": {"type": "string", "description": "Repository name"},
                "title": {"type": "string", "description": "PR title (for create)"},
                "source_branch": {"type": "string", "description": "Source branch (for create)"},
                "target_branch": {"type": "string", "description": "Target branch (for create, default: main)"},
                "pr_id": {"type": "string", "description": "PR ID (for merge/close)"}
            }),
            vec!["action"]
        ),
        tool!(
            "delta_push",
            "Push code to a Delta repository",
            json!({
                "repo": {"type": "string", "description": "Repository name"},
                "branch": {"type": "string", "description": "Branch to push"}
            }),
            vec![]
        ),
        tool!(
            "delta_ci_status",
            "Get CI pipeline status for a repository",
            json!({
                "repo": {"type": "string", "description": "Repository name"},
                "pipeline_id": {"type": "string", "description": "Specific pipeline ID"}
            }),
            vec![]
        ),
        // ----- Aequi accounting tools (5) -----
        tool!(
            "aequi_estimate_quarterly_tax",
            "Calculate estimated quarterly tax liability",
            json!({
                "quarter": {"type": "string", "description": "Quarter number (1-4)"},
                "year": {"type": "string", "description": "Tax year (e.g. 2026)"}
            }),
            vec![]
        ),
        tool!(
            "aequi_schedule_c_preview",
            "Generate a Schedule C (Profit or Loss) preview",
            json!({"year": {"type": "string", "description": "Tax year (e.g. 2026)"}}),
            vec![]
        ),
        tool!(
            "aequi_import_bank_statement",
            "Import a bank statement file (OFX, QFX, CSV)",
            json!({
                "file_path": {"type": "string", "description": "Path to the statement file"},
                "format": {"type": "string", "description": "File format: ofx, qfx, csv (auto-detected if omitted)"}
            }),
            vec!["file_path"]
        ),
        tool!(
            "aequi_account_balances",
            "Get current account balances",
            json!({"account_type": {"type": "string", "description": "Filter by type: asset, liability, equity, revenue, expense"}}),
            vec![]
        ),
        tool!(
            "aequi_list_receipts",
            "List receipts with optional status filter",
            json!({
                "status": {"type": "string", "description": "Filter: pending_review, reviewed, matched, all"},
                "limit": {"type": "integer", "description": "Max results to return"}
            }),
            vec![]
        ),
        // ----- Agnostic QA platform tools (5) -----
        tool!(
            "agnostic_run_suite",
            "Run a QA test suite",
            json!({
                "suite": {"type": "string", "description": "Test suite name or ID"},
                "target_url": {"type": "string", "description": "Target application URL to test"},
                "agents": {"type": "array", "description": "Agent types to use: ui, api, security, performance, accessibility, self-healing"}
            }),
            vec!["suite"]
        ),
        tool!(
            "agnostic_test_status",
            "Get status of a running or completed test run",
            json!({"run_id": {"type": "string", "description": "Test run ID"}}),
            vec!["run_id"]
        ),
        tool!(
            "agnostic_test_report",
            "Get detailed test report with findings",
            json!({
                "run_id": {"type": "string", "description": "Test run ID"},
                "format": {"type": "string", "description": "Report format: summary, full, json (default: summary)"}
            }),
            vec!["run_id"]
        ),
        tool!(
            "agnostic_list_suites",
            "List available QA test suites",
            json!({"category": {"type": "string", "description": "Filter by category: ui, api, security, performance, all"}}),
            vec![]
        ),
        tool!(
            "agnostic_agent_status",
            "Get status of QA testing agents",
            json!({"agent_type": {"type": "string", "description": "Filter by agent type: ui, api, security, performance, accessibility, self-healing"}}),
            vec![]
        ),
        // ----- Photis Nadi task management tools (6) -----
        tool!(
            "photis_list_tasks",
            "List tasks with optional filters",
            json!({
                "status": {"type": "string", "description": "Filter by status: todo, in_progress, done"},
                "board_id": {"type": "string", "description": "Filter by board ID"}
            }),
            vec![]
        ),
        tool!(
            "photis_create_task",
            "Create a new task",
            json!({
                "title": {"type": "string", "description": "Task title"},
                "description": {"type": "string", "description": "Task description"},
                "board_id": {"type": "string", "description": "Board to add task to"},
                "priority": {"type": "string", "description": "Priority: low, medium, high"}
            }),
            vec!["title"]
        ),
        tool!(
            "photis_update_task",
            "Update an existing task",
            json!({
                "task_id": {"type": "string", "description": "UUID of the task to update"},
                "title": {"type": "string", "description": "New task title"},
                "status": {"type": "string", "description": "New status: todo, in_progress, done"},
                "priority": {"type": "string", "description": "New priority: low, medium, high"}
            }),
            vec!["task_id"]
        ),
        tool!(
            "photis_get_rituals",
            "Get daily rituals/habits",
            json!({"date": {"type": "string", "description": "ISO date (e.g. 2026-03-06)"}}),
            vec![]
        ),
        tool!(
            "photis_analytics",
            "Get productivity analytics",
            json!({
                "period": {"type": "string", "description": "Period: day, week, month"},
                "metric": {"type": "string", "description": "Metric: tasks_completed, streak, velocity"}
            }),
            vec![]
        ),
        tool!(
            "photis_sync",
            "Trigger sync with Supabase backend",
            json!({"direction": {"type": "string", "description": "Sync direction: push, pull, both"}}),
            vec![]
        ),
        // ----- Edge fleet management tools (5) -----
        tool!(
            "edge_list",
            "List edge nodes in the fleet with optional status filter",
            json!({"status": {"type": "string", "description": "Filter by status: online, suspect, offline, updating, decommissioned"}}),
            vec![]
        ),
        tool!(
            "edge_deploy",
            "Deploy a task to an edge node (routes to best match if no node specified)",
            json!({
                "task": {"type": "string", "description": "Task description or binary to deploy"},
                "node_id": {"type": "string", "description": "Target node ID (optional — auto-routes if omitted)"},
                "required_tags": {"type": "array", "items": {"type": "string"}, "description": "Required capability tags"},
                "require_gpu": {"type": "boolean", "description": "Whether task requires GPU"}
            }),
            vec!["task"]
        ),
        tool!(
            "edge_update",
            "Trigger OTA update on an edge node",
            json!({
                "node_id": {"type": "string", "description": "Edge node ID to update"},
                "version": {"type": "string", "description": "Target version (default: latest)"}
            }),
            vec!["node_id"]
        ),
        tool!(
            "edge_health",
            "Get health status of an edge node or the entire fleet",
            json!({"node_id": {"type": "string", "description": "Specific node ID (omit for fleet-wide stats)"}}),
            vec![]
        ),
        tool!(
            "edge_decommission",
            "Decommission an edge node (mark for removal from fleet)",
            json!({"node_id": {"type": "string", "description": "Edge node ID to decommission"}}),
            vec!["node_id"]
        ),
    ];

    McpToolManifest { tools }
}
