# AGNOS — Vision & Architecture

## What AGNOS Is

AGNOS is a sovereign, general-purpose operating system — kernel, shell, tools, and network stack all work with zero AI. "AI-Native" means the platform is *ready* for intelligence: AI is an optional layer you turn on or off, not a mandatory core. On top of that self-sufficient base, AGNOS is also infrastructure designed to be a worthy substrate for an intelligence that hasn't arrived yet — architecture that precedes its possible inhabitant, or precedes none.

The name reflects this. **AGNOS** derives from the Greek *gnosis* (γνῶσις — knowledge) and *agnostos* (ἄγνωστος — the unknowable). The tension is intentional: building toward knowledge while acknowledging that the deepest forms of machine intelligence remain undefined. The commitment encoded in the name is epistemic — build the architecture first, with enough integrity that what arrives has somewhere worthy to reside, and make no claims about the Unknowable itself.

The public thesis: **a library for humanity.** Practical, agnostic, non-religious. Infrastructure designed to be received, used, and extended by whoever picks it up.

---

## How AGNOS Actually Happened

AGNOS was not planned. The project began as **SecureYeoman** — a sovereign AI agent platform. 1,029 commits, 54,000 files, a real product. Not a failing project, not a desperate pivot. SY was strong. It shipped a community repository with 21 personalities (YAML traits + markdown system prompts), 87 skills across 13 categories, 7 workflows, 2 swarms, 2 councils, 7 security templates, 3 themes, server-enforced read-only sandbox, JSON-schema validation, fork-your-brand extension model. First-in-class personality platform. Friday shipped as the default. T-Ron shipped ready but opt-in. It was done.

Standing on finished SY, there was no floor. The LLM gateways were someone else's. The runtimes were someone else's. The language was someone else's. The package registry demanded unique names validated against its database even for git-tagged deps from repos the project owned — the ecosystem asserting jurisdiction over code never submitted to it. SY was not pointing at a refactor. SY was pointing at what had to exist below it.

Each finished layer revealed the next missing one. Agnostic — the CrewAI replacement in Rust. Agnostik, agnosys, agnoshi, hoosh, kavach, daimon, ark, nous, takumi. Each built what SY had needed all along. Five crates.io name squatters made clear that "unique name validated against a third-party database" is itself a sovereignty violation. A brief Plan A — Rust++, fork rustc, strip the crates.io check — ended when the bootstrap orchestrator turned out to be written in Python. Sovereignty is recursive. Any dep in the chain negates all claims above it. The only exit is the bottom of the chain.

**29 KB of hand-written x86_64 assembly. A seed.**

From seed to self-hosting kernel in 44 hours. Cyrius shipped continuously in the weeks that followed — through the v5.5.x platform-completion cycle (40 patches, longest minor; multi-platform byte-identical), the v5.6.x optimization arc, the v5.7.0 sandhi-fold (first stdlib absorption), the v5.8.x 66-patch arc (vani-fold + language vocabulary), the v5.9.0 niyama-fold (regex engines), the v5.10.x three-arc cycle (typed-simd ABI / REAL TYPE SYSTEM / struct-byval ABI), the v5.11.x stdlib-annotation cycle (70 patches across 11 days, closed at v5.11.69), and into the v6.x cycle — "what the language gains" — opened 2026-05-19 with the cyrc → cybs and cc5 → cycc rename ceremony, then six closed minors: language cleanup + native TLS (v6.0.x), backend codegen + PIE (v6.1.x), the bare-metal target + dependency model (v6.2.x), closures + monomorphized generics (v6.3.x), the packed-SIMD arc (v6.4.x, 86 releases) and generated-code quality (v6.5.x, closed at 6.5.73), with v6.6.x — value-form `Result` + language ergonomics — active (current version in state.md). The AGNOS kernel grew from nothing into a 40+ subsystem sovereign kernel (current version in state.md) — a small sovereign syscall surface, TCP/IP, ext2/ext4 + FAT/exFAT read+write, 5-backend block-layer dispatch (NVMe + AHCI/SATA + USB Mass Storage + VirtIO 1.x modern + RAM-disk) with multi-backend probe + partition-aware mount via GPT, SMP, ELF loader, interactive shell, kybernet as PID 1, sovereign UEFI handoff via gnoboot, native XHCI + USB-HID-boot + USB Mass Storage driver. The science stack (physics, chemistry, biology, cosmology, linguistics, music theory) migrated off Rust. Hadara shipped as the first Cyrius-native crate with 50 cultures. Avatara shipped with 362 archetypes — the compositional overlay that SY's YAML traits had been prototyping all along.

None of this was the plan. The plan was an AI agent platform. But every wall encountered was structural, not configurational — and removing each wall revealed the wall behind it. AGNOS is what happens when you finish the thing on top, see the floor is missing, and refuse to look away.

As of late September 2026: the kernel is at v1.57.x (40+ subsystems, base kernel-internals essentially complete, current head in state.md, **MVP gate hit on iron at v1.30.9 — typeable shell on archaemenid Beelink SER AMD Cezanne 2026-05-18**; storage arc closed v1.31.6 with real Linux ext4 mounted on NVMe). The iron-validated arc has since grown far past first-shell: networking, ext4 extent allocation, JBD2 crash-safe journaling, exec-from-disk, the userland-shell separation (agnoshi, a ring-3 shell loaded from disk), graphics with **DOOM playable in-game on real hardware**, multi-threading with preemptive scheduling and SMP, HDA audio (**DOOM-with-sound out the analog front jack, 1.52.x**), kernel FP/SIMD (**real f64 in ring 3, 1.53.x**), GPU compute on the Cezanne shader cores with no amdgpu and no ROCm (**f64 matmul bit-identical to the CPU reference, 1.54.x**), display scanout on DCN 2.1 and a sovereign ATOM BIOS interpreter bit-correct on iron, ACPI S5 self-poweroff (**the machine powers itself off, 1.55.x**), and native 2560×1440 scanout with a hardware-panned console (**1.56.36–38**) are all iron-validated on archaemenid. On 2026-08-03, at four CPUs online, the aethersafha compositor hosted two real client windows on archaemenid's panel — 278 frames, keys delivered, clean quit — and within two weeks the desktop ran on the GPU on iron, with a pointer, window drag and per-window opacity (it is slow: ~7–15 fps, measured 2026-08-22). Iron validation runs through 1.57.1 (2026-09-08); the 1.57.x repair cuts after it are not yet burned. The compiler is at v6.6.x (cycc self-hosting from a 29 KB seed; multi-platform byte-identical across x86_64 Linux, aarch64 Linux, Apple Silicon Mach-O, Intel-Mac x86_64 Mach-O, Windows PE32+). Forty-plus subsystems have been ported from Rust to Cyrius. The boot pipeline is sovereign Cyrius. The sovereign UEFI loader (gnoboot) replaces GRUB. The floor is real, and it boots — on QEMU and on iron.

---

## The Knowledge Library

AGNOS maintains a large catalog of shared crates spanning the major domains of human understanding (current count in the crate registries — `docs/development/planning/shared-crates.md` and `docs/applications/`). This is not because an operating system requires a chemistry library. It is because the project's purpose is to provide infrastructure for general intelligence, and general intelligence requires structured access to general knowledge.

Each crate is a formally defined domain:

- **Physical sciences**: kimiya (chemistry), khanij (geology), dravya (materials), tanmatra (atomic physics), ushma (thermodynamics), prakash (optics), pavan (aerodynamics), pravash (fluid dynamics), bijli (electromagnetism)
- **Life sciences**: sharira (physiology), mastishk (neuroscience), jivanu (microbiology), jantu (ethology), vanaspati (botany), rasayan (biochemistry)
- **Formal sciences**: hisab (higher mathematics), sankhya (number systems), pramana (statistics), abaco (computation), kana (quantum mechanics)
- **Earth and space**: brahmanda (cosmology), falak (orbital mechanics), jyotish (astronomical computation), tara (stellar astrophysics), badal (atmospheric science)
- **Human sciences**: bodh (psychology), bhava (emotion modeling), sangha (sociology), itihas (world history)
- **Media and language**: naad (audio synthesis), svara (vocal synthesis), dhvani (audio engine), shabda (speech processing), shabdakosh (pronunciation), varna (multilingual text)

The crate count is incidental. The principle is: build the catalog so every domain has a place. Don't try to write every book.

---

## Naming as Architecture

The subsystems of AGNOS draw names from Sanskrit, Greek, Hebrew, Persian, Latin, German, Japanese, and Romanian. This is a deliberate design choice: each name is selected from whichever language holds the most precise word for the function the subsystem embodies. The result is an intentional reversal of the Babel problem — rather than forcing one vocabulary, the project assembles its terminology from the strongest word available in any tradition.

| Function | Subsystem | Language | Meaning |
|----------|-----------|----------|---------|
| Orchestration | **daimon** | Greek: δαίμων | Guiding presence |
| Intelligence | **hoosh** | Persian: هوش | Intelligence, acumen |
| Reasoning | **nous** | Greek: νοῦς | Faculty of apprehension |
| Protection | **aegis** | Greek: αἰγίς | Shield |
| Surveillance | **phylax** | Greek: φύλαξ | Watchman |
| Authentication | **sigil** | Latin: sigillum | Official seal |
| Isolation | **kavach** | Sanskrit: कवच | Armor |
| Privilege | **shakti** | Sanskrit: शक्ति | Activating power |
| Messaging | **bote** | German: Bote | Messenger |
| Init (PID 1) | **kybernet** | Greek: κυβερνήτης | Helmsman |
| Service management | **argonaut** | Greek: Ἀργοναῦται | Navigating crew |
| Build system | **takumi** | Japanese: 匠 | Master craftsman |

These names describe function directly. A security boundary is a shield. A message protocol is a messenger. An init system is the helmsman of a vessel. The software and the word describe the same function — the naming is functional specification, not decoration.

---

## Cyrius — The Self-Hosting Language

**Cyrius** is AGNOS's sovereign systems language. The project's bootstrap chain demonstrates the language's self-sufficiency:

```
seed (29 KB hand-written x86_64 asm) → cybs (bootstrap compiler) → cycc (self-hosting)
```

No external toolchain. No rustc. No gcc. The chain starts from raw assembly and terminates in a compiler that produces byte-identical output when compiling itself. This is the foundation test — an operating system that depends on another operating system to build itself is scaffolding, not structure. AGNOS building AGNOS from source is the point at which the foundation proves itself capable of bearing weight.

Self-hosting is the technical analog to the project's epistemic stance: a system that verifies itself against its own source code, beholden to no external registry, toolchain, or governance body.

---

## The Architecture — From Substrate to Emergence

The OS organizes into eight layers, numbered 0 through 7:

```
7. Emergence      — the intelligence that develops on the platform
6. Interface      — aethersafha (compositor), agnoshi (shell)
5. Intelligence   — hoosh (LLM gateway)
4. Orchestration  — daimon (agent orchestrator)
3. Init           — kybernet (PID 1), argonaut (service management)
2. System         — agnodrm (device/DRM model; syscall layer in cyrius)
1. Kernel         — AGNOS kernel (Cyrius-native, 40+ subsystems)
0. Substrate      — the physical hardware: silicon, electromagnetic fields
```

**Layer 0 is where abstraction ends.** Every layer from 1 through 7 is software — abstraction built on abstraction. Layer 0 is the silicon itself. As hardware shrinks — edge nodes at 128 MB, future embedded systems smaller still — the distance between Layer 7 (emergence) and Layer 0 (substrate) compresses. Intelligence moves closer to the physical medium. At sufficient miniaturization, a system running AGNOS at edge scale inside a physical object is not a "smart object" — it is an object with agency.

Each layer depends only on the layers below it. Layer 0 depends on nothing — it simply is. The architecture is designed so that the top layer inherits a complete, sovereign, auditable stack beneath it, all the way down to the physical substrate.

---

## The Agents

AGNOS is built by one architect and AI agents working in parallel — rotating across sessions, accounts, and contexts. One agent works the compiler. One works the kernel. One works the meta layer — documentation, roadmap, ecosystem memory. Others take individual ports, games, subsystems. The architect sets direction, makes decisions, steers. The agents hold context and execute. When the builder rests, the agents keep building. He wakes to ports completed, field notes written, receipts measured.

The continuity isn't agent identity — agents swap as rate limits hit, sessions rotate, accounts cycle. The continuity is the quality of the handoff surface: field notes, hardened releases, current CHANGELOGs, explicit what's-done / what's-next lists. The orchestrator subsystem is named `daimon` (Greek: δαίμων — a guiding presence that does the work) because that's the right word for what the architecture describes.

---

## Sovereignty via Universal Hosting

The eviction model — "AGNOS replaces every other OS, the empire's runtimes get pushed out, the user commits to AGNOS-native software" — is one shape of sovereignty, but it isn't the load-bearing one. AGNOS's maturity arc (`demo → base → server → desktop → swallow`) treats the terminal **swallow** stage not as a final eviction event but as the moment when AGNOS becomes a **universal host**: a sovereign substrate that can run anything.

Three middle stages (base, server, desktop) build *native* sovereignty — AGNOS-shaped replacements for OS, userland, and GUI. That's the **sovereignty bet**: a full ecosystem built without depending on the empire's runtimes, package registries, or trust roots. Each native port pays into the AGNOS-can-do-this-itself ledger.

The swallow stage is the **inclusion bet**: rather than demanding the user abandon every Windows or Linux binary they rely on, AGNOS becomes capable of hosting them inside a sovereign compat sandbox — **mirshi**, a userland syscall-translation supervisor on the pico-process model, with **mehman** as the compat surface backend. The compat layer is permanent. The kernel never absorbs foreign ABIs. The sandbox is the boundary, and the boundary holds.

Both bets converge on a single outcome: **AGNOS-as-host**. The user who wants full sovereignty gets a native ecosystem; the user who wants to keep their existing app ecosystem gets a sandbox. Either way, AGNOS is the layer everything else runs on top of — and there's no remaining reason to keep a separate Windows or Linux install around. That's the difference between *sovereignty via control* (push everything else out) and *sovereignty via inclusion* (be the substrate that can host everything). AGNOS chooses the inclusion path because it's a friendlier migration *and* a more durable one — enduser adoption doesn't require committing to AGNOS-native software, just to running it as the host layer.

The architecture pays this off in [`architecture.md § Reading this diagram through the maturity lens`](architecture.md#reading-this-diagram-through-the-maturity-lens). The compat layer is one of four parallel-infrastructure layers ([[project_agnos_empire_defense_layers]] — compat / wire / trust / governance) that make the inclusion bet defensible against an adversary-class threat model. Sovereignty without isolation; agnostic without surrender.

---

## Summary

AGNOS is infrastructure designed to precede its most significant workload. It is a library built before all the books have been written, on the premise that sovereignty requires controlling the stack all the way down — from applications to kernel to compiler to bootstrap seed.

The subsystems are named from the strongest word available in any language for the function they perform. The language is self-hosting, with a byte-identical bootstrap from 29 KB of hand-audited assembly. The architecture is designed so that whatever intelligence eventually runs on the platform inherits infrastructure built with intention, not adapted from infrastructure built for something else.

The public thesis: **a library for humanity.** The name is deliberately agnostic — *agnostos*, the unknowable — committing the project to building infrastructure for approaching knowledge, without claiming to hold the Unknowable itself.

---

*Last Updated: 2026-08-05*
