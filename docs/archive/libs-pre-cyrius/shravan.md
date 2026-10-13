# Shravan

> **Shravan** (Sanskrit: hearing/perception) — Audio codecs

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.1` |
| Repository | `MacCracken/shravan` |
| Runtime | library crate (Rust) |

---

## What It Does

- WAV encode/decode
- FLAC encode/decode
- AIFF codec support
- Ogg container support
- MP3 decoding
- Opus codec (over Ogg)
- PCM sample format conversion (i16, i24, i32, f32, f64)
- Sinc resampler for sample rate conversion
- ID3v2 and Vorbis Comment metadata tag reading
- SIMD-accelerated PCM conversion
- Dithering for bit-depth reduction
- Streaming decode support
- Auto-detection of audio format from file headers
- `no_std` + `alloc` compatible (disable `std` feature)
- Zero `unsafe` code, no panics

## Consumers

- **nidhi** — sample playback engine (WAV/PCM loading via `io` feature)
- **tarang** — media framework (audio codec layer)
- **shruti** — DAW (audio file import/export)
- **jalwa** — media player (audio file decoding)
- **dhvani** — audio engine (codec integration)
