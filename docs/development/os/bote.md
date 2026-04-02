# Bote

> **Bote** (German: messenger) — MCP core service

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.22.3` |
| Repository | `MacCracken/bote` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/bote.toml` |
| crates.io | [bote](https://crates.io/crates/bote) |

---

## What It Does

- JSON-RPC 2.0 protocol implementation for Model Context Protocol (MCP)
- Tool registry with dynamic registration and discovery
- Audit integration via libro for all tool invocations
- TypeScript bridge for interop with Node.js/Bun tool providers
- Batch tool calls with parallel execution support

## Consumers

- **daimon** — Agent orchestrator (MCP dispatch for 151+ built-in tools)
- All MCP tool providers across the AGNOS ecosystem
- **t-ron** — Security monitor (wraps bote calls with audit/filtering)

## Architecture

- Protocol layer with request/response/notification handling
- Tool schema validation against JSON Schema definitions
- Dependencies: serde, serde_json, tokio, libro

## Roadmap

Stable — published on crates.io. Future: MCP protocol versioning, streaming tool results, capability negotiation.
