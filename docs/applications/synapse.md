# Synapse

> **Synapse** (Greek: connection) — LLM management and training tool

| Field | Value |
|-------|-------|
| Status | Released |
| Version | `2026.3.18-2` |
| Repository | `MacCracken/synapse` |
| Runtime | native-binary (~9.6MB) |
| Recipe | `recipes/marketplace/synapse.toml` |
| MCP Tools | 7 `synapse_*` |
| Agnoshi Intents | 7 |
| Port | 8080 |

---

## Why First-Party

Synapse is core infrastructure for hoosh — it manages model downloads, fine-tuning jobs, and serving configuration. No existing tool integrates with the AGNOS LLM pipeline end-to-end. It provides a unified interface across llama.cpp, Candle, Ollama, vLLM, ONNX, and TensorRT backends, with fine-tuning methods (LoRA, QLoRA, full, DPO, RLHF, distillation) that feed directly back into hoosh's model registry.

## What It Does

- Model lifecycle management: download, convert, quantize, serve across multiple backends
- Fine-tuning pipeline: LoRA, QLoRA, full fine-tuning, DPO, RLHF, and distillation
- Backend orchestration for llama.cpp, Candle, Ollama, vLLM, ONNX, and TensorRT
- Training job scheduling with GPU resource management
- Model catalog and version tracking with performance benchmarks

## AGNOS Integration

- **Daimon**: Registers as an agent; publishes model availability events; exposes training job status via API
- **Hoosh**: Direct integration as the model management backend; synapse-managed models are served through hoosh's inference gateway
- **MCP Tools**: `synapse_models`, `synapse_download`, `synapse_finetune`, `synapse_serve`, `synapse_status`, `synapse_benchmark`, `synapse_catalog`
- **Agnoshi Intents**: `synapse models`, `synapse download <model>`, `synapse finetune <config>`, `synapse serve <model>`, `synapse status`, `synapse benchmark <model>`, `synapse catalog`
- **Marketplace**: AI/Infrastructure category; sandbox profile allows GPU access, network for model downloads, read-write model storage directories

## Architecture

- **Crates**:
  - `synapse-core` — model registry, backend abstraction, configuration
  - `synapse-train` — fine-tuning pipeline (LoRA/QLoRA/full/DPO/RLHF/distillation)
  - `synapse-serve` — model serving, backend orchestration, health monitoring
  - `synapse-api` — REST API (port 8080), job management
  - `synapse-cli` — command-line interface
- **Dependencies**: CUDA/ROCm (GPU compute), llama.cpp (GGUF inference), SQLite (job database)

## Roadmap

Active development. Known review items R1-R7:
- R1: Stub RAG integration needs full implementation
- R2: Incomplete hoosh bridge for model hot-swap
- R3: Empty model catalog (seed with curated models)
- R4-R7: Training pipeline hardening, multi-GPU scheduling, checkpoint management
