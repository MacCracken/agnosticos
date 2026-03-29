# Ifran

> **Ifran** (Persian: deep knowledge) — Local LLM inference, training, and fleet management platform

| Field | Value |
|-------|-------|
| Status | Stable |
| Version | `1.1.0` |
| Repository | `MacCracken/ifran` |
| Runtime | native-binary (server + CLI) |
| License | AGPL-3.0-only |

---

## What It Does

- Local LLM inference with 16 backend feature flags: llama.cpp, Candle, GGUF, ONNX, TensorRT, TPU, vLLM, Ollama, WASM, Gaudi, Inferentia, Metal, Vulkan, oneAPI, Qualcomm, XDNA
- Training pipeline: LoRA, QLoRA, full fine-tuning, DPO, RLHF, distillation
- Model evaluation: MMLU, perplexity, custom benchmarks
- Fleet management: multi-node coordination, GPU scheduling
- REST + gRPC API server (with `server` feature)
- Model registry, versioning, lineage tracking, and lifecycle management
- A/B testing, drift detection, scoring, and experiment tracking
- RAG integration and dataset management
- Hardware acceleration detection via ai-hwaccel
- OpenTelemetry OTLP trace export
- SQLite storage (default) with optional PostgreSQL and Redis backends
- hoosh integration for LLM gateway bridging

## Consumers

- **hoosh** — LLM gateway (delegates local inference to ifran backends)
- **tanur** — desktop LLM studio (GUI client connecting over Unix socket)
- **daimon** — agent orchestrator (model lifecycle management)
