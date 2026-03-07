# ADR-022: Argonaut — Custom Init System

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS targets <3 second boot to agent-ready state. systemd is powerful but brings ~200+ unit files, complex dependency resolution, and startup overhead that AGNOS doesn't need. AGNOS boots exactly 4-6 services depending on mode — a custom init is simpler and faster.

## Decision

Create `argonaut` — a single Rust binary init system with three boot modes:

### Boot Modes
- **Server**: agent-runtime (port 8090) + llm-gateway (port 8088). Headless.
- **Desktop**: + aethersafha compositor + agnoshi shell. Full desktop.
- **Minimal**: agent-runtime only. For containers and embedded.

### Boot Sequence
```
1. argonaut starts as PID 1
2. Mount /proc, /sys, /dev, /run
3. Start eudev (device manager)
4. Verify rootfs (dm-verity via sigil)
5. Start aegis (security daemon)
6. Start daimon (agent-runtime, port 8090)
7. Start hoosh (llm-gateway, port 8088)     [server + desktop]
8. Start aethersafha (compositor)             [desktop only]
9. Start agnoshi (shell)                      [desktop only]
```

### Service Management
- Dependency-ordered startup via topological sort
- Health checks (HTTP, TCP, process alive)
- Ready checks (block boot until service responds)
- Restart policies (Always, OnFailure, Never)
- Graceful shutdown in reverse order (SIGTERM → timeout → SIGKILL)

### Design Principles
- Single static binary, no dynamic linking except glibc
- No shell scripts in boot path
- No runlevels — three modes, that's it
- All config in a single TOML file
- Boot log to kernel ring buffer + /run/argonaut/boot.log

## Consequences

### Positive
- Sub-3-second boot to agent-ready
- Minimal attack surface (one binary, no shell scripts)
- Deterministic boot sequence (no parallel dependency races)
- Simple to audit (hundreds of lines, not thousands)

### Negative
- Must maintain our own init (systemd handles edge cases we'll discover)
- No cgroup management (must implement separately if needed)
- No socket activation (services must start eagerly)

### Mitigations
- Cgroup setup is a boot step, not init complexity
- Socket activation unnecessary — AGNOS has 4-6 services, not 100
- Edge cases addressed as discovered in beta testing

## Related
- ADR-018: LFS-Native Distribution (argonaut is the init for AGNOS base)
- ADR-019: Sigil (argonaut verifies boot components via sigil)
- ADR-020: Aegis (argonaut starts aegis early in boot)
