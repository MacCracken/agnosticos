# Shabda

> **Shabda** (Sanskrit: word/sound) — Grapheme-to-phoneme conversion

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/shabda` |
| Runtime | library crate (Rust) |

---

## What It Does

- Text-to-phoneme (G2P) conversion for vocal synthesis pipelines
- Text normalization: lowercase, number expansion, punctuation handling
- Tokenization with sentence boundary detection
- G2P engine: dictionary lookup (via shabdakosh) with rule-based fallback
- Prosody mapping: stress and intonation from punctuation and syntax
- Outputs `Vec<PhonemeEvent>` sequences compatible with svara
- Built on shabdakosh for pronunciation dictionaries and svara for phoneme types
- Optional JSON serialization of phoneme sequences
- `no_std` + `alloc` compatible

## Consumers

- **svara** — vocal synthesis (receives phoneme event sequences)
- **vansh** — voice AI shell (text-to-speech pipeline)
- **dhvani** — audio engine (TTS integration)
