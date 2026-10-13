# Archive

> Historical documentation preserved for provenance. **Not authoritative for current state** — every file here describes AGNOS as it was before one of three inflection points: the Cyrius pivot (2026-04-04), the monolith extraction (2026-04-01), or an earlier rename/refactor.

## Why keep these

- **Narrative receipts.** The project's pivot from Rust-bootstrapped to sovereign Cyrius-from-seed is itself part of the story. Articles reference these docs as the "before" side of the before/after.
- **Naming archaeology.** Subsystems that were renamed (or retired) leave traces here. If a reader finds an old reference to `nein` or `majra` in a commit message or article, the archive says what those were.
- **No authoritative claims.** Nothing in `archive/` should be linked to as current documentation. When a historical context is needed, link to the specific archived file, not the directory.

## Organization

```
docs/archive/
├── README.md                            (this file)
├── *-rust-era.md                        (root-level: pre-Cyrius Rust-era docs)
├── *-pre-cyrius.md                      (root-level: planning docs that predated Cyrius)
├── cyrius-lang-migration.md             (root-level: the migration roadmap itself)
├── libs-pre-cyrius/                     (lib docs before the Cyrius port)
├── os-pre-cyrius/                       (OS-layer docs before the Cyrius port)
└── os-modules-pre-extraction/           (OS modules before the 2026-04-01 monolith extraction)
```

## Root-level files

| File | What it was | Archived | Superseded by |
|------|-------------|----------|---------------|
| `AGNOS-rust-era-2026-04-03.md` | Overall AGNOS description, Rust era | 2026-04-13 | `../AGNOS.md` + the development roadmap |
| `agent-runtime-rust-era.md` | Daimon agent runtime spec (Rust) | 2026-04-07 | `MacCracken/daimon` repo |
| `agnostik-roadmap-pre-cyrius.md` | Agnostik 0.90→1.0 roadmap, pre-Cyrius | 2026-04-03 | `MacCracken/agnostik` repo (now v0.97.1 Cyrius-ported) |
| `api-readme-rust-era.md` | AGNOS API reference, Rust-era | 2026-03-11 | Pending — full API re-docs land with Phase 13A |
| `cyrius-lang-migration.md` | The migration planning doc that triggered the pivot | 2026-04-04 | Complete — `MacCracken/cyrius` is the successor. Kept as the "before" artifact of the pivot decision. |
| `desktop-environment-rust-era.md` | aethersafha desktop environment spec, Rust era | 2026-03-11 | `MacCracken/aethersafha` (pending Cyrius port) |
| `example_claude-rust-era.md` | CLAUDE.md template for Rust-era sibling repos | 2026-04-13 | Each active repo now maintains its own Cyrius-era CLAUDE.md |
| `first-party-standards-rust-era.md` | First-party application standards, Rust era | 2026-04-08 | development planning docs + per-repo CONTRIBUTING.md |
| `license-fixes-rust-era.md` | 42-item license-cleanup TODO checklist (Cargo.toml, crates.io, SPDX-string fixes) | 2026-05-06 | Moot post-Cyrius pivot — `cyrius.cyml` replaces Cargo.toml, ark/zugot replaces crates.io. Per-repo LICENSE accuracy is per-repo housekeeping. |
| `monolith-extraction.md` | The extraction roadmap doc — code extraction completed 2026-04-01 but the doc kept describing "reassembly in progress" through April-15 | 2026-05-12 | live ecosystem state + ISO-assembly + iron-boot development docs |
| `end-of-4x-independent-audit.md` | Neutral-infra cold-clone audit from 2026-04-14 finding "Bootstrap ✓ / Kernel Boot ✓" | 2026-05-12 | Bootstrap finding still holds; Kernel-Boot finding contradicted on real iron 2026-05-12 (GRUB rejected `e_shoff=0` ELF; repair in Cyrius v5.11.29/.30/.31). |

## `libs-pre-cyrius/` — Library docs before the Cyrius port

Each file documents a library as it existed in Rust before being ported (or retired) under Cyrius. Where a current successor exists, it lives as a standalone repo at `MacCracken/<name>`.

> **Successor-column accuracy note (2026-05-12 sweep)**: prior cleanup passes labeled several entries "Retired" based on local-filesystem absence (`/home/macro/Repos/<name>/` missing). On a fresh-flashed devbox where only a handful of repos are recloned, that's a false-positive signal. The 2026-05-12 sweep `curl`'d every "Retired" entry against `github.com/MacCracken/<name>` — **all of them were alive**. The Successor column below reflects the corrected verdict. **Rule for future audits**: treat the GitHub remote as authoritative over local-filesystem absence.

| File | Successor |
|------|-----------|
| `abaco.md` | `MacCracken/abaco` v2.1.0 (Cyrius, +12× Miller-Rabin) |
| `ai-hwaccel.md` | `MacCracken/ai-hwaccel` v2.0.0 (Cyrius, 131 crates → 0) |
| `bote.md` | `MacCracken/bote` v2.5.1 (Cyrius) |
| `hadara.md` | `MacCracken/hadara` v1.0.0 (Cyrius-**native**, never re-ported — predates Cyrius transition) |
| `hoosh.md` | `MacCracken/hoosh` v2.0.0 (Cyrius, 40 crates → 0) |
| `itihas.md` | `MacCracken/itihas` v2.2.0 (Cyrius) |
| `kavach.md` | `MacCracken/kavach` v3.0.0 (Cyrius, 448 crates → 1) |
| `libro.md` | `MacCracken/libro` v2.0.5 (Cyrius) |
| `mabda.md` | `MacCracken/mabda` — **currently at 3.0.0-rc.2 soaking pre-GA**; pinned at 2.5.0 in cyrius stdlib `[deps.mabda]`. 3.0 GA fold conditionally lands in cyrius v5.11.x close window (24-hour soak gate). Earlier "Folded into Cyrius stdlib v3.4.19" line was incorrect — fold has not happened yet. |
| `majra.md` | **Not retired** (correction 2026-05-12) — `MacCracken/majra` alive on GitHub, last pushed 2026-05-11 (Concurrency Queue). Cyrius-ported. Treat as active ecosystem member. |
| `nein.md` | **Not retired** (correction 2026-05-12) — `MacCracken/nein` alive on GitHub, last pushed 2026-05-11 (nftables firewall in Rust → Cyrius port). |
| `shravan.md` | `MacCracken/shravan` v2.3.2 (Cyrius) |
| `soorat.md` | **Not retired** (correction 2026-05-12) — `MacCracken/soorat` alive on GitHub, last pushed 2026-03-30 (Rust era rendering engine). Cyrius port pending — same status as `stiva` below. |
| `stiva.md` | **Not retired** (correction 2026-05-12) — `MacCracken/stiva` alive on GitHub, last pushed 2026-04-29 (Rust era container runtime). Cyrius port pending. Live successor doc: [`../applications/libs/stiva.md`](../applications/libs/stiva.md). |
| `szal.md` | **Not retired** (correction 2026-05-12) — `MacCracken/szal` alive on GitHub, last pushed 2026-04-29 (Rust era DAG workflow engine). Cyrius port pending. |
| `t-ron.md` | `MacCracken/t-ron` v2.0.0 (Cyrius) |
| `vidya.md` | `MacCracken/vidya` (Cyrius-ported, alive — last pushed 2026-05-12. Reference library, separately maintained.) |
| `yukti.md` | `MacCracken/yukti` v2.2.2 — **both folded into Cyrius stdlib `lib/yukti.cyr` (v5.8.65) AND still maintained as a standalone repo** (last pushed 2026-05-11). The stdlib copy is vendored byte-identical per the sandhi pattern; the repo continues for non-stdlib consumers. |

## `os-pre-cyrius/` — OS-layer docs before the Cyrius port

| File | Successor |
|------|-----------|
| `agnoshi.md` | `MacCracken/agnoshi` v1.0.0 (Cyrius) |
| `argonaut.md` | `MacCracken/argonaut` v1.2.0 (Cyrius) |
| `bote.md` | `MacCracken/bote` v2.5.1 (duplicate — original lived at both layers) |
| `daimon.md` | `MacCracken/daimon` v1.1.1 (Cyrius) |
| `libro.md` | `MacCracken/libro` v2.0.5 (duplicate) |
| `nein.md` | **Not retired** (correction 2026-05-12) — `MacCracken/nein` alive on GitHub (duplicate entry; see libs-pre-cyrius table for canonical status) |
| `t-ron.md` | `MacCracken/t-ron` v2.0.0 (duplicate) |
| `yukti.md` | Cyrius stdlib (duplicate) |

## `os-modules-pre-extraction/` — OS modules before the 2026-04-01 monolith extraction

Module-level docs as they existed inside the `userland/` Cargo workspace, before every module was extracted into its own repo on 2026-04-01. Files here describe modules that lived together in a single tree; the live documentation now lives in each repo's own `README.md` + `CLAUDE.md`.

Covered: ai-hwaccel, hoosh, kavach, mabda, majra, sigil, soorat, stiva, szal.

> **Status correction 2026-05-12**: `stiva` is NOT retired (earlier entry above said it was). GitHub remote exists, Cyrius port pending. See [`../applications/libs/stiva.md`](../applications/libs/stiva.md) for the live spec.

See [`monolith-extraction.md`](monolith-extraction.md) (archived 2026-05-12) for the extraction event itself.

## Policy

### When to add a doc here
- A doc becomes inaccurate because of a structural change (pivot, rename, extraction, retirement)
- The doc has historical value (narrative continuity, naming archaeology) that makes deletion inappropriate

### When NOT to add a doc here
- A doc is merely stale — edit it in place, don't archive
- A doc describes current but unfinished work — leave it active with a status banner

### When to restore from archive
- Essentially never. If the state described here ever returns (unlikely), write a new doc referencing the archived one as prior art rather than copying forward.

### When to delete from archive
- Per-file: essentially never. The whole point is preservation. Deletion only makes sense if a file was archived by mistake (e.g., a current doc with an unfortunate filename).
- **Rust-era purge (planned):** after a few tagged GA releases past Beta — once the Rust-era and monolith context stops being actively referenced in day-to-day work — the entire Rust-era `docs/archive/` is deleted in a single commit. The preservation guarantee moves to git: the immediately-prior tag (`archive-final-v<N>`) becomes the canonical pointer, and historical refs use `git show <tag>:docs/archive/<file>` or a permalink to that tag's tree on GitHub.

### After the purge

`docs/archive/` may be recreated if — and only if — a Cyrius-era doc undergoes a structural supersession severe enough that the new doc can't carry the old context. The bar is **higher** than the Rust era: Rust-era archival was a bulk "the world changed" event; post-purge archival is a rare "this specific supersession is historically significant" event. Expected post-purge contents: near-zero. Normal doc updates edit in place.
