# Tarang

> **Tarang** (Sanskrit: wave) — AI-native media framework replacing ffmpeg

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.16-1` |
| Repository | `MacCracken/tarang` |
| Runtime | native-binary (~4.4MB) |
| Recipe | `recipes/marketplace/tarang.toml` |
| MCP Tools | 8 `tarang_*` |
| Agnoshi Intents | 8 |
| Port | N/A (library + CLI) |

---

## Why First-Party

Tarang replaces the ffmpeg dependency with a Rust-native media pipeline. Pure Rust audio decoding via symphonia and C FFI video codecs (dav1d, openh264, libvpx) give AGNOS a media foundation that can be audited, sandboxed, and extended without shipping a monolithic C binary. No existing framework provides AI content classification, audio fingerprinting, and transcription as first-class features integrated with a local LLM gateway.

## What It Does

- Full encode/decode pipeline for audio and video with magic bytes format detection
- Container demuxers and muxers for WAV, MP4, OGG, and MKV
- Audio fingerprinting and similarity search via vector embeddings
- AI-powered scene detection, content classification, and thumbnail generation
- Transcription routing through hoosh for speech-to-text

## AGNOS Integration

- **Daimon**: Stores audio fingerprints in the vector store; registers multimodal capabilities; ingests media metadata into RAG
- **Hoosh**: LLM content description for media files, transcription routing
- **MCP Tools**: `tarang_probe`, `tarang_analyze`, `tarang_codecs`, `tarang_transcribe`, `tarang_formats`, `tarang_fingerprint_index`, `tarang_search_similar`, `tarang_describe`
- **Agnoshi Intents**: `tarang probe <file>`, `tarang analyze <file>`, `tarang codecs`, `tarang transcribe <file>`, `tarang formats`, `tarang fingerprint <file>`, `tarang search <query>`, `tarang describe <file>`
- **Marketplace**: Media/Library category; sandbox profile allows read access to media directories, network for remote streams, GPU access for hardware-accelerated encoding

## Architecture

- **Crates**:
  - `tarang-core` — format detection (magic bytes), container types, codec registry
  - `tarang-demux` — demuxers and muxers for WAV, MP4, OGG, MKV
  - `tarang-audio` — pure Rust audio decode/encode via symphonia
  - `tarang-video` — C FFI video codecs: dav1d (AV1 decode), openh264 (H.264), libvpx (VP8/VP9)
  - `tarang-ai` — fingerprinting, scene detection, content classification, daimon/hoosh integration
- **Dependencies**: symphonia (audio), dav1d (AV1), openh264 (H.264), libvpx (VP8/VP9)

## Roadmap

Stable — maintenance mode. Future work includes hardware-accelerated encoding (VA-API/NVENC) and additional container formats (WebM muxer, FLAC encoder).
