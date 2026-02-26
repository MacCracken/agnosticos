# Changelog

All notable changes to AGNOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
