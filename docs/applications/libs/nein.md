# Nein

> **Nein** (German: no) — Programmatic nftables firewall

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.24.3` |
| Repository | `MacCracken/nein` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/nein.toml` |
| crates.io | [nein](https://crates.io/crates/nein) |

---

## What It Does

- Programmatic nftables rule generation and management
- Network policy enforcement (allow/deny by IP, port, protocol, CIDR)
- NAT and port mapping for container and service networking
- Service access control with agent-level granularity
- Rule diffing and atomic apply (no traffic disruption during updates)

## Consumers

- **daimon** — Agent orchestrator (agent network isolation)
- **stiva** — Container runtime (container networking, bridge/NAT)
- **aegis** — Security daemon (threat response, IP blocking)

## Architecture

- Rust builder API that generates nftables rulesets
- Atomic ruleset replacement via nft CLI or netlink
- Dependencies: serde, tokio, libc

## Roadmap

Stable — published on crates.io. Future: eBPF fast path, connection tracking visualization, integration with service mesh policies.
