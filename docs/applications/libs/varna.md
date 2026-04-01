# Varna

> **Varna** (Sanskrit: वर्ण — letter, syllable, sound) — Multilingual language engine

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/varna` |
| Runtime | library crate (Rust) |

---

## What It Does

- **Phoneme**: IPA phoneme inventories per language, articulatory features (manner, place, voicing, height, backness), stress patterns, tone systems
- **Script**: Writing system metadata — alphabet, syllabary, logographic, abjad, abugida. Unicode ranges, directionality, romanization
- **Grammar**: Morphological typology (isolating, agglutinative, fusional, polysynthetic), word order, case systems, gender, classifiers
- **Lexicon**: Core vocabulary per language — Swadesh lists, frequency-ranked word lists, cognate detection

## Relationship to Speech Crates

Varna provides the *structural data* about languages. shabda converts text to phonemes using varna's inventories. shabdakosh provides pronunciation dictionaries keyed by varna's IPA system. svara consumes phoneme sequences to produce vocal audio.

```
varna (structure) → shabda (G2P) → shabdakosh (dictionary) → svara (synthesis) → dhvani (audio)
```

## Consumers

- **shabda** — multilingual G2P conversion (phoneme sets per language)
- **shabdakosh** — pronunciation dictionary (IPA dictionaries beyond CMUdict)
- **svara** — vocal synthesis (phoneme-to-audio mapping)
- **jnana** — multilingual knowledge access
- **vidya** — programming concepts in native languages
- **sankhya** — script-aware numeral display, transliteration for ancient math systems
