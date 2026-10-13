# ADR-008: Cyrius as Sovereign Systems Language

**Status:** Accepted
**Date:** 2026-04-04
**Recorded:** 2026-05-06 (catch-up entry)
**Supersedes:** [ADR-001](adr-001-foundation-and-architecture.md) — *Rust as Primary Language* section only. Other ADR-001 decisions (daimon orchestration, hoosh gateway, cross-project integration) remain in force.

---

## Context

ADR-001 (2026-03-07) chose Rust as AGNOS's primary language. The choice was correct on its technical merits — memory safety without GC, zero-cost abstractions, strong type system — and remains correct *as a language*. The decision being superseded is **what AGNOS depends on**, not what Rust does.

By March 2026, the Rust dependency had crystallized into four problems that no amount of code quality could fix:

1. **Registry sovereignty.** crates.io is a centralized name registry governed by a foundation AGNOS does not control. A name-squatting incident in late March 2026 made this concrete: AGNOS cannot guarantee that a package name it depends on today will be available tomorrow on terms it accepts. An OS whose package manager (ark) ultimately resolves through someone else's registry is not sovereign over its own dependency graph.
2. **Bootstrap chain opacity.** rustc requires a working rustc to compile (or a Python+LLVM+C++ bootstrap chain to start from C). The AGNOS thesis is that sovereignty is recursive — every dependency you cannot personally audit is a dependency you do not own. A self-hosting compiler with a hand-auditable seed was the only way to make the sovereignty claim honest at the language layer.
3. **OS-native primitives are library abstractions in Rust.** Agents, sandboxes, capabilities, IPC — these are AGNOS's first-class concerns. In Rust they live in libraries (tokio, async-std, etc.) that AGNOS does not control. To make agents a kernel primitive (eventually a Phase 20 goal), the language itself needs to know about them.
4. **Toolchain politics.** Governance changes at language foundations have, historically, derailed projects on multi-year horizons. AGNOS commits to a 50-year time horizon (`philosophy.md`). External governance over the language layer is incompatible with that horizon.

A 29KB hand-auditable assembly seed — the smallest viable starting point that produces a working compiler that produces a working OS — was attempted as a 48-hour spike on 2026-04-03.

## Decisions

### 1. Cyrius is AGNOS's primary systems language

**C.Y.R.I.U.S.** — *Consciousness Yields Righteous Intelligence Unveiling Self*. Named after Cyrus the Great — the king who decreed the rebuilding of the Temple. Sovereignty through restoration, not conquest.

All AGNOS-native production code (kernel, PID 1, init, shell, sandbox, crypto, audit, package manager, build system, science crates, applications) is written in Cyrius. Rust code that has not yet been ported is preserved with `rust-old/` directories alongside the Cyrius port until parity, then retired.

**Alternatives rejected at this decision point:**
- **Stay on Rust** — fails registry-sovereignty + bootstrap-opacity criteria.
- **Fork rustc, strip crates.io** — possible but inherits the entire Rust governance surface and the LLVM dependency. Doesn't fix the bootstrap chain.
- **Adopt Zig** — solves some bootstrap concerns but not registry sovereignty; also externally governed.
- **Write everything in C** — surrenders the memory-safety property that motivated the original Rust choice.

### 2. The bootstrap chain must be hand-auditable

The chain is:

```
CPU → 29KB assembly seed → cyrc (12KB bootstrap compiler)
    → bridge.cyr (bridge compiler) → cc5 (~741KB self-hosting modular compiler)
    → AGNOS kernel + all userland
```

Four items between silicon and the OS. The 29KB seed is a single hand-auditable x86_64 assembly file. No Python. No LLVM. No libc. No external Rust. The bootstrap loop closed on 2026-04-04 (stage1f → asm.cyr → stage1f_v2 byte-exact match), at which point the original Rust seed was retired.

### 3. Cyrius owns the full stack — language, compiler, stdlib, package manager, build system

- **Compiler**: cc5 (self-hosting from 29KB seed). cc5 → cyc rename queued for v6.0 — single one-and-done cleanup so the binary name decouples from the version.
- **Standard library**: 42+ modules (string, alloc, io, fmt, vec, str, args, syscalls, process, fs, toml/cyml, json, csv, net, http, http_server, ws, tls, thread, async, math, regex, hashmap, bench, tagged unions, mmap, cffi, u128, …). Built from scratch in Cyrius. Sibling distfiles (sandhi, vani, niyama) absorb into the stdlib via the **fold-in pattern** when multi-consumer gates are met.
- **Package manager**: ark (Cyrius-native, 4× smaller than the Rust predecessor). Distribution through ark, not crates.io. **Names belong to the builders, not the squatters.**
- **Build system**: `cyrius build` (auto-resolves deps from `cyrius.cyml`, auto-prepends includes). Replaces Cargo. Manifest is single source of truth — no separate `.cyrius-toolchain` file.
- **Recipes**: zugot (421 base + 90 bazaar). takumi consumes zugot (Cyrius port active; Rust-old authoritative until parity).

### 4. Migration is incremental, byte-identical, and reversible until retirement

Each subsystem is ported one at a time. The Rust version is preserved with a git tag and benchmark CSVs. The Cyrius version must demonstrate parity (functional + security + performance) before the Rust version is retired. Cross-arch byte-identical reproducibility is a hard gate (achieved 2026-05-x for x86_64 Linux, aarch64 Linux on real Pi, Apple Silicon Mach-O, and Windows PE32+).

This is non-negotiable: a port without byte-identical reproducibility across the supported targets is not a finished port.

### 5. Cyrius governance stays inside AGNOS

The language is governed by AGNOS, not by an external foundation, council, or registry. ADRs covering language-level changes live in `cyrius/docs/adr/`. The compiler's release cycle (currently v5.x) is owned by the AGNOS team. Outside contributions are welcome but the governance bar is sovereignty alignment, not popularity.

This is the explicit answer to the toolchain-politics concern in the Context section: AGNOS will not be on the receiving end of someone else's governance decision.

## Consequences

### Positive

- **Registry sovereignty achieved.** crates.io is no longer in AGNOS's dependency graph. Names belong to whoever builds them, distributed through ark.
- **Hand-auditable bootstrap.** A reader with assembly fluency can audit the entire chain from silicon to OS in one sitting. The 29KB seed is the smallest sovereign starting point known for a self-hosting language + working OS.
- **Port receipts are dramatic.** kybernet 6.7MB (Rust) → 486KB (Cyrius, 14× smaller); hoosh 5.1MB / 40 crates → 474KB / 0 deps (10.8× smaller, 70× faster compile); agnosys 6.9MB → 117KB (59× smaller). The minimum-viable `exit42` baseline is ~2,269× smaller than Rust stripped (152 B vs 345 KB).
- **Multi-platform byte-identical** across x86_64 Linux, aarch64 Linux (real Pi), Apple Silicon Mach-O, Windows PE32+ — closed by Cyrius v5.5.x.
- **First-class OS primitives become possible.** Agents, sandboxes, capabilities can become language-level constructs in later Cyrius cycles, not library abstractions.
- **Bootstrap chain is permanent.** No future change in Rust governance, LLVM licensing, or Python tooling can derail AGNOS at the language layer.

### Negative / accepted trade-offs

- **Ecosystem starts at zero.** No package ecosystem, no community libraries, no Stack Overflow. Every dependency must be written or absorbed via fold-in. This is a multi-year compounding cost.
- **Hiring pool is one person at the start.** Cyrius engineers do not exist in the labor market. Onboarding contributors requires teaching the language. AGNOS accepts this as a near-term limit.
- **Toolchain immaturity.** No mature debuggers, profilers, IDE support, or LSP at the start. The cycle (currently v5.9.x) is closing these one by one (`cyrlint`, `cyrfmt`, `cyrdoc`, `cyrius` build/test/bench/fuzz already shipped).
- **Cross-port-debt accumulates.** Every Rust subsystem must be ported, and the port arc is a multi-cycle effort (state.md tracks 30+ Cyrius-native ports as of 2026-05-06; ~5 still pending). Pending: bhava, aegis (scaffold), aethersafha (scaffold), takumi (parity in flight), mela.
- **Reversal cost is now prohibitive.** As of 2026-05-06, the kernel, compiler, ark/nous, kybernet, sigil, libro, hoosh, daimon, and 30+ other subsystems are Cyrius-native. Returning to Rust would be more expensive than continuing forward. The decision is effectively irreversible.

### Specific supersession of ADR-001

- **ADR-001 § "Rust as Primary Language"**: superseded by this ADR. All other ADR-001 decisions (multi-agent orchestration via daimon, LLM gateway via hoosh, cross-project integration) remain in force.
- ADR-001's stated alternatives-rejected (C, C++, Go, Zig) are no longer the relevant comparison set. The relevant comparison is now Cyrius vs. Rust + crates.io + LLVM, which this ADR resolves in favor of Cyrius.

## Receipts

- Bootstrap loop closed: 2026-04-04 (stage1f → asm.cyr → stage1f_v2 byte-exact)
- Self-hosting compiler shipped: Cyrius 1.0 — 2026-04-04 (1,467 lines, 43KB binary, 9ms self-compile, 41ms full bootstrap)
- 44-hour scaffold-to-kernel-solid window: 2026-04-03 → 2026-04-04
- Multi-platform byte-identical: x86_64 Linux, aarch64 Linux (Pi 4), Apple Silicon, Windows PE32+ — Cyrius v5.5.x
- 30+ subsystems ported as of 2026-05-06 (Cyrius v5.9.0 cut day)
- Stdlib fold-in pattern formalized via sandhi (v5.7.0), vani (v5.8.0), niyama (v5.9.0)

## References

- [`philosophy.md`](../philosophy.md) — sovereignty as recursive, ideological basis
- `cyrius` repo `docs/adr/` — language-level ADRs (governed inside AGNOS, not outside)
