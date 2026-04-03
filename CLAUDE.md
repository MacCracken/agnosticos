# AGNOS — Claude Code Instructions

## Project Identity

**AGNOS** — AI-Native General Operating System

- **Type**: Genesis repository — the brain of the OS
- **License**: GPL-3.0-only
- **Version**: CalVer `2026.3.31` (YYYY.M.D, patches as `-N`)
- **Version file**: `VERSION` at repo root (single source of truth)
- **MSRV**: 1.89
- **Status**: Pre-Beta — monolith fully dismantled, all code in standalone repos

## Role

This repo is the **genesis layer** — it owns system init and the infrastructure to build AGNOS from nothing. Once the system boots and ark takes over, this repo's job is done.

**Owns:**
- **kernel/** — Linux kernel configs (what we boot)
- **scripts/** — Bootstrap toolchain, ISO/image build, chroot, validation (33 scripts)
- **docs/** — Architecture, roadmap, specs, security (161 files)
- **.github/workflows/** — CI/CD that validates the whole system (15 workflows)
- **docker/** — Dockerfiles for dev/edge/installer
- **Makefile** — Top-level build orchestration
- **userland/examples/** — Agent SDK examples (only Cargo workspace member)

**Does NOT own (extracted):**
- **Recipes** → **zugot** (`MacCracken/zugot`) — all takumi build recipes
- **Production code** → standalone repos under `/home/macro/Repos/{name}/`

## Standalone Repos

| Subsystem | Version | Role |
|-----------|---------|------|
| **zugot** | — | Recipe repository (all takumi build recipes) |
| **agnostik** | 0.90.0 | Shared types, domain primitives (10 feature gates) |
| **agnosys** | 0.51.0 | Kernel interface (Landlock, seccomp, syscalls) |
| **daimon** | 0.6.0 | Agent orchestrator, 144 MCP tools |
| **hoosh** | 1.2.0 | LLM inference gateway, 15 providers |
| **agnoshi** | 0.90.0 | AI shell |
| **aethersafha** | 0.1.0 | Wayland compositor |
| **kybernet** | 0.51.0 | PID 1 binary |
| **argonaut** | 0.90.0 | Init system library |
| **sigil** | 1.0.0 | Trust/crypto boundary |
| **ark** | 0.1.0 | Package manager |
| **nous** | 0.1.0 | Package resolver |
| **takumi** | 0.1.0 | Build system |
| **aegis** | 0.1.0 | Security daemon |
| **shakti** | 0.1.0 | Privilege escalation |
| **kavach** | 2.0.0 | Sandbox execution |
| **bote** | 0.92.0 | MCP core + host registry |
| **t-ron** | 0.90.0 | MCP security |
| **phylax** | 0.22.3 | Threat detection |

## Development Process

### Work Loop (continuous)

1. Work phase — script fixes, kernel configs, doc improvements, CI/CD
2. If touching examples: `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings` (from `userland/`)
3. Documentation — update CHANGELOG, roadmap, docs
4. Version check — VERSION and docs all in sync
5. Return to step 1

### Task Sizing

- **Low/Medium effort**: Batch freely — multiple items per work loop cycle
- **Large effort**: Small bites only — break into sub-tasks, verify each before moving to the next
- **If unsure**: Treat it as large

## DO NOT

- **Do not commit or push** — the user handles all git operations
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies

## Documentation Structure

```
Root files (required):
  README.md, CHANGELOG.md, CLAUDE.md, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, LICENSE

docs/ (required):
  architecture/overview.md — module map, data flow, consumers
  development/roadmap.md — completed, backlog, future, v1.0 criteria
  development/applications/shared-crates.md — 77-crate registry

docs/ (when earned):
  adr/ — architectural decision records
  guides/usage.md — patterns and examples
```

## CHANGELOG Format

Follow [Keep a Changelog](https://keepachangelog.com/). Performance claims MUST include benchmark numbers. Breaking changes get a **Breaking** section with migration guide.
