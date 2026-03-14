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
        // ----- Shruti DAW tools (5) -----
        tool!(
            "shruti_session",
            "Manage Shruti DAW sessions (create, open, save, close, info, list)",
            json!({
                "action": {"type": "string", "description": "Action: create, open, save, close, info, list"},
                "name": {"type": "string", "description": "Session name (for create/open)"}
            }),
            vec!["action"]
        ),
        tool!(
            "shruti_tracks",
            "Manage tracks in a Shruti session (add, remove, list, rename)",
            json!({
                "action": {"type": "string", "description": "Action: add, remove, list, rename"},
                "name": {"type": "string", "description": "Track name"},
                "kind": {"type": "string", "description": "Track type: audio, midi, synth, instrument, bus"}
            }),
            vec!["action"]
        ),
        tool!(
            "shruti_mixer",
            "Control Shruti mixer (gain, mute, solo per track)",
            json!({
                "track": {"type": "string", "description": "Track name or ID"},
                "gain": {"type": "number", "description": "Gain in dB"},
                "mute": {"type": "boolean", "description": "Mute track"},
                "solo": {"type": "boolean", "description": "Solo track"}
            }),
            vec!["track"]
        ),
        tool!(
            "shruti_transport",
            "Control Shruti playback (play, pause, stop, seek, set_tempo, status)",
            json!({
                "action": {"type": "string", "description": "Action: play, pause, stop, seek, set_tempo, status"},
                "value": {"type": "string", "description": "Value for seek (seconds) or set_tempo (BPM)"}
            }),
            vec!["action"]
        ),
        tool!(
            "shruti_export",
            "Export/bounce Shruti session to audio file",
            json!({
                "path": {"type": "string", "description": "Output file path"},
                "format": {"type": "string", "description": "Audio format: wav, flac, mp3, aac"}
            }),
            vec![]
        ),
        // ----- Tazama video editor tools (5) -----
        tool!(
            "tazama_project",
            "Manage Tazama video projects (create, open, save, close, info, list)",
            json!({
                "action": {"type": "string", "description": "Action: create, open, save, close, info, list"},
                "name": {"type": "string", "description": "Project name (for create/open)"}
            }),
            vec!["action"]
        ),
        tool!(
            "tazama_timeline",
            "Manage timeline clips (add, remove, split, trim, list, reorder)",
            json!({
                "action": {"type": "string", "description": "Action: add, remove, split, trim, list, reorder"},
                "clip_id": {"type": "string", "description": "Clip ID"},
                "position": {"type": "number", "description": "Position in seconds"},
                "duration": {"type": "number", "description": "Duration in seconds"}
            }),
            vec!["action"]
        ),
        tool!(
            "tazama_effects",
            "Apply effects and transitions to video clips",
            json!({
                "action": {"type": "string", "description": "Action: apply, remove, list, preview"},
                "effect_type": {"type": "string", "description": "Effect type: transition, color_grade, filter, text_overlay"},
                "name": {"type": "string", "description": "Effect name"},
                "clip_id": {"type": "string", "description": "Target clip ID"}
            }),
            vec!["action"]
        ),
        tool!(
            "tazama_ai",
            "AI video features (scene detection, auto-cut, subtitles, style transfer)",
            json!({
                "action": {"type": "string", "description": "Action: scene_detect, auto_cut, subtitle_gen, style_transfer, color_grade, smart_transition"},
                "options": {"type": "string", "description": "Additional options as JSON string"}
            }),
            vec!["action"]
        ),
        tool!(
            "tazama_export",
            "Export/render Tazama video project",
            json!({
                "path": {"type": "string", "description": "Output file path"},
                "format": {"type": "string", "description": "Video format: mp4, webm, mov, avi, mkv"},
                "resolution": {"type": "string", "description": "Output resolution (e.g. 1920x1080)"},
                "quality": {"type": "string", "description": "Quality: low, medium, high, lossless"}
            }),
            vec![]
        ),
        // ----- Rasa image editor tools (5) -----
        tool!(
            "rasa_canvas",
            "Manage Rasa image canvases (create, open, save, close, info, list)",
            json!({
                "action": {"type": "string", "description": "Action: create, open, save, close, info, list"},
                "name": {"type": "string", "description": "Canvas/image name"},
                "width": {"type": "integer", "description": "Canvas width in pixels"},
                "height": {"type": "integer", "description": "Canvas height in pixels"}
            }),
            vec!["action"]
        ),
        tool!(
            "rasa_layers",
            "Manage image layers (add, remove, reorder, merge, list, duplicate)",
            json!({
                "action": {"type": "string", "description": "Action: add, remove, reorder, merge, list, duplicate"},
                "layer_id": {"type": "string", "description": "Layer ID"},
                "name": {"type": "string", "description": "Layer name"},
                "kind": {"type": "string", "description": "Layer type: raster, vector, text, adjustment"}
            }),
            vec!["action"]
        ),
        tool!(
            "rasa_tools",
            "Apply image editing tools (brush, select, crop, resize, transform, fill)",
            json!({
                "action": {"type": "string", "description": "Action: brush, select, crop, resize, transform, fill"},
                "params": {"type": "string", "description": "Tool-specific parameters as JSON string"}
            }),
            vec!["action"]
        ),
        tool!(
            "rasa_ai",
            "AI image features (inpainting, upscaling, background removal, generative fill)",
            json!({
                "action": {"type": "string", "description": "Action: inpaint, upscale, remove_bg, gen_fill, style_transfer, text_to_image, smart_select"},
                "prompt": {"type": "string", "description": "Text prompt for generative features"},
                "options": {"type": "string", "description": "Additional options as JSON string"}
            }),
            vec!["action"]
        ),
        tool!(
            "rasa_export",
            "Export Rasa image to file",
            json!({
                "path": {"type": "string", "description": "Output file path"},
                "format": {"type": "string", "description": "Image format: png, jpg, webp, svg, tiff, psd"},
                "quality": {"type": "integer", "description": "Quality 1-100 (for lossy formats)"}
            }),
            vec![]
        ),
        // ----- Mneme knowledge base tools (5) -----
        tool!(
            "mneme_notebook",
            "Manage Mneme notebooks (create, open, delete, list, info)",
            json!({
                "action": {"type": "string", "description": "Action: create, open, delete, list, info"},
                "name": {"type": "string", "description": "Notebook name"}
            }),
            vec!["action"]
        ),
        tool!(
            "mneme_notes",
            "Manage notes within Mneme notebooks (create, edit, delete, list, get)",
            json!({
                "action": {"type": "string", "description": "Action: create, edit, delete, list, get"},
                "notebook_id": {"type": "string", "description": "Parent notebook ID"},
                "title": {"type": "string", "description": "Note title"},
                "content": {"type": "string", "description": "Note content (markdown)"}
            }),
            vec!["action"]
        ),
        tool!(
            "mneme_search",
            "Semantic search across Mneme knowledge base",
            json!({
                "query": {"type": "string", "description": "Search query"},
                "notebook_id": {"type": "string", "description": "Filter to specific notebook"},
                "limit": {"type": "integer", "description": "Max results to return"},
                "mode": {"type": "string", "description": "Search mode: keyword, semantic, hybrid"}
            }),
            vec!["query"]
        ),
        tool!(
            "mneme_ai",
            "AI knowledge features (summarize, extract concepts, auto-link, generate)",
            json!({
                "action": {"type": "string", "description": "Action: summarize, extract_concepts, auto_link, generate, translate"},
                "note_id": {"type": "string", "description": "Target note ID"},
                "prompt": {"type": "string", "description": "Additional prompt/instructions"}
            }),
            vec!["action"]
        ),
        tool!(
            "mneme_graph",
            "Knowledge graph operations (view, connections, suggest links, stats)",
            json!({
                "action": {"type": "string", "description": "Action: view, connections, suggest_links, stats"},
                "node_id": {"type": "string", "description": "Graph node ID"},
                "depth": {"type": "integer", "description": "Traversal depth for connections"}
            }),
            vec!["action"]
        ),
    ];

    McpToolManifest { tools }
}
