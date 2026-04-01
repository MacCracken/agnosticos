# Lipi

> **Lipi** (Sanskrit: लिपि — script, writing system) — Multilingual language engine

| Field | Value |
|-------|-------|
| Status | Pre-1.0 |
| Version | `0.1.0` |
| Repository | `MacCracken/lipi` |
| Runtime | library crate (Rust) |

---

## What It Does

- **Phoneme**: IPA phoneme inventories per language, articulatory features (manner, place, voicing, height, backness), stress patterns, tone systems
- **Script**: Writing system metadata — alphabet, syllabary, logographic, abjad, abugida. Unicode ranges, directionality, romanization
- **Grammar**: Morphological typology (isolating, agglutinative, fusional, polysynthetic), word order, case systems, gender, classifiers
- **Lexicon**: Core vocabulary per language — Swadesh lists, frequency-ranked word lists, cognate detection

## Relationship to Speech Crates

Lipi provides the *structural data* about languages. shabda converts text to phonemes using lipi's inventories. shabdakosh provides pronunciation dictionaries keyed by lipi's IPA system. svara consumes phoneme sequences to produce vocal audio.

```
lipi (structure) → shabda (G2P) → shabdakosh (dictionary) → svara (synthesis) → dhvani (audio)
```

## Consumers

- **shabda** — multilingual G2P conversion (phoneme sets per language)
- **shabdakosh** — pronunciation dictionary (IPA dictionaries beyond CMUdict)
- **svara** — vocal synthesis (phoneme-to-audio mapping)
- **jnana** — multilingual knowledge access
- **vidya** — programming concepts in native languages
- **sankhya** — script-aware numeral display, transliteration for ancient math systems
