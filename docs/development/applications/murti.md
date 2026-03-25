# Murti — Local Model Runtime Engine

> **Murti** (Sanskrit: मूर्ति — form, embodiment, manifestation) — core model lifecycle and inference engine for AGNOS

| Field | Value |
|-------|-------|
| Status | Scaffolded (0.1.0) |
| Priority | 1 — foundational crate for hoosh + Irfan, Ollama replacement |
| Crate | `murti` (crates.io, available) |
| Repository | `MacCracken/murti` |
| Runtime | library crate (no binary, no HTTP server) |
| Domain | LLM model storage, inference backends, GPU allocation |

---

## Why First-Party

Ollama is a monolithic Go binary that bundles model management, inference, and API into one process. It has no awareness of agent fleets, token budgets, hardware acceleration profiles, or sandboxed execution. More critically, it can't be composed — you either use all of Ollama or none of it.

AGNOS needs model runtime capabilities in two very different contexts:

- **Hoosh** (gateway, port 8088) needs to route local inference alongside 15 cloud providers, with caching, rate limiting, and token budgets. It doesn't need a CLI, training, or a desktop UI.
- **Irfan** (desktop app, port 8420) needs the same model lifecycle plus training, evaluation, marketplace, fleet management, and a full CLI/GUI experience.

Today, Irfan has all of this implemented in `ifran-core` and `ifran-backends` — model registry, 15 inference backends, 10 hardware accelerator families, pull with resume, quantization. But hoosh can't use any of it without depending on Irfan as a running service.

**Murti extracts the core engine from Irfan into a shared crate.** Both hoosh and Irfan depend on murti. Irfan becomes the desktop experience layer; hoosh becomes the gateway layer. The engine is shared.

## Design Principles

1. **Library, not service** — murti is a Rust crate with no HTTP server, no CLI, no binary. Consumers (hoosh, Irfan) own their own API surfaces.
2. **Extract, don't rewrite** — murti's initial code comes directly from `ifran-core` (model registry, store, pull, lifecycle) and `ifran-backends` (inference backends, GPU allocation). This is a refactor, not a greenfield project.
3. **VRAM-aware by default** — murti queries ai-hwaccel for available VRAM and automatically determines GPU layer offloading. Consumers can override.
4. **Backend-agnostic** — the `InferenceBackend` trait abstracts over llama.cpp, vLLM, TensorRT, Candle, and hardware-specific runtimes. New backends are feature-gated.
5. **Model configs are TOML** — consistent with AGNOS conventions. No custom DSL.

## Architecture

### The Stack

```
┌─────────────────────────┐  ┌──────────────────────────────────┐
│         Hoosh           │  │             Irfan                 │
│    (LLM Gateway)        │  │     (Desktop LLM Manager)        │
│  port 8088              │  │  port 8420                        │
│                         │  │                                   │
│  - OpenAI-compat API    │  │  - CLI (pull/run/list/train/eval) │
│  - 15 cloud providers   │  │  - Desktop GUI                    │
│  - Caching              │  │  - Training pipeline              │
│  - Rate limiting        │  │  - Evaluation benchmarks          │
│  - Token budgets        │  │  - Marketplace                    │
│  - Per-agent accounting  │  │  - Fleet management              │
│  - Cloud fallback       │  │  - RLHF / distillation           │
│                         │  │  - SecureYeoman bridge            │
├─────────────────────────┤  ├──────────────────────────────────┤
│    MurtiProvider        │  │  ifran-core (thin wrapper)        │
│  (impl LlmProvider)    │  │  ifran-backends → re-exports      │
└────────┬────────────────┘  └──────────┬───────────────────────┘
         │                              │
         └──────────┬───────────────────┘
                    │
         ┌──────────▼──────────────────┐
         │          Murti              │
         │   (Core Model Runtime)      │
         │                             │
         │  - ModelRegistry            │
         │  - ModelStore               │
         │  - PullManager              │
         │  - InferenceEngine          │
         │  - GpuAllocator             │
         │  - ModelPool                │
         │  - BackendManager           │
         │  - ModelConfig              │
         │  - OllamaCompat            │
         └──────────┬──────────────────┘
                    │
         ┌──────────▼──────────────────┐
         │       ai-hwaccel            │
         │  (GPU detection crate)      │
         └─────────────────────────────┘
```

### What Moves Where

| Current Location | Becomes | Notes |
|---|---|---|
| `ifran-core` model registry | `murti::registry` | HuggingFace, OCI, direct URL, local, Ollama library |
| `ifran-core` model store + pull | `murti::store`, `murti::pull` | Content-addressable blobs, resume, SHA-256/BLAKE3 |
| `ifran-core` lifecycle (load/unload) | `murti::engine` | Process management, health checks, crash recovery |
| `ifran-core` hardware detection | `murti::gpu` | VRAM queries, layer splitting, multi-GPU |
| `ifran-backends` (all 15) | `murti::backends` | Trait + feature-gated implementations |
| `ifran-core` quantization | `murti::quantize` | 15 quantization levels |
| `ifran-core` fleet model distribution | `murti::fleet` | Edge push, dedup, VRAM-aware placement |
| `ifran-core` RAG / embeddings | stays in `ifran-core` | Consumer-specific, not core runtime |
| `ifran-core` multi-tenancy | stays in `ifran-core` | Consumer-specific |
| `ifran-train` (all training) | stays in `ifran-train` | Irfan-specific |
| `ifran-api` (REST/gRPC) | stays in `ifran-api` | Irfan-specific |
| `ifran-bridge` (SY bridge) | stays in `ifran-bridge` | Irfan-specific |
| `ifran-cli` | stays in `ifran-cli` | Irfan-specific (wraps murti for model ops) |

### Crate Structure

```
murti/
├── Cargo.toml
├── src/
│   ├── lib.rs                # Public API: Murti struct, MurtiConfig
│   ├── registry.rs           # ModelRegistry — resolve names to download URLs
│   │                         #   Sources: HuggingFace, OCI, Ollama library, local, custom
│   ├── store.rs              # ModelStore — content-addressable local storage
│   │                         #   /var/lib/agnos/models/ layout, TOML index, dedup
│   ├── pull.rs               # PullManager — async download, resume, integrity
│   │                         #   HTTP range requests, SHA-256/BLAKE3, progress channel
│   ├── quantize.rs           # Quantizer — 15 GGUF quantization levels
│   │                         #   f32, f16, bf16, q8_0, q6k, q5_k_m, q5_k_s, q4_k_m,
│   │                         #   q4_k_s, q4_0, q3_k_m, q3_k_s, q2k, iq4_xs, iq3_xxs
│   ├── engine.rs             # InferenceEngine — backend process lifecycle
│   │                         #   Spawn, health check, graceful shutdown, crash recovery
│   ├── backends/
│   │   ├── mod.rs            # InferenceBackend trait
│   │   ├── llama_cpp.rs      # llama-server management (default)
│   │   ├── ollama.rs         # Ollama API client (compat/migration)
│   │   ├── vllm.rs           # vLLM backend
│   │   ├── tensorrt.rs       # TensorRT-LLM
│   │   ├── candle.rs         # Candle (pure Rust, future)
│   │   ├── gguf.rs           # Direct GGUF loading (future)
│   │   ├── onnx.rs           # ONNX Runtime
│   │   ├── tpu.rs            # Google TPU
│   │   ├── gaudi.rs          # Intel Gaudi
│   │   ├── inferentia.rs     # AWS Inferentia/Trainium
│   │   ├── oneapi.rs         # Intel Arc (OneAPI)
│   │   ├── qualcomm.rs       # Qualcomm AI 100
│   │   ├── metal.rs          # Apple Metal
│   │   ├── vulkan.rs         # Vulkan compute
│   │   └── xdna.rs           # AMD XDNA (Ryzen AI)
│   ├── gpu.rs                # GpuAllocator — VRAM queries + layer splitting via ai-hwaccel
│   ├── pool.rs               # ModelPool — multi-model LRU by VRAM budget
│   ├── config.rs             # ModelConfig — TOML model definitions
│   ├── fleet.rs              # FleetDistributor — edge push, dedup, VRAM-aware placement
│   └── compat.rs             # OllamaCompat — import models + convert Modelfiles
```

### Key Types (Public API)

```rust
/// Top-level entry point — consumers create one Murti instance
pub struct Murti {
    registry: Arc<ModelRegistry>,
    store: Arc<ModelStore>,
    engine: Arc<InferenceEngine>,
    pool: Arc<ModelPool>,
    gpu: Arc<GpuAllocator>,
    config: MurtiConfig,
}

impl Murti {
    /// Pull a model from registry into local store
    pub async fn pull(&self, name: &str) -> mpsc::Receiver<PullProgress>;

    /// Ensure model is available (pull if needed) and load into engine
    pub async fn ensure_loaded(&self, model_id: &str) -> Result<LoadedModel>;

    /// Run inference (auto-loads if needed via pool)
    pub async fn infer(&self, model_id: &str, request: &InferenceRequest) -> Result<InferenceResponse>;

    /// Stream inference
    pub async fn infer_stream(&self, model_id: &str, request: InferenceRequest)
        -> Result<mpsc::Receiver<Result<String>>>;

    /// List models in local store
    pub async fn list_stored(&self) -> Result<Vec<StoredModel>>;

    /// List currently loaded/running models
    pub async fn list_loaded(&self) -> Result<Vec<LoadedModel>>;

    /// Unload a model from engine (keeps in store)
    pub async fn unload(&self, model_id: &str) -> Result<()>;

    /// Remove a model from store (unloads first if loaded)
    pub async fn remove(&self, model_id: &str) -> Result<()>;

    /// Import Ollama models into murti store
    pub async fn import_ollama(&self) -> Result<Vec<StoredModel>>;

    /// Get GPU allocation status
    pub async fn gpu_status(&self) -> Result<GpuStatus>;

    /// Re-quantize a stored model
    pub async fn quantize(&self, model_id: &str, level: QuantLevel) -> Result<StoredModel>;
}
```

### How Irfan Changes

Irfan's `ifran-core` and `ifran-backends` become thin wrappers around murti:

```toml
# ifran-core/Cargo.toml
[dependencies]
murti = { version = "0.21", features = ["all-backends"] }
```

```rust
// ifran-core — model operations delegate to murti
pub struct ModelManager {
    murti: Arc<murti::Murti>,
    // Irfan-specific additions:
    lineage: LineageTracker,       // Dataset → training → eval → deployment tracking
    tenancy: TenantManager,       // Multi-tenant resource isolation
    marketplace: Marketplace,     // Model publishing/discovery
}

impl ModelManager {
    pub async fn pull(&self, name: &str, tenant: &TenantId) -> Result<...> {
        // Tenant quota check, then delegate
        self.tenancy.check_storage_quota(tenant)?;
        self.murti.pull(name).await
    }

    pub async fn infer(&self, model_id: &str, request: &InferenceRequest) -> Result<...> {
        // Lineage tracking, then delegate
        self.lineage.record_inference(model_id);
        self.murti.infer(model_id, request).await
    }
}
```

What stays Irfan-only (not in murti):
- **Training** — LoRA, QLoRA, full fine-tune, DPO, RLHF, distillation (`ifran-train`)
- **Evaluation** — MMLU, HellaSwag, HumanEval, perplexity (`ifran-core` eval module)
- **RAG pipeline** — chunking, embedding, retrieval (`ifran-core` rag module)
- **Multi-tenancy** — per-tenant isolation, GPU budgets
- **Lineage tracking** — dataset → training → eval → deployment chain
- **Marketplace** — model publishing, peer-to-peer sharing
- **RLHF annotation** — human feedback sessions
- **Fleet management** — node registration, health states (uses murti's `FleetDistributor` for model push)
- **SecureYeoman bridge** — bidirectional gRPC delegation
- **CLI** — `ifran pull/run/train/eval/serve` (wraps murti for model ops)
- **REST/gRPC API** — port 8420 (wraps murti for model endpoints)
- **Desktop GUI** — model browser, training dashboard

### How Hoosh Changes

Hoosh replaces `LlamaCppProvider` and `OllamaProvider` with a single `MurtiProvider`:

```rust
// hoosh providers.rs — replaces LlamaCppProvider + OllamaProvider

pub struct MurtiProvider {
    murti: Arc<murti::Murti>,
}

#[async_trait]
impl LlmProvider for MurtiProvider {
    async fn infer(&self, request: &InferenceRequest) -> Result<InferenceResponse> {
        self.murti.infer(&request.model, request).await
    }

    async fn infer_stream(&self, request: InferenceRequest)
        -> Result<mpsc::Receiver<Result<String>>>
    {
        self.murti.infer_stream(&request.model, request).await
    }

    async fn load_model(&self, model_id: &str) -> Result<ModelInfo> {
        let loaded = self.murti.ensure_loaded(model_id).await?;
        Ok(loaded.into())
    }

    async fn unload_model(&self, model_id: &str) -> Result<()> {
        self.murti.unload(model_id).await
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let models = self.murti.list_stored().await?;
        Ok(models.into_iter().map(Into::into).collect())
    }
}
```

Hoosh gains local model management without owning any model lifecycle code. The existing cloud providers (OpenAI, Anthropic, Google, etc.) remain unchanged. Hoosh's routing logic can now seamlessly fall back from local (murti) to cloud when local inference is saturated.

**Hoosh CLI gains** (thin wrappers around murti):
```
hoosh pull mistral:7b-q4          # murti.pull()
hoosh models                       # murti.list_stored()
hoosh ps                           # murti.list_loaded()
hoosh rm mistral:7b-q4            # murti.remove()
hoosh import-ollama               # murti.import_ollama()
```

**Hoosh API gains** (new endpoints):
```
GET    /v1/models/local             # List models in store
POST   /v1/models/pull              # Trigger async pull (SSE progress stream)
DELETE /v1/models/:name             # Remove from store
GET    /v1/models/running           # List loaded models with GPU allocation
POST   /v1/models/import/ollama     # Import Ollama models
GET    /v1/models/gpu               # GPU allocation status
```

## What Ollama Does vs What We Replace It With

| Capability | Ollama | AGNOS (murti + hoosh + Irfan) |
|---|---|---|
| Model download/pull | `ollama pull` | **murti** — registry + pull + store |
| Model storage | `~/.ollama/models/` | **murti** — `/var/lib/agnos/models/` content-addressable |
| GGUF quantization | Via Modelfile `FROM` | **murti** — 15 quantization levels |
| Inference backends | llama.cpp only (CGo) | **murti** — 15 backends (llama.cpp, vLLM, TensorRT, Metal, Vulkan...) |
| GPU layer splitting | Auto VRAM | **murti** — ai-hwaccel, multi-GPU, 10 accelerator families |
| Model config | Modelfile (custom DSL) | **murti** — TOML configs |
| Multi-model serving | Auto load/unload | **murti** — LRU pool by VRAM budget |
| OpenAI-compatible API | Yes (port 11434) | **hoosh** (port 8088) — already done |
| Streaming | Yes | **hoosh** — already done |
| Multi-provider fallback | No | **hoosh** — 15 cloud providers + local murti |
| Per-agent rate limiting | No | **hoosh** — already done |
| Token budgets | No | **hoosh** — already done |
| Training | No | **Irfan** — LoRA, QLoRA, DPO, RLHF, distillation |
| Evaluation | No | **Irfan** — MMLU, HellaSwag, HumanEval, perplexity |
| Desktop GUI | No | **Irfan** — model browser, training dashboard |
| Fleet distribution | No | **murti** fleet + daimon edge |
| Marketplace | No | **Irfan** — model publishing + discovery |
| Sandboxed inference | No | **murti** + agnosys Landlock/seccomp |
| Edge fleet | No | **murti** fleet + daimon edge module |

## Model Storage Layout

```
/var/lib/agnos/models/
├── index.toml                     # Model index (name:tag → blob SHA)
├── blobs/
│   ├── sha256-ab3f...             # Content-addressable GGUF files
│   └── sha256-cd91...
├── configs/
│   ├── mistral-7b-q4.toml        # Per-model configuration
│   └── llama3-8b.toml
└── tmp/                           # In-progress downloads (resume support)
```

### TOML Model Config

```toml
# configs/mistral-7b-q4.toml

[model]
name = "mistral"
tag = "7b-q4_K_M"
base = "mistral-7b-instruct-v0.2"
quantization = "Q4_K_M"

[parameters]
temperature = 0.7
top_p = 0.9
top_k = 40
repeat_penalty = 1.1
context_length = 8192

[system]
prompt = "You are a helpful assistant."

[gpu]
layers = "auto"                    # "auto", "none", or explicit count
vram_limit_mb = 4096               # Optional per-model VRAM cap

[serve]
timeout_secs = 300                 # Unload after idle
batch_size = 512
threads = 4                        # CPU threads (for non-GPU layers)

[backend]
prefer = "llama-cpp"               # Backend preference (auto-selected if omitted)
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `ai-hwaccel` | GPU detection, VRAM queries, accelerator profiles |
| `reqwest` | HTTP downloads (HuggingFace API, model blobs) |
| `tokio` | Async runtime, process management |
| `serde` + `toml` | Model config parsing |
| `sha2` + `blake3` | Integrity verification (dual hash) |
| `chrono` | Timestamps |
| `tracing` | Structured logging |
| `thiserror` | Error types |

Backend-specific deps are feature-gated. The llama.cpp binary is a system dependency managed by `recipes/ai/llama-cpp.toml`.

## Feature Flags

```toml
[features]
default = ["llama-cpp"]
llama-cpp = []
ollama-compat = []
vllm = []
tensorrt = []
candle = []
onnx = []
metal = []
vulkan = []
tpu = []
gaudi = []
inferentia = []
oneapi = []
qualcomm = []
xdna = []
all-backends = ["llama-cpp", "ollama-compat", "vllm", "tensorrt", "candle",
                "onnx", "metal", "vulkan", "tpu", "gaudi", "inferentia",
                "oneapi", "qualcomm", "xdna"]
fleet = []                         # Edge fleet distribution
```

Hoosh uses `murti = { features = ["llama-cpp"] }` (lightweight). Irfan uses `murti = { features = ["all-backends", "fleet"] }` (full power).

## Security

- **Sandboxed inference**: Backend processes spawned by murti run under Landlock + seccomp via agnosys. Read-only access to model blobs, no network.
- **Integrity verification**: All downloaded models verified by SHA-256 + BLAKE3 before use.
- **No arbitrary code execution**: GGUF is a data format. Murti validates magic bytes before loading.
- **Registry sources**: Configured by consumer (hoosh config or Irfan config), not user-supplied at pull time.
- **Data sensitivity routing**: Model configs can declare sensitivity level; murti respects routing constraints.

## Migration Path

### Phase 0 — Extract (refactor, no new features)
- [ ] Create `murti` crate from `ifran-core` model registry, store, pull, lifecycle, GPU allocation
- [ ] Move `ifran-backends` into `murti::backends` (all 15 backends)
- [ ] `ifran-core` depends on `murti`, delegates model ops
- [ ] All existing Irfan tests pass (murti is an extraction, not a rewrite)
- [ ] Publish `murti` v0.21.0 to crates.io

### Phase 1 — Hoosh Integration
- [ ] `MurtiProvider` in hoosh replaces `LlamaCppProvider` + `OllamaProvider`
- [ ] `hoosh pull/models/ps/rm` CLI commands
- [ ] `/v1/models/*` API endpoints in hoosh
- [ ] Hoosh integration tests with murti
- [ ] Remove dead `LlamaCppProvider` and `OllamaProvider` from hoosh

### Phase 2 — Polish
- [ ] `OllamaCompat` import (models + Modelfile → TOML conversion)
- [ ] `ModelPool` LRU eviction by VRAM budget
- [ ] Edge fleet model distribution via `murti::fleet` + daimon
- [ ] MCP tools: `murti_pull`, `murti_list`, `murti_status`, `murti_recommend`
- [ ] Agnoshi intents: "pull llama3", "what models are loaded", "recommend a model for code"

### Phase 3 — Advanced Inference (see also: Roadmap Phase 17)
- [ ] Candle backend (pure Rust GGUF runtime — no llama.cpp dependency)
- [ ] Speculative decoding (draft + verify model pairs)
- [ ] LoRA adapter hot-swap (switch adapters without reloading base model)
- [ ] Safetensors → GGUF on-pull conversion

### Phase 4 — Activation Sparsity (PowerInfer-inspired, Roadmap Phase 17A)

Exploit neuron activation locality to run large models (40B–175B) on consumer GPUs. Inspired by [PowerInfer](https://github.com/Tiiny-AI/PowerInfer) but integrated across the AGNOS stack (agnosys, ai-hwaccel, hoosh).

- [ ] Neuron activation profiler — offline analysis to identify hot/cold neuron sets per layer
- [ ] Sparse FFN operators — CPU (AVX2/NEON) and GPU (CUDA/ROCm) kernels that skip inactive neurons
- [ ] Adaptive neuron predictor — lightweight model bundled alongside weights, predicts per-input activation
- [ ] GPU-CPU hybrid split — hot neurons persistent on GPU, cold neurons computed on CPU on-demand
- [ ] PowerInfer GGUF import — read PowerInfer-format models with embedded predictor weights
- [ ] TurboSparse conversion — `murti quantize --sparsify` for SwiGLU→ReLU model conversion (watching upstream maturity)

**Limitation**: Currently only viable for ReLU-family activation models. TurboSparse extends this to SwiGLU models but is not yet production-quality for all architectures. Track progress in Roadmap Phase 17 Maturity Watch List.

**Why murti can outperform PowerInfer**: PowerInfer runs on generic Linux with a llama.cpp fork. Murti coordinates with agnosys (huge-page buffers, GPU memory pinning), ai-hwaccel (NUMA-aware placement, thermal monitoring), and hoosh (cloud fallback when local inference saturates). Full-stack co-optimization is not possible in a standalone engine.

### Phase 5 — System Co-optimization (Roadmap Phase 17C)
- [ ] Huge-page model buffers via agnosys (2MB/1GB pages, reduced TLB misses)
- [ ] GPU memory pinning (persistent hot neuron allocation survives idle)
- [ ] NUMA-aware cold neuron placement (GPU-local NUMA node for CPU fallback)
- [ ] Inference-priority cgroup profiles via argonaut
- [ ] Thermal-aware load shedding to cloud via hoosh
- [ ] Edge-optimized profiles for constrained devices (RPi, Pocket Lab-class)

---

*Last Updated: 2026-03-24*
