# First-Party Application Standards

> **Status**: Active | **Last Updated**: 2026-03-22
>
> Standards, conventions, and workflows for all AGNOS first-party consumer applications.
> These are non-negotiable for interoperability with daimon, agnoshi, mela, and the marketplace infrastructure.
>
> **Reference implementations**: [libro](https://github.com/MacCracken/libro) (gold standard), [hisab](https://github.com/MacCracken/hisab) (benchmark practices).

---

## Scaffolding

### Required Project Structure

```
{project}/
├── VERSION                          # Single source of truth (CalVer or SemVer)
├── Cargo.toml                       # Flat crate or workspace root
├── rust-toolchain.toml              # channel = "stable", components = ["rustfmt", "clippy"]
├── Makefile                         # check/fmt/clippy/test/audit/deny/bench/coverage/build/doc/clean
├── README.md                        # Architecture, quick start, feature flags, usage examples
├── CHANGELOG.md                     # Keep a Changelog format
├── CONTRIBUTING.md                  # Contribution guidelines
├── CODE_OF_CONDUCT.md               # Code of conduct
├── SECURITY.md                      # Security policy and reporting
├── LICENSE                          # GPL-3.0 or AGPL-3.0
├── deny.toml                        # cargo-deny license + advisory config
├── codecov.yml                      # Coverage reporting config
├── scripts/
│   ├── version-bump.sh              # Updates VERSION + Cargo.toml + Cargo.lock
│   └── bench-history.sh             # Runs criterion, outputs CSV + MD (see Benchmarking)
├── docs/
│   ├── architecture/overview.md     # System diagram, module structure, consumers
│   └── development/roadmap.md       # Versioned milestones through v1.0
├── src/
│   ├── main.rs                      # CLI entrypoint (if binary)
│   ├── lib.rs                       # Library root with doc examples
│   └── {modules}.rs                 # Feature-gated domain modules
├── tests/
│   └── integration.rs               # Cross-module integration tests
├── examples/
│   └── basic.rs                     # At least one runnable example
├── benches/
│   └── benchmarks.rs                # Criterion benchmarks (required for shared crates)
├── .github/workflows/
│   ├── ci.yml                       # fmt + clippy + deny + audit + test + msrv + coverage
│   └── release.yml                  # CI gate → build → version verify → publish → release
└── .gitignore
```

### Flat vs Workspace

**Prefer flat crates** (single `Cargo.toml`, modules under `src/`, feature-gated):
- Shared library crates: abaco, hisab, yukti, dhvani, ai-hwaccel, libro, kavach, majra, etc.
- Single compilation unit — optimizer sees through everything
- Feature flags control what gets compiled

**Use workspaces** only when the project has genuinely independent binaries or crates with different dependency trees:
- Consumer apps with separate `-core`, `-ai`, `-mcp` crates: kiran, phylax, joshua
- Projects with both a library and a binary that shouldn't share all deps

### Cargo.toml Metadata (Required)

```toml
[package]
name = "{project}"
version = "0.1.0"
edition = "2024"
rust-version = "1.89"
license = "GPL-3.0"                  # or "AGPL-3.0-only"
description = "{Project} — one-line description"
homepage = "https://github.com/MacCracken/{project}"
repository = "https://github.com/MacCracken/{project}"
readme = "README.md"
documentation = "https://docs.rs/{project}"
keywords = ["keyword1", "keyword2"]  # max 5, lowercase
categories = ["category"]           # from crates.io categories list
exclude = [".claude/", ".github/", "docs/", "scripts/"]
```

### Crate Naming

- Prefer **clean single-word names**: `hisab`, `dhvani`, `tarang`, `kavach`, `yukti`, `libro`
- Hyphens only for workspace sub-crates: `{project}-core`, `{project}-ai`
- Never mix hyphens and underscores in the same workspace
- For workspace projects: minimum crates are `-core` and `-ai` (with `daimon.rs`)

### Own the Stack

When an AGNOS crate wraps an external library, **depend on the AGNOS crate, not the external one**:

| Need | Use | NOT |
|------|-----|-----|
| Vectors, matrices, transforms | `hisab` (wraps glam) | `glam` directly |
| Physics simulation | `impetus` (wraps rapier, uses hisab) | `rapier` + `glam` |
| Expression evaluation | `abaco` | Custom parser |
| DSP math | `abaco::dsp` | Inline `powf`/`log10` |
| Hardware detection | `ai-hwaccel` | Internal GPU probing |
| Media decode/encode | `tarang` | ffmpeg shelling |
| Rendering (wgpu, shaders, PBR) | `soorat` | Custom draw calls, inline shaders |
| Optics / light physics | `prakash` | Inline Fresnel, hardcoded color temp |
| Image processing | `ranga` | Manual color conversion |
| Synthesis (oscillators, filters, envelopes) | `naad` | Inline oscillators, custom DSP primitives |
| Formant / vocal synthesis | `svara` (uses naad) | Inline vocal tract, custom phoneme tables |
| Audio pipeline | `dhvani` (uses naad, svara, abaco::dsp) | Internal DSP reimplementation |
| Device abstraction | `yukti` | Direct udev/sysfs |
| LLM inference | `hoosh` (client) | Direct provider API calls |
| Queue/pubsub | `majra` | Custom channel implementations |
| Sandboxing | `kavach` | Internal sandbox backends |
| Audit logging | `libro` | Custom hash chains |
| MCP protocol | `bote` | Custom JSON-RPC |
| Threat detection | `phylax` | Inline YARA/entropy |
| MCP security | `t-ron` | Per-app authorization |
| Emotion/personality | `bhava` | Per-app mood systems |
| Statistics/probability | `pramana` | Inline stats, custom distributions |
| Ancient math systems | `sankhya` | Inline calendar math, custom number systems |
| Navigation/pathfinding | `raasta` | Custom A*, inline pathfinding |

Only one crate should directly depend on each external library. Extract when **3+ projects** implement the same pattern.

---

## Versioning

### CalVer (consumer apps)

```
YYYY.M.D[-N]
```

- `YYYY.M.D` — date of release (no zero-padding on month/day)
- `-N` — patch number within the same day (optional, starts at `-1`)

### SemVer (shared crates on crates.io)

```
0.D.M     (pre-1.0: day.month from CalVer)
M.N.P     (post-1.0: standard SemVer)
```

### VERSION File

- **Single source of truth**: `VERSION` file at project root
- Contains one line, no trailing newline
- CI reads it: `VERSION=$(cat VERSION | tr -d '[:space:]')`
- Release workflow verifies VERSION matches tag
- Git tags match exactly: `git tag $VERSION`

---

## CI/CD Workflows

### ci.yml — Every Push & PR

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  check:           # cargo fmt --check, cargo clippy -D warnings, cargo check
  security:        # cargo audit
  deny:            # EmbarkStudios/cargo-deny-action@v2
  test:            # Multi-OS: ubuntu-latest + macos-latest
                   # cargo test --all-features
                   # cargo test --doc
  msrv:            # Verify MSRV 1.89: cargo check + cargo test
  coverage:        # cargo-llvm-cov → lcov.info → codecov/codecov-action@v4
```

### release.yml — Tag Push Only

```yaml
jobs:
  ci:              # uses: ./.github/workflows/ci.yml (CI gate)
  build:           # Multi-platform matrix:
                   #   x86_64-unknown-linux-gnu (linux-amd64)
                   #   aarch64-unknown-linux-gnu (linux-arm64, cross)
                   #   aarch64-apple-darwin (macos-arm64)
  publish:         # AFTER build succeeds
                   # Verify VERSION matches tag
                   # cargo publish (CARGO_REGISTRY_TOKEN)
  release:         # AFTER publish
                   # softprops/action-gh-release@v2
                   # generate_release_notes: true
```

Reference: [libro ci.yml](https://github.com/MacCracken/libro/blob/main/.github/workflows/ci.yml), [libro release.yml](https://github.com/MacCracken/libro/blob/main/.github/workflows/release.yml).

---

## Benchmarking

### Required for Shared Crates

Every shared crate must have criterion benchmarks with CSV history tracking.

### Benchmark Script (`scripts/bench-history.sh`)

Dual output:
1. **CSV history** (appended each run) — for trend tracking and regression detection
2. **Markdown table** (overwritten) — latest run stats for quick display

The script must:
- Run `cargo bench`, capture output
- Parse criterion's `time: [low mid high]` format (handle both single-line and wrapped names)
- Normalize all units to nanoseconds
- Append to CSV with timestamp, commit, branch
- Generate markdown with human-readable units

### 3-Point Trend (hisab pattern — recommended)

For mature crates, generate a 3-point trend table: **baseline → optimized → current** with delta percentages. This catches regressions and proves optimization holds across commits.

```markdown
| Benchmark | Baseline (`abc123`) | Optimized (`def456`) | Current (`789abc`) |
|-----------|---------------------|---------------------|--------------------|
| `transform3d_apply_point` | 13.7 ns | 5.9 ns **-57%** | 5.9 ns **-57%** |
```

Reference: [hisab bench-history.sh](https://github.com/MacCracken/hisab/blob/main/scripts/bench-history.sh).

### Batch Benchmarks

Include batch/throughput benchmarks alongside single-call latency:
- `ray_sphere×100` — what a broadphase actually does
- `dsp_batch_4096` — a real audio buffer
- `parse_100_expressions` — real workload

### Makefile Target

```makefile
bench:
	./scripts/bench-history.sh
```

---

## Makefile

Standard targets (all projects must have these):

```makefile
.PHONY: check fmt clippy test audit deny bench coverage build doc clean

check: fmt clippy test audit      # Run all CI checks locally

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-features --all-targets -- -D warnings

test:
	cargo test --all-features

audit:
	cargo audit

deny:
	cargo deny check

bench:
	./scripts/bench-history.sh

coverage:
	cargo llvm-cov --all-features --html --output-dir coverage/

build:
	cargo build --release --all-features

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

clean:
	cargo clean
```

---

## MCP Integration

### Tool Naming

```
{project}_{verb}        # e.g. jalwa_play, rasa_export
{project}_{noun}        # e.g. tarang_codecs, mneme_notebook
```

- All lowercase, underscores between words
- Prefix with project name — no exceptions
- 5-8 tools per project (minimum 5)
- Every tool must have a JSON schema for inputs

### Required Tests

- `test_tool_list()` — verifies all tools appear
- One test per tool — happy path
- One test per tool — error/invalid input path

---

## Daimon Integration

### Required: AI module with `daimon.rs`

For flat crates: `src/ai.rs` with `daimon.rs` logic, feature-gated behind `ai`.
For workspaces: `crates/{project}-ai/src/daimon.rs`.

```rust
pub struct DaimonConfig {
    pub endpoint: String,       // default: http://localhost:8090
    pub api_key: Option<String>,
}

pub struct HooshConfig {
    pub endpoint: String,       // default: http://localhost:8088
}

pub struct DaimonClient { /* reqwest::Client, 30s timeout */ }
```

### Integration Tiers

| Tier | What | When |
|------|------|------|
| **1 — Lifecycle** | `register_agent()`, heartbeat (if long-running) | Always required |
| **2 — Search** | `index_vector()`, `search_vector()` | Apps with searchable content |
| **3 — Knowledge** | `ingest_rag()`, `query_rag()` | Apps with documents/text |
| **4 — Inference** | LLM calls via hoosh | Apps with AI features |

---

## Testing

### Conventions

- **Unit tests**: inline in the same file: `#[cfg(test)] mod tests { }`
- **Integration tests**: `tests/integration.rs` for cross-module behavior
- **Examples**: at least one in `examples/` — runnable with `cargo run --example`
- **Doc tests**: `cargo test --doc` must pass
- Minimum **100 tests** across all modules for a releasable project
- Target: **80%+ code coverage** (cargo-llvm-cov)
- Benchmarks: **required** for shared crates, optional for consumer apps

### DO

- Test domain logic extensively
- Test MCP tools with mock state (happy + error paths)
- Test daimon client with mock HTTP responses
- Test all error variants and Display impls
- Test serde roundtrips for all public types
- Use `#[ignore]` for tests requiring external services

### DON'T

- Use process-global state (env vars) in parallel tests — they race
- `unwrap()` or `panic!()` in library code
- Write tests that depend on network access without `#[ignore]`

---

## Error Handling

| Context | Crate | Pattern |
|---------|-------|---------|
| Library crates | `thiserror` | `#[derive(Error)]` enum per module, `#[non_exhaustive]` |
| Application / CLI | `anyhow` | `anyhow::Result`, `.context("msg")` |
| MCP tools | JSON-RPC error | `{ "code": -32000, "message": "..." }` |

---

## Logging & Audit Tracing

### Structured Logging (Required)

Every crate must use `tracing` for structured, auditable log output. This feeds into libro's audit chain and AGNOS's observability infrastructure.

**Dependency**: `tracing = "0.1"` (always, not optional).

### What to Log

| Level | When | Example |
|-------|------|---------|
| `error!` | Operation failed, caller must handle | `error!(path = %path, "file not found")` |
| `warn!` | Degraded behavior, operation succeeded with concerns | `warn!(error = %e, "chain verification failed")` |
| `info!` | Lifecycle events, state transitions, audit-worthy actions | `info!(entries = count, "store opened")` |
| `debug!` | Detailed operation internals, useful for debugging | `debug!(rule = name, "YARA rule compiled")` |
| `trace!` | Per-call hot-path tracing (high volume, perf-sensitive) | `trace!(target = %path, "scanning file")` |

### Structured Fields

Always use structured key-value pairs, not string interpolation:

```rust
// DO — structured, machine-parseable, audit-friendly
info!(agent_id = %id, action = "register", status = "success");
warn!(device = %path, fs_type = %fs, "unsupported filesystem");
error!(rule = name, target = %file, "scan failed");

// DON'T — unstructured, unparseable
info!("Agent {} registered successfully", id);
```

### Logging Init Module (Feature-Gated)

Library crates should provide an optional `logging` feature with a convenience init:

```rust
// src/logging.rs (only compiled with `logging` feature)

/// Initialise {project} logging with the `{PROJECT}_LOG` environment variable.
/// Falls back to `info` if not set. Safe to call multiple times.
pub fn init() {
    init_with_level("info");
}

pub fn init_with_level(default_level: &str) {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    let filter = EnvFilter::try_from_env("{PROJECT}_LOG")
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(true))
        .with(filter)
        .try_init();
}
```

**Env var convention**: `{PROJECT}_LOG` in SCREAMING_SNAKE_CASE. Examples: `GANIT_LOG`, `PHYLAX_LOG`, `BHAVA_LOG`.

Supports per-module filtering: `GANIT_LOG=hisab::num=debug,hisab::geo=trace`.

### Cargo.toml

```toml
[dependencies]
tracing = "0.1"                              # Always required
tracing-subscriber = { version = "0.3",      # Optional — for logging init
    features = ["env-filter", "fmt"],
    optional = true }

[features]
logging = ["dep:tracing-subscriber"]
```

### Audit-Critical Events

These events MUST be logged at `info!` level or higher — they feed the audit trail:

- Agent registration/deregistration
- Device mount/unmount/eject
- Security scan results (findings, severity)
- MCP tool calls (via t-ron)
- Personality changes, mood stimuli above threshold
- File operations (create, delete, permission change)
- Configuration changes

Reference: [hisab logging.rs](https://github.com/MacCracken/hisab/blob/main/src/logging.rs), [libro chain.rs](https://github.com/MacCracken/libro/blob/main/src/chain.rs).

---

## Naming Conventions

| Thing | Convention | Example |
|-------|-----------|---------|
| Project name | Multilingual (Arabic, Persian, Hebrew, Sanskrit, Greek, Japanese, etc.) | jalwa, tarang, mneme, hisab, yukti |
| Crate names | `{project}-{subsystem}`, hyphens (workspaces only) | `kiran-core`, `phylax-ai` |
| MCP tools | `{project}_{verb}`, underscores | `jalwa_play`, `rasa_export` |
| Agnoshi intents | Match MCP tool names | `jalwa_play` pattern |
| Binary name | Project name, lowercase | `jalwa`, `tarang` |
| Config dir | `~/.{project}/` or `~/.local/share/{project}/` | `~/.jalwa/` |
| Systemd unit | `{project}.service` | `phylax.service` |
| Desktop entry | `{project}.desktop` | `abacus.desktop` |

---

## Marketplace Recipe

### Required: `recipes/marketplace/{project}.toml`

```toml
[package]
name = "{project}"
version = "YYYY.M.D"
description = "{Project} — one-line description"
license = "GPL-3.0"
groups = ["{domain}"]

[source]
github_release = "MacCracken/{project}"
release_asset = "{project}-*-linux-amd64.tar.gz"

[depends]
runtime = ["glibc"]
build = ["rust"]

[marketplace]
category = "{category}"
runtime = "native-binary"
publisher = "AGNOS"
tags = [...]
min_agnos_version = "2026.3.22"

[marketplace.sandbox]
seccomp_mode = "basic"
network_access = true/false
data_dir = "~/.{project}/"

[build]
make = "cargo build --release"
check = "cargo test"

[security]
hardening = ["pie", "fullrelro", "fortify", "stackprotector", "bindnow"]
```

---

## Project Flow

### New Project Lifecycle

```
1. Scaffold       → Cargo.toml + VERSION + README + CHANGELOG + CONTRIBUTING +
                     CODE_OF_CONDUCT + SECURITY + LICENSE + deny.toml + codecov.yml +
                     Makefile + rust-toolchain.toml + .gitignore +
                     scripts/{version-bump,bench-history}.sh +
                     .github/workflows/{ci,release}.yml
2. Core logic     → src/lib.rs + domain modules, feature-gated
3. Tests          → inline tests + tests/integration.rs + examples/basic.rs
4. Benchmarks     → benches/benchmarks.rs + scripts/bench-history.sh
5. AI integration → src/ai.rs + daimon.rs (feature-gated)
6. MCP server     → 5+ tools, JSON-RPC on stdio (if applicable)
7. CLI            → src/main.rs, subcommands (if binary)
8. Docs           → docs/architecture/overview.md + docs/development/roadmap.md
9. First release  → VERSION, CHANGELOG, git tag, CI builds + publishes
10. AGNOS integration:
    a. Marketplace recipe    → recipes/marketplace/{project}.toml
    b. Agnoshi intents       → ai-shell/src/interpreter/patterns.rs
    c. MCP tool registration → agent-runtime MCP tool list
    d. Application doc       → docs/applications/{project}.md
    e. Bundle test           → ark-bundle.sh {recipe}
```

### P(-1): Scaffold Hardening

Before any new feature work begins, every scaffolded project must go through a hardening phase. The scaffold gets you compiling — P(-1) makes it production-grade.

```
┌──────────────────────────────────────────────────────────────┐
│                  P(-1): SCAFFOLD HARDENING                   │
│                                                              │
│  1. TEST + BENCHMARK SWEEP                                   │
│     Comprehensive test coverage of existing scaffold code    │
│     Criterion benchmarks for all hot paths                   │
│                                                              │
│  2. CLEANLINESS CHECK                                        │
│     cargo fmt --all -- --check                               │
│     cargo clippy --all-features --all-targets -- -D warnings │
│     cargo audit                                              │
│     cargo deny check                                         │
│                                                              │
│  3. GET BASELINE                                             │
│     ./scripts/bench-history.sh                               │
│     First CSV entry — this is the starting line              │
│                                                              │
│  4. INITIAL REFACTOR + AUDIT                                 │
│     Code review: performance, memory, security, edge cases   │
│     Apply standard patterns: #[inline], Cow, Vec arena,      │
│     write! over format!, #[non_exhaustive], #[must_use]      │
│                                                              │
│  5. CLEANLINESS CHECK                                        │
│     cargo fmt / clippy / audit / deny — must be clean        │
│                                                              │
│  6. ADDITIONAL TESTS + BENCHMARKS                            │
│     From audit observations: edge cases, error paths,        │
│     regression tests, new benchmark targets                  │
│                                                              │
│  7. POST-AUDIT BENCHMARKS                                    │
│     ./scripts/bench-history.sh                               │
│     Compare against step 3 — prove the wins                  │
│                                                              │
│  8. IF AUDIT HEAVY → return to step 4                        │
│     Keep drilling until clean                                │
│                                                              │
│  Exit: Crate is audit-clean, clippy-clean, fmt-clean,        │
│  security-clean, with baseline benchmarks.                   │
│  Enter the Development Loop.                                 │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Why P(-1) exists:** A scaffold is a skeleton — it compiles and has basic tests, but it hasn't been audited, optimized, or stress-tested. Building features on an unaudited foundation means every feature inherits the scaffold's shortcuts. P(-1) pays the debt before it compounds.

### Development Loop

The continuous improvement cycle for every crate. Each pass makes the crate measurably better.

```
┌──────────────────────────────────────────────────────────────┐
│                    DEVELOPMENT LOOP                          │
│                                                              │
│  1. WORK PHASE                                               │
│     New features, roadmap items, bug fixes                   │
│                                                              │
│  2. CLEANLINESS CHECK                                        │
│     cargo fmt --all -- --check                               │
│     cargo clippy --all-features --all-targets -- -D warnings │
│     cargo audit                                              │
│     cargo deny check                                         │
│                                                              │
│  3. TEST + BENCHMARK ADDITIONS                               │
│     Comprehensive coverage for new code                      │
│     New benchmarks for new hot paths                         │
│                                                              │
│  4. RUN BENCHMARKS                                           │
│     ./scripts/bench-history.sh                               │
│     Baseline captured in CSV                                 │
│                                                              │
│  5. AUDIT PHASE                                              │
│     Review: performance, optimizations, memory, security,    │
│     throughput, correctness, edge cases                      │
│                                                              │
│  6. CLEANLINESS CHECK                                        │
│     cargo fmt / clippy / audit / deny — must be clean        │
│                                                              │
│  7. TEST + BENCHMARK DEEPER ADDITIONS                        │
│     From audit observations: edge cases, error paths,        │
│     regression tests, new benchmark targets                  │
│                                                              │
│  8. RUN BENCHMARKS                                           │
│     ./scripts/bench-history.sh                               │
│     Compare against step 4 baseline — prove the wins         │
│                                                              │
│  9. IF AUDIT TOO HEAVY → return to step 5                    │
│     Keep drilling until clean                                │
│                                                              │
│ 10. DOCUMENTATION PHASE                                      │
│     Update CHANGELOG with changes                            │
│     Remove completed roadmap items                           │
│     Add/update ADRs, guides, docs as needed                  │
│                                                              │
│ 11. RETURN TO STEP 1                                         │
│     Next work phase begins                                   │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Key principles:**
- Never skip benchmarks. Numbers don't lie.
- Audit after every work phase, not just before release.
- The CSV history is the proof. 3-point trends catch regressions.
- If the audit reveals deep issues, loop steps 4-7 until clean.
- Documentation is the *last* step — document what *is*, not what *might be*.

### Release Checklist

```
[ ] All tests pass (cargo test --all-features)
[ ] No clippy warnings (cargo clippy --all-features --all-targets -- -D warnings)
[ ] No fmt issues (cargo fmt --all -- --check)
[ ] cargo audit clean
[ ] cargo deny check clean
[ ] Benchmarks run (./scripts/bench-history.sh)
[ ] VERSION file updated
[ ] CHANGELOG.md updated
[ ] Git tag matches VERSION
[ ] CI passes on tag push
[ ] Both amd64 + arm64 artifacts published
[ ] AGNOS marketplace recipe updated (after release)
```

### DON'T

- Tag before CI passes on the branch
- Release without arm64 builds
- Skip the changelog — it's the audit trail
- Amend tags — create a new `-N` patch version instead
- **NEVER** use `gh` CLI — `curl` to GitHub API only
- Skip benchmarks — if you can't measure it, you can't claim it

---

*Last Updated: 2026-03-23*
