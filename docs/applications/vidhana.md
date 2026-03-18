# Vidhana

> **Vidhana** (Sanskrit: regulation/arrangement) — AI-native system settings

| Field | Value |
|-------|-------|
| Status | Released |
| Version | 2026.3.18 |
| Repository | `MacCracken/vidhana` |
| Runtime | native-binary (Rust) |
| Recipe | `recipes/marketplace/vidhana.toml` |
| MCP Tools | 5 `vidhana_*` |
| Agnoshi Intents | 5 |
| Port | 8099 |

---

## Why First-Party

System settings are a core OS component that must deeply integrate with every AGNOS subsystem — aethersafha (display, theme), daimon (agent management), hoosh (LLM configuration), and all hardware interfaces. Natural-language control ("turn on dark mode", "connect to WiFi network X") requires OS-level access that no third-party settings app could safely provide. Vidhana is the single pane of glass for all AGNOS configuration.

## What It Does

- Display settings: resolution, scaling, multi-monitor layout, theme (light/dark/high-contrast)
- Audio settings: output/input device selection, volume, PipeWire configuration
- Network settings: WiFi, Ethernet, VPN, DNS, firewall rules via nftables
- Privacy and security: Landlock policies, agent permissions, audit log viewer
- Power management: sleep/hibernate, battery profiles, scheduled shutdown
- Accessibility: screen reader, high-contrast, font scaling, keyboard navigation

## AGNOS Integration

- **Daimon**: Registers on port 8090; manages agent sandbox profiles, permission grants, and system-wide agent policies
- **Hoosh**: LLM inference for NL settings control, configuration search, and troubleshooting suggestions
- **MCP Tools**: `vidhana_display`, `vidhana_network`, `vidhana_audio`, `vidhana_privacy`, `vidhana_power`
- **Agnoshi Intents**: `vidhana display`, `vidhana network`, `vidhana audio`, `vidhana privacy`, `vidhana power`
- **Marketplace**: Category: system/settings. Privileged sandbox with hardware access, network configuration, and display server control

## Architecture

- **Crates**: 6 crates (core, display, network, audio, privacy, power) + egui GUI
- **Dependencies**: egui/eframe, serde, tokio, reqwest, nix (system calls), zbus (D-Bus for PipeWire/NetworkManager)

## Roadmap

v1 release (2026.3.18) — 76+ tests passing. Future considerations: Bluetooth panel, printer management, locale/timezone settings, parental controls.
