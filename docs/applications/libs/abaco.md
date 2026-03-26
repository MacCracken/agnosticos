# Abaco

> **Abaco** (Italian/Spanish: abacus) — Basic math and unit conversion library crate

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.1.0` |
| Repository | `MacCracken/abaco` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/abaco.toml` |
| Port | N/A |

---

## Why First-Party

Abaco provides the foundational math layer for the AGNOS ecosystem: expression parsing, evaluation, and unit conversions. Any app that needs to evaluate mathematical expressions or convert between units depends on abaco rather than reimplementing these primitives. The desktop calculator app (abacus) is the primary consumer.

## What It Does

- Full expression parser with operator precedence, variables, and functions
- 60+ unit conversions across length, weight, temperature, volume, speed, data, and currency
- AI-assisted NL math parsing support (via hoosh integration in the ai crate)
- Step-by-step explanation mode for complex expressions

## Consumers

- **Abacus** — Desktop calculator GUI (primary consumer)
- Any AGNOS app needing expression evaluation or unit conversion

## Architecture

- **Crates**: core, eval (expression parser/evaluator), units (conversion engine), ai (NL parsing via hoosh)
- **Dependencies**: serde, tokio, reqwest (hoosh client)

## Roadmap

Stable — 61 tests passing. Future considerations: symbolic algebra, financial functions, integration with hisab (higher math crate) for advanced operations.
