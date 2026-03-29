# Ghurni

> **Ghurni** (Sanskrit: rotation/spinning) — Mechanical sound synthesis

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.0.0` |
| Repository | `MacCracken/ghurni` |
| Runtime | library crate (Rust) |

---

## What It Does

- Procedural synthesis of mechanical sounds from physical models: rotational harmonics, combustion impulses, resonant bodies
- Engine synthesis: combustion cycle, exhaust/intake resonance, firing order (diesel, petrol, multi-cylinder)
- Gear and transmission: tooth mesh frequency, metallic ring, shift transients, synchro whine
- Motor and turbine: electromagnetic hum, commutator noise, blade pass frequency
- Drivetrain: differential hypoid whine, chain drive rattle, belt squeal
- Forced induction: turbo spool, supercharger whine, blow-off valve
- RPM-driven parameterization with load (0.0-1.0) and material (steel, aluminum, cast iron)
- Common `Synthesizer` trait for mixers, effects chains, and generic composition
- Optional naad backend for DSP primitives
- `no_std` + `alloc` compatible

## Consumers

- **kiran** — game engine (vehicle and machinery audio)
- **dhvani** — audio engine (mechanical sound layers)
