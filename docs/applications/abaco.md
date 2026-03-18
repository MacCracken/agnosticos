# Abaco

> **Abaco** (Italian/Spanish: abacus) — AI-native calculator and unit converter

| Field | Value |
|-------|-------|
| Status | Released |
| Version | Latest GitHub release |
| Repository | `MacCracken/abaco` |
| Runtime | native-binary (Rust) |
| Recipe | `recipes/marketplace/abaco.toml` |
| MCP Tools | 5 `abaco_*` |
| Agnoshi Intents | 5 |
| Port | N/A |

---

## Why First-Party

Abaco demonstrates the AI-native approach at its simplest: a calculator that understands natural language. Users say "what's 15% tip on $47.50" or "convert 72kg to pounds" and get instant results. By integrating with hoosh, it can explain complex math step-by-step. No existing calculator combines NL parsing with a proper expression evaluator and 60+ unit conversions in a single lightweight tool.

## What It Does

- Natural-language math parsing via hoosh ("split $127.50 four ways plus 20% tip")
- Full expression evaluator with variables, functions, and operator precedence
- 60+ unit conversions across length, weight, temperature, volume, speed, data, and currency
- Step-by-step explanation mode for complex calculations
- History and variable persistence across sessions

## AGNOS Integration

- **Daimon**: Registers as a lightweight agent; uses memory store for calculation history
- **Hoosh**: LLM inference for NL math parsing, ambiguity resolution, and step-by-step explanations
- **MCP Tools**: `abaco_calculate`, `abaco_convert`, `abaco_explain`, `abaco_history`, `abaco_define`
- **Agnoshi Intents**: `abaco calc`, `abaco convert`, `abaco explain`, `abaco history`, `abaco define`
- **Marketplace**: Category: utilities/calculator. Minimal sandbox — no filesystem or network beyond localhost

## Architecture

- **Crates**: core, eval (expression parser/evaluator), units (conversion engine), ai (NL parsing)
- **Dependencies**: serde, tokio, reqwest (hoosh client)

## Roadmap

Stable — 61 tests passing. Future considerations: graphing mode, symbolic algebra, financial calculator presets.
