# SecureYeoman

> **SecureYeoman** — Flagship AI agent platform for AGNOS

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/SecureYeoman` |
| Runtime | native-binary (~124MB) |
| Recipe | `recipes/marketplace/secureyeoman.toml` |
| MCP Tools | 5 `yeoman_*` |
| Agnoshi Intents | 5 |
| Port | N/A |

---

## Why First-Party

SecureYeoman is the flagship product that showcases AGNOS as an AI-native operating system. No existing agent framework provides OS-level sandboxing (Landlock, seccomp), cryptographic trust verification (sigil), and integrated marketplace distribution. It is the reference implementation for building production AI agents on AGNOS, with three deployment profiles covering desktop, standalone, and IoT edge scenarios.

## What It Does

- Full AI agent development and deployment platform built on TypeScript/Bun
- OS-level agent sandboxing with Landlock filesystem and seccomp syscall filtering
- Trust verification and code signing via sigil for agent integrity
- Three deployment variants: full platform, Lite (standalone agent), Edge (IoT, ~7MB)
- Marketplace publishing with automated CI/CD and GitHub release integration

## AGNOS Integration

- **Daimon**: Registers agents with full lifecycle management; uses sandbox profiles, heartbeat, and audit APIs
- **Hoosh**: LLM inference for agent reasoning, planning, and tool selection
- **MCP Tools**: `yeoman_deploy`, `yeoman_status`, `yeoman_sandbox`, `yeoman_trust`, `yeoman_manage`
- **Agnoshi Intents**: `yeoman deploy`, `yeoman status`, `yeoman list`, `yeoman sandbox`, `yeoman trust`
- **Marketplace**: Category: development/agents. Full sandbox profile with custom seccomp filters

## Variants

| Variant | Recipe | Size | Use Case |
|---------|--------|------|----------|
| SecureYeoman | `secureyeoman.toml` | ~124MB | Full platform |
| SecureYeoman Lite | `secureyeoman-lite.toml` | ~124MB | Standalone agent |
| SecureYeoman Edge | `recipes/edge/secureyeoman-edge.toml` | ~7MB | IoT/edge devices |

## Architecture

- **Crates**: TypeScript/Bun monorepo (not Rust crates)
- **Dependencies**: bun, typescript, esbuild

## Roadmap

Stable — active development. Edge variant expanding for fleet management use cases. Tracking Bun runtime updates.
