# Phase 6.7: Alpha Polish — Developer Guide

> **Status**: Complete | **ADR**: [ADR-002](../adr/adr-002-agent-runtime-and-lifecycle.md)

This guide documents the 14 features added in Phase 6.7 and how to use them.

---

## 1. Question Intent (AI Shell)

The AI Shell now routes questions to the LLM Gateway for natural language answers.

```
agnsh> what is a Landlock ruleset?
Thinking...
A Landlock ruleset is a Linux kernel security feature (since 5.13) that allows
unprivileged processes to restrict their own filesystem access...
```

**How it works**: `Intent::Question` is parsed when input starts with "what", "how", "why", etc. The query is sent to `LlmClient::answer_question()` which calls the LLM Gateway at `http://127.0.0.1:8088/v1/chat/completions`. If the gateway is unreachable, a graceful fallback message is shown.

**Configuration**: Set `llm_endpoint` in `~/.agnsh_config.toml` to point to a custom gateway.

---

## 2. Tab-Completion

`CompletionEngine` provides prefix-based completion for:
- Built-in commands (`help`, `clear`, `exit`, `mode`, `cd`, `history`)
- Intent keywords (`show`, `list`, `scan`, `audit`, `agent`, `service`, etc.)
- Network tool names (all 34 tools from the networking toolkit)
- Dynamically registered agent and service names

**Context-aware completions**: After `start`, `stop`, or `restart`, only service names are offered. After `agent`, only agent names. After `mode`, only mode names.

```rust
use ai_shell::completion::CompletionEngine;

let mut engine = CompletionEngine::new();
engine.register_agent("file-indexer".to_string());

let matches = engine.complete("sca"); // ["scan"]
let matches = engine.complete_contextual(&["start", "fi"]); // ["file-indexer"]
```

---

## 3. Pipeline Support

The shell now supports piped command chains:

```
agnsh> agent list | grep running
agnsh> scan ports on 192.168.1.0/24 then show open sockets
```

Pipes (`|`) create shell pipelines executed via `sh -c`. The NL keyword `then` chains two interpreted commands sequentially. Pipelines require `Elevated` permission level.

---

## 4. Shell Aliases

User-defined shorthand commands, persisted in config:

```toml
# ~/.agnsh_config.toml
[aliases]
scan = "network scan --profile quick"
agents = "show all agents"
logs = "show journal logs"
```

At runtime:
```
agnsh> alias scan "network scan --profile quick"
agnsh> scan 192.168.1.0/24   # expands to: network scan --profile quick 192.168.1.0/24
```

---

## 5. Agent Persistent Memory (KV Store)

Each agent gets an isolated key-value store under `/var/lib/agnos/agent-memory/<agent-id>/`.

```rust
use agent_runtime::memory_store::AgentMemoryStore;

let store = AgentMemoryStore::new();

// Store a value
store.set(agent_id, "last_scan_results", json!({"hosts": 42}), vec!["scan".into()]).await?;

// Retrieve it (survives restarts)
let entry = store.get(agent_id, "last_scan_results").await?;

// List keys by tag
let scan_keys = store.list_by_tag(agent_id, "scan").await?;

// Check storage usage
let bytes = store.usage_bytes(agent_id).await?;
```

**Safety features**:
- Path traversal prevention (keys are sanitized, `..` and `/` rejected)
- Atomic writes via temp file + rename
- 1 MB value size limit
- 256-byte key length limit
- Per-agent isolation (agents cannot access each other's memory)

---

## 6. Conversation Context Window

Sliding window of recent interactions for multi-turn agent reasoning:

```rust
use agent_runtime::learning::ConversationContext;

let mut ctx = ConversationContext::new(50); // 50 entries max

ctx.push(agent_id, ContextEntry {
    role: "user".into(),
    content: "scan the network".into(),
    timestamp: Utc::now().to_rfc3339(),
    metadata: json!({}),
});

// Get formatted context for LLM injection
let llm_context = ctx.format_for_llm(agent_id);

// Export for persistence to memory store
let entries = ctx.export(agent_id);
```

---

## 7. Structured Reasoning Traces

Chain-of-thought logging for debugging agent decisions:

```rust
use agent_runtime::tool_analysis::{TraceBuilder, format_trace};

let mut builder = TraceBuilder::new("agent-001", "scan network for vulnerabilities");

builder.add_step(
    "port_scan",
    "Start with a broad port scan to identify active services",
    Some("nmap -sV 192.168.1.0/24"),
    Some("Found 12 hosts, 47 open ports"),
    3200,
    true,
);

builder.add_step(
    "vuln_assess",
    "Run vulnerability scanner against discovered services",
    Some("nuclei -target hosts.txt"),
    Some("3 findings: 1 high, 2 medium"),
    8500,
    true,
);

let trace = builder.complete("Found 3 vulnerabilities across 12 hosts");
println!("{}", format_trace(&trace));
```

Output:
```
Trace: a1b2c3d4-... [COMPLETED]
Agent: agent-001
Input: scan network for vulnerabilities
Duration: 11700ms
────────────────────────────────────────
Step 1: port_scan [OK] (3200ms)
  Rationale: Start with a broad port scan to identify active services
  Tool: nmap -sV 192.168.1.0/24
  Output: Found 12 hosts, 47 open ports
Step 2: vuln_assess [OK] (8500ms)
  Rationale: Run vulnerability scanner against discovered services
  Tool: nuclei -target hosts.txt
  Output: 3 findings: 1 high, 2 medium
────────────────────────────────────────
Result: Found 3 vulnerabilities across 12 hosts
```

---

## 8. Agent Activity Dashboard

Live htop-style view of the agent fleet:

```
AGNOS Agent Dashboard — 5 agents (3 running) | CPU: 45.2% | Mem: 1536 MB | Errors: 0
────────────────────────────────────────────────────────────────────────────
ID                                   STATUS       CPU%   MEM(MB)  TASKS   ERRS LAST ACTION
────────────────────────────────────────────────────────────────────────────
a1b2c3d4-e5f6-...                    running      15.2     512      1      0   port scanning
b2c3d4e5-f6a7-...                    running      20.0     768      1      0   vuln assessment
c3d4e5f6-a7b8-...                    running      10.0     256      0      0   idle
d4e5f6a7-b8c9-...                    registered    —        —       0      0   —
e5f6a7b8-c9d0-...                    stopped       —        —       0      0   —
```

Fetched from the Agent Runtime API (`/v1/agents`).

---

## 9. Structured Event Log Viewer

Query audit logs with filters:

```rust
use ai_shell::audit::{AuditViewer, AuditFilter};

let viewer = AuditViewer::new(PathBuf::from("~/.agnsh_audit.log"));

let entries = viewer.query(&AuditFilter {
    agent: Some("file-indexer".into()),
    action: Some("execute".into()),
    approved: Some(true),
    limit: Some(20),
    since_seconds: Some(3600), // last hour
}).await?;

println!("{}", AuditViewer::format_table(&entries));
```

---

## 10. Agent Output Capture

Ring buffer of recent agent stdout/stderr:

```rust
use agent_runtime::supervisor::{OutputCapture, OutputStream};

let mut capture = OutputCapture::new(1000);
capture.push(OutputStream::Stdout, "Scan started...".into());
capture.push(OutputStream::Stderr, "Warning: rate limited".into());

// Get last 10 lines
let recent = capture.tail(10);

// Filter stderr only
let errors = capture.filter_stream(OutputStream::Stderr);

// Formatted display
println!("{}", capture.format_display(50));
```

---

## 11. Enriched Health Endpoint

`GET /v1/health` now returns component-level status:

```json
{
  "status": "ok",
  "service": "agnos-agent-runtime",
  "version": "2026.3.7",
  "agents_registered": 5,
  "uptime_seconds": 3600,
  "components": {
    "llm_gateway": {"status": "ok", "message": "LLM Gateway reachable"},
    "agent_registry": {"status": "ok", "message": "5 agents registered"}
  },
  "system": {
    "hostname": "agnos-dev",
    "load_average": [0.5, 0.3, 0.2],
    "memory_total_mb": 16384,
    "memory_available_mb": 8192,
    "disk_free_mb": 50000
  }
}
```

---

## 12. Agent Hot-Reload

Change agent configuration without restarting:

```rust
let changed = lifecycle_manager.reload_config(agent_id, new_config, &current_config).await?;
// Returns: ["resource_limits.max_memory", "permissions"]
```

Emits `LifecycleEvent::ConfigReloaded` with the list of changed fields. Hooks registered for this event can apply changes (e.g., update cgroup limits).

---

## 13. Declarative Fleet Config

Define desired agent fleet state in TOML:

```toml
# /etc/agnos/fleet.toml
[[services]]
name = "file-indexer"
exec_start = "/usr/lib/agnos/agents/file-indexer"
enabled = true

[[services]]
name = "log-monitor"
exec_start = "/usr/lib/agnos/agents/log-monitor"
enabled = true
after = ["file-indexer"]
```

Reconciliation:
```rust
let fleet = FleetConfig::from_file(Path::new("/etc/agnos/fleet.toml")).await?;
let plan = fleet.reconcile(&currently_running);

println!("{}", plan.summary());
// "start: log-monitor | stop: old-scanner | unchanged: file-indexer"
```

---

## 14. Environment Profiles

Named configuration presets:

| Profile | Bind Address | Log Level | Security | Auto-Approve | Agents |
|---------|-------------|-----------|----------|-------------|--------|
| dev | 127.0.0.1 | debug | relaxed | yes | 100 |
| staging | 127.0.0.1 | info | standard | no | 50 |
| prod | 127.0.0.1 | warn | strict | no | 200 |

```rust
use agnos_common::config::EnvironmentProfile;

// Load from AGNOS_PROFILE env var (defaults to "dev")
let profile = EnvironmentProfile::from_env();

if profile.is_production() {
    // Enforce strict settings
}
```

Set via environment: `AGNOS_PROFILE=prod agnos-runtime`

---

## Test Coverage

Each item includes comprehensive tests. Run all Phase 6.7 tests:

```bash
# AI Shell features
cargo test -p ai_shell -- completion aliases dashboard

# Agent Runtime features
cargo test -p agent_runtime -- memory_store reasoning_trace context_entry output_capture fleet reconcil

# Common features
cargo test -p agnos-common -- environment_profile
```

---

*Last Updated: 2026-03-07*
