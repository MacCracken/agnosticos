# AGNOS Applications & Libraries

> Documentation for all first-party AGNOS software.
>
> See also: [Monolith Extraction](../archive/monolith-extraction.md) (archived 2026-05-12)

---

## Consumer Applications

Desktop, CLI, and service applications built on the AGNOS stack.

> **Status legend**: **Released** = ships and runs today; **Planned** = doc/spec exists, implementation pending. Per-app doc headers carry the authoritative status; this index reflects them as of 2026-09-27.
>
> ⚠ Most per-app docs date from a 2026-04 planning batch. Their **Agnoshi Intents** rows describe that era's planned per-app intents — the current (Cyrius) agnoshi has a fixed, generic intent set with no per-app intents. Their daimon API references (screen capture, anomaly baselines, …) likewise predate the Cyrius daimon, whose HTTP API covers agents, edge nodes, MCP, RAG, the scheduler, health and metrics. For the apps without a local checkout (GitHub-only), the rest of the doc has not been re-verified since.

| App | Domain | Status | Notes |
|-----|--------|--------|-------|
| [Abacus](abacus.md) | Desktop calculator | Released | Rust; built on the Rust abaco crate (abaco itself is Cyrius since 2.0.0) |
| [Aequi](aequi.md) | Self-employed accounting | Released | Tauri |
| [Agnostic](agnostic.md) | Agent automation platform | Released | Cyrius port on AgnosAI, pre-1.0; the Python/CrewAI line is its parity oracle |
| [BullShift](bullshift.md) | Trading platform | Released | |
| [Delta](delta.md) | Code hosting (git, PRs, CI/CD) | Released | Port 8070 |
| [Dhara](dhara.md) | Media streaming server | Planned | Port 8078, built on tarang |
| [Ifran](irfan.md) | ML training control plane | Released | Cyrius since 2.0.0 (was a Rust inference + training engine). Filename `irfan.md` predates the Synapse → Irfan → Ifran rename |
| [Jalwa](jalwa.md) | Media player | Released | Cyrius since 1.0.0; audio via shravan → dhvani → vani; video not yet ported (waits on tarang, still Rust) |
| [Mneme](mneme.md) | Knowledge base | Released | Cyrius port; `mneme serve` MCP endpoint |
| [Nazar](nazar.md) | System monitor | Released | Port 8095 |
| [Photis Nadi](photisnadi.md) | Productivity app | Released | |
| [Rahd](rahd.md) | Calendar & contacts | Released | |
| [Rasa](rasa.md) | Image editor | Released | |
| [SecureYeoman](secureyeoman.md) | Flagship AI agent platform | Released | Rust + TypeScript; backend migrating TypeScript → Rust |
| [Selah](selah.md) | Screenshot & annotation | Released | Rust library crate (no binary since 0.24.3) |
| [Shruti](shruti.md) | DAW (digital audio workstation) | Released | |
| [Sutra](sutra.md) | Infrastructure orchestrator | Released | |
| [Tazama](tazama.md) | Video editor | Released | |
| [Vidhana](vidhana.md) | System settings | Released | |

## Binaries & Tools (v1.0+)

Standalone v1.0+ binaries — the kernel, shell, init, package manager, build system, and the sovereign userland (coreutils, editors, viewers, terminal-aesthetics tools). See **[binaries.md](binaries.md)** for the full table.

## Shared Library Crates

Reusable libraries that form the stack. See **[libs/](libs/README.md)** for the full categorized index.

## Third-Party Packages

Packaged external software for AGNOS. See **[thirdparty/](thirdparty/)**.
