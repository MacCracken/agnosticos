# vani

Audio device I/O (वाणी — *voice, speech*) — direct ALSA ioctls for PCM playback and capture, format negotiation, a power-of-two ring buffer, XRUN recovery and mixer control; vendored into the Cyrius stdlib as `lib/vani.cyr`, with an agnos backend on the `snd_*` syscall band.

- **Repository**: [github.com/MacCracken/vani](https://github.com/MacCracken/vani)
- **License**: GPL-3.0-only
- **Status**: Stable — v1.0+
- **Language**: Cyrius

Consumers declaring `[deps.vani]` (local checkouts, 2026-09-28): dhvani, jalwa — while cyrius-doom, mishran, cyrius-polyomino and cyrius-bb vendor its `core` shim instead.

See the [libs registry](README.md) for the full catalog and dependency context.
