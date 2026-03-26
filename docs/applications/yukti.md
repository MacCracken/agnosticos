# Yukti

> **Yukti** (Sanskrit: device/instrument) — Device abstraction layer

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.25.3` |
| Repository | `MacCracken/yukti` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/yukti.toml` |
| crates.io | [yukti](https://crates.io/crates/yukti) |

---

## What It Does

- USB device enumeration, descriptor parsing, and bulk/interrupt transfers
- Optical drive detection and disc media management
- Block device discovery with partition table and filesystem identification
- udev hotplug monitoring with async event streams
- Mount and eject operations with safe unmount handling

## Consumers

- **daimon** — Agent orchestrator (device event forwarding to agents)
- **aethersafha** — Desktop compositor (input device management, HID)
- Any AGNOS application needing hardware device access

## Architecture

- Unified Device trait with vendor-specific backends
- Async udev monitor built on tokio (inotify/netlink)
- 175 tests
- Dependencies: tokio, serde, libc, udev

## Roadmap

Stable — published on crates.io. Future: Bluetooth device support, device permissions via Landlock, virtual device emulation for testing.
