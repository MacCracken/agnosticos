# SecureYeoman

> **SecureYeoman** — Flagship AI agent platform for AGNOS

| Field | Value |
|-------|-------|
| Status | Released |
| Version | See [`VERSION`](https://github.com/MacCracken/secureyeoman/blob/main/VERSION) |
| Repository | `MacCracken/SecureYeoman` |
| Runtime | native binary — the Rust `sy-core` server (Node.js left the server runtime at 0.5.0); the TypeScript packages under `packages/` still build on Node ≥ 22 |
| Recipe | `zugot/marketplace/secureyeoman.cyml` |
| MCP Tools | SecureYeoman's own MCP server (`packages/mcp`). The `yeoman_*` names below are the 2026-04 planned AGNOS-side set; they do not appear in its source |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A |

---

## Why First-Party

SecureYeoman is the flagship product that showcases AGNOS as an AI-native operating system. No existing agent framework provides OS-level sandboxing (Landlock, seccomp), cryptographic trust verification (sigil), and integrated marketplace distribution. It is the reference implementation for building production AI agents on AGNOS, with three deployment profiles covering desktop, standalone, and IoT edge scenarios.

## What It Does

- Full AI agent development and deployment platform — a hybrid Rust + TypeScript monorepo whose backend moved from TypeScript to Rust at 0.5.0 (2026-04-18, the "Rust-native" release that took Node.js out of the server runtime; SemVer since then, CalVer before)
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

- **Crates**: Rust workspace `crates/` — one crate, `sy-core`, shipping two binaries: `sy-core` (server) and `sy-edge` (fleet runtime). TypeScript npm workspaces under `packages/` (core, dashboard, mcp, shared, desktop, mobile) remain in the tree; the server has been Rust since 0.5.0
- **Dependencies**: Rust (edition 2024) for the server; Node ≥ 22 and TypeScript for the packages. ⚠ SecureYeoman's own sources disagree about the release binary: its CHANGELOG (0.5.0) and README name the Rust `sy-core` as the single binary, while its `release-binary.yml` still runs the Bun compile pipeline in `scripts/build-binary.sh`

## Roadmap

Stable — active development. Edge variant expanding for fleet management use cases. The backend has been Rust (`sy-core`) since 0.5.0; 0.5.4 (2026-09-26) was a security and correctness review of that server.
