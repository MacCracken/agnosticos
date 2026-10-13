# AGNOS Shared Libraries — Released (v1.0+)

> Reusable library crates that form the AGNOS stack. Consumer [applications](../README.md) depend on these — they should never depend on external libraries when an AGNOS crate covers the domain.
>
> **94 libraries at v1.0+** (92 standalone + 2 stdlib-folded: sandhi v5.7.0, niyama v5.9.0; **samvada 1.0.1 + chitra 1.0.3 graduated, kashi 1.0.10 and agnosai 2.1.0 catalogued 2026-09-27** — the D-Bus client (OS & Infrastructure), the raster image decoder and the console-font subsystem (Graphics & Rendering); **samay 1.0.1 + vani 1.1.3 catalogued 2026-08-05** (1.0.0 tagged 2026-07-21 and 2026-07-06) — the task scheduler (OS & Infrastructure) and the ALSA audio-device-I/O lib (Media & Audio), both crossed v1.0 in the planning registry and moved here per the rule that v1.0+ crates live under `docs/applications/`; **cmdit 1.1.0 catalogued 2026-07-03** — sovereign CLI/arg-parsing lib that was shipped-but-unregistered, found by the audit's completeness sweep, added to OS & Infrastructure; **bhumi 1.0.0 + mehman 1.0.0 graduated 2026-07-02/03** — the two AGNOS-compositor backends (platform + compat/"swallow"), now in Graphics & Rendering; **tula 1.0.0 added 2026-07-02** — sovereign ML weight-file format, M0 of the Type-3 weight-import chain; **yantra 1.0.0 catalogued 2026-06-18** (tagged 2026-06-16) — UI automation lib; **ganita + bayan added 2026-06-14**; aegis 1.0.0 graduated from pre-1.0 during v5.10.x; **mihi 1.0.0 graduated 2026-05-20** as system-info probe substrate for the terminal-aesthetics cohort) — pre-1.0 libs tracked in the development planning registry. Binary tools at v1.0+ (agnos, agnoshi, agora, anuenue, argonaut, **ark**, **bannermanor**, **commandress**, cyim, cyim-lsp, **darshini**, hapi, **iam**, **ifran**, kii, **kriya**, kybernet, **mela**, **mirshi**, nous, owl, **sit**, **takumi**) are listed separately in **[binaries.md](../binaries.md)** (moved out of the planning registry 2026-06-23, per the rule that v1.0+ crates live under `docs/applications/`). **2026-05-20 cohort**: mihi 1.0.0 (OS & Infrastructure, probe lib), iam 1.0.0 (Binaries & Tools, fastfetch-equivalent), bannermanor 1.0.0 (Binaries & Tools, figlet-equivalent — graduated PM after CLI surface + CYML font format + default font set frozen).
> This registry is the authoritative public catalog of v1.0+ libraries.
>
> **Last Updated**: 2026-09-27 (Version column synced to the local `VERSION` files — 108 cells across the three registries. **samvada 1.0.1** (OS & Infrastructure) and **chitra 1.0.3** (Graphics & Rendering) graduated, **kashi 1.0.10** — v1.0 since 2026-05-28, never catalogued — was added to Graphics & Rendering, `agnosai` 2.1.0 — Cyrius since 2.0.0 (2026-08-14), dropped here 2026-07-05 as unported Rust — was re-catalogued in OS & Infrastructure, and `soorat` moved to Graphics & Rendering from OS & Infrastructure; stale row descriptions were corrected and the Rust-era per-lib stubs collapsed to the thin pointer pattern. Prior 2026-08-23: **darshana 1.0.0 GRADUATED** — the TTY/raw-mode primitives lib froze its API at 1.0.0 (ADR 0003 enumerates the surface: 29 functions + 37 constants, machine-checked bidirectionally in CI). Pre-1.0 row REMOVED from [`shared-crates.md`](../../development/planning/shared-crates.md) per the classification rule; added here to **OS & Infrastructure (29→30)**, total **89→90**. Verified against the live `VERSION` file. Five consumers (chakshu, anuenue, cyim, kii, bannermanor) are still on 0.7.1–0.9.0 dep pins at the tag — tracked in darshana's own roadmap, not a blocker for this registry. Prior 2026-08-05: **version-drift sweep against local `VERSION` files — the first since 2026-07-05, a month of drift.** **2 graduations**: **samay 1.0.1** → OS & Infrastructure (28→29) and **vani 1.1.3** → Media & Audio (12→13), both out of the planning registry; Science & Knowledge, Graphics, Language and Physics keep their counts. Total **87→89** (87 standalone + 2 folded). ⚠ **One status correction, not a bump**: `stiva` carried `—` / "Rust-era scaffold; Cyrius port pending" — the live repo is **Cyrius-native at 3.0.16** with the Rust→Cyrius port complete (oracle frozen at `rust-old/`, 34 of 36 CLI verbs live); flagged for a human call on whether a 36-verb CLI belongs in [binaries.md](../binaries.md) instead. **45 version bumps** — OS/Infra: aegis→1.1.4 · agnostik→1.3.4 · agnodrm 1.4.5→**1.5.0** · ai-hwaccel→2.3.16 · bayan 1.0.4→**1.4.0** · bote 3.0.0→**3.3.0** · cmdit 1.1.0→**1.2.2** · daimon 1.2.9→**2.0.0** (major) · hoosh 2.4.11→**2.6.0** · kavach 3.7.0→**3.11.7** · libro→2.8.4 · mabda→4.0.8 · majra→2.5.3 · nein 1.5.4→**1.6.4** · patra→1.12.12 · phylax→1.2.4 · sakshi→2.4.8 (bumped mid-sweep — 2.4.7 when first read, re-read before writing) · sankoch 2.4.9→**2.7.6** · sigil 3.10.0→**3.12.2** · szal→2.1.0 · t-ron→2.1.8 · vidya 2.7.3→**2.8.0** · yantra→1.0.2 · yukti 2.2.7→**2.3.2**; Science: abaco→2.3.3 · avatara 2.7.2→**2.14.0** · ganita→1.0.4 · hisab 2.6.7→**2.8.4** · itihas→2.4.0 · sankhya 2.0.0→**3.0.0** (major); Media: dhvani 1.1.0→**2.2.1** (major) · garjan→**2.0.0** (major) · ghurni→**2.0.0** (major) · naad→2.1.1 · prani 1.1.0→**2.0.1** (major) · shabda→**3.0.1** (major) · shabdakosh→**3.0.2** (major) · shravan→2.6.7 · svara 3.0.0→**3.1.1**; Graphics: bhumi 1.0.0→**1.1.3** · bsp→1.2.1 · mehman→1.0.1; Language: varna→2.1.0 · vyakarana 2.2.3→**2.3.2**; Physics: prakash 1.2.0→**2.0.0** (major). **Two volatile counts deleted rather than re-guessed** — daimon's "144 MCP tools" and hoosh's "15 providers" were unverifiable at the new majors and are gone per the repo's don't-chase-a-drifting-number rule. **Forward-only, unchanged:** the GitHub-only Science / Physics / Language rows with no local clone (badal, bijli, bodh, brahmanda, dravya, falak, hisab-mimamsa, jantu, jivanu, jyotish, kana, khanij, kimiya, mastishk, pramana, rasayan, sangha, sharira, tara, vanaspati, kiran, raasta, impetus, pavan, pravash, ushma) — no live `VERSION` to read, so none was touched. Prior 2026-07-05: **akshara 1.0.0 + tyche 1.0.0 graduated** — clean multi-consumer-soak freezes of the last two pre-1.0 ML-substrate leafs (tokenizer + statistical PRNG); the attn11-extraction trio rosnet/tyche/akshara is now ALL v1.0+. Science & Knowledge 29→31, total 85→**87**. Same day: **rosnet 1.0.0→1.1.0** — conv2d/conv1d added, FD-gated, per-axis stride/pad: the modality axis is substrate-complete. **Two membership corrections caught in the sweep**: `ifran` removed from OS & Infrastructure — it is a BINARY, correctly registered in [binaries.md](../binaries.md) at 2.0.0 (the libs row was a stale 1.3.0 duplicate); `agnosai` removed — it is an UNPORTED Rust repo (a software-port-path Tier-A target, triaged 2026-07-05 as likely fold-not-port), never a v1.0+ Cyrius lib. OS & Infra 30→28; total 87→**85** (83 standalone + 2 folded). Prior 2026-07-04: **rosnet 1.0.0 graduated** — the ML-substrate freeze: sovereign dense f64 tensor algebra (the BLAS layer under attn11/tarka/tentib/prajna/rupantara/anukūlana), API frozen (`docs/api.md`, CPU + GPU profiles), benchmarks + audit captured, six consumers green at the cut → Science & Knowledge (28→29), total **86→87**. Prior 2026-07-03: **three-registry version-drift audit** in lockstep with the planning registry — swept against 113 local `VERSION` files. **2 graduations added here**: **bhumi 1.0.0** + **mehman 1.0.0** (the AGNOS-compositor platform + compat backends) → Graphics & Rendering (3→5); plus **cmdit 1.1.0** — a shipped-but-unregistered CLI/arg-parsing lib the completeness sweep caught — added to OS & Infrastructure (29→30). Total **83→86** libs. **25 version bumps**: OS/Infra — mabda 3.4.4→**4.0.2** (wgpu-native retirement) · sigil 3.9.2→**3.10.0** (added native UEFI Authenticode PE signing) · kavach 3.5.2→**3.7.0** · bote 2.7.6→**3.0.0** · hoosh 2.4.6→**2.4.11** · libro→2.7.10 · patra→1.12.7 · sankoch→2.4.9 · sakshi→2.4.3 · majra→2.5.0 · t-ron→2.1.7 · yukti→2.2.7 · phylax→1.2.3 · bayan→1.0.4 · aegis→1.1.3 · agnostik→1.3.3 · agnodrm→1.4.5; Science — abaco→2.3.1 · hisab→2.6.7; Media — goonj 1.4.3→**2.0.0** · nidhi 1.1.0→**2.0.0** · svara 2.0.0→**3.0.0** · naad 1.2.5→**2.1.0** · shravan→2.6.6; Graphics — bsp→1.2.0. Binary-tools pointer gained **mirshi** (graduated → [binaries.md](../binaries.md)). **Flagged, not changed:** stiva local `VERSION` 2.0.0 vs this doc's `—` ("Cyrius port pending") — needs a human status check. **Forward-only:** GitHub-only crates absent from the local clone. Prior 2026-07-02 (**tula 1.0.0 graduated** — sovereign ML weight-file format (M0 of the Type-3 weight-import chain), added to OS & Infrastructure (28→29 libs, total 82→83); **ganita 1.0.1 → 1.0.2** (`f64_tanh` NaN-overflow fix, surfaced by anukūlana's real GPT-2-small forward + folded into cyrius stdlib 6.3.31). The rest of the chain lives in the planning registry: rupantara 0.4.0 (pre-1.0 forward lib), anukūlana 0.2.0 (Type-3 reference binary), attn11 1.12.0. Prior 2026-06-23: **version-drift sweep** against local `VERSION` files, in lockstep with the planning registry — no new v1.0 graduations (every v1.0+ crate already documented here; tarka 1.0.0 stays a Non-Library reference binary by design). OS/Infra bumps: mabda 3.0.2→**3.4.4** · sigil 3.7.13→**3.9.2** · kavach 3.4.1→**3.5.2** · libro 2.7.3→**2.7.7** · patra 1.11.2→**1.12.3** · sankoch 2.3.1→**2.4.4** · sakshi 2.3.0→**2.4.1** · nein→1.5.4 · bote→2.7.6 · daimon→1.2.9 · hoosh→2.4.6 · majra→2.4.7 · t-ron→2.1.6 · ai-hwaccel→2.3.12 · yukti→2.2.6 · bayan→1.0.2 · agnostik→1.3.1; Science: abaco→2.3.0 · avatara→2.7.2 · hadara 1.0.0→**1.1.0** · hisab→2.6.6 · itihas→2.3.5; Graphics: bsp→1.1.4; Language: varna 1.0.0→**2.0.0**. Prior 2026-06-19 (**graduated-doc relocation**: moved the per-crate docs for **aegis / ark / mela / takumi** out of the old `docs/development/os/` planning area into this `libs/` tree, and renamed **agnosys.md → agnodrm.md** for the decomposition. ✅ Authored those 16 missing stubs the same day — agnostik, bayan, ganita, mihi, yantra + agora, bannermanor, commandress, cyim, cyim-lsp, darshini, hapi, iam, kii, kriya, sit — and removed the stale `os/nous` + `os/phylax` duplicates: **every graduated crate now has a doc in `libs/`.** Prior 2026-06-18: relocated the 4 v1.0 graduations from the development registry's pre-1.0 sections into the application area: **yantra 1.0.0** added here as an OS & Infrastructure lib; **ark / mela / takumi 1.0.0** added to the binary-tools pointer line — they live in the full planning registry. Prior 2026-06-14: full local-VERSION sweep — OS/Infra + Science + Language lib bumps synced to the 6.0→6.2 cyrius arc; **ganita + bayan added** as v1.0+ libs; binary-tools pointer refreshed for the sit + darshini graduations. GitHub-only science/media/physics libs not in the local clone left as-is. Prior 2026-06-04: version columns re-synced to the VERSION files — 17 stable-crate bumps. Prior 2026-05-22: post-1.31.6 close drift sweep.)

---

## OS & Infrastructure (31)

| Crate | Version | Domain |
|-------|---------|--------|
| aegis | 1.1.8 | Security daemon — absorbed agnosys's PAM at 1.1.0 (agnosys → agnodrm decomposition) |
| agnostik | 1.6.4 | Shared types & domain primitives (Cyrius, GitHub-release only) |
| agnodrm | 1.6.2 | Device / DRM model — udev + DRM/KMS (was **agnosys**; decomposed 2026-06-19: trust→sigil, sec/mac/audit→kavach, pam→aegis, logging→sakshi, syscall layer→cyrius) |
| agnosai | 2.1.0 | Agent orchestration engine — multi-agent crews over task DAGs, LLM routing, fleet distribution and sandboxed tool execution behind an HTTP API; the CrewAI replacement. **Cyrius since 2.0.0 (2026-08-14)** — this registry dropped it on 2026-07-05 as an unported Rust repo; the Rust tree survives only as `rust-old/` history. Ships a standalone server binary and a library: agnostic declares `[deps.agnosai]` (`dist/agnosai.cyr`), and thoth vendors its `[lib.guard]` output-safety profile |
| ai-hwaccel | 2.4.0 | GPU detection |
| bayan | 1.5.7 | Data-format, document & big-integer distfile — json/yaml/toml/cyml/csv/base64/pdf/bigint/u128 (PDF read/write since 1.5.0); folded byte-identical into the Cyrius stdlib as `lib/bayan.cyr` since the cyrius 6.1.25 carve (sandhi pattern) |
| bote | 3.3.13 | MCP core (~5us/message, streamable HTTP) |
| cmdit | 1.2.5 | Sovereign CLI / argument-parsing library (getopt-long shaped, zero external code) — the one place AGNOS tools register flags, parse argv, and print `--help`/`--version` instead of hand-rolling on the bare `args` primitive. API frozen at 1.0.0. Consumers declaring `[deps.cmdit]` (2026-09-27): anuenue, commandress, dig, ifran, kii, stiva |
| daimon | 2.4.3 | Agent orchestrator (MCP tool surface — count drifts per cut) |
| darshana | 1.1.2 | TTY / raw-mode primitives (दर्शन — *viewing/showing*) — termios raw/cooked via ioctl, ANSI escapes, cursor positioning, alt-screen, SGR color (16 / 256 / truecolor), `signalfd` delivery. Linux **and** AGNOS peers; **not** a TUI framework. API frozen at 1.0.0 (ADR 0003: 29 fns + 37 constants) |
| hoosh | 2.7.1 | LLM gateway |
| kavach | 3.13.1 | Sandbox execution — absorbed agnosys's security (Landlock/seccomp) + mac + audit at 3.5.0 (decomposition) |
| libro | 2.10.3 | Cryptographic audit chain |
| mabda | 4.1.5 | GPU foundation — consumes chitra for image decode; exposes the GPU compute surface that attn11 and ranga (its optional `gpu` feature) declare — puka stopped declaring it at 0.6.8 |
| majra | 2.9.1 | Queue/pub-sub |
| mihi | 1.2.7 | System-info probe library (CPU / RAM / GPU / kernel / uptime / distro / hostname) — substrate for iam, chakshu. Agnos CPU model via CPUID brand string (1.2.x) |
| nein | 1.7.0 | Programmatic nftables firewall |
| patra | 1.15.0 | Structured storage & SQL — B+ tree, WAL (Cyrius-native) |
| phylax | 1.2.7 | Threat detection — YARA, entropy, magic bytes, PE/ELF parsing, LLM triage via hoosh |
| sakshi | 2.5.5 | Tracing, error handling, structured logging (Cyrius-native) |
| samay | 1.1.5 | Task scheduler (समय — *time*) — real cron + resource-aware, ai-hwaccel-conscious task placement; priority queue (Normal/High/Critical/Emergency), JSON snapshot/restore with a security-audited restore path, deterministic scheduling. Rust→Cyrius port complete; consumable as the committed `dist/samay.cyr` bundle (`src/main.cyr` is an in-tree demo entry, excluded from `[lib]` — the kavach / ai-hwaccel convention). **1.0.0 tagged 2026-07-21; catalogued here 2026-08-05** (was 0.1.0 in the planning registry). Consumers declaring `[deps.samay]`: daimon (task scheduling), kavach (sandboxed execution, optional), stiva (`accel` feature) |
| samvada | 1.0.1 | Native **D-Bus client** (संवाद — *dialogue*) — speaks D-Bus over a raw unix socket in pure Cyrius: SASL `EXTERNAL`, framing, marshal/unmarshal, `SCM_RIGHTS` fd passing, and the logind session layer (`TakeControl` / `TakeDevice` / `ReleaseDevice`). The native path needs no libsystemd and no C shim (the libsystemd shim is kept as a fallback, slated for removal at 1.1.0). Public API frozen at 1.0.0 (2026-09-09), mechanically checked since 1.0.1. First consumer: mabda (`[deps.samvada]`, logind `TakeDevice` for its present path). ⚠ Per samvada's own README, a `TakeDevice` that hands over a real descriptor has not yet run on a seated session; single-threaded |
| sankoch | 2.8.0 | Lossless compression — LZ4, DEFLATE, zlib, gzip; since 2.4.x also xz/LZMA2 and bzip2, since 2.5.x zstd (decode, then encode), Brotli decode since 2.8.0 (2026-09-16), plus tar (2.5.0) and ZIP (2.6.0) containers |
| sigil | 3.13.3 | Trust verification & crypto — AES-NI + SHA-NI hardware accel (TLS 1.3 live-verified); absorbed agnosys's trust stack at 3.9.0 (decomposition) |
| stiva | 3.0.20 | OCI-compatible container runtime (Romanian *stivă* — stack/pile). ⭐ **The Rust→Cyrius port is complete** — the frozen Rust crate stays at `rust-old/` as the parity oracle; 34 of 36 CLI verbs live. Real OCI image layout, registry pull/push (bearer auth, multi-arch index, digest-verified streaming), `Stivafile` build with a fingerprinted layer cache, `oci-archive`/`docker-archive` interop, gzip+zstd layers with whiteouts; stateful container manager with detached `run -d` and `exec` via `nsenter`. Built on kavach / majra / nein / bote. ⚠ **Was catalogued here as `—` "Rust-era scaffold; port pending"** — corrected 2026-08-05 against the live repo; ⚠ its 36-verb CLI makes it arguably a [binaries.md](../binaries.md) entry, an operator call |
| szal | 2.2.0 | Workflow engine — step/flow/DAG + branching/retry/rollback. **Cyrius-native (2.0.0 = Rust → Cyrius port graduation)** |
| t-ron | 2.1.11 | MCP security |
| tula | 1.0.2 | **Sovereign ML weight-file format** (तुला — *balance/scale*) — safetensors/GGUF analog with a **sigil-signed header** they lack: 64B header + typed manifest (f64/int8/ternary/nf4) + 8B-aligned payload; builder/reader + heap & zero-copy-mmap read + Ed25519 sign/verify. Format v1 FROZEN (105 assertions + 2M-iter fuzz + security audit). M0 of the Type-3 chain. Consumers declaring `[deps.tula]` (2026-09-27): anukūlana (import) and ifran (its signed `.tula` model store); attn11/tentib checkpoints and the murti load-seam are named as planned users in tula's own docs |
| vidya | 2.8.5 | Programming reference |
| yantra | 1.0.5 | Sovereign UI automation (Cyrius library) — browser + mobile; `.tcyr` files include `lib/yantra.cyr` and drive Chromium / Firefox / WebKit / Android / iOS. `cyrius test` stays the runner (not a framework). 1.0.0 tagged 2026-06-16; catalogued here 2026-06-18 |
| yukti | 2.3.14 | Device abstraction (USB, block, udev) |

## Science & Knowledge (31)

| Crate | Version | Domain |
|-------|---------|--------|
| abaco | 2.4.6 | Math engine |
| avatara | 2.15.0 | Divine archetype overlay |
| akshara | 1.0.3 | **Sovereign tokenizer** (अक्षर — *indivisible text/sound unit*) — the text→token-id layer: adaptive byte vocab, opt-in BPE (pure i64, bit-reproducible), width-generic packed token store + streaming read. Third attn11 extraction (with rosnet/tyche). **1.0.0 (2026-07-05) is a clean freeze** — no behavior change; froze what attn11 (1060-suite) + tarka exercised unchanged since 2026-06-22 (`docs/api.md`; the checkpoint-serialized `g_*` tokenizer state freezes with the surface) |
| ganita | 1.2.7 | Linear algebra (matrix, linalg) + advanced math (transcendental + number theory); folded byte-identical into the Cyrius stdlib as `lib/ganita.cyr` since the cyrius 6.1.26 carve (sandhi pattern). 1.0.2 made `ganita_f64_tanh` saturate for \|x\|>20 (was inf/inf=NaN for \|x\|>~709 — surfaced by anukūlana's real GPT-2 forward through GELU x³; folded into cyrius stdlib at 6.3.31) |
| rosnet | 1.1.2 | **Sovereign dense f64 tensor algebra — the BLAS substrate under the whole ML family** (storage, BLAS-1, matmul + its hand-derived FD-gated gradient; no BLAS/libc/autodiff). Dual-profile: CPU `dist/rosnet.cyr` (mabda-free) + GPU `dist/rosnet-gpu.cyr` (`[lib.gpu]`, mabda-gated, bit-exact/~1e-13 vs the CPU oracle; provider coverage evolves under mabda — 4.0 added basic NVIDIA). **1.0.0 froze the surface six shipping consumers exercised** (attn11/tarka/tentib/prajna/rupantara/anukūlana); the no-bounds-checks caller-guarantees contract is the audited substrate design (`docs/api.md` + `docs/audit/`). **1.1.0 (2026-07-05) added conv2d/conv1d** (NCHW, per-axis stride/pad, hand-derived FD-gated gradients — the modality axis is now substrate-complete). Remaining additive lane: f32/f16/bf16 wideners (GGUF-triggered), pooling, im2col/SIMD conv optimization |
| tyche | 1.1.0 | **Sovereign deterministic statistical PRNG** (xorshift64 + splitmix64 seed finalizer + Marsaglia-polar normal) — **NOT a CSPRNG** (crypto → sigil). **1.0.0 (2026-07-05) is a clean freeze** — no behavior change; froze the 4-fn surface four consumers exercised unchanged (attn11, tarka, rosnet `t_randn`, anukūlana via rosnet); the `_rng_state` checkpoint-capture cell freezes with it; per-stream handles = the flagged SMP-arc additive (`docs/api.md`) |
| badal | 1.1.0 | Weather/atmosphere |
| bhava | 2.0.0 | Emotion/personality |
| bijli | 1.1.0 | Electromagnetism |
| bodh | 1.0.0 | Psychology |
| brahmanda | 1.0.0 | Galactic cosmology |
| dravya | 1.2.0 | Material science |
| falak | 1.0.0 | Orbital mechanics |
| hadara | 1.1.2 | Culture modeling (Cyrius-native, 50 cultures) |
| hisab | 3.2.1 | Higher math |
| hisab-mimamsa | 1.0.0 | Theoretical physics |
| itihas | 2.5.1 | World history |
| jantu | 1.1.0 | Ethology/behavior |
| jivanu | 1.0.0 | Microbiology |
| jyotish | 1.0.0 | Astronomical computation |
| kana | 1.1.0 | Quantum mechanics |
| khanij | 1.1.0 | Geology/mineralogy |
| kimiya | 1.1.1 | Chemistry |
| mastishk | 1.1.0 | Neuroscience |
| pramana | 1.2.0 | Statistics |
| rasayan | 1.0.0 | Biochemistry |
| sangha | 1.0.0 | Sociology |
| sankhya | 3.0.1 | Ancient math systems |
| sharira | 1.1.0 | Physiology |
| tara | 1.0.0 | Stellar astrophysics |
| vanaspati | 1.0.0 | Botany |

## Media & Audio (13)

| Crate | Version | Domain |
|-------|---------|--------|
| dhvani | 2.2.4 | Audio engine |
| garjan | 2.5.2 | Environmental sound |
| ghurni | 2.6.1 | Mechanical sound |
| goonj | 2.0.5 | Acoustics |
| naad | 2.2.4 | Audio synthesis |
| nidhi | 2.1.2 | Sample playback |
| prani | 2.0.13 | Creature vocals |
| shabda | 3.0.5 | G2P conversion |
| shabdakosh | 3.0.7 | Pronunciation dict |
| shravan | 2.8.1 | Audio codecs |
| svara | 3.5.5 | Vocal synthesis |
| tarang | 1.0.0 | Media framework (containers, decode/encode) |
| vani | 1.2.7 | Audio device I/O (वाणी — *voice/speech*) — direct ALSA ioctls: PCM playback + capture, format negotiation, pow-of-two ring buffer, XRUN recovery, mixer control. Library only, no CLI binary. Vendored into the Cyrius stdlib as `lib/vani.cyr`; the standalone repo continues for consumers needing newer surface than the folded snapshot. **1.0.0 tagged 2026-07-06; catalogued here 2026-08-05** (was 0.9.6 in the planning registry). Live consumers per vani's own README (2026-09-27): dhvani and jalwa declare `[deps.vani]`; cyrius-doom, mishran, cyrius-polyomino and cyrius-bb vendor the `core` shim (`vendor/vani-core.cyr`); shravan / naad / shruti / agnoshi are listed as planned, with no code calling vani yet. The agnos backend for the `audio_*` shim (the `snd_*` syscall band) has been in since 0.9.7 (2026-07-04, QEMU-validated) |

## Graphics & Rendering (8)

| Crate | Version | Domain |
|-------|---------|--------|
| bhumi | 1.4.5 | Sovereign compositor **platform** backend (भूमि — *ground*) — the DRM/KMS + libinput + logind replacement trio: output via agnos `fbinfo`#38 / `blit`#39, keyboard input via `kbscan`#42, pointer input via `ptrscan`#98 (since 1.1.4; wheel since 1.4.3), capability-gated single-seat (no logind/uids); since 1.2.0–1.4.0 (all 2026-08-12) Linux is a real target too (fbdev scanout, evdev keyboard + pointer). 70-fn frozen API. aethersafha sits directly on it. Graduated 2026-07-02 (was pre-1.0). ⚠ **1.1.3 is load-bearing on agnos** — agnos `#38 fbinfo` returns **0** on success (the display band's 0-ok convention) while this function's published contract has always promised *bytes written* (24); the agnos arm handed the kernel's 0 straight to callers, and every caller in the ecosystem tests `== 24`, so a good query read as failure: aethersafha fell back to its hardcoded 1280x720 on an 800x600 panel, discarding the kernel's real screen geometry on every agnos boot. The fix moves the translation into a pure `_bhumi_fbinfo_rc` the host can assert (invisible off agnos, which is why it survived every test) |
| bsp | 1.2.5 | BSP geometry (Cyrius-native) |
| chitra | 1.0.3 | CPU raster **image decoder** (चित्र — *image / picture*) — PNG (every bit depth × colour type, Adam7), baseline JPEG, BMP and GIF (first frame) → canonical RGBA8, pure Cyrius, decode-only; bounds on every read, decompression-bomb caps on every compressed path, and a budgeted decode entry. Public API/ABI frozen at 1.0.0 (2026-08-24). Its PNG core is a one-time fork of kii's decoder, which kii then adopted back. Consumers declaring `[deps.chitra]` (2026-09-27): mabda (`gpu_texture_load_png`), aethersafha, crab, jalwa, kii |
| kashi | 1.0.10 | AGNOS **console-font** subsystem (काशि — *shining*) — bitmap console fonts: a freestanding font-data core (VGA 8x16, CGA 8x8, VGA 9x16, full CP437 — no stdlib, heap or syscalls) that the agnos kernel includes directly, plus a stdlib-linked library face for runtime fonts (PSF1/PSF2, BDF, PCF, sidecar Unicode tables; consumable as `dist/kashi.cyr` since 1.0.6). Public API frozen at 1.0.0 (2026-05-28); 1.0.x since then is toolchain, test, packaging and docs work. Consumers: the agnos kernel, plus aethersafha, crab, dhancha, jalwa and puka (all on the freestanding core); thoth carries a vendored copy of that core (`src/vendor/kashi.cyr`) instead of declaring the dep |
| kiran | 1.0.0 | Game engine (ECS, scene hierarchy) |
| mehman | 1.0.3 | Sovereign compositor **compat / "swallow"** backend (مهمان — *guest*) — hosts foreign-ABI app surfaces as kavach-sandboxed guests (XWayland's actual job, done sovereign); orthogonal to bhumi the platform backend. Graduated 2026-07-03 (was pre-1.0) |
| ranga | 2.0.2 | Image processing (color, blend, GPU compute) |
| soorat | 1.0.0 | GPU rendering |

## Language & Navigation (3)

| Crate | Version | Domain |
|-------|---------|--------|
| raasta | 1.0.0 | Pathfinding |
| varna | 2.4.2 | Multilingual language engine |
| vyakarana | 2.4.2 | Source-code grammar + tokenizer (Cyrius-native, ten-kind palette, CYML grammars) |

## Physics & Engineering (6)

| Crate | Version | Domain |
|-------|---------|--------|
| impetus | 1.3.0 | Physics |
| pavan | 1.1.0 | Aerodynamics |
| prakash | 2.10.0 | Optics/light |
| pravash | 1.2.0 | Fluid dynamics |
| tanmatra | 1.3.0 | Atomic physics |
| ushma | 1.3.0 | Thermodynamics |

## Stdlib-Folded (2)

Sibling distfiles vendored byte-identical into the Cyrius stdlib `lib/`. Standalone repos remain for direct consumers needing newer surface than the folded snapshot; fixes land in the standalone repo first and reach the stdlib when a Cyrius release re-folds it. The stdlib's fold table also vendors bayan, ganita, mabda, patra, sakshi, sankoch, sigil, vani, yantra and yukti; those rows stay in their domain sections above.

| Crate | Folded At | Domain |
|-------|-----------|--------|
| sandhi | Cyrius v5.7.0 | Service-boundary layer — HTTP/HTTP2/WS/TLS/JSON/net (376 KB / 9,649 lines / 469 fns at the v5.7.0 fold). Set the fold pattern; sandhi repo entered maintenance mode per ADR 0002. |
| niyama | Cyrius v5.9.0 | Regex engines — bre / re2 / pcre / fuzzy / vim (6,664 lines / 7 modules at the v5.9.0 fold). Multi-consumer gate: cyim + queued AGNOS bare-metal kernel. |
