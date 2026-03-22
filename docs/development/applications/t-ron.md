# T-Ron — MCP Security Monitor

> **T-Ron** (Tron reference: security program that fights the MCP) — real-time MCP tool call monitoring, audit, and threat detection for AGNOS

| Field | Value |
|-------|-------|
| Status | Planned |
| Priority | 2 — secures all MCP tool traffic across the ecosystem |
| Crate | `t-ron` (crates.io, available) |
| Repository | `MacCracken/t-ron` |
| Runtime | library crate (middleware for bote) |
| Domain | MCP security / tool call auditing / threat detection |

---

## Why First-Party

AGNOS has 151+ MCP tools across the ecosystem — every consumer app registers tools that agents can call. Today, there is no unified security layer between the agent and the tool handler. Each app does its own (or no) authorization check. A compromised or misbehaving agent can call any registered tool with any parameters.

T-Ron sits between bote (MCP protocol) and tool handlers as a security middleware. Every `tools/call` passes through t-ron before dispatch. It enforces per-agent tool permissions, detects anomalous call patterns, scans payloads for injection attempts, and logs every call to libro's cryptographic audit chain.

The name comes from Tron — the security program that fights the MCP (Master Control Program). In AGNOS, t-ron fights threats to the MCP (Model Context Protocol).

## Design Principles

1. **Middleware, not replacement** — t-ron wraps bote. It doesn't handle JSON-RPC or tool dispatch — bote does that. T-ron is the security gate in between.
2. **Default-deny for sensitive tools** — tools tagged as `admin`, `system`, or `destructive` require explicit agent permission grants.
3. **Zero-latency for permitted calls** — permission checks are in-memory lookups. No network round-trips. Target: <1µs overhead per call.
4. **Every call audited** — every tool invocation (allowed or denied) is logged to libro with agent_id, tool name, parameters (redacted for secrets), verdict, and latency.
5. **Pattern detection, not just rules** — t-ron learns normal call patterns per agent and flags anomalies, not just policy violations.

## Architecture

### Where T-Ron Sits

```
Agent
  └── bote (JSON-RPC protocol, tool registry)
        └── t-ron (security gate) ←── THIS CRATE
              ├── Permission check (per-agent tool ACLs)
              ├── Rate limiting (per-agent, per-tool)
              ├── Payload scanning (injection detection)
              ├── Pattern analysis (anomaly detection)
              ├── Audit logging (libro chain)
              │
              └── tool handler (actual tool execution)
                    ├── aegis (security tools)
                    ├── phylax (threat detection tools)
                    ├── ark (package tools)
                    └── ... (151+ tools)
```

### Integration with T.Ron (SecureYeoman Personality)

```
T.Ron (SY personality / agent)
  │
  ├── Queries t-ron for:
  │   ├── Recent security events (denied calls, anomalies)
  │   ├── Agent risk scores (who's misbehaving)
  │   ├── Tool call statistics (most called, most denied)
  │   ├── Active threat patterns (injection attempts, exfil chains)
  │   └── Policy recommendations ("agent X should lose access to Y")
  │
  └── Acts on t-ron intelligence:
      ├── Alerts humans about threats
      ├── Recommends policy changes
      ├── Provides security briefings
      └── Explains why calls were denied
```

T.Ron (the personality) becomes the human-facing security advisor. t-ron (the crate) is the engine that powers the intelligence. T.Ron asks t-ron "what happened?" and translates the answer into actionable security guidance.

### Crate Structure

```
t-ron/
├── Cargo.toml
├── src/
│   ├── lib.rs              # TRon struct, TRonConfig, middleware API
│   ├── policy.rs           # ToolPolicy — per-agent tool ACLs, sensitivity tags
│   ├── gate.rs             # SecurityGate — the middleware that wraps bote dispatch
│   ├── audit.rs            # AuditLogger — logs every call to libro
│   ├── rate.rs             # RateLimiter — per-agent, per-tool rate limits
│   ├── scanner.rs          # PayloadScanner — injection detection in tool params
│   ├── pattern.rs          # PatternAnalyzer — anomaly detection on call sequences
│   ├── score.rs            # AgentRiskScore — per-agent threat scoring
│   ├── query.rs            # QueryAPI — what T.Ron personality queries
│   └── error.rs            # TRonError
```

### Key Types

```rust
/// Top-level security monitor.
pub struct TRon {
    policy: Arc<PolicyEngine>,
    rate_limiter: Arc<RateLimiter>,
    scanner: Arc<PayloadScanner>,
    pattern: Arc<PatternAnalyzer>,
    audit: Arc<AuditLogger>,
}

/// Security gate middleware — wraps bote tool dispatch.
impl TRon {
    /// Check if a tool call is permitted. Returns verdict + logs to libro.
    pub async fn check(&self, call: &ToolCall) -> Verdict;

    /// Wrap a bote handler with security checks.
    pub fn guard<F>(&self, handler: F) -> GuardedHandler<F>;
}

/// A tool call to be checked.
pub struct ToolCall {
    pub agent_id: String,
    pub tool_name: String,
    pub params: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Security verdict.
pub enum Verdict {
    /// Call is permitted.
    Allow,
    /// Call is denied with reason.
    Deny { reason: String, code: DenyCode },
    /// Call is permitted but flagged for review.
    Flag { reason: String },
}

/// Why a call was denied.
pub enum DenyCode {
    /// Agent doesn't have permission for this tool.
    Unauthorized,
    /// Rate limit exceeded.
    RateLimited,
    /// Injection detected in parameters.
    InjectionDetected,
    /// Tool is disabled globally.
    ToolDisabled,
    /// Anomalous call pattern.
    AnomalyDetected,
}
```

### Tool Policy Model

```toml
# Per-agent tool permissions
[agent."web-agent-01"]
allow = ["tarang_*", "rasa_*", "mneme_*"]
deny = ["aegis_*", "phylax_*", "ark_*"]
rate_limit = { calls_per_minute = 60 }

[agent."admin-agent"]
allow = ["*"]
rate_limit = { calls_per_minute = 300 }

# Tool sensitivity tags
[tool."aegis_quarantine"]
sensitivity = "destructive"
require_approval = true

[tool."ark_install"]
sensitivity = "system"
require_approval = false

# Global defaults
[defaults]
unknown_agent = "deny"
unknown_tool = "deny"
max_param_size_bytes = 65536
```

### What T-Ron Detects

| Threat | Detection Method | Action |
|--------|-----------------|--------|
| Unauthorized tool access | Policy ACL lookup | Deny + audit |
| Rate limit abuse | Token bucket per agent per tool | Deny + audit + flag |
| Prompt injection in params | Pattern matching on tool parameters (SQL, shell, template injection) | Deny + audit + alert |
| Data exfiltration chain | Sequence analysis: read-sensitive → network tool within N seconds | Flag + audit + alert |
| Privilege escalation | Sequence analysis: benign tool → admin tool → system tool | Flag + audit + alert |
| Tool enumeration | Agent calling list/discover tools excessively | Flag + audit |
| Parameter stuffing | Unusually large or deeply nested JSON params | Deny + audit |
| Replay attacks | Duplicate tool call within time window | Flag + audit |

### What T.Ron (Personality) Can Query

```rust
/// Query API for the T.Ron SecureYeoman personality.
pub struct TRonQuery;

impl TRonQuery {
    /// Recent security events (denied calls, flags, anomalies).
    pub async fn recent_events(&self, limit: usize) -> Vec<SecurityEvent>;

    /// Per-agent risk score (0.0 = trusted, 1.0 = hostile).
    pub async fn agent_risk_score(&self, agent_id: &str) -> f64;

    /// Most denied tools in the last N minutes.
    pub async fn top_denied(&self, minutes: u64, limit: usize) -> Vec<(String, u64)>;

    /// Most active agents in the last N minutes.
    pub async fn top_agents(&self, minutes: u64, limit: usize) -> Vec<(String, u64)>;

    /// Active threat patterns (ongoing sequences).
    pub async fn active_threats(&self) -> Vec<ThreatPattern>;

    /// Policy recommendation based on recent behavior.
    pub async fn recommendations(&self) -> Vec<PolicyRecommendation>;

    /// Full audit trail for a specific agent.
    pub async fn agent_audit(&self, agent_id: &str, limit: usize) -> Vec<AuditEntry>;

    /// Explain why a specific call was denied.
    pub async fn explain_denial(&self, event_id: &str) -> String;
}
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `bote` | MCP protocol types (ToolCall, ToolResult) |
| `libro` | Cryptographic audit chain |
| `serde` + `serde_json` | Policy config, tool params |
| `tokio` | Async runtime |
| `chrono` | Timestamps |
| `tracing` | Structured logging |
| `thiserror` | Error types |
| `dashmap` | Concurrent rate limiter state |

## Security

- **No bypass** — t-ron is middleware in the bote dispatch path. Tools cannot be called without passing through t-ron.
- **Policy is declarative** — TOML config, versionable, auditable. No runtime policy mutations without audit logging.
- **Audit chain is tamper-proof** — every verdict logged to libro's SHA-256 hash chain.
- **Scanner is conservative** — false positives flag (allow + review), only confirmed patterns deny.

## Roadmap

### Phase 1 — Core Gate
- [ ] `SecurityGate` middleware wrapping bote dispatch
- [ ] `ToolPolicy` — per-agent ACLs from TOML config
- [ ] `AuditLogger` — every call logged to libro
- [ ] `RateLimiter` — per-agent, per-tool token bucket
- [ ] `Verdict` types (Allow, Deny, Flag)
- [ ] Basic tests: 50+

### Phase 2 — Detection
- [ ] `PayloadScanner` — SQL injection, shell injection, template injection patterns
- [ ] `PatternAnalyzer` — call sequence anomaly detection
- [ ] `AgentRiskScore` — rolling risk score per agent
- [ ] Exfiltration chain detection
- [ ] Privilege escalation chain detection

### Phase 3 — T.Ron Integration
- [ ] `TRonQuery` API for SecureYeoman T.Ron personality
- [ ] Recent events, risk scores, top denied, active threats
- [ ] Policy recommendations engine
- [ ] Denial explanation generation
- [ ] MCP tools: `tron_status`, `tron_events`, `tron_risk`, `tron_policy`, `tron_explain`
- [ ] Agnoshi intents: "tron what happened", "tron who's suspicious", "tron explain denial"

### Phase 4 — Advanced
- [ ] ML-based anomaly detection (train on normal patterns per agent)
- [ ] Cross-agent correlation (detect coordinated attacks)
- [ ] Auto-quarantine (temporarily revoke agent permissions on high risk score)
- [ ] Integration with phylax (scan tool output for threats, not just input)
- [ ] Dashboard panel in tanur/nazar

---

*Last Updated: 2026-03-22*
