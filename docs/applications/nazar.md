# Nazar

> **Nazar** (Arabic/Persian: watchful eye) — AI-native system monitor

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/nazar` |
| Runtime | native-binary (Rust) |
| Recipe | `recipes/marketplace/nazar.toml` |
| MCP Tools | 5 `nazar_*` |
| Agnoshi Intents | 5 |
| Port | 8095 |

---

## Why First-Party

No existing system monitor provides LLM-powered diagnostic suggestions or integrates with an AI agent orchestrator for anomaly detection. Nazar reads /proc directly for local metrics and connects to daimon (port 8090) and hoosh (port 8088) to offer natural-language system queries like "why is my CPU usage high?" with AI-generated root-cause analysis.

## What It Does

- Real-time system metrics from /proc (CPU, memory, disk, network, processes)
- AI-powered anomaly detection with baseline learning via daimon anomaly APIs
- Natural-language system diagnostics and health queries via hoosh
- Desktop GUI (egui) with live graphs and alert dashboards
- Historical metric storage and trend analysis

## AGNOS Integration

- **Daimon**: Registers on port 8090; uses anomaly baseline/alert APIs, agent heartbeat, and metric submission
- **Hoosh**: LLM inference for diagnostic explanations, anomaly triage, and NL system queries
- **MCP Tools**: `nazar_status`, `nazar_metrics`, `nazar_diagnose`, `nazar_alerts`, `nazar_processes`
- **Agnoshi Intents**: `nazar status`, `nazar metrics`, `nazar diagnose`, `nazar alerts`, `nazar top`
- **Marketplace**: Category: system/monitoring. Sandboxed with read-only /proc access and network access to localhost ports

## Architecture

- **Crates**: core, api, ui (egui), ai, mcp
- **Dependencies**: egui/eframe, serde, tokio, reqwest (daimon/hoosh clients), sysinfo

## Roadmap

Stable — 27 tests passing. Future considerations: GPU monitoring, per-container metrics, remote node monitoring via edge fleet APIs.
