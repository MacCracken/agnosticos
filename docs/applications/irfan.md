# Ifran

> **Ifran** (Arabic: knowledge/wisdom — عرفان, *gnosis*) — the AGNOS **ML training control plane** (formerly Synapse → Irfan → Ifran). Through 1.x it was a Rust LLM inference & training engine; the Rust → Cyrius port shipped as **2.0.0** (2026-07-05) and the Rust line was removed after 2.2.0. Its v1.0+ registry row is in [binaries.md](binaries.md).

| Field | Value |
|-------|-------|
| Status | Released (Cyrius; frozen 2.x surface) |
| Version | See [`VERSION`](https://github.com/MacCracken/ifran/blob/main/VERSION) |
| Repository | `MacCracken/ifran` |
| Runtime | native-binary (Cyrius CLI) |
| Recipe | `zugot/marketplace/ifran.cyml` |
| MCP Tools | None yet — a bote-MCP surface is a roadmap lane (the Rust line's `ifran_*` tools were not carried) |
| Agnoshi Intents | None (the current agnoshi has no per-app intents) |
| Port | N/A — CLI. The Rust line's REST API (port 8080) was not carried; its SecureYeoman bridge contract survives as reference docs in the repo |

---

## Why First-Party

The sovereign ML siblings (attn11, tarka, tentib, prajna, anukūlana) prove the learning primitives; ifran owns **the orchestration around a training run — never the math**. It drives their binaries as recorded jobs and holds everything around them: signed model artifacts, datasets, sweeps, evals and preference data.

*Through 1.x* the Rust line was instead a multi-backend engine — llama.cpp, Candle, Ollama, vLLM, ONNX and TensorRT backends, with LoRA/QLoRA/full/DPO/RLHF/distillation fine-tuning feeding hoosh's model registry. The Cyrius port deliberately owns none of that: serving is hoosh's, foreign inference engines are mehman's, and GPU is mabda / ai-hwaccel's.

## What It Does

- **Jobs** — CYML job specs run as fork + pipe + execve with full output capture, exit-code propagation, quoted args and `timeout_s` enforcement; a patra-backed run store (`ifran run` / `runs` / `show` / `reap`)
- **Model store** — signed `.tula` artifacts: structural validation, Ed25519 verification against the operator key (`ifran keys`), sha256 content addressing with dedup, tamper-detecting re-verification
- **Datasets** — content-addressed text corpora with exact-line dedup deriving curated children; jobs reference datasets by id
- **Sweeps** — grid or seeded-random expansion of a job template, every combo a first-class run, with a post-hoc leaderboard (`sweep best`)
- **Evals** — sibling gates recorded as benchmarks: the exit code is the gate, with an optional metric extracted from the run log
- **Preferences** — DPO/IPO pairs and KTO thumbs, exported as JSONL for tarka's preference surface

## AGNOS Integration

- **ML siblings**: the port's acceptance proof ran attn11 training (a job plus a sweep), anukūlana's fidelity oracle as a gated eval, and tarka's full gate suite — all as recorded ifran jobs
- **tula + sigil**: every ingested artifact is validated and verified through tula; Ed25519 + sha256 come from sigil
- **patra**: the run, model, dataset, eval and preference stores
- **cmdit**: CLI verb dispatch and generated help
- **Not owned, by design**: serving (hoosh), lineage (itihas), marketplace (mela), fleet (seema), GPU (mabda / ai-hwaccel), foreign inference engines (mehman)

## Architecture

- **Modules** (`src/`): `jobspec`, `run`, `runstore`, `keys`, `store`, `dataset`, `sweep`, `eval`, `pref`, `main`
- **Dependencies**: patra, sigil, tula, cmdit, plus the Cyrius stdlib — see the repo's `cyrius.cyml`
- The Rust workspace (`ifran-core` / `-train` / `-serve` / `-api` / `-cli`; CUDA/ROCm, llama.cpp, SQLite) was held at `rust-old/` as the parity oracle, then removed after a zero-loss pre-removal audit; it survives in git history

## Roadmap

The 2.x surface is frozen; post-2.0 work is additive lanes, each opened by a trigger — see the repo's [`docs/development/roadmap.md`](https://github.com/MacCracken/ifran/blob/main/docs/development/roadmap.md). The bote-MCP interface waits on SecureYeoman re-wiring off its HTTP proxy to the Rust ifran. The Rust line's review items (stub RAG, hoosh model hot-swap bridge, empty catalog, training-pipeline hardening) went with that line.
