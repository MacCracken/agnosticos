# tula

Sovereign ML weight-file format (तुला — *balance / scale; the instrument that weighs*) — a safetensors/GGUF analog, Cyrius-native, with a **sigil-signed header** those formats lack.

- **Repository**: [github.com/MacCracken/tula](https://github.com/MacCracken/tula)
- **License**: GPL-3.0-only
- **Status**: Stable — v1.0+ (format v1 **FROZEN**)
- **Language**: Cyrius

64-byte header + typed manifest entries (`f64` / `int8` / ternary-packed / nf4-block) + 8-byte-aligned payload; builder/reader with heap and zero-copy-mmap read paths; Ed25519 sign/verify over the content region (`sig_off`/`sig_len`), rejecting unsigned/tampered/wrong-key checkpoints at load. Format frozen after 105 assertions + a 2,003,000-iteration fuzz + a security audit.

**M0 of the Type-3 weight-import chain.** Depends on [sigil](sigil.md) (signed header). Consumers declaring `[deps.tula]` (2026-09-27): `anukūlana` (the Type-3 importer) and ifran (its signed model store); tula's own docs name attn11/tentib checkpoints and the murti model-load seam as planned users.

See the [libs registry](../README.md) for the full catalog and dependency context.
