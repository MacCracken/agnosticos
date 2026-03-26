# Hoosh

> **Hoosh** (Persian: intelligence, the word for AI) — AI inference gateway

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `0.25.3` |
| Repository | `MacCracken/hoosh` |
| Runtime | library crate (Rust) |
| Recipe | `recipes/marketplace/hoosh.toml` |
| crates.io | [hoosh](https://crates.io/crates/hoosh) |
| Port | 8088 |

---

## What It Does

- OpenAI-compatible API gateway routing to 15 LLM providers (Ollama, llama.cpp, OpenAI, Anthropic, Google, DeepSeek, Mistral, Grok, Groq, OpenRouter, LM Studio, LocalAI, OpenCode, Letta, Synapse)
- Local model serving with automatic hardware detection via ai-hwaccel
- Speech-to-text and text-to-speech routing
- Token budget management (reserve, check, report, release) with named pools
- Response caching, per-agent rate limiting, and cert pinning

## Consumers

- Every AGNOS component needing LLM inference (daimon, agnoshi, agnostic, all AI-enabled apps)
- Consumer projects use hoosh as the single entry point for all AI capabilities

## Architecture

- Multi-provider router with fallback chains and priority ordering
- Streaming support (SSE) for chat completions
- Acceleration layer for local models (GGUF, ONNX)
- Dependencies: tokio, reqwest, serde, ai-hwaccel

## Roadmap

Stable — published on crates.io. Future: model quantization pipeline, provider health scoring, federated inference across edge nodes.
