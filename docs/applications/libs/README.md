# AGNOS Shared Library Crates

> Reusable library crates that form the AGNOS stack. Consumer [applications](../README.md) depend on these — they should never depend on external libraries when an AGNOS crate covers the domain.
>
> See also: [First-Party Standards — Own the Stack](../../development/applications/first-party-standards.md#own-the-stack) | [Shared Crates Registry](../../development/applications/shared-crates.md) | [Science Crate Specs](../../development/science-crate-specs.md)

---

## Core Infrastructure

| Crate | Domain | Docs |
|-------|--------|------|
| [agnosai](agnosai.md) | Agent orchestration types | [crates.io](https://crates.io/crates/agnosai) |
| [aethersafta](aethersafta.md) | Desktop compositor (Wayland) | — |
| [bote](bote.md) | MCP protocol | [crates.io](https://crates.io/crates/bote) |
| [hoosh](hoosh.md) | LLM inference gateway | [crates.io](https://crates.io/crates/hoosh) |
| [kavach](kavach.md) | Sandboxing (Landlock, seccomp) | [crates.io](https://crates.io/crates/kavach) |
| [libro](libro.md) | Audit chain logging | [crates.io](https://crates.io/crates/libro) |
| [majra](majra.md) | Queue, pub/sub, heartbeat | [crates.io](https://crates.io/crates/majra) |
| [nein](nein.md) | Programmatic nftables firewall | [crates.io](https://crates.io/crates/nein) |
| [phylax](phylax.md) | Threat detection (YARA, entropy) | — |
| [stiva](stiva.md) | OCI container runtime | [crates.io](https://crates.io/crates/stiva) |
| [szal](szal.md) | Workflow engine | [crates.io](https://crates.io/crates/szal) |
| [t-ron](t-ron.md) | MCP security monitor | — |
| [yukti](yukti.md) | Device abstraction (USB, block) | [crates.io](https://crates.io/crates/yukti) |

## Media & Audio

| Crate | Domain | Docs |
|-------|--------|------|
| [dhvani](dhvani.md) | Audio engine (DSP, mixing, PipeWire) | [crates.io](https://crates.io/crates/dhvani) |
| [naad](../../../README.md) | Synthesis primitives (oscillators, filters, envelopes) | Scaffolded |
| [svara](../../../README.md) | Formant & vocal synthesis | Scaffolded |
| [ranga](ranga.md) | Image processing, color conversion | [crates.io](https://crates.io/crates/ranga) |
| [soorat](soorat.md) | Rendering engine (wgpu, shaders, PBR) | — |
| [tarang](tarang.md) | Media framework (audio/video codec) | [crates.io](https://crates.io/crates/tarang) |

## Math, Physics & Simulation

| Crate | Domain | Docs |
|-------|--------|------|
| [abaco](abaco.md) | Basic math, unit conversion | — |
| [hisab](hisab.md) | Linear algebra, geometry, spatial | [crates.io](https://crates.io/crates/hisab) |
| [pramana](pramana.md) | Statistics, probability, Bayesian inference | Scaffolded |
| [sankhya](sankhya.md) | Ancient math (Mayan, Babylonian, Egyptian, Vedic, Chinese, Greek) | Scaffolded |
| [impetus](impetus.md) | Physics engine (Rapier wrapper) | — |
| [prakash](prakash.md) | Optics & light simulation | [crates.io](https://crates.io/crates/prakash) |
| [raasta](raasta.md) | Navigation & pathfinding | — |

## Science

| Crate | Domain | Docs |
|-------|--------|------|
| [badal](badal.md) | Weather & atmospheric | — |
| [bijli](bijli.md) | Electrical engineering | — |
| [dravya](dravya.md) | Material science | — |
| [goonj](goonj.md) | Acoustics & room simulation | — |
| [khanij](khanij.md) | Mineralogy & crystallography | — |
| [kimiya](kimiya.md) | Chemistry | — |
| [pavan](pavan.md) | Aerodynamics | — |
| [pravash](pravash.md) | Fluid dynamics (CFD) | — |
| [ushma](ushma.md) | Thermodynamics | — |

## AI & Behavior

| Crate | Domain | Docs |
|-------|--------|------|
| [ai-hwaccel](ai-hwaccel.md) | GPU/hardware detection | [crates.io](https://crates.io/crates/ai-hwaccel) |
| [bhava](bhava.md) | Human emotion & personality | [crates.io](https://crates.io/crates/bhava) |
| [jantu](jantu.md) | Animal ethology & creature behavior | [crates.io](https://crates.io/crates/jantu) |
