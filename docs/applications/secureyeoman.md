# SecureYeoman

> **SecureYeoman** — Flagship AI agent platform for AGNOS

| Field | Value |
|-------|-------|
| Status | Released |
| Version | See [`VERSION`](https://github.com/MacCracken/secureyeoman/blob/main/VERSION) |
| Repository | `MacCracken/SecureYeoman` |
| Runtime | native-binary — a Rust `sy-core` server plus TypeScript packages on Node ≥ 22 |
| Recipe | `zugot/marketplace/secureyeoman.cyml` |
| MCP Tools | SecureYeoman's own MCP server (`packages/mcp`). The `yeoman_*` names below are the 2026-04 planned AGNOS-side set; they do not appear in its source |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A |

---

## Why First-Party

SecureYeoman is the flagship product that showcases AGNOS as an AI-native operating system. No existing agent framework provides OS-level sandboxing (Landlock, seccomp), cryptographic trust verification (sigil), and integrated marketplace distribution. It is the reference implementation for building production AI agents on AGNOS, with three deployment profiles covering desktop, standalone, and IoT edge scenarios.

## What It Does

- Full AI agent development and deployment platform — a hybrid Rust + TypeScript monorepo whose backend is migrating from TypeScript to Rust (SemVer since 0.5.0, the "Rust-native era"; CalVer before)
- OS-level agent sandboxing with Landlock filesystem and seccomp syscall filtering
- Trust verification and code signing via sigil for agent integrity
- Three deployment variants: full platform, Lite (standalone agent), Edge (IoT — the Rust `sy-edge` binary)
- Marketplace publishing with automated CI/CD and GitHub release integration

## AGNOS Integration

- **Daimon**: Registers agents with full lifecycle management; uses sandbox profiles, heartbeat, and audit APIs
- **Hoosh**: LLM inference for agent reasoning, planning, and tool selection
- **MCP Tools** (planned set, above): `yeoman_deploy`, `yeoman_status`, `yeoman_sandbox`, `yeoman_trust`, `yeoman_manage`
- **Agnoshi Intents** (the Rust era's planned set — not in the current agnoshi): `yeoman deploy`, `yeoman status`, `yeoman list`, `yeoman sandbox`, `yeoman trust`
- **Marketplace**: Category: development/agents. Full sandbox profile with custom seccomp filters

## Variants

| Variant | Recipe | Use Case |
|---------|--------|----------|
| SecureYeoman | `secureyeoman.cyml` | Full platform |
| SecureYeoman Lite | `secureyeoman-lite.cyml` | Standalone agent |
| SecureYeoman Edge | `zugot/edge/secureyeoman-edge.cyml` | IoT/edge devices |

Binary sizes drift per release — see the SecureYeoman README.

## Architecture

- **Crates**: Rust workspace `crates/` — one crate, `sy-core`, shipping two binaries: `sy-core` (server) and `sy-edge` (fleet runtime). TypeScript npm workspaces under `packages/` (core, dashboard, mcp, shared, desktop, mobile); the TypeScript backend is being phased out in favor of Rust
- **Dependencies**: Node ≥ 22, TypeScript, Rust (edition 2024); the single-binary distribution is compiled with Bun

## Roadmap

Stable — active development. Edge variant expanding for fleet management use cases. The backend is migrating from TypeScript to Rust (`sy-core`), converging with the AGNOS ecosystem crates.
