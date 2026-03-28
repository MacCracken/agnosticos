# svara

> Sanskrit: स्वर — voice / tone / musical note

Formant and vocal synthesis: glottal source modeling, vocal tract filtering, phoneme-level synthesis, prosodic control, and sequenced speech rendering.

- **Version**: 1.0.0
- **crates.io**: [svara](https://crates.io/crates/svara)
- **Repository**: [github.com/MacCracken/svara](https://github.com/MacCracken/svara)
- **Consumers**: dhvani, vansh, prani, shabda

## Role in AGNOS

Human vocal synthesis engine. Converts phoneme sequences into audio samples at ~1,000x real-time. The voice of the system — everything that speaks passes through svara.

## Key Capabilities

- **Dual glottal models**: Rosenberg B polynomial + LF (Liljencrants-Fant) with Rd voice quality
- **SOA formant filter**: Structure-of-arrays biquad bank (8-wide) with compiler auto-vectorization
- **48 phonemes**: Full English coverage — vowels, consonants, diphthongs, affricates, glottal stop, tap/flap
- **Hillenbrand formant data**: Per-vowel frequencies and bandwidths (1995 male averages)
- **Vocal tract**: Parallel formant bank + nasal coupling (place-dependent) + subglottal resonance + source-filter interaction + lip radiation + DC blocking + gain normalization
- **Prosody**: Monotone cubic f0 contours (hisab), intonation patterns, stress
- **Coarticulation**: Look-ahead onset, sigmoid crossfades (hisab smootherstep), per-phoneme resistance (Recasens DAC), F2 locus equations
- **Spectral analysis**: FFT via hisab, formant estimation, compensated RMS
- **`no_std` compatible**: Core DSP via `libm` + `alloc`

## Dependencies

- **hisab** 1.2 — FFT, easing, compensated summation, monotone cubic interpolation
- **naad** 1.0 — oscillators, filters (optional, via `naad-backend` feature)
- **libm** — `no_std` math fallback
- **serde** — all types serialize/deserialize
- **thiserror** — error types
- **tracing** — structured logging

## Performance

| Benchmark | Time | Real-time factor |
|-----------|------|-----------------|
| Formant filter (1024 samples) | 5.4 µs | ~8,000x |
| Glottal source (1024 samples) | 4.3 µs | ~5,400x |
| Vocal tract (1024 samples) | 25 µs | ~930x |
| Phoneme render (vowel, 100ms) | 112 µs | ~890x |
| 5-phoneme sequence | 453 µs | ~770x |
| Full speak (shabda G2P + render) | ~2 ms | ~500x |
