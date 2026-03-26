# Bhava

> **Bhava** (Sanskrit: emotion/feeling) — Emotion and personality engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/bhava` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/bhava.toml` |
| crates.io | `bhava` |

---

## What It Does

- 15-trait personality model with PAD (Pleasure-Arousal-Dominance) mood vectors
- OCC appraisal theory for event-driven emotional responses
- Archetypes, relationships, sentiment analysis, and display rules
- Stress regulation, emotional growth, circadian rhythm, energy, and flow states
- Micro-expressions, EQ scoring, ACT-R activation, preference learning, SQLite persistence

## Consumers

- **SecureYeoman** — SY personality (soul/brain emotional layer)
- **kiran / joshua** — NPC emotions and personality simulation
- Any AGNOS agent that needs believable personality or emotional modeling

## Architecture

- 785 tests, 105 benchmarks (criterion)
- 30 modules extracted from SecureYeoman's soul/brain subsystem
- SQLite persistence for long-running personality state

## Roadmap

Stable at 1.1.0. Extracted from SY as a standalone crate. Future: group dynamics modeling, cultural display rule presets, integration with hoosh for LLM-driven emotional reasoning.
