# MCP Tools & Agnoshi Intents Reference

> **Last Updated**: 2026-03-14
> **Total**: 71 MCP tools, 65 agnoshi intents across 12 tool groups

---

## Overview

AGNOS exposes agent runtime operations as MCP (Model Context Protocol) tools
that external services can discover and call via the daimon API at port 8090.
Each tool group also has corresponding agnoshi shell intents for natural
language access via the AI shell.

**Endpoints**:
- `GET  /v1/mcp/tools` — list all tools (built-in + external)
- `POST /v1/mcp/tools/call` — call a tool by name
- `POST /v1/mcp/tools` — register an external tool
- `DELETE /v1/mcp/tools/:name` — deregister an external tool

---

## Tool Groups

### AGNOS Core (10 tools)

System-level agent runtime operations.

| Tool | Description | Required Args |
|------|-------------|---------------|
| `agnos_health` | Check agent runtime health status | — |
| `agnos_list_agents` | List all registered agents | — |
| `agnos_get_agent` | Get details for a specific agent | `agent_id` |
| `agnos_register_agent` | Register a new agent | `name` |
| `agnos_deregister_agent` | Deregister an agent | `agent_id` |
| `agnos_heartbeat` | Send agent heartbeat | `agent_id` |
| `agnos_get_metrics` | Get runtime metrics | — |
| `agnos_forward_audit` | Forward an audit event | `action`, `source` |
| `agnos_memory_get` | Get agent memory value | `agent_id`, `key` |
| `agnos_memory_set` | Set agent memory value | `agent_id`, `key`, `value` |

**Bridge**: Built-in (no external service).
**Agnoshi intents**: Covered by `AgentInfo`, `AuditView`, `ServiceControl`.

---

### Aequi — Accounting (5 tools, 5 intents)

Self-employed accounting platform. Bridge: `AequiBridge` → `http://127.0.0.1:8085`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `aequi_estimate_quarterly_tax` | Calculate estimated quarterly tax | — |
| `aequi_schedule_c_preview` | Generate Schedule C preview | — |
| `aequi_import_bank_statement` | Import OFX/QFX/CSV statement | `file_path` |
| `aequi_account_balances` | Get current account balances | — |
| `aequi_list_receipts` | List receipts with status filter | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `AequiTaxEstimate` | "aequi tax estimate Q2" |
| `AequiScheduleC` | "aequi schedule c 2026" |
| `AequiImportBank` | "aequi import bank /tmp/statement.ofx" |
| `AequiBalance` | "aequi balances" |
| `AequiReceipts` | "aequi receipts pending" |

---

### AGNOSTIC — QA Platform (5 tools, 5 intents)

AI-powered QA testing platform. Bridge: `AgnosticBridge` → `http://127.0.0.1:8000`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `agnostic_run_suite` | Run a QA test suite | `suite` |
| `agnostic_test_status` | Get test run status | `run_id` |
| `agnostic_test_report` | Get detailed test report | `run_id` |
| `agnostic_list_suites` | List available test suites | — |
| `agnostic_agent_status` | Get QA agent status | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `AgnosticRunSuite` | "agnostic run suite smoke" |
| `AgnosticTestStatus` | "agnostic test status abc-123" |
| `AgnosticTestReport` | "agnostic test report abc-123" |
| `AgnosticListSuites` | "agnostic list suites" |
| `AgnosticAgentStatus` | "agnostic agent status" |

---

### Delta — Code Hosting (5 tools, 5 intents)

Self-hosted git platform with CI/CD. Bridge: `DeltaBridge` → `http://127.0.0.1:8070`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `delta_create_repository` | Create a git repository | `name` |
| `delta_list_repositories` | List repositories | — |
| `delta_pull_request` | Manage pull requests | `action` |
| `delta_push` | Push code to Delta | — |
| `delta_ci_status` | Get CI pipeline status | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `DeltaCreateRepo` | "delta create repo my-project" |
| `DeltaListRepos` | "delta list repos" |
| `DeltaPr` | "delta pr create my-repo" |
| `DeltaPush` | "delta push main" |
| `DeltaCiStatus` | "delta ci status" |

---

### Photis Nadi — Productivity (6 tools, 5 intents)

Flutter productivity app. Bridge: `PhotisBridge` → `http://127.0.0.1:8095`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `photis_list_tasks` | List tasks with filters | — |
| `photis_create_task` | Create a new task | `title` |
| `photis_update_task` | Update existing task | `task_id` |
| `photis_get_rituals` | Get daily rituals/habits | — |
| `photis_analytics` | Get productivity analytics | — |
| `photis_sync` | Trigger Supabase sync | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `TaskList` | "photis tasks" |
| `TaskCreate` | "photis create task Review PR" |
| `TaskUpdate` | "photis update task abc done" |
| `RitualCheck` | "photis rituals today" |
| `ProductivityStats` | "photis analytics week" |

---

### Edge — Fleet Management (5 tools, 5 intents)

Edge/IoT fleet management. Bridge: Built-in (uses daimon state).

| Tool | Description | Required Args |
|------|-------------|---------------|
| `edge_list` | List edge nodes | — |
| `edge_deploy` | Deploy task to edge node | `task` |
| `edge_update` | Trigger OTA update | `node_id` |
| `edge_health` | Get node/fleet health | — |
| `edge_decommission` | Decommission a node | `node_id` |

| Agnoshi Intent | Example |
|----------------|---------|
| `EdgeListNodes` | "edge list nodes online" |
| `EdgeDeploy` | "edge deploy sensor-reader" |
| `EdgeUpdate` | "edge update node-abc" |
| `EdgeHealth` | "edge health" |
| `EdgeDecommission` | "edge decommission node-abc" |

---

### Shruti — DAW (5 tools, 5 intents)

Rust-native Digital Audio Workstation. Bridge: `ShrutiBridge` → `http://127.0.0.1:8091`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `shruti_session` | Manage DAW sessions | `action` |
| `shruti_tracks` | Manage tracks | `action` |
| `shruti_mixer` | Control mixer (gain/mute/solo) | `track` |
| `shruti_transport` | Playback control | `action` |
| `shruti_export` | Export/bounce session | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `ShrutiSession` | "shruti session create My Song" |
| `ShrutiTrack` | "shruti add track vocals type audio" |
| `ShrutiMixer` | "shruti mixer vocals gain -3" |
| `ShrutiTransport` | "shruti play" |
| `ShrutiExport` | "shruti export as flac" |

---

### Tazama — Video Editor (5 tools, 5 intents)

AI-native video editor. Bridge: `TazamaBridge` → `http://127.0.0.1:8092`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `tazama_project` | Manage video projects | `action` |
| `tazama_timeline` | Manage timeline clips | `action` |
| `tazama_effects` | Apply effects/transitions | `action` |
| `tazama_ai` | AI video features (scene detect, auto-cut, etc.) | `action` |
| `tazama_export` | Export/render video | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `TazamaProject` | "tazama project create My Video" |
| `TazamaTimeline` | "tazama add clip intro" |
| `TazamaEffects` | "tazama apply effect color_grade" |
| `TazamaAi` | "tazama scene_detect" |
| `TazamaExport` | "tazama export as mp4" |

---

### Rasa — Image Editor (5 tools, 5 intents)

AI-native image editor. Bridge: `RasaBridge` → `http://127.0.0.1:8093`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `rasa_canvas` | Manage image canvases | `action` |
| `rasa_layers` | Manage layers | `action` |
| `rasa_tools` | Apply editing tools | `action` |
| `rasa_ai` | AI image features (inpaint, upscale, etc.) | `action` |
| `rasa_export` | Export image | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `RasaCanvas` | "rasa canvas create Banner" |
| `RasaLayers` | "rasa add layer text" |
| `RasaTools` | "rasa crop" |
| `RasaAi` | "rasa remove_bg" |
| `RasaExport` | "rasa export as png" |

---

### Mneme — Knowledge Base (5 tools, 5 intents)

AI-native knowledge base with semantic search. Bridge: `MnemeBridge` → `http://127.0.0.1:8094`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `mneme_notebook` | Manage notebooks | `action` |
| `mneme_notes` | Manage notes | `action` |
| `mneme_search` | Semantic search | `query` |
| `mneme_ai` | AI features (summarize, auto-link, etc.) | `action` |
| `mneme_graph` | Knowledge graph operations | `action` |

| Agnoshi Intent | Example |
|----------------|---------|
| `MnemeNotebook` | "mneme notebook create Research" |
| `MnemeNotes` | "mneme create note Meeting Notes" |
| `MnemeSearch` | "mneme search kubernetes deployment" |
| `MnemeAi` | "mneme summarize note-abc" |
| `MnemeGraph` | "mneme graph connections node-abc" |

---

### Synapse — LLM Management (5 tools, 5 intents)

LLM management and training platform. Bridge: `SynapseBridge` → `http://127.0.0.1:8080`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `synapse_models` | Manage LLM models (download, delete, list) | `action` |
| `synapse_serve` | Start/stop model serving | `action` |
| `synapse_finetune` | Fine-tuning jobs (LoRA, QLoRA, DPO, RLHF) | `action` |
| `synapse_chat` | Run inference/completion | `model` |
| `synapse_status` | Health, GPU usage, loaded models | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `SynapseModels` | "synapse models list" |
| `SynapseServe` | "synapse serve start llama-3.1-8b" |
| `SynapseFinetune` | "synapse finetune start llama method lora" |
| `SynapseChat` | "synapse chat llama-3.1-8b hello" |
| `SynapseStatus` | "synapse status" |

---

### BullShift — Trading (5 tools, 5 intents)

High-performance trading platform. Bridge: `BullShiftBridge` → `http://127.0.0.1:8075`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `bullshift_portfolio` | View portfolio, positions, P&L | `action` |
| `bullshift_orders` | Place/cancel/list orders | `action` |
| `bullshift_market` | Market data, quotes, watchlist | `action` |
| `bullshift_alerts` | Price alerts | `action` |
| `bullshift_strategy` | Trading strategies (backtest, start, stop) | `action` |

| Agnoshi Intent | Example |
|----------------|---------|
| `BullShiftPortfolio` | "bullshift portfolio summary" |
| `BullShiftOrders` | "bullshift orders list" |
| `BullShiftMarket` | "bullshift quote AAPL" |
| `BullShiftAlerts` | "bullshift alerts list" |
| `BullShiftStrategy` | "bullshift strategy backtest momentum" |

---

### SecureYeoman — AI Agent Platform (5 tools, 5 intents)

Sovereign AI agent platform with 279 built-in MCP tools. Bridge: `YeomanBridge` → `http://127.0.0.1:18789`

| Tool | Description | Required Args |
|------|-------------|---------------|
| `yeoman_agents` | Manage AI agents (deploy, stop, list) | `action` |
| `yeoman_tasks` | Assign/manage agent tasks | `action` |
| `yeoman_tools` | Query MCP tools catalog (279 tools) | `action` |
| `yeoman_integrations` | Platform integrations (Slack, GitHub, etc.) | `action` |
| `yeoman_status` | Platform health and metrics | — |

| Agnoshi Intent | Example |
|----------------|---------|
| `YeomanAgents` | "yeoman agents list" |
| `YeomanTasks` | "yeoman tasks assign research-agent" |
| `YeomanTools` | "yeoman tools search github" |
| `YeomanIntegrations` | "yeoman integrations list" |
| `YeomanStatus` | "yeoman status" |

---

## External Tool Registration

Third-party tools can be registered at runtime via the API:

```bash
curl -s -X POST http://127.0.0.1:8090/v1/mcp/tools \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my_custom_tool",
    "description": "Does something useful",
    "callback_url": "https://my-service.example.com/mcp/callback",
    "input_schema": {
      "type": "object",
      "properties": { "input": { "type": "string" } }
    }
  }'
```

**Security**: Callback URLs are validated against SSRF policy (no private IPs,
no localhost, no credentials in URL). Names must not collide with built-in tools.

---

## Adding New Tool Groups

To add a new consumer project integration (5 MCP tools + 5 intents):

1. **MCP Handler**: `agent-runtime/src/mcp_server/handlers/{project}.rs` — Bridge struct + 5 async handler functions
2. **Handler registration**: `handlers/mod.rs` — add `pub(crate) mod {project};`
3. **Dispatch**: `mcp_server/mod.rs` — add `pub use`, `use handlers::{project}::*`, and 5 match arms
4. **Manifest**: `mcp_server/manifest.rs` — add 5 `tool!()` entries
5. **Intent enum**: `ai-shell/src/interpreter/intent.rs` — add 5 enum variants
6. **Patterns**: `interpreter/patterns.rs` — add 5 regex patterns
7. **Parse**: `interpreter/parse.rs` — add 5 pattern match blocks
8. **Translator**: `ai-shell/src/interpreter/translate/{project}.rs` — translate function
9. **Translator registration**: `translate/mod.rs` — add `mod {project};` and match arms
10. **Marketplace recipe**: `recipes/marketplace/{project}.toml`
11. **Tests**: Update manifest tool count assertion in `mcp_server/tests.rs`

See existing implementations (e.g., `shruti.rs`) as reference.

---

## Summary

| Group | Tools | Intents | Bridge Port | Status |
|-------|-------|---------|-------------|--------|
| AGNOS Core | 10 | 3* | 8090 (built-in) | Released |
| Aequi | 5 | 5 | 8085 | Released |
| AGNOSTIC | 5 | 5 | 8000 | Released |
| Delta | 5 | 5 | 8070 | Released |
| Photis Nadi | 6 | 5 | 8095 | Released |
| Edge | 5 | 5 | 8090 (built-in) | Released |
| Shruti | 5 | 5 | 8091 | Pre-release |
| Tazama | 5 | 5 | 8092 | Pre-release |
| Rasa | 5 | 5 | 8093 | Pre-release |
| Mneme | 5 | 5 | 8094 | Pre-release |
| Synapse | 5 | 5 | 8080 | Released |
| BullShift | 5 | 5 | 8075 | Released |
| SecureYeoman | 5 | 5 | 18789 | Released |
| **Total** | **71** | **63+** | | |

*Core intents are shared across `AgentInfo`, `AuditView`, `ServiceControl` rather than 1:1 mapped.

---

*For API endpoint documentation, see [docs/api/explorer.html](/docs/api/explorer.html).
For agent development, see [agent-development.md](agent-development.md).*
