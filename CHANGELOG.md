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

### Fixed
- `agnos-examples` crate: added missing workspace dependencies (`anyhow`, `async-trait`, `tracing`, `tracing-subscriber`) so `file_manager_agent` and `quick_start` examples compile cleanly
- Removed stray `use async_trait::async_trait` import placed after entry-point macro in `file-manager-agent.rs`
- Removed unused `use serde_json::json` import from `file-manager-agent.rs`

### Added (Compatibility)
- `docs/adr/adr-007-agnostic-integration.md`: ADR documenting the OpenAI-compatible HTTP API for the LLM Gateway and the integration architecture with the Agnostic QA platform
- `docs/AGNOSTIC_INTEGRATION.md`: Integration guide — how to run Agnostic on AGNOS OS with full gateway routing
- `userland/llm-gateway/src/main.rs`: 16 unit tests for `GatewayConfig`, `LlmGateway` lifecycle, `SharedSession`, `InferenceRequest` JSON serialisation, and the port-8088 contract regression guard

### Planned
- Implement `agnos-sys` agent message loop and LLM gateway communication
- Implement Landlock and seccomp-bpf sandbox enforcement
- Implement IPC routing by agent name
- **LLM Gateway HTTP server** (port 8088, OpenAI-compatible `/v1` API) — enables Agnostic and other apps to route through AGNOS gateway
- Performance benchmarks
- Package signing and update system

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
