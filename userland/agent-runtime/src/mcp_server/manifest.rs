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
        // ----- GPU & Model Inventory (SY integration) -----
        tool!(
            "agnos_gpu_status",
            "Probe GPU devices: VRAM, vendor, compute capability, availability",
            json!({}),
            vec![]
        ),
        tool!(
            "agnos_local_models",
            "List locally available LLM models from hoosh (Ollama, llama.cpp, etc.)",
            json!({}),
            vec![]
        ),
        tool!(
            "agnos_gpu_probe",
            "Probe GPU devices and write snapshot to /var/lib/agnosys/gpu.json; returns VRAM, vendor, compute capability",
            json!({}),
            vec![]
        ),
        tool!(
            "agnos_gpu_recommend",
            "Recommend gpu_memory_budget_mb values for a model at fp16/q8/q4/q2 quantization levels",
            json!({
                "model_name": {
                    "type": "string",
                    "description": "Model name string (e.g. \"llama3-8b\", \"mistral-7b\", \"llama-70b\"). \
                                    Parameter count is inferred from size suffix."
                },
                "model_params": {
                    "type": "number",
                    "description": "Explicit model size in billions of parameters (e.g. 7.0 for a 7B model). \
                                    Takes precedence over model_name."
                }
            }),
            vec![]
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
        // ----- Agnostic QA platform tools (18) -----
        // Task management
        tool!(
            "agnostic_submit_task",
            "Submit a QA task (routes through quality crew)",
            json!({
                "title": {"type": "string", "description": "Task title"},
                "description": {"type": "string", "description": "Task description"},
                "target_url": {"type": "string", "description": "Target URL to test"},
                "priority": {"type": "string", "description": "Priority: critical, high, medium, low"},
                "size": {"type": "string", "description": "Quality team size: lean, standard, large"},
                "agents": {"type": "array", "description": "Agent types to use"},
                "standards": {"type": "array", "description": "Compliance standards to check"}
            }),
            vec!["title", "description"]
        ),
        tool!(
            "agnostic_task_status",
            "Get QA task status by ID",
            json!({"task_id": {"type": "string", "description": "Task ID"}}),
            vec!["task_id"]
        ),
        // Security
        tool!(
            "agnostic_security_scan",
            "Run OWASP/GDPR/PCI DSS compliance scan via quality crew",
            json!({
                "target_url": {"type": "string", "description": "Target URL to scan"},
                "title": {"type": "string", "description": "Scan title"},
                "standards": {"type": "array", "description": "Standards: OWASP, GDPR, PCI_DSS, SOC2"},
                "size": {"type": "string", "description": "Team size: lean, standard, large"}
            }),
            vec!["target_url"]
        ),
        tool!(
            "agnostic_security_findings",
            "Retrieve security findings for a QA session",
            json!({"session_id": {"type": "string", "description": "Session ID"}}),
            vec!["session_id"]
        ),
        // Performance
        tool!(
            "agnostic_performance_test",
            "Run load testing and P95/P99 latency profiling",
            json!({
                "target_url": {"type": "string", "description": "Target URL to test"},
                "duration_seconds": {"type": "integer", "description": "Test duration (default: 60)"},
                "concurrency": {"type": "integer", "description": "Concurrent connections (default: 10)"},
                "size": {"type": "string", "description": "Team size: lean, standard, large"}
            }),
            vec!["target_url"]
        ),
        tool!(
            "agnostic_performance_results",
            "Retrieve performance test results for a session",
            json!({"session_id": {"type": "string", "description": "Session ID"}}),
            vec!["session_id"]
        ),
        // Results & reports
        tool!(
            "agnostic_structured_results",
            "Get typed results (security, perf, tests) for a session",
            json!({
                "session_id": {"type": "string", "description": "Session ID"},
                "result_type": {"type": "string", "description": "Type: security, performance, test_execution"}
            }),
            vec!["session_id"]
        ),
        tool!(
            "agnostic_generate_report",
            "Generate a QA report for a session",
            json!({
                "session_id": {"type": "string", "description": "Session ID"},
                "format": {"type": "string", "description": "Format: json, html, pdf"}
            }),
            vec!["session_id"]
        ),
        tool!(
            "agnostic_list_reports",
            "List available QA reports",
            json!({}),
            vec![]
        ),
        // Dashboard & metrics
        tool!(
            "agnostic_dashboard",
            "Get QA dashboard snapshot",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_list_sessions",
            "List active QA sessions",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_agent_status",
            "Get QA agent status overview",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_quality_trends",
            "Quality metrics over time: pass rates, regression frequency",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_session_diff",
            "Compare two QA sessions for regression analysis",
            json!({
                "session_a": {"type": "string", "description": "First session ID"},
                "session_b": {"type": "string", "description": "Second session ID"}
            }),
            vec!["session_a", "session_b"]
        ),
        tool!(
            "agnostic_agent_metrics",
            "Per-agent QA metrics",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_llm_usage",
            "LLM token usage and cost metrics",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_health",
            "Agnostic service health check",
            json!({}),
            vec![]
        ),
        tool!(
            "agnostic_preset_recommend",
            "Recommend a crew preset based on description",
            json!({"description": {"type": "string", "description": "What you want to accomplish"}}),
            vec!["description"]
        ),
        // A2A protocol
        tool!(
            "agnostic_a2a_delegate",
            "Delegate a task to Agnostic via A2A protocol",
            json!({
                "title": {"type": "string", "description": "Task title"},
                "description": {"type": "string", "description": "Task description"},
                "target_url": {"type": "string", "description": "Target URL"},
                "preset": {"type": "string", "description": "Crew preset name"},
                "priority": {"type": "string", "description": "Priority: critical, high, medium, low"}
            }),
            vec!["title", "description"]
        ),
        tool!(
            "agnostic_a2a_status",
            "Query task status via A2A protocol",
            json!({"task_id": {"type": "string", "description": "Task ID"}}),
            vec!["task_id"]
        ),
        tool!(
            "agnostic_a2a_heartbeat",
            "Send A2A heartbeat to Agnostic",
            json!({}),
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
        // ----- Synapse LLM management tools (7) -----
        tool!(
            "synapse_models",
            "Manage LLM models in Synapse (download, delete, list, info)",
            json!({
                "action": {"type": "string", "description": "Action: download, delete, list, info"},
                "name": {"type": "string", "description": "Model name or ID"},
                "source": {"type": "string", "description": "Model source: huggingface, ollama, local"}
            }),
            vec!["action"]
        ),
        tool!(
            "synapse_serve",
            "Start/stop/status of model serving in Synapse",
            json!({
                "action": {"type": "string", "description": "Action: start, stop, status, list"},
                "model": {"type": "string", "description": "Model name to serve"},
                "port": {"type": "string", "description": "Serving port"}
            }),
            vec!["action"]
        ),
        tool!(
            "synapse_finetune",
            "Manage fine-tuning jobs in Synapse (LoRA, QLoRA, full, DPO, RLHF). GPU allocation hints are forwarded to Synapse when starting a job.",
            json!({
                "action": {"type": "string", "description": "Action: start, status, cancel, list"},
                "model": {"type": "string", "description": "Base model name"},
                "dataset": {"type": "string", "description": "Training data path"},
                "method": {"type": "string", "description": "Fine-tuning method: lora, qlora, full, dpo, rlhf"},
                "gpu_required": {"type": "boolean", "description": "Request dedicated GPU allocation from the scheduler before starting the job"},
                "min_gpu_memory_mb": {"type": "integer", "description": "Minimum GPU VRAM required in megabytes (e.g. 16384 for 16 GB)"}
            }),
            vec!["action"]
        ),
        tool!(
            "synapse_chat",
            "Run inference/completion via Synapse",
            json!({
                "model": {"type": "string", "description": "Model name to use"},
                "prompt": {"type": "string", "description": "Text prompt"},
                "temperature": {"type": "number", "description": "Sampling temperature (0.0-2.0)"},
                "max_tokens": {"type": "string", "description": "Maximum tokens to generate"}
            }),
            vec!["model"]
        ),
        tool!(
            "synapse_status",
            "Get Synapse health, GPU usage, and loaded models",
            json!({
                "detail": {"type": "string", "description": "Detail level: brief, full"}
            }),
            vec![]
        ),
        tool!(
            "synapse_benchmark",
            "Benchmark and compare LLM models in Synapse",
            json!({
                "action": {"type": "string", "description": "Action: run, compare, list, status"},
                "models": {"type": "string", "description": "Comma-separated model names to benchmark"},
                "dataset": {"type": "string", "description": "Benchmark dataset name"},
                "metric": {"type": "string", "description": "Metric: latency, throughput, accuracy, perplexity"}
            }),
            vec!["action"]
        ),
        tool!(
            "synapse_quantize",
            "Quantize/convert LLM models (GGUF, GPTQ, AWQ)",
            json!({
                "action": {"type": "string", "description": "Action: start, status, list, cancel"},
                "model": {"type": "string", "description": "Model name to quantize"},
                "format": {"type": "string", "description": "Target format: gguf, gptq, awq, bnb"},
                "bits": {"type": "string", "description": "Quantization bits: 4, 8"}
            }),
            vec!["action"]
        ),
        // ----- BullShift trading tools (7) -----
        tool!(
            "bullshift_portfolio",
            "View BullShift portfolio, positions, and P&L",
            json!({
                "action": {"type": "string", "description": "Action: summary, positions, history, pnl"},
                "account": {"type": "string", "description": "Account ID"},
                "period": {"type": "string", "description": "Time period: 1d, 1w, 1m, 3m, 1y, all"}
            }),
            vec!["action"]
        ),
        tool!(
            "bullshift_orders",
            "Place, cancel, or list trading orders in BullShift",
            json!({
                "action": {"type": "string", "description": "Action: place, cancel, list, status"},
                "symbol": {"type": "string", "description": "Ticker symbol"},
                "side": {"type": "string", "description": "Order side: buy, sell"},
                "quantity": {"type": "string", "description": "Order quantity"},
                "order_type": {"type": "string", "description": "Order type: market, limit, stop"},
                "price": {"type": "string", "description": "Limit/stop price"},
                "order_id": {"type": "string", "description": "Order ID (for cancel/status)"}
            }),
            vec!["action"]
        ),
        tool!(
            "bullshift_market",
            "Get market data, quotes, and watchlist from BullShift",
            json!({
                "action": {"type": "string", "description": "Action: quote, search, watchlist, history"},
                "symbol": {"type": "string", "description": "Ticker symbol"},
                "query": {"type": "string", "description": "Search term"},
                "period": {"type": "string", "description": "History period: 1d, 1w, 1m"}
            }),
            vec!["action"]
        ),
        tool!(
            "bullshift_alerts",
            "Manage price alerts in BullShift",
            json!({
                "action": {"type": "string", "description": "Action: set, remove, list, triggered"},
                "symbol": {"type": "string", "description": "Ticker symbol"},
                "condition": {"type": "string", "description": "Alert condition: above, below, percent_change"},
                "value": {"type": "string", "description": "Price or percentage value"},
                "alert_id": {"type": "string", "description": "Alert ID (for remove)"}
            }),
            vec!["action"]
        ),
        tool!(
            "bullshift_strategy",
            "Manage trading strategies in BullShift (list, start, stop, backtest)",
            json!({
                "action": {"type": "string", "description": "Action: list, start, stop, backtest, status"},
                "name": {"type": "string", "description": "Strategy name"},
                "params": {"type": "string", "description": "Strategy parameters as JSON string"}
            }),
            vec!["action"]
        ),
        tool!(
            "bullshift_accounts",
            "Manage broker accounts in BullShift",
            json!({
                "action": {"type": "string", "description": "Action: list, switch, status, info"},
                "account_id": {"type": "string", "description": "Account identifier"},
                "broker": {"type": "string", "description": "Broker name"}
            }),
            vec!["action"]
        ),
        tool!(
            "bullshift_history",
            "View trade history and generate reports from BullShift",
            json!({
                "action": {"type": "string", "description": "Action: trades, dividends, tax_report, export"},
                "period": {"type": "string", "description": "Time period: 1d, 1w, 1m, 3m, 1y, all"},
                "format": {"type": "string", "description": "Export format: json, csv"}
            }),
            vec!["action"]
        ),
        // ----- SecureYeoman AI platform tools (7 base + 7 bridge = 14) -----
        tool!(
            "yeoman_agents",
            "Manage AI agents in SecureYeoman (list, deploy, stop, status)",
            json!({
                "action": {"type": "string", "description": "Action: list, deploy, stop, status, info"},
                "agent_id": {"type": "string", "description": "Agent UUID"},
                "name": {"type": "string", "description": "Agent name (for deploy)"},
                "template": {"type": "string", "description": "Agent template"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_tasks",
            "Assign and manage tasks for SecureYeoman agents",
            json!({
                "action": {"type": "string", "description": "Action: assign, list, status, cancel"},
                "agent_id": {"type": "string", "description": "Target agent UUID"},
                "description": {"type": "string", "description": "Task description"},
                "task_id": {"type": "string", "description": "Task ID (for status/cancel)"},
                "priority": {"type": "string", "description": "Priority: low, medium, high"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_tools",
            "Query SecureYeoman's MCP tools catalog (279 built-in tools)",
            json!({
                "action": {"type": "string", "description": "Action: list, search, info, categories"},
                "query": {"type": "string", "description": "Search term"},
                "category": {"type": "string", "description": "Tool category filter"},
                "name": {"type": "string", "description": "Tool name (for info)"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_integrations",
            "Manage SecureYeoman platform integrations (Slack, Discord, GitHub, etc.)",
            json!({
                "action": {"type": "string", "description": "Action: list, enable, disable, status"},
                "name": {"type": "string", "description": "Integration name"},
                "config": {"type": "string", "description": "Integration config as JSON string"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_status",
            "Get SecureYeoman platform health, metrics, and active agent count",
            json!({
                "detail": {"type": "string", "description": "Detail level: brief, full"}
            }),
            vec![]
        ),
        tool!(
            "yeoman_logs",
            "Query SecureYeoman agent logs",
            json!({
                "action": {"type": "string", "description": "Action: query, stream, tail, search"},
                "agent_id": {"type": "string", "description": "Filter by agent UUID"},
                "level": {"type": "string", "description": "Log level: debug, info, warn, error"},
                "limit": {"type": "string", "description": "Max entries to return"},
                "query": {"type": "string", "description": "Search term"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_workflows",
            "Manage SecureYeoman automation workflows",
            json!({
                "action": {"type": "string", "description": "Action: list, create, run, stop, status, delete"},
                "name": {"type": "string", "description": "Workflow name"},
                "workflow_id": {"type": "string", "description": "Workflow ID (for run/stop/status/delete)"}
            }),
            vec!["action"]
        ),
        // ----- SecureYeoman Deep Integration Bridge (7 tools) -----
        tool!(
            "yeoman_register_tools",
            "Fetch SecureYeoman MCP tool catalog and register into daimon registry",
            json!({
                "filter": {"type": "string", "description": "Optional name filter for tools"},
                "dry_run": {"type": "boolean", "description": "If true, list tools without registering"}
            }),
            vec![]
        ),
        tool!(
            "yeoman_tool_execute",
            "Execute a SecureYeoman tool by name via bridge",
            json!({
                "tool_name": {"type": "string", "description": "Name of the SY tool to execute"},
                "tool_args": {"type": "object", "description": "Tool arguments as JSON object"}
            }),
            vec!["tool_name"]
        ),
        tool!(
            "yeoman_brain_query",
            "Query SecureYeoman knowledge brain for matching entries",
            json!({
                "query": {"type": "string", "description": "Knowledge query string"},
                "limit": {"type": "string", "description": "Max results to return"},
                "category": {"type": "string", "description": "Category filter"}
            }),
            vec!["query"]
        ),
        tool!(
            "yeoman_brain_sync",
            "Bidirectional knowledge sync between SecureYeoman brain and AGNOS RAG",
            json!({
                "action": {"type": "string", "description": "Direction: to_agnos, from_agnos"},
                "category": {"type": "string", "description": "Category filter"},
                "limit": {"type": "string", "description": "Max entries to sync"},
                "query": {"type": "string", "description": "Query text (for from_agnos)"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_token_budget",
            "Query/manage SecureYeoman agent token budgets via hoosh",
            json!({
                "action": {"type": "string", "description": "Action: list, check, reserve, release"},
                "pool_name": {"type": "string", "description": "Token pool name"},
                "agent_id": {"type": "string", "description": "Agent identifier"},
                "amount": {"type": "integer", "description": "Token amount (for check/reserve)"},
                "reservation_id": {"type": "string", "description": "Reservation ID (for release)"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_events",
            "Subscribe/query SecureYeoman event stream and alerts",
            json!({
                "action": {"type": "string", "description": "Action: recent, subscribe, alerts"},
                "limit": {"type": "string", "description": "Max events to return"},
                "event_type": {"type": "string", "description": "Event type filter"}
            }),
            vec!["action"]
        ),
        tool!(
            "yeoman_swarm",
            "Query SecureYeoman swarm topology — agents, teams, relationships",
            json!({
                "action": {"type": "string", "description": "View: topology, teams, relationships"},
                "team_id": {"type": "string", "description": "Team ID filter"},
                "agent_id": {"type": "string", "description": "Agent ID filter"}
            }),
            vec!["action"]
        ),
        // ----- Delta code hosting tools (additional) -----
        tool!(
            "delta_branches",
            "Manage git branches in Delta",
            json!({
                "action": {"type": "string", "description": "Action: list, create, delete, protect, info"},
                "repo": {"type": "string", "description": "Repository name"},
                "name": {"type": "string", "description": "Branch name"},
                "from": {"type": "string", "description": "Base branch (for create)"}
            }),
            vec!["action"]
        ),
        tool!(
            "delta_review",
            "Code review operations in Delta",
            json!({
                "action": {"type": "string", "description": "Action: request, approve, reject, comment, list"},
                "pr_id": {"type": "string", "description": "Pull request ID"},
                "body": {"type": "string", "description": "Review comment body"},
                "repo": {"type": "string", "description": "Repository name"}
            }),
            vec!["action"]
        ),
        // ----- Aequi accounting tools (additional) -----
        tool!(
            "aequi_invoices",
            "Manage invoices in Aequi",
            json!({
                "action": {"type": "string", "description": "Action: create, list, send, void, status"},
                "client": {"type": "string", "description": "Client name"},
                "amount": {"type": "string", "description": "Invoice amount"},
                "invoice_id": {"type": "string", "description": "Invoice ID (for send/void/status)"},
                "due_date": {"type": "string", "description": "Due date (ISO format)"}
            }),
            vec!["action"]
        ),
        tool!(
            "aequi_reports",
            "Generate financial reports from Aequi",
            json!({
                "action": {"type": "string", "description": "Action: pnl, balance_sheet, cash_flow, summary"},
                "period": {"type": "string", "description": "Period: month, quarter, year, ytd"},
                "year": {"type": "string", "description": "Tax year"}
            }),
            vec!["action"]
        ),
        // ----- Agnostic AAS crew management tools (7) -----
        tool!(
            "agnostic_run_crew",
            "Run an agent crew from a preset or inline definitions",
            json!({
                "title": {"type": "string", "description": "Task title"},
                "description": {"type": "string", "description": "Task description"},
                "preset": {"type": "string", "description": "Preset name: qa-standard, data-engineering, devops"},
                "agent_keys": {"type": "array", "description": "Agent definition keys to assemble"},
                "agent_definitions": {"type": "array", "description": "Inline agent definitions"},
                "target_url": {"type": "string", "description": "Target application URL"},
                "priority": {"type": "string", "description": "Priority: critical, high, medium, low"},
                "gpu_required": {"type": "boolean", "description": "Whether this crew requires GPU acceleration"},
                "min_gpu_memory_mb": {"type": "integer", "description": "Minimum GPU VRAM in MB (e.g. 8192 for 8GB)"}
            }),
            vec!["title", "description"]
        ),
        tool!(
            "agnostic_crew_status",
            "Get status of a running or completed crew",
            json!({"crew_id": {"type": "string", "description": "Crew ID"}}),
            vec!["crew_id"]
        ),
        tool!(
            "agnostic_list_crews",
            "List crews with optional status filter and pagination",
            json!({
                "status": {"type": "string", "description": "Filter by status: running, completed, pending, cancelled, failed"},
                "page": {"type": "string", "description": "Page number (default: 1)"},
                "per_page": {"type": "string", "description": "Items per page (default: 20)"}
            }),
            vec![]
        ),
        tool!(
            "agnostic_cancel_crew",
            "Cancel a running or pending crew",
            json!({"crew_id": {"type": "string", "description": "Crew ID to cancel"}}),
            vec!["crew_id"]
        ),
        tool!(
            "agnostic_list_presets",
            "List available agent crew presets (QA, data-engineering, devops, etc.)",
            json!({"domain": {"type": "string", "description": "Filter by domain"}}),
            vec![]
        ),
        tool!(
            "agnostic_list_definitions",
            "List available individual agent definitions",
            json!({"domain": {"type": "string", "description": "Filter by domain"}}),
            vec![]
        ),
        tool!(
            "agnostic_create_agent",
            "Create a new agent definition on the Agnostic platform",
            json!({
                "agent_key": {"type": "string", "description": "Unique agent key (kebab-case)"},
                "name": {"type": "string", "description": "Display name"},
                "role": {"type": "string", "description": "Agent role description"},
                "goal": {"type": "string", "description": "Agent goal"},
                "backstory": {"type": "string", "description": "Agent backstory for LLM context"},
                "domain": {"type": "string", "description": "Domain: qa, data-engineering, devops, etc."},
                "tools": {"type": "array", "description": "Tool names to attach"}
            }),
            vec!["agent_key", "name", "role", "goal", "backstory"]
        ),
        tool!(
            "agnostic_crew_gpu",
            "Get GPU placement data for a crew: gpu_placement, gpu_vram, per-agent GPU assignments. \
             Focused data source for HUD GPU badges on crew cards.",
            json!({
                "crew_id": {"type": "string", "description": "Crew ID to query for GPU placement data"}
            }),
            vec!["crew_id"]
        ),
        // (agnostic tools moved to main block above)
        // ----- Shruti DAW tools (additional) -----
        tool!(
            "shruti_plugins",
            "Manage audio plugins in Shruti (VST3, CLAP, LV2)",
            json!({
                "action": {"type": "string", "description": "Action: list, load, unload, scan, info"},
                "name": {"type": "string", "description": "Plugin name"},
                "format": {"type": "string", "description": "Plugin format: vst3, clap, lv2"},
                "path": {"type": "string", "description": "Plugin path (for load)"}
            }),
            vec!["action"]
        ),
        tool!(
            "shruti_ai",
            "AI-assisted audio features in Shruti",
            json!({
                "action": {"type": "string", "description": "Action: mix_suggest, master, stem_split, denoise, transcribe, generate"},
                "track": {"type": "string", "description": "Target track name"},
                "options": {"type": "string", "description": "AI parameters as JSON string"}
            }),
            vec!["action"]
        ),
        // ----- Tazama video editor tools (additional) -----
        tool!(
            "tazama_media",
            "Manage Tazama media library",
            json!({
                "action": {"type": "string", "description": "Action: import, list, info, delete, transcode"},
                "path": {"type": "string", "description": "File path (for import)"},
                "media_id": {"type": "string", "description": "Media ID (for info/delete)"},
                "format": {"type": "string", "description": "Target transcode format"}
            }),
            vec!["action"]
        ),
        tool!(
            "tazama_subtitles",
            "Manage subtitles in Tazama (generate, edit, export)",
            json!({
                "action": {"type": "string", "description": "Action: generate, edit, export, import, list"},
                "language": {"type": "string", "description": "Subtitle language"},
                "format": {"type": "string", "description": "Format: srt, vtt, ass"},
                "path": {"type": "string", "description": "File path (for import/export)"}
            }),
            vec!["action"]
        ),
        // ----- Rasa image editor tools (additional) -----
        tool!(
            "rasa_batch",
            "Batch image operations in Rasa (resize, convert, optimize)",
            json!({
                "action": {"type": "string", "description": "Action: resize, convert, optimize, watermark, list"},
                "path": {"type": "string", "description": "Input directory or glob pattern"},
                "output": {"type": "string", "description": "Output directory"},
                "format": {"type": "string", "description": "Target format"},
                "width": {"type": "string", "description": "Target width"},
                "height": {"type": "string", "description": "Target height"}
            }),
            vec!["action"]
        ),
        tool!(
            "rasa_templates",
            "Manage design templates in Rasa",
            json!({
                "action": {"type": "string", "description": "Action: list, create, apply, delete, info"},
                "name": {"type": "string", "description": "Template name"},
                "category": {"type": "string", "description": "Category: social, print, web, banner"},
                "template_id": {"type": "string", "description": "Template ID (for apply/delete/info)"}
            }),
            vec!["action"]
        ),
        tool!(
            "rasa_adjustments",
            "Manage non-destructive adjustment layers in Rasa (brightness, contrast, curves, levels)",
            json!({
                "action": {"type": "string", "description": "Action: add, set, remove, list"},
                "type": {"type": "string", "description": "Adjustment type: brightness_contrast, hue_saturation, curves, levels"},
                "document_id": {"type": "string", "description": "Document UUID"},
                "layer_id": {"type": "string", "description": "Adjustment layer ID (for set/remove)"},
                "params": {"type": "string", "description": "Adjustment parameters as JSON string"}
            }),
            vec!["action"]
        ),
        // ----- Mneme knowledge base tools (additional) -----
        tool!(
            "mneme_import",
            "Import documents into Mneme knowledge base",
            json!({
                "action": {"type": "string", "description": "Action: file, url, clipboard, bulk, status"},
                "path": {"type": "string", "description": "File path or URL"},
                "notebook_id": {"type": "string", "description": "Target notebook"},
                "format": {"type": "string", "description": "Format: markdown, pdf, html, txt"}
            }),
            vec!["action"]
        ),
        tool!(
            "mneme_tags",
            "Manage tags in Mneme knowledge base",
            json!({
                "action": {"type": "string", "description": "Action: list, create, delete, assign, unassign, search"},
                "tag": {"type": "string", "description": "Tag name"},
                "note_id": {"type": "string", "description": "Note ID (for assign/unassign)"},
                "color": {"type": "string", "description": "Tag color"}
            }),
            vec!["action"]
        ),
        // ----- Photis Nadi tools (additional) -----
        tool!(
            "photis_boards",
            "Manage task boards in Photis Nadi",
            json!({
                "action": {"type": "string", "description": "Action: list, create, delete, rename, info"},
                "name": {"type": "string", "description": "Board name"},
                "board_id": {"type": "string", "description": "Board ID (for delete/rename/info)"}
            }),
            vec!["action"]
        ),
        tool!(
            "photis_notes",
            "Manage quick notes in Photis Nadi",
            json!({
                "action": {"type": "string", "description": "Action: create, list, get, delete, search"},
                "content": {"type": "string", "description": "Note content"},
                "task_id": {"type": "string", "description": "Attach to task ID"},
                "note_id": {"type": "string", "description": "Note ID (for get/delete)"},
                "query": {"type": "string", "description": "Search term"}
            }),
            vec!["action"]
        ),
        // ----- Edge fleet tools (additional) -----
        tool!(
            "edge_logs",
            "Query edge node logs",
            json!({
                "action": {"type": "string", "description": "Action: query, tail, search, export"},
                "node_id": {"type": "string", "description": "Filter by node ID"},
                "level": {"type": "string", "description": "Log level: debug, info, warn, error"},
                "limit": {"type": "string", "description": "Max entries"},
                "since": {"type": "string", "description": "Time window: 1h, 1d, 1w"}
            }),
            vec!["action"]
        ),
        tool!(
            "edge_config",
            "Manage edge node configuration",
            json!({
                "action": {"type": "string", "description": "Action: get, set, list, reset"},
                "node_id": {"type": "string", "description": "Target node ID"},
                "key": {"type": "string", "description": "Config key"},
                "value": {"type": "string", "description": "Config value (for set)"}
            }),
            vec!["action"]
        ),
        // ----- Tarang media framework tools (9) -----
        tool!(
            "tarang_probe",
            "Probe a media file and return format, codec, duration, and stream info",
            json!({"path": {"type": "string", "description": "Path to media file"}}),
            vec!["path"]
        ),
        tool!(
            "tarang_analyze",
            "AI-powered media content analysis — classify type, quality, suggest codecs",
            json!({"path": {"type": "string", "description": "Path to media file"}}),
            vec!["path"]
        ),
        tool!("tarang_codecs", "List all supported audio and video codecs with their backends"),
        tool!(
            "tarang_transcribe",
            "Prepare a transcription request for audio content (routes to hoosh)",
            json!({
                "path": {"type": "string", "description": "Path to media file"},
                "language": {"type": "string", "description": "Language hint (e.g. 'en', 'ja')"}
            }),
            vec!["path"]
        ),
        tool!(
            "tarang_formats",
            "Detect media container format from file header magic bytes",
            json!({"path": {"type": "string", "description": "Path to media file"}}),
            vec!["path"]
        ),
        tool!(
            "tarang_fingerprint_index",
            "Compute audio fingerprint and index in the AGNOS vector store for similarity search",
            json!({"path": {"type": "string", "description": "Path to audio file"}}),
            vec!["path"]
        ),
        tool!(
            "tarang_search_similar",
            "Find media files similar to a given file using audio fingerprint matching",
            json!({
                "path": {"type": "string", "description": "Path to reference audio file"},
                "top_k": {"type": "integer", "description": "Number of results (default: 5)"}
            }),
            vec!["path"]
        ),
        tool!(
            "tarang_describe",
            "Generate a rich AI content description using LLM analysis via hoosh",
            json!({"path": {"type": "string", "description": "Path to media file"}}),
            vec!["path"]
        ),
        tool!(
            "tarang_hw_accel",
            "Probe hardware video decode capabilities (VA-API, NVDEC) available to Tarang/Jalwa for hardware-accelerated playback and transcoding"
        ),
        // ----- Jalwa media player tools (5) -----
        tool!(
            "jalwa_play",
            "Play a media file in Jalwa",
            json!({"path": {"type": "string", "description": "Path to media file"}}),
            vec!["path"]
        ),
        tool!("jalwa_pause", "Pause or resume playback in Jalwa"),
        tool!("jalwa_status", "Get current playback status from Jalwa"),
        tool!(
            "jalwa_search",
            "Search the Jalwa media library",
            json!({"query": {"type": "string", "description": "Search query"}}),
            vec!["query"]
        ),
        tool!(
            "jalwa_recommend",
            "Get AI-powered media recommendations based on a media item",
            json!({"item_id": {"type": "string", "description": "Media item ID for recommendations"}}),
            vec!["item_id"]
        ),
        tool!(
            "jalwa_queue",
            "Manage the Jalwa play queue (list, enqueue, clear, shuffle)",
            json!({
                "action": {"type": "string", "description": "Action: list, enqueue, clear, shuffle"},
                "item_id": {"type": "string", "description": "UUID of media item (for enqueue)"}
            }),
            vec!["action"]
        ),
        tool!(
            "jalwa_library",
            "Manage the Jalwa media library (stats, scan, list)",
            json!({
                "action": {"type": "string", "description": "Action: stats, scan, list"},
                "path": {"type": "string", "description": "Directory path (for scan)"}
            }),
            vec!["action"]
        ),
        tool!(
            "jalwa_playlist",
            "Manage Jalwa playlists (list, create, add, remove, export)",
            json!({
                "action": {"type": "string", "description": "Action: list, create, add, remove, export"},
                "name": {"type": "string", "description": "Playlist name"},
                "item_id": {"type": "string", "description": "UUID of media item (for add/remove)"},
                "output": {"type": "string", "description": "Output M3U file path (for export)"}
            }),
            vec!["action"]
        ),
        // ----- Phylax threat detection tools (5) -----
        tool!(
            "phylax_scan",
            "Scan a file for threats using phylax detection engine",
            json!({
                "target": {"type": "string", "description": "File path to scan"},
                "mode": {"type": "string", "description": "Scan mode: on_demand, pre_install, pre_exec (default: on_demand)"}
            }),
            vec!["target"]
        ),
        tool!("phylax_status", "Get phylax threat scanner status and statistics"),
        tool!(
            "phylax_rules",
            "List loaded YARA-compatible detection rules",
            json!({
                "enabled_only": {"type": "boolean", "description": "Only show enabled rules (default: false)"}
            }),
            vec![]
        ),
        tool!(
            "phylax_findings",
            "Get recent threat detection findings",
            json!({
                "severity": {"type": "string", "description": "Filter by severity: critical, high, medium, low"},
                "limit": {"type": "integer", "description": "Max findings to return (default: 50)"}
            }),
            vec![]
        ),
        tool!(
            "phylax_quarantine",
            "Forward findings to aegis for quarantine action",
            json!({
                "agent_id": {"type": "string", "description": "Agent ID to check for quarantine-worthy findings"}
            }),
            vec!["agent_id"]
        ),
    ];

    McpToolManifest { tools }
}
