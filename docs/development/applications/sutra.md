# Sutra — Infrastructure Orchestrator

> **Sutra** (Sanskrit: सूत्र — thread, rule, formula) — AI-native infrastructure orchestration for AGNOS

| Field | Value |
|-------|-------|
| Status | Released (2026.3.18) |
| Priority | 2 — strong utility, critical for fleet + self-hosting |
| Repository | `MacCracken/sutra` |
| Runtime | native-binary |
| Domain | Infrastructure orchestration / configuration management |

---

## Why First-Party

No Rust-based orchestrator exists at Ansible's level. Existing tools (Ansible, pyinfra, Salt) are Python-based and bring a heavy runtime dependency that conflicts with AGNOS's Rust-native philosophy. More importantly, no orchestrator integrates with a local LLM gateway or has native awareness of agent fleets, package managers, and init systems.

Sutra fills the gap between daimon's agent lifecycle management and full infrastructure-as-code. It treats AGNOS subsystems (ark, argonaut, aegis, daimon) as first-class modules rather than shelling out to system commands.

## Design Principles

1. **User owns the source of truth** — TOML playbooks are the canonical format. Versionable, diffable, auditable. No magic state hidden in a database.
2. **AI assists, user approves** — Markdown and NL are input formats that generate TOML. The user reviews, edits, and commits before anything runs.
3. **Dry-run by default** — `sutra apply` shows a diff of intended changes. `sutra apply --confirm` executes. No surprises.
4. **Idempotent** — Every module is idempotent. Running a playbook twice produces the same result.
5. **Local-first** — Works on a single node with no fleet. Scales to fleet via daimon edge module.

## Playbook Formats

### TOML — Canonical (Infrastructure as Code)

```toml
# playbooks/deploy-tarang.toml

[playbook]
name = "Deploy tarang to edge fleet"
description = "Install and enable tarang media framework on all aarch64 edge nodes"

[[target]]
role = "edge"
arch = "aarch64"
# Also supports: node_id, tag, capability, all

[[task]]
module = "ark"
action = "install"
package = "tarang"
version = "2026.3.18"

[[task]]
module = "argonaut"
action = "enable"
service = "tarang"

[[task]]
module = "verify"
action = "port_listening"
port = 8070
timeout_secs = 10

[[task]]
module = "daimon"
action = "report"
status = "tarang deployed"
```

### Markdown — Human/AI Input (translates to TOML)

```markdown
# Deploy tarang to edge fleet

## Target
- role: edge
- arch: aarch64

## Tasks
- Install `tarang` version `2026.3.18` via ark
- Enable `tarang.service` via argonaut
- Verify port 8070 is listening
- Report status to daimon
```

### Natural Language — Via hoosh (translates to TOML)

```
"ensure all edge nodes are running tarang 2026.3.18"
```

### Flow

```
Markdown / NL ──→ hoosh (LLM) ──→ TOML playbook ──→ user reviews ──→ sutra apply --dry-run ──→ user confirms ──→ sutra apply --confirm
                                        │
                                        └── git commit (IaC)
```

The user always sees and approves the TOML before execution. Markdown and NL are convenience inputs, not authority.

## Modules

Core modules map 1:1 to AGNOS subsystems:

| Module | Subsystem | Actions | Notes |
|--------|-----------|---------|-------|
| `ark` | ark/nous | install, remove, upgrade, pin, list | Package state |
| `argonaut` | argonaut | enable, disable, start, stop, restart, status | Service state |
| `aegis` | aegis | enforce, audit, quarantine | Security policy |
| `file` | — | template, copy, absent, permissions, line_in_file | File state |
| `daimon` | daimon | register, deregister, report, mcp_call | Agent lifecycle |
| `edge` | edge | target, heartbeat, update, decommission | Fleet operations |
| `shell` | — | run (with audit logging) | Escape hatch |
| `user` | — | present, absent, groups | User/group state |
| `nftables` | — | rule, chain, policy | Firewall state |
| `sysctl` | — | set | Kernel parameters |
| `verify` | — | port_listening, file_exists, service_running, http_ok | Post-task assertions |

### Module Contract

Every module implements:

```rust
pub trait SutraModule: Send + Sync {
    fn name(&self) -> &str;
    fn actions(&self) -> &[&str];

    /// Return the diff between current and desired state.
    fn plan(&self, task: &Task, node: &NodeInfo) -> Result<TaskPlan>;

    /// Apply the change. Only called after user confirmation.
    fn apply(&self, task: &Task, node: &NodeInfo) -> Result<TaskResult>;

    /// Check if desired state is already met (idempotency).
    fn check(&self, task: &Task, node: &NodeInfo) -> Result<bool>;
}
```

## Transport

| Target | Transport | Notes |
|--------|-----------|-------|
| Local node | Direct function call | No IPC needed |
| AGNOS node (daimon) | Daimon HTTP API | Uses existing `/v1/edge/*` endpoints |
| Remote node (SSH) | SSH | Fallback for non-AGNOS hosts |

Priority: daimon IPC > SSH. If a node is registered in daimon's edge fleet, use the fleet API. SSH is the fallback for unmanaged or non-AGNOS nodes.

## Inventory

### Sources

1. **Static** — `inventory.toml` file listing nodes, roles, tags
2. **Dynamic** — Query daimon `/v1/edge/nodes` for live fleet
3. **Hybrid** — Static overrides merged with dynamic discovery

```toml
# inventory.toml

[[node]]
id = "rpi-kitchen"
host = "192.168.1.50"
role = "edge"
arch = "aarch64"
tags = ["iot", "home"]
transport = "daimon"  # or "ssh"

[[node]]
id = "nuc-office"
host = "192.168.1.10"
role = "desktop"
arch = "x86_64"
tags = ["workstation"]
transport = "ssh"
ssh_user = "user"
```

### Dynamic Discovery

```bash
sutra inventory --from-daimon    # Pull all registered edge nodes
sutra inventory --merge          # Merge static + dynamic
```

## CLI

```
sutra apply <playbook.toml>              # Dry-run by default
sutra apply <playbook.toml> --confirm    # Execute changes
sutra apply <playbook.toml> --limit rpi-kitchen  # Single node
sutra check <playbook.toml>              # Verify current state matches desired
sutra plan <playbook.toml>               # Show detailed execution plan
sutra translate <playbook.md>            # Markdown → TOML via hoosh
sutra translate --nl "install tarang on all edge nodes"  # NL → TOML
sutra inventory                          # List all nodes
sutra inventory --from-daimon            # Pull fleet inventory
sutra modules                            # List available modules
sutra validate <playbook.toml>           # Syntax + dependency check
sutra history                            # Show past runs (audit log)
sutra rollback <run-id>                  # Reverse a previous run
```

## Architecture

### Crates

| Crate | Role |
|-------|------|
| `sutra-core` | Playbook parser, task graph, execution engine, module trait |
| `sutra-modules` | Built-in module implementations (ark, argonaut, file, etc.) |
| `sutra-transport` | SSH client, daimon HTTP client, local executor |
| `sutra-ai` | Markdown parser, NL→TOML via hoosh, daimon integration |
| `sutra-mcp` | MCP server (5-8 tools) |

### Key Dependencies

- `tokio` — async runtime
- `serde` / `toml` — playbook parsing
- `reqwest` — daimon/hoosh HTTP
- `russh` — SSH transport (pure Rust, no libssh2)
- `comrak` — Markdown parsing
- `thiserror` / `anyhow` — error handling
- `tracing` — structured logging

## AGNOS Integration

- **Daimon**: Agent registration, fleet inventory, MCP tools, audit reporting
- **Hoosh**: NL→TOML translation, Markdown→TOML parsing, drift explanation
- **MCP Tools**: `sutra_apply`, `sutra_plan`, `sutra_check`, `sutra_inventory`, `sutra_translate`
- **Agnoshi Intents**: "deploy X to Y", "check fleet state", "translate playbook", "show inventory", "rollback last deployment"
- **Marketplace**: `recipes/marketplace/sutra.toml`, category: infrastructure

## Security

- All remote execution is audited (daimon audit chain + local log)
- SSH keys managed via aegis/sigil trust chain
- Playbooks can require approval for destructive actions (remove, shell)
- `shell` module logs full command + output to audit trail
- Dry-run is default — accidental `sutra apply` shows plan, doesn't execute

## Roadmap

### Phase 1 — Core (MVP)
- [ ] Playbook parser (TOML)
- [ ] Module trait + 4 core modules (ark, argonaut, file, verify)
- [ ] Local executor (single node)
- [ ] CLI: apply, check, plan, validate
- [ ] 50+ tests

### Phase 2 — Fleet
- [ ] SSH transport (russh)
- [ ] Daimon transport (edge API)
- [ ] Inventory (static + dynamic)
- [ ] Parallel execution across nodes
- [ ] Rollback support

### Phase 3 — AI
- [ ] Markdown→TOML translation via hoosh
- [ ] NL→TOML translation
- [ ] Drift detection + explanation
- [ ] MCP tools + agnoshi intents
- [ ] Daimon agent registration

### Phase 4 — Extended Modules
- [ ] aegis, daimon, user, nftables, sysctl, edge modules
- [ ] Custom module loading (dynamic .so or WASM)
- [ ] Playbook composition (include/import)

---

*Last Updated: 2026-03-18*
