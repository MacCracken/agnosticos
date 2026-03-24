# Abacus

> **Abacus** — Desktop calculator and unit converter

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.22` |
| Repository | `MacCracken/abacus` |
| Runtime | native-binary (Rust, GUI) |
| Recipe | `recipes/marketplace/abacus.toml` |
| Port | N/A |

---

## Why First-Party

Abacus is the desktop calculator application for AGNOS, providing a native GUI for mathematical expressions and unit conversions. Built on the abaco library crate for expression evaluation and unit conversion logic.

## What It Does

- Desktop GUI calculator with full expression support
- Unit conversions (length, weight, temperature, volume, speed, data, currency)
- AI-assisted natural-language math parsing via hoosh
- Session history and variable persistence

## AGNOS Integration

- **Abaco**: Uses the abaco library crate for expression parsing, evaluation, and unit conversion
- **Hoosh**: NL math parsing ("what's 15% tip on $47.50")
- **Marketplace**: Category: desktop-utility. Desktop seccomp mode, no network access, data dir `~/.abacus/`

## Architecture

- Single-crate binary built on abaco
- Desktop entry: `abacus.desktop` (Category: Utility/Calculator)
