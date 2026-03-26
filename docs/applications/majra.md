# Majra

> **Majra** (Arabic: channel/conduit) — Distributed queue and multiplex engine

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.22.3` |
| Repository | `MacCracken/majra` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/majra.toml` |
| crates.io | [majra](https://crates.io/crates/majra) |

---

## What It Does

- Pub/sub messaging with topic-based routing and fan-out
- Priority queues with configurable scheduling policies
- Relay and multiplex for inter-process and inter-node communication
- Heartbeat protocol for liveness detection and connection management
- Rate limiting with token bucket and sliding window algorithms

## Consumers

- **daimon** — Agent orchestrator (agent IPC, event bus)
- **stiva** — Container runtime (container-to-host messaging)
- **federation** — Multi-node coordination (cross-node relay)
- **joshua** — Game manager (multiplayer event distribution)

## Architecture

- Async channels built on tokio with backpressure support
- Pluggable transport layer (Unix sockets, TCP, in-process)
- Dependencies: tokio, serde, bytes

## Roadmap

Stable — published on crates.io. Future: persistent queue (WAL-backed), NATS/AMQP bridge, dead letter queues.
