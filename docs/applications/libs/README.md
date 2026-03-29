# AGNOS Shared Library Crates

> Reusable library crates that form the AGNOS stack. Consumer [applications](../README.md) depend on these — they should never depend on external libraries when an AGNOS crate covers the domain.
>
> **63 library crates** — 34 at v1.0+ stable, 29 pre-1.0 | Last updated: 2026-03-29
>
> See also: [First-Party Standards — Own the Stack](../../development/applications/first-party-standards.md#own-the-stack) | [Shared Crates Registry](../../development/applications/shared-crates.md) | [Science Crate Specs](../../development/science-crate-specs.md)

---

## Core Infrastructure

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [agnosai](agnosai.md) | Agent orchestration | 1.0.2 | [crates.io](https://crates.io/crates/agnosai) |
| [aethersafta](aethersafta.md) | Media compositing | 0.25.3 | — |
| [bote](bote.md) | MCP protocol | 0.50.0 | [crates.io](https://crates.io/crates/bote) |
| [hoosh](hoosh.md) | LLM inference gateway | 1.1.0 | [crates.io](https://crates.io/crates/hoosh) |
| [ifran](ifran.md) | Local LLM inference/training | 1.1.0 | — |
| [kavach](kavach.md) | Sandboxing (Landlock, seccomp) | 1.0.1 | [crates.io](https://crates.io/crates/kavach) |
| [libro](libro.md) | Audit chain logging | 0.25.3 | [crates.io](https://crates.io/crates/libro) |
| [mabda](mabda.md) | GPU foundation (wgpu) | 0.1.0 | — |
| [majra](majra.md) | Queue, pub/sub, heartbeat | 1.0.2 | [crates.io](https://crates.io/crates/majra) |
| [muharrir](muharrir.md) | Shared editor primitives | 0.23.5 | — |
| [nein](nein.md) | Programmatic nftables firewall | 0.24.3 | [crates.io](https://crates.io/crates/nein) |
| [phylax](phylax.md) | Threat detection (YARA, entropy) | 0.5.0 | — |
| [stiva](stiva.md) | OCI container runtime | 1.0.0 | [crates.io](https://crates.io/crates/stiva) |
| [szal](szal.md) | Workflow engine | 1.0.1 | [crates.io](https://crates.io/crates/szal) |
| [t-ron](t-ron.md) | MCP security monitor | 0.26.3 | — |
| [yukti](yukti.md) | Device abstraction (USB, block) | 0.25.3 | [crates.io](https://crates.io/crates/yukti) |

## GPU & Rendering

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [mabda](mabda.md) | GPU foundation — device, buffers, compute, textures | 0.1.0 | — |
| [soorat](soorat.md) | Rendering engine (wgpu, PBR, shadows) | 0.29.3 | — |
| [ranga](ranga.md) | Image processing, color conversion | 0.29.4 | [crates.io](https://crates.io/crates/ranga) |
| [selah](selah.md) | Screenshot capture, annotation, redaction | 0.29.4 | — |
| [kiran](kiran.md) | Game engine (ECS, scene, physics, audio) | 0.26.3 | — |
| [raasta](raasta.md) | Navigation & pathfinding | 0.26.3 | — |

## Media & Audio

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [tarang](tarang.md) | Media framework (audio/video codec) | 0.21.3 | [crates.io](https://crates.io/crates/tarang) |
| [dhvani](dhvani.md) | Audio engine (DSP, mixing, PipeWire) | 0.22.4 | [crates.io](https://crates.io/crates/dhvani) |
| [shravan](shravan.md) | Audio codecs (WAV, FLAC, Opus, MP3) | 1.0.1 | — |
| [naad](naad.md) | Synthesis primitives (oscillators, filters) | 1.0.0 | — |
| [nidhi](nidhi.md) | Sample playback engine (SFZ/SF2) | 1.1.0 | — |
| [svara](svara.md) | Formant & vocal synthesis | 1.1.0 | — |
| [garjan](garjan.md) | Environmental sound synthesis | 1.0.0 | — |
| [ghurni](ghurni.md) | Mechanical sound synthesis | 1.0.0 | — |
| [prani](prani.md) | Creature vocal synthesis | 1.1.0 | — |
| [shabda](shabda.md) | Grapheme-to-phoneme (G2P) | 1.0.0 | — |
| [shabdakosh](shabdakosh.md) | Pronunciation dictionary | 1.0.0 | — |
| [goonj](goonj.md) | Acoustics & room simulation | 1.1.0 | — |

## Math & Physics

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [hisab](hisab.md) | Linear algebra, geometry, spatial | 1.3.0 | [crates.io](https://crates.io/crates/hisab) |
| [abaco](abaco.md) | Basic math, unit conversion | 1.1.0 | — |
| [pramana](pramana.md) | Statistics, probability, Bayesian | 1.0.0 | — |
| [impetus](impetus.md) | Physics engine (rigid body, collision) | 1.2.0 | — |
| [prakash](prakash.md) | Optics & light simulation | 1.2.0 | [crates.io](https://crates.io/crates/prakash) |

## Science

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [bijli](bijli.md) | Electromagnetism, FDTD | 1.0.1 | — |
| [ushma](ushma.md) | Thermodynamics | 1.1.0 | — |
| [pravash](pravash.md) | Fluid dynamics (SPH, CFD) | 1.1.0 | — |
| [kimiya](kimiya.md) | Chemistry | 1.0.0 | — |
| [dravya](dravya.md) | Material science | 1.0.0 | — |
| [goonj](goonj.md) | Acoustics | 1.1.0 | — |
| [pavan](pavan.md) | Aerodynamics | 1.0.0 | — |
| [badal](badal.md) | Weather & atmospheric | 1.0.0 | — |
| [khanij](khanij.md) | Geology & mineralogy | 1.0.0 | — |
| [tanmatra](tanmatra.md) | Atomic/subatomic physics | 1.0.0 | — |
| [jantu](jantu.md) | Ethology & creature behavior | 1.0.0 | — |

## AI & Behavior

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [ai-hwaccel](ai-hwaccel.md) | GPU/hardware detection | 1.0.0 | [crates.io](https://crates.io/crates/ai-hwaccel) |
| [bhava](bhava.md) | Emotion & personality engine | 1.2.0 | [crates.io](https://crates.io/crates/bhava) |

## Programming & Education

| Crate | Domain | Version | Docs |
|-------|--------|---------|------|
| [vidya](vidya.md) | Programming reference library — multi-language best practices | Planned | — |
