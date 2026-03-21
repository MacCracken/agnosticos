# First-Party Application Standards

> **Status**: Active | **Last Updated**: 2026-03-20
>
> Standards, conventions, and workflows for all AGNOS first-party consumer applications.
> These are non-negotiable for interoperability with daimon, agnoshi, mela, and the marketplace infrastructure.

---

## Scaffolding

### Required Project Structure

```
{project}/
├── VERSION                          # Single source of truth (CalVer or SemVer)
├── Cargo.toml                       # Workspace root, resolver = "2"
├── rust-toolchain.toml              # channel = "stable", components = ["rustfmt", "clippy"]
├── Makefile                         # check/fmt/clippy/test/audit/deny/build/doc/clean
├── README.md                        # Comprehensive: architecture, quick start, API, roadmap
├── CHANGELOG.md                     # Keep a Changelog format
├── deny.toml                        # cargo-deny license + advisory config
├── scripts/
│   └── version-bump.sh              # Updates VERSION + Cargo.toml + Cargo.lock
├── docs/
│   ├── architecture/overview.md     # System diagram, module structure, consumers
│   └── development/roadmap.md       # Versioned milestones through v1.0
├── src/
│   ├── main.rs                      # CLI entrypoint (if binary)
│   ├── lib.rs                       # Library root with doc examples
│   └── mcp.rs                       # MCP server (stdio JSON-RPC 2.0)
├── crates/                          # (multi-crate projects only)
│   ├── {project}-core/src/lib.rs    # Domain logic, no IO
│   ├── {project}-ai/src/
│   │   ├── lib.rs
│   │   └── daimon.rs                # Daimon + hoosh integration
│   ├── {project}-mcp/src/lib.rs     # MCP tool definitions (if separate crate)
│   └── ...                          # Domain-specific crates
├── benches/                         # Criterion benchmarks
├── .github/workflows/
│   ├── ci.yml                       # fmt + clippy + deny + audit + test + msrv + coverage
│   └── release.yml                  # CI gate → build → publish → release
└── LICENSE
```

### Crate Naming

- Use **hyphens** in crate names: `{project}-core`, `{project}-ai`
- Never mix hyphens and underscores in the same workspace
- Minimum crates: `-core` and `-ai` (with `daimon.rs`)
- Typical count: 5-8 crates per project
- Keep crate count proportional to actual domain boundaries — don't over-split

### Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2024"
license = "GPL-3.0"          # Or MIT — match project

[workspace.dependencies]
# Centralize ALL shared deps here. Crates use { workspace = true }.
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.12", features = ["json"] }
```

---

## Shared Crate Dependencies

First-party apps should use ecosystem shared crates instead of reimplementing common functionality.
Published on crates.io under AGPL-3.0.

| Need | Use | NOT |
|------|-----|-----|
| Hardware detection | `ai-hwaccel` | Internal GPU probing |
| Media decode/encode | `tarang` | ffmpeg shelling, custom demuxers |
| Image processing | `ranga` | Manual color conversion, blend modes |
| Audio DSP/mixing | `dhvani` | Internal buffer types, custom DSP |
| LLM inference | `hoosh` (client) | Direct provider API calls |
| Queue/pubsub | `majra` | Custom channel implementations |
| Sandboxing | `kavach` | Internal sandbox backends |
| Compositing | `aethersafta` | Custom frame blending |

### When to extract a shared crate

Extract when **3+ projects** implement the same pattern. Until then, keep it in-project.
Signs it's time to extract:
- You're copying a module between repos
- Two projects have different implementations of the same algorithm
- A bug fix in one project should automatically benefit another

---

## Versioning

### CalVer (consumer apps)

Consumer applications (jalwa, tazama, shruti, etc.) that ship as AGNOS marketplace binaries use CalVer:

### CalVer Format

```
YYYY.M.D[-N]
```

- `YYYY.M.D` — date of release (no zero-padding on month/day)
- `-N` — patch number within the same day (optional, starts at `-1`)
- Examples: `2026.3.18`, `2026.3.18-1`, `2026.3.18-2`

### VERSION File

- **Single source of truth**: `VERSION` file at project root
- Contains one line, no trailing newline
- CI reads it: `VERSION=$(cat VERSION | tr -d '[:space:]')`
- Git tags match exactly: `git tag $VERSION`
- Marketplace recipes pull version from release tag, not from the recipe file

### SemVer (shared crates on crates.io)

Shared crates published to crates.io use SemVer with a `0.D.M` pre-1.0 scheme:

```
0.D.M     (pre-1.0: day.month from CalVer)
M.N.P     (post-1.0: standard SemVer)
```

- `0.21.3` = March 21st, pre-1.0
- `1.0.0` = stable API, real SemVer from here

### DO

- Update `VERSION` before tagging
- Update `CHANGELOG.md` before tagging
- Tag, then let CI build and release
- Use `-N` suffix for same-day patches

### DON'T

- Hardcode versions in `Cargo.toml` `[package]` — use `version.workspace = true` or read from VERSION
- Create tags without updating CHANGELOG
- Push tags before CI passes on the branch

---

## CI/CD Workflows

### ci.yml — Every Push & PR

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  check:
    # cargo fmt --all -- --check
    # cargo clippy --workspace --all-targets -- -D warnings
    # cargo check --workspace

  security:
    # cargo audit
    # cargo deny check (EmbarkStudios/cargo-deny-action@v2)

  test:
    # Multi-OS: ubuntu-latest + macos-latest
    # cargo test --workspace
    # cargo test --doc

  msrv:
    # Verify minimum supported Rust version (1.89)
    # cargo check + cargo test with pinned toolchain

  coverage:
    # cargo-llvm-cov → lcov.info → codecov upload
```

### release.yml — Tag Push Only

```yaml
on:
  push:
    tags: ['*']

jobs:
  ci:
    uses: ./.github/workflows/ci.yml    # CI gate — must pass first

  build:
    needs: [ci]
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu    (linux-amd64)
          - target: aarch64-unknown-linux-gnu   (linux-arm64, cross)
          - target: aarch64-apple-darwin        (macos-arm64)
          - target: x86_64-pc-windows-msvc      (windows-amd64, optional)
    steps:
      # Build, tar/zip, sha256sum
      # Artifact: {project}-{version}-{platform}.tar.gz + .sha256

  publish:                               # crates.io (shared crates only)
    needs: [ci, build]                   # IMPORTANT: publish AFTER build succeeds
    # cargo publish

  release:
    needs: [ci, build, publish]
    # softprops/action-gh-release@v2 with artifacts + SHA256
```

### DO

- Always run fmt + clippy + test before release builds
- Build both amd64 and arm64 in matrix
- Generate SHA256 checksums for every artifact
- Use `cancel-in-progress` to avoid wasted runner time
- Install system deps explicitly (libpipewire-dev, libdbus-1-dev, etc.)

### DON'T

- Skip clippy warnings with `--allow`
- Use `cargo test` without `--workspace`
- Publish releases without arm64 builds
- Use `gh` CLI in CI scripts — use `curl` to GitHub API instead

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

### MCP Server Implementation

```rust
// JSON-RPC 2.0 on stdio
// Required methods: initialize, tools/list, tools/call
// Shared state: Arc<Mutex<AppState>>
```

### Required Tests

- `test_tool_list()` — verifies all tools appear
- One test per tool — happy path
- One test per tool — error/invalid input path

### DO

- Return structured JSON from every tool
- Include `description` field in tool schema
- Keep tools focused — one action per tool
- Test MCP tools inline in the MCP module

### DON'T

- Create tools that require multi-step interaction
- Return raw strings when structured data is available
- Skip error responses — always return JSON-RPC error objects
- Exceed 10 tools without good reason — keep the surface small

---

## Daimon Integration

### Required: `crates/{project}-ai/src/daimon.rs`

Every first-party app must integrate with daimon (port 8090) and optionally hoosh (port 8088).

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

### DO

- Register with daimon on startup, deregister on shutdown
- Use reqwest with 30s timeout and optional Bearer token auth
- Gracefully degrade if daimon is unavailable (app still works standalone)

### DON'T

- Hard-fail if daimon is down — the app must function without it
- Skip agent registration — mela and agnoshi depend on it
- Talk to LLM providers directly — always route through hoosh

---

## Agnoshi Intents

### Pattern Format

Intents live in `userland/ai-shell/src/interpreter/patterns.rs` in the AGNOS repo:

```rust
r("{project}_{action}", r"(?i)^(?:{project}\s+)?{action}\s+(.+)$");
```

- Minimum 5 intents per project
- Match project MCP tools 1:1 where possible
- Case-insensitive, allow project name prefix to be optional

### DO

- Test intent patterns with representative user input
- Support both `"{project} {action} {args}"` and `"{action} {args}"` forms
- Keep patterns simple — complex NL parsing goes through hoosh

### DON'T

- Create intents without corresponding MCP tools
- Use overly greedy patterns that steal other projects' intents

---

## Marketplace Recipe

### Required: `recipes/marketplace/{project}.toml`

```toml
[package]
name = "{project}"
version = "YYYY.M.D"        # Updated by ark-bundle.sh from release tag
description = "{Project} — one-line description"
license = "GPL-3.0"
groups = ["{domain}"]

[source]
github_release = "MacCracken/{project}"
release_asset = "{project}-*-linux-amd64.tar.gz"

[depends]
runtime = ["glibc"]          # Minimal — only what's actually linked
build = ["rust"]

[marketplace]
category = "{category}"
runtime = "native-binary"    # or "flutter", "python-container"
publisher = "AGNOS"
tags = [...]
min_agnos_version = "2026.3.18"

[marketplace.sandbox]
seccomp_mode = "basic"       # or "desktop" for GUI apps
network_access = true/false
data_dir = "~/.{project}/"

[build]
make = "cargo build --release --workspace"
check = "cargo test --workspace"

[security]
hardening = ["pie", "fullrelro", "fortify", "stackprotector", "bindnow"]
```

### Sandbox Rules

- Landlock: grant `rw` to app data dir and `/tmp/{project}/`
- Landlock: grant `ro` to config dirs the app reads
- Landlock: grant `rw` to `/run/agnos/` for IPC with daimon
- Network: whitelist only `localhost` ports the app needs (8090, 8088, own port)
- GPU: add `/dev/dri` (rw) and CUDA/ROCm paths (ro) only if needed

### DO

- Keep runtime deps minimal — most Rust binaries only need `glibc` and maybe `openssl`
- Include a `.desktop` entry for GUI apps
- Include a systemd user service for daemon apps
- Set `min_agnos_version` to the version that has the APIs you need

### DON'T

- List build-only deps in runtime
- Grant broader Landlock access than needed
- Skip the `[security]` hardening section

---

## Changelog

### Format: Keep a Changelog

```markdown
# Changelog

## [YYYY.M.D[-N]] - YYYY-MM-DD

### Added
- New features

### Changed
- Modifications to existing features

### Fixed
- Bug fixes

### Security
- Security fixes with file:line references and severity

### Breaking Changes
- API or behavior changes that require user action
```

### DO

- List security fixes with severity (HIGH/MEDIUM/LOW) and file locations
- Include test count updates
- Note dependency additions/removals
- Group changes by crate when relevant

### DON'T

- Use `Unreleased` section — update changelog at release time, not continuously
- Skip security fixes — these are critical for audit trail
- Omit the date — every release entry needs `YYYY-MM-DD`

---

## Project Flow

### New Project Lifecycle

```
1. Scaffold       → Workspace + crates + VERSION + README + CHANGELOG
2. Core logic     → {project}-core crate, domain types, no IO
3. AI integration → {project}-ai crate, daimon.rs, hoosh calls
4. MCP server     → 5+ tools, JSON-RPC on stdio, full test coverage
5. CLI            → src/main.rs, subcommands, config loading
6. CI/CD          → ci.yml + release.yml, fmt/clippy/test/build
7. First release  → VERSION, CHANGELOG, git tag, CI builds artifacts
8. AGNOS integration:
   a. Marketplace recipe    → recipes/marketplace/{project}.toml
   b. Agnoshi intents       → ai-shell/src/interpreter/patterns.rs
   c. MCP tool registration → agent-runtime MCP tool list
   d. Roadmap entry         → docs/applications/{project}.md
   e. Bundle test           → ark-bundle.sh {recipe}
```

### Release Checklist

```
[ ] All tests pass (cargo test --workspace)
[ ] No clippy warnings (cargo clippy --workspace --all-targets -- -D warnings)
[ ] No fmt issues (cargo fmt --all -- --check)
[ ] cargo audit clean (no known vulnerabilities)
[ ] VERSION file updated
[ ] CHANGELOG.md updated with all changes
[ ] Git tag matches VERSION
[ ] CI passes on tag push
[ ] Both amd64 + arm64 artifacts published
[ ] SHA256 checksums published
[ ] AGNOS marketplace recipe version updated (after release)
[ ] AGNOS roadmap updated
```

### DON'T

- Tag before CI passes on the branch
- Release without arm64 builds
- Skip the changelog — it's the audit trail
- Amend tags — create a new `-N` patch version instead
- Use `gh` CLI anywhere — `curl` to GitHub API only

---

## Error Handling

| Context | Crate | Pattern |
|---------|-------|---------|
| Library crates | `thiserror` | `#[derive(Error)]` enum per module |
| Application / CLI | `anyhow` | `anyhow::Result`, `.context("msg")` |
| MCP tools | JSON-RPC error | `{ "code": -32000, "message": "..." }` |

### DO

- Use `thiserror` in `-core` and `-ai` crates
- Use `anyhow` in `main.rs` and CLI code
- Return meaningful error messages in MCP responses

### DON'T

- `unwrap()` or `panic!()` in library code
- Use `panic!()` in tests to signal expected failures — use `assert!` macros
- Swallow errors silently — log at minimum

---

## Testing

### Conventions

- Tests live **inline** in the same file: `#[cfg(test)] mod tests { }`
- Minimum **100 tests** across all crates for a releasable project
- MCP module: test every tool (happy + error path)
- Target: **80%+ code coverage** (tarpaulin)
- No separate `tests/` directory unless testing cross-crate integration
- Benchmarks (optional): `benches/{name}_bench.rs` via criterion

### DO

- Test domain logic in `-core` extensively
- Test MCP tools with mock state
- Test daimon client with mock HTTP responses
- Use `#[ignore]` for tests requiring external services, not `#[cfg(feature)]`

### DON'T

- Use process-global state (env vars) in parallel tests — they race
- Mock the database when integration tests are feasible
- Write tests that depend on network access without `#[ignore]`

---

## Naming Conventions

| Thing | Convention | Example |
|-------|-----------|---------|
| Project name | Multilingual (Arabic, Persian, Hebrew, Sanskrit, Greek, Japanese, etc.) | jalwa, tarang, mneme |
| Crate names | `{project}-{subsystem}`, hyphens | `jalwa-core`, `tarang-ai` |
| MCP tools | `{project}_{verb}`, underscores | `jalwa_play`, `rasa_export` |
| Agnoshi intents | Match MCP tool names | `jalwa_play` pattern |
| Binary name | Project name, lowercase | `jalwa`, `tarang` |
| Config dir | `~/.{project}/` or `~/.local/share/{project}/` | `~/.jalwa/` |
| Systemd unit | `{project}.service` | `irfan.service` |
| Desktop entry | `{project}.desktop` | `vidhana.desktop` |

---

*Last Updated: 2026-03-21*
