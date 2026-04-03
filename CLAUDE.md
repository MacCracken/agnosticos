# AGNOS — Claude Code Instructions

## Project Identity

**AGNOS** — AI-Native General Operating System

- **Type**: Meta-repository (recipes, docs, scripts, kernel configs, CI/CD, examples)
- **License**: GPL-3.0-only
- **Version**: CalVer `2026.3.31` (YYYY.M.D, patches as `-N`)
- **Version file**: `VERSION` at repo root (single source of truth)
- **MSRV**: 1.89
- **Status**: Pre-Beta — monolith fully dismantled, all code in standalone repos

## What This Repo Contains

All production code has been extracted to standalone repos. This repo is the OS meta-repository:

- **recipes/** — 422 takumi build recipes (base, desktop, AI, edge, marketplace, bazaar)
- **scripts/** — 33 build/deploy/validation scripts
- **docs/** — 161 files (roadmap, architecture, specs, security)
- **kernel/** — Linux kernel configs (6.6, 6.x, 7.0)
- **docker/** — Dockerfiles for dev/edge/installer
- **.github/workflows/** — 15 CI/CD GitHub Actions workflows
- **userland/examples/** — Agent SDK examples (only Cargo workspace member)
- **Makefile** — Top-level build orchestration

## Standalone Repos

All production code lives in standalone repos under `/home/macro/Repos/{name}/`:

| Subsystem | Version | Role |
|-----------|---------|------|
| **agnostik** | 0.90.0 | Shared types, domain primitives (10 feature gates) |
| **agnosys** | 0.51.0 | Kernel interface (Landlock, seccomp, syscalls) |
| **daimon** | 0.6.0 | Agent orchestrator, 144 MCP tools |
| **hoosh** | 1.1.0 | LLM inference gateway, 15 providers |
| **agnoshi** | 0.1.0 | AI shell |
| **aethersafha** | 0.1.0 | Wayland compositor |
| **kybernet** | 0.51.0 | PID 1 binary |
| **argonaut** | 0.1.0 | Init system library |
| **sigil** | 1.0.0 | Trust/crypto boundary |
| **ark** | 0.1.0 | Package manager |
| **nous** | 0.1.0 | Package resolver |
| **takumi** | 0.1.0 | Build system |
| **aegis** | 0.1.0 | Security daemon |
| **shakti** | 0.1.0 | Privilege escalation |
| **kavach** | 2.0.0 | Sandbox execution |
| **bote** | 0.90.0 | MCP core + host registry |
| **t-ron** | 0.90.0 | MCP security |
| **phylax** | 0.22.3 | Threat detection |

## Development Process

### Work Loop (continuous)

1. Work phase — recipe updates, doc improvements, script fixes, roadmap items
2. Validate recipes: `./scripts/ark-validate-recipes.sh`
3. If touching examples: `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings` (from `userland/`)
4. Documentation — update CHANGELOG, roadmap, docs
5. Version check — VERSION, recipes, and docs all in sync
6. Return to step 1

### Recipe Work

- **Every recipe change requires full field audit** — never just bump version
- Verify: name, version, SHA256, license (`-only` suffix), tags, build commands, `min_agnos_version`, dependencies
- Cross-check versions against crates.io or upstream release tags
- Use `./scripts/ark-validate-recipes.sh` after changes

### Task Sizing

- **Low/Medium effort**: Batch freely — multiple items per work loop cycle
- **Large effort**: Small bites only — break into sub-tasks, verify each before moving to the next
- **If unsure**: Treat it as large

## DO NOT

- **Do not commit or push** — the user handles all git operations
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies
- Do not skip recipe validation

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
