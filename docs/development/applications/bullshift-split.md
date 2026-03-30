# BullShift — Engine/GUI Split Roadmap

> **Status**: Planned | **Last Updated**: 2026-03-30
>
> Split BullShift monorepo into standalone engine crate + desktop GUI app.
> Same pattern as ifran/tanur, sahifa/scriba.

---

## Current State

Single monorepo with two subdirectories:

```
bullshift/
├── rust/                    # bullshift-core (26K lines)
│   ├── Cargo.toml           # name = "bullshift-core", cdylib + rlib
│   └── src/
│       ├── agnos/           # Daimon/MCP integration
│       ├── ai_bridge/       # LLM integration
│       ├── algo/            # Trading algorithms
│       ├── audit/           # Audit logging
│       ├── bullrunnr/       # Strategy runner
│       ├── database/        # Data persistence
│       ├── data_stream/     # Market data feeds
│       ├── indicators/      # Technical indicators
│       ├── integration/     # Exchange integrations
│       ├── mobile/          # Mobile API
│       ├── monitoring/      # Health monitoring
│       ├── options/         # Options trading
│       ├── paper_hands/     # Paper trading
│       ├── plugins/         # Plugin system
│       ├── rbac/            # Role-based access
│       ├── security/        # Security layer
│       ├── sentiment/       # Sentiment analysis
│       ├── sheets/          # Spreadsheet export
│       ├── trading/         # Core trading engine
│       ├── trendsetter/     # Trend detection
│       ├── webhooks/        # Webhook integrations
│       └── websocket/       # WebSocket feeds
├── flutter/                 # Desktop GUI (Flutter/Dart)
│   ├── pubspec.yaml
│   └── lib/
├── docker-compose.yml
├── Dockerfile
└── VERSION                  # 2026.3.10-1
```

## Target State

Two repos:

### bullshift (engine) — GPL-3.0-only

```
bullshift/
├── Cargo.toml               # name = "bullshift", publishable to crates.io
├── src/
│   ├── lib.rs
│   ├── algo/                # Trading algorithms
│   ├── indicators/          # Technical indicators
│   ├── trading/             # Core engine
│   ├── data_stream/         # Market data feeds
│   ├── options/             # Options trading
│   ├── paper_hands/         # Paper trading simulation
│   ├── bullrunnr/           # Strategy runner
│   ├── trendsetter/         # Trend detection
│   ├── sentiment/           # Sentiment analysis
│   ├── webhooks/            # Exchange webhooks
│   ├── websocket/           # WebSocket feeds
│   ├── database/            # Persistence
│   ├── security/            # Auth, encryption
│   ├── rbac/                # Role-based access
│   ├── audit/               # Audit chain (uses libro)
│   ├── plugins/             # Plugin system
│   ├── sheets/              # Spreadsheet export
│   ├── ai/                  # AI bridge + daimon.rs + MCP tools
│   └── bin/
│       └── server.rs        # Headless API server
```

### bullshift-app (GUI) — AGPL-3.0-only

Flutter desktop app, depends on bullshift engine via FFI (cdylib).

```
bullshift-app/
├── rust/                    # Thin FFI bridge to bullshift crate
│   └── Cargo.toml           # depends on bullshift = "2026.x"
├── flutter/
│   ├── pubspec.yaml
│   └── lib/
├── Dockerfile
└── docker-compose.yml
```

## Migration Steps

1. [ ] Extract `rust/src/` into standalone `bullshift` repo with proper Cargo.toml
2. [ ] Remove `cdylib` from engine — make it a pure `rlib` crate
3. [ ] Create `bullshift-app` repo with Flutter + thin FFI bridge
4. [ ] FFI bridge in app depends on `bullshift` crate (path dep during dev, crates.io for release)
5. [ ] Move Docker/deploy infrastructure to app repo
6. [ ] Update marketplace recipe to point at app repo releases
7. [ ] Publish engine to crates.io as `bullshift` (GPL-3.0-only)
8. [ ] Update `docs/applications/bullshift.md` to reflect split
9. [ ] License cleanup: engine = GPL-3.0-only, app = AGPL-3.0-only

## Considerations

- **FFI bridge**: Flutter talks to Rust via `cdylib` + `dart:ffi`. After split, the cdylib lives in the app repo as a thin wrapper around the engine crate.
- **Mobile**: The `mobile/` module stays in the engine — it's API code, not GUI.
- **Docker**: `Dockerfile` and `docker-compose.yml` move to the app repo since they build the full stack (engine + GUI).
- **Marketplace recipe**: Currently `recipes/marketplace/bullshift.toml` — update to fetch from app repo releases.

---

*Last Updated: 2026-03-30*
