# Shabdakosh

> **Shabdakosh** (Sanskrit: dictionary) — Pronunciation dictionary

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/shabdakosh` |
| Runtime | library crate (Rust) |

---

## What It Does

- 10,000+ entry English pronunciation dictionary generated at compile time from CMUdict
- Bidirectional ARPABET-to-svara phoneme mapping
- IPA (International Phonetic Alphabet) support
- User overlay dictionaries for application-specific entries
- Import/export in CMUdict text format and JSON (with `json` feature)
- Region-aware pronunciations
- Diff computation between dictionary versions
- Built on svara phoneme types
- `no_std` + `alloc` compatible (uses hashbrown for no-std hash maps)

## Consumers

- **shabda** — G2P engine (primary dictionary backend for word lookup)
- **svara** — vocal synthesis (phoneme type definitions)
- **vansh** — voice AI shell (pronunciation lookup)
