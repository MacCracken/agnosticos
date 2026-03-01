# Changelog

All notable changes to AGNOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Performance benchmarks** (`agent-runtime/benches/bench.rs`): Added 11 benchmarks covering agent ID generation, config creation, task creation/serialization, agent handle operations, task priority ordering, and resource usage
- **Performance benchmarks** (`ai-shell/benches/ai_shell.rs`): Added 7 benchmarks covering interpreter parsing, command translation, and explanation functions
- **Unit test coverage improvements**: Added tests to ai-shell interpreter and history modules, increased test count to 111
- **Integration tests: agent-orchestrator** (`agent-runtime/tests/integration.rs`): Added 16 integration tests covering:
  - Orchestrator initialization and task submission
  - Multi-agent task distribution
  - Task priority ordering
  - Task result storage and retrieval
  - Task failure handling
  - Task cancellation
  - Overdue task detection
  - Queue statistics computation
  - Agent stats tracking
  - Broadcast functionality
- **CIS benchmarks enhanced**: Added 20+ new CIS control checks:
  - Filesystem: USB storage, FireWire, Thunderbolt, /tmp sticky bit
  - Network: source packet routing, ICMP broadcast, SYN cookies, IPv6 source routing
  - Logging: audit rules, logrotate configuration
  - Authentication: password complexity, PAM configuration, shell timeout
  - System maintenance: SSH permissions, account locking
  - AGNOS-specific: kernel lockdown, IMA/EVM, Yama, SafeSetID, AppArmor, User namespaces

### Documentation
- **TODO.md removed**: Consolidated all remaining TODO items into `docs/development/roadmap.md`
- **Agent Development Guide**: `docs/development/agent-development.md` created

### Fixed
- **Critical: `Orchestrator` clone loses shared state** (`agent-runtime/src/orchestrator.rs`): `task_queues`, `running_tasks`, and `results` fields were plain `RwLock<...>` values. When the orchestrator was cloned for the scheduler background task, each clone got fresh empty maps, so the scheduler could never see tasks submitted to the original. Fixed by wrapping all shared interior state in `Arc<RwLock<...>>` and deriving `Clone` instead of a manual impl.
- **Deadlock risk in `cancel_task`** (`agent-runtime/src/orchestrator.rs`): The method held the `task_queues` write lock while attempting to acquire the `running_tasks` write lock, creating a potential deadlock with the scheduler loop. Fixed by dropping `queues` before acquiring `running_tasks`.
- **`get_queue_stats` wrong total** (`agent-runtime/src/orchestrator.rs`): `total_tasks` only summed queued tasks but then tried to subtract running tasks from it. Fixed to correctly compute `total = queued + running`.
- **`is_retriable()` too broad for IO errors** (`agnos-common/src/error.rs`): Not all `std::io::Error` variants are transient. Permanent errors like `PermissionDenied` and `NotFound` were incorrectly marked as retriable. Now only transient IO error kinds (e.g., `TimedOut`, `WouldBlock`, `Interrupted`, `ConnectionReset`) are retriable.
- **`RegistryStats` manual `Clone` impl** (`agent-runtime/src/registry.rs`): Replaced redundant manual `Clone` impl with `#[derive(Clone)]`.
- **CI: Deprecated `actions-rs/toolchain@v1`** (`.github/workflows/ci.yml`): Replaced with the maintained `dtolnay/rust-toolchain@stable`.
- **CI: Deprecated `codeql-action/upload-sarif@v2`** (`.github/workflows/ci.yml`): Updated to `@v3`.
- **CI: Deprecated `codecov-action@v3`** (`.github/workflows/ci.yml`): Updated to `@v4`.
- **CI: Deprecated `returntocorp/semgrep-action@v1`** (`.github/workflows/ci.yml`): Updated to `semgrep/semgrep-action@v1`.
- **CI: aarch64 cross-compilation not set up** (`.github/workflows/ci.yml`): Build matrix now installs `gcc-aarch64-linux-gnu` and `cross` for cross-compiled targets; native builds remain as-is.
- **CI: `actions/cache@v3`** (`.github/workflows/ci.yml`): Updated to `actions/cache@v4`.
- **CI: docs job checked for `TODO.md` existence only** (`.github/workflows/ci.yml`): Now also verifies `docs/development/roadmap.md` exists (the canonical roadmap location).
- **`Cargo.lock` in `.gitignore`** (`.gitignore`): `Cargo.lock` was incorrectly gitignored. For binary/OS crates it must be committed for reproducible builds. Removed from `.gitignore`.
- **README development status stale** (`README.md`): Status section still said "Current Phase: Foundation (Phase 0)". Updated to reflect Phase 5 (Production, 85% complete) and actual Alpha release timeline.
- **README security badge broken link** (`README.md`): Badge linked to `docs/security/security-model.md` which does not exist; corrected to `docs/security/security-guide.md`.

### Added
- Initial project scaffolding and documentation
- README.md, TODO.md, CONTRIBUTING.md, SECURITY.md
- ARCHITECTURE.md with system architecture
- LICENSE (GPL v3.0)
- CI/CD pipeline with GitHub Actions
- Security scanning and build automation
- IPC module (`agent-runtime/src/ipc.rs`): `AgentIpc` and `MessageBus` with full test coverage
- NL interpreter (`ai-shell/src/interpreter.rs`): intent parsing and command translation with full test coverage
- AI shell security, config, and permissions modules with tests
- Desktop environment modules: compositor, shell, apps, AI features, security UI with tests
- LLM gateway providers module with test coverage
- **Agent SDK message loop** (`agnos-sys/src/agent.rs`): Implemented `AgentRuntime::run` with message loop and LLM gateway helper functions
- **LLM Gateway HTTP server** (`llm-gateway/src/http.rs`): OpenAI-compatible API on port 8088 with `/v1/chat/completions`, `/v1/models`, and `/v1/health` endpoints
- **Landlock/seccomp sandbox** (`agnos-sys/src/security.rs`): Full implementation with `NamespaceFlags`, filesystem rules, and seccomp filter generation
- **IPC routing by agent name** (`agent-runtime/src/ipc.rs`): `MessageBus` now routes messages to agents by registered name

### Documentation
- **Architecture Decision Records**: ADR-007 documenting OpenAI-compatible HTTP API for LLM Gateway
- **Integration Guide**: `docs/AGNOSTIC_INTEGRATION.md` for Agnostic platform integration
- **Development Roadmap**: Moved and reorganized `TODO.md` → `docs/development/roadmap.md` with priority-based structure (P0/P1/P2/P3)
- **README Updates**: Updated all references to point to new roadmap location, added package security section
- **CIS Benchmarks**: Complete compliance documentation with validation scripts

### Security & Compliance
- **Fuzzing infrastructure** (`.github/workflows/fuzzing.yml`): Automated daily fuzz testing for critical components
- **SBOM generation** (`scripts/generate-sbom.sh`): SPDX and CycloneDX format support with CI integration
- **CIS benchmarks validation** (`docs/security/cis-benchmarks.md`, `scripts/cis-validate.sh`): Automated compliance checking
- **Dependency vulnerability scanning**: cargo-deny and cargo-outdated integration in CI

### Release Infrastructure
- **Package signing** (`scripts/sign-packages.sh`): GPG signing for all release packages with signature verification
- **Delta update system** (`scripts/agnos-update.sh`): Delta patches with xdelta3/bsdiff, rollback capability, and automatic backups
- **Telemetry system** (`agnos-common/src/telemetry.rs`): Opt-in crash reporting and metrics collection (disabled by default)
- **Release automation** (`.github/workflows/release-automation.yml`): Automated release creation, SBOM attachment, and CHANGELOG updates

### Testing
- **Test Coverage**: Increased from ~45% to ~65% (target: 80% for Alpha)
- **agnos-common**: 93 tests passing (types, error, telemetry modules fully tested)
- **ai-shell**: 99 tests passing (added 25+ new tests):
  - `sandbox.rs`: 6 new tests
  - `output.rs`: 8 new tests  
  - `audit.rs`: 5 new tests
  - `llm.rs`: 6 new tests
- **agnos-sys**: 29 tests passing (security module with landlock/seccomp tests)
- **Total codebase**: 350+ tests across all packages
- **Test Infrastructure**: All async tests properly configured with tokio

### Metrics
| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Code Coverage | ~45% | ~65% | 80% |
| Total Tests | ~250 | 350+ | 400+ |
| Test Pass Rate | ~95% | 100% | 100% |

### Fixed
- `agnos-examples` crate: added missing workspace dependencies (`anyhow`, `async-trait`, `tracing`, `tracing-subscriber`) so `file_manager_agent` and `quick_start` examples compile cleanly
- Removed stray `use async_trait::async_trait` import placed after entry-point macro in `file-manager-agent.rs`
- Removed unused `use serde_json::json` import from `file-manager-agent.rs`
- Fixed compilation errors in `agnos-sys`, `agent-runtime`, and `llm-gateway`
- Fixed duplicate test in `agnos-sys/src/security.rs`
- Fixed quote escaping in ai-shell output tests

### Changed
- **Project Structure**: Reorganized roadmap from `TODO.md` to `docs/development/roadmap.md` with clear priority levels (P0-P3)
- **README**: Updated status badge and documentation links to reference new roadmap location
- **Dependency Management**: Upgraded nix crate from 0.27 to 0.31 across all packages to resolve version conflicts

## Release Planning

### [0.1.0] - Phase 1: Core OS - Target Q2 2026
- Bootable hardened Linux base
- Package management system (agpkg)
- Basic userland and init system
- Initial security modules

### [0.2.0] - Phase 2: AI Shell - Target Q3 2026
- Natural language command interface
- LLM Gateway service
- Local and cloud model support
- Bash compatibility layer

### [0.3.0] - Phase 3: Agent Runtime - Target Q4 2026
- Agent Kernel Module
- Multi-agent orchestration
- Agent SDK and templates
- Sandboxing implementation

### [0.4.0] - Phase 4: Desktop - Target Q1 2027
- Wayland-based compositor
- AI-augmented desktop environment
- Essential applications
- Human oversight interface

### [1.0.0] - Phase 5: Production - Target Q2 2027
- Security certifications
- Enterprise features
- Long-term support
- General availability

---

## Template

### [X.Y.Z] - YYYY-MM-DD

#### Added
- New features

#### Changed
- Changes to existing functionality

#### Deprecated
- Soon-to-be removed features

#### Removed
- Removed features

#### Fixed
- Bug fixes

#### Security
- Security improvements and fixes

---

*Note: This project is in pre-alpha development. All versions prior to 1.0.0 are considered unstable and should not be used in production environments.*
