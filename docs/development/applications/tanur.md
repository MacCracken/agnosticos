# Tanur — Desktop LLM Studio

> **Tanur** (Persian/Arabic: تنور — forge, kiln) — desktop GUI for model management, training, and inference on AGNOS

| Field | Value |
|-------|-------|
| Status | Scaffolded (0.1.0) |
| Priority | 2 — desktop experience for Irfan, LM Studio replacement |
| Repository | `MacCracken/tanur` |
| Runtime | native-binary (egui or iced) |
| Recipe | `recipes/marketplace/tanur.toml` |
| Domain | Desktop LLM management / training studio |

---

## Why First-Party

LM Studio is the gold standard for desktop LLM management — model browsing, one-click downloads, chat, GPU monitoring. But it's closed-source, Electron-based, and limited to inference. No training, no evaluation, no fleet management, no marketplace. It also can't integrate with an OS-level agent runtime or respect per-agent token budgets.

AGNOS already has Irfan — a full LLM management server with 21 REST endpoint groups covering model lifecycle, training (6 methods), evaluation (5 benchmarks), RLHF, experiments, RAG, fleet management, marketplace, distributed training, lineage tracking, and an OpenAI-compatible API. Irfan even has a nascent Tauri desktop crate (`ifran-desktop`) with 5 command modules.

**Tanur replaces `ifran-desktop` as a standalone consumer project.** It's a pure GUI client that connects to Irfan over Unix socket. All intelligence lives in Irfan (which embeds murti for model runtime and connects to hoosh for inference routing). Tanur just renders the experience.

This separation means Tanur can be installed independently, updated on its own release cycle, and run on machines where Irfan runs headless — connecting remotely to an Irfan instance on a GPU server.

## Design Principles

1. **Pure client** — Tanur has no model runtime, no inference engine, no training logic. It connects to Irfan over Unix socket and renders UI. All state lives server-side.
2. **Socket-first** — Default connection is `/run/agnos/irfan.sock`. TCP fallback for remote Irfan instances (`irfan.local:8420`).
3. **Real-time** — SSE streams for inference chunks, training progress, experiment trials, and fleet health. No polling.
4. **Native** — Rust + egui or iced. No Electron, no web runtime. Lightweight, fast startup.
5. **Progressive disclosure** — Model browsing and chat are front-and-center. Training, eval, RLHF, fleet are discoverable but not overwhelming.

## Architecture

### Connection Model

```
Tanur (GUI)
  └──socket──→ Irfan (server)
                  ├── murti (embedded, model lifecycle + backends)
                  └──socket──→ Hoosh (inference routing, budgets, cloud)
                                  └── murti (embedded, local inference)
```

Tanur connects to exactly one endpoint: Irfan's Unix socket (or TCP for remote). Everything flows through Irfan's API. Tanur never touches murti, hoosh, or model files directly.

### Irfan API Surface (what Tanur consumes)

Every panel in Tanur maps to existing Irfan endpoints:

#### Model Hub Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Browse local models | `GET /models` | Paginated, shows format/quant/size/params |
| Model details | `GET /models/{id}` | Architecture, SHA, path, pulled date |
| Pull model | `POST /models/pull` (planned) | Currently CLI-driven, needs streaming endpoint |
| Delete model | `DELETE /models/{id}` | |
| Discover local servers | `GET /models/discover` | Auto-find Ollama/LM Studio/LocalAI models |
| Search marketplace | `GET /marketplace/search` | Query, filter by format/size |
| Pull from peer | `POST /marketplace/pull` | Remote Irfan instances |
| Publish model | `POST /marketplace/publish` | Share to marketplace |

#### Chat Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Send message | `POST /v1/chat/completions` | OpenAI-compatible |
| Stream response | `POST /v1/chat/completions` `stream: true` | SSE chunks |
| System prompt | `system` field in request | Per-conversation |
| Temperature/top_p/top_k | Request params | Per-message or per-conversation |
| Multiple conversations | Client-side state | Irfan is stateless for chat |
| Model selector | `GET /models` | Switch model mid-conversation |

#### Training Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Create job | `POST /training/jobs` | Method, dataset, hyperparams, LoRA config |
| List jobs | `GET /training/jobs` | Filter by status |
| Job progress | `GET /training/jobs/{id}/stream` | SSE: step, loss, epoch, progress % |
| Cancel job | `POST /training/jobs/{id}/cancel` | |
| View checkpoints | `GET /training/jobs/{id}/checkpoints` | |
| View metrics | `GET /training/jobs/{id}/metrics` | Loss curves, learning rate schedule |
| Training events | `GET /system/training/events` | Global SSE stream |

#### Distributed Training Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Create distributed job | `POST /training/distributed/jobs` | Strategy: data/model/pipeline parallel |
| List distributed jobs | `GET /training/distributed/jobs` | |
| Assign workers | `POST .../workers` | Rank, endpoint, device IDs |
| Auto-place workers | `POST .../auto-place` | GPU affinity, load balanced, custom |
| Start distributed job | `POST .../start` | Requires all workers assigned |
| Aggregate checkpoints | `POST .../aggregate` | Average or weighted average |

#### Experiments Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Create experiment | `POST /experiments` | Hyperparameter search program |
| List experiments | `GET /experiments` | Filter by status |
| Experiment details | `GET /experiments/{id}` | Trials with scores |
| Trial leaderboard | `GET /experiments/{id}/leaderboard` | Ranked by objective |
| Stop experiment | `POST /experiments/{id}/stop` | |

#### Evaluation Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Create eval run | `POST /eval/runs` | MMLU, HellaSwag, HumanEval, perplexity, custom |
| List eval runs | `GET /eval/runs` | |
| Eval results | `GET /eval/runs/{id}` | Score, samples, duration per benchmark |
| Compare models | Client-side | Side-by-side eval results |

#### Dataset Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Preview dataset | `POST /datasets/preview` | |
| Validate dataset | `POST /datasets/validate` | |
| Auto-label | `POST /datasets/auto-label` | LLM-powered labeling |
| Augment | `POST /datasets/augment` | 5 strategies |
| Label job status | `GET /datasets/auto-label/jobs/{id}` | |

#### RAG Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Create pipeline | `POST /rag/pipelines` | Chunk size, overlap, embedding model |
| List pipelines | `GET /rag/pipelines` | |
| Ingest document | `POST /rag/pipelines/{id}/ingest` | Drag-and-drop files |
| Query with retrieval | `POST /rag/query` | Shows answer + source chunks with scores |
| Delete pipeline | `DELETE /rag/pipelines/{id}` | |

#### RLHF Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Create session | `POST /rlhf/sessions` | Model + name |
| List sessions | `GET /rlhf/sessions` | |
| Add comparison pairs | `POST /rlhf/sessions/{id}/pairs` | Batch upload |
| Annotate (A/B/Tie) | `POST /rlhf/pairs/{id}/annotate` | Side-by-side UI |
| Get next pair | `GET /rlhf/sessions/{id}/pairs` | Unannotated queue |
| Session stats | `GET /rlhf/sessions/{id}/stats` | Progress bar |
| Export annotations | `POST /rlhf/sessions/{id}/export` | For DPO training |

#### Fleet Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Register node | `POST /fleet/nodes` | ID, endpoint, GPU info |
| List nodes | `GET /fleet/nodes` | Filter by health |
| Node health | Heartbeat-derived | Online/Suspect/Offline |
| Fleet stats | `GET /fleet/stats` | Total nodes, GPUs, memory |
| Remove node | `DELETE /fleet/nodes/{id}` | |
| GPU telemetry | `GET /system/gpu/telemetry` | Utilization, memory, temperature |

#### Lineage Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| Record node | `POST /lineage` | Stage: DataPrep→Training→Eval→Deploy |
| View lineage | `GET /lineage` | Filter by stage |
| Ancestry DAG | `GET /lineage/{id}/ancestry` | Visual graph |
| Model versions | `GET /versions` | Family tree |
| Version lineage | `GET /versions/{id}/lineage` | Parent/child DAG |

#### System Panel
| Feature | Irfan Endpoint | Notes |
|---------|---------------|-------|
| System status | `GET /system/status` | Version, models, backends, hardware |
| Hardware info | `GET /system/status` → cpu/gpu | CPU cores, GPU name/VRAM |
| GPU telemetry | `GET /system/gpu/telemetry` | Real-time utilization/temp/memory |
| Health check | `GET /health`, `GET /ready` | |
| Bridge status | `GET /bridge/status` | SecureYeoman connection |

### Crate Structure

Tanur is a single-crate consumer project (flat, like tarang):

```
tanur/
├── Cargo.toml
├── VERSION
├── src/
│   ├── main.rs               # Entry point, app lifecycle
│   ├── app.rs                 # TanurApp — top-level egui/iced app state
│   ├── connection.rs          # IrfanConnection — Unix socket + TCP client
│   ├── stream.rs              # SSE stream handling (inference, training, events)
│   ├── panels/
│   │   ├── mod.rs
│   │   ├── models.rs          # Model Hub — browse, pull, delete, discover
│   │   ├── chat.rs            # Chat — conversations, streaming, model selector
│   │   ├── training.rs        # Training — job creation, progress, checkpoints
│   │   ├── distributed.rs     # Distributed Training — workers, placement, aggregation
│   │   ├── experiments.rs     # Experiments — hyperparameter search, leaderboard
│   │   ├── eval.rs            # Evaluation — benchmarks, comparison
│   │   ├── datasets.rs        # Datasets — preview, validate, auto-label, augment
│   │   ├── rag.rs             # RAG — pipelines, ingest, query
│   │   ├── rlhf.rs            # RLHF — annotation sessions, A/B comparison
│   │   ├── fleet.rs           # Fleet — node monitoring, GPU telemetry
│   │   ├── lineage.rs         # Lineage — DAG visualization, versioning
│   │   └── system.rs          # System — hardware, health, bridge status
│   ├── widgets/
│   │   ├── mod.rs
│   │   ├── chat_bubble.rs     # Message rendering (markdown, code blocks)
│   │   ├── progress_bar.rs    # Training/pull progress
│   │   ├── gpu_meter.rs       # VRAM/utilization gauge
│   │   ├── loss_chart.rs      # Training loss curve
│   │   ├── dag_viewer.rs      # Lineage DAG graph
│   │   └── model_card.rs      # Model info card (name, quant, size, params)
│   ├── theme.rs               # AGNOS theme integration (aethersafha theme_bridge)
│   └── config.rs              # TanurConfig — connection settings, UI preferences
└── assets/
    └── icons/                 # Panel icons
```

### Connection Layer

```rust
pub struct IrfanConnection {
    transport: Transport,
    base_url: String,          // Constructed from socket or TCP
}

pub enum Transport {
    Unix { path: PathBuf },    // /run/agnos/irfan.sock (default)
    Tcp { host: String, port: u16 },  // Remote: irfan.local:8420
}

impl IrfanConnection {
    /// Auto-detect: try socket first, fall back to localhost:8420
    pub async fn auto_connect() -> Result<Self>;

    /// Generic request — all panels use this
    pub async fn request<T: DeserializeOwned>(
        &self, method: Method, path: &str, body: Option<&impl Serialize>
    ) -> Result<T>;

    /// SSE stream — for inference, training progress, events
    pub async fn stream(
        &self, method: Method, path: &str, body: Option<&impl Serialize>
    ) -> Result<impl Stream<Item = Result<Event>>>;

    /// Health check
    pub async fn is_healthy(&self) -> bool;
}
```

## AGNOS Integration

- **Daimon**: Registers as agent for MCP tool access; no direct daimon API calls (everything through Irfan)
- **Hoosh**: No direct connection — inference flows through Irfan → hoosh socket chain
- **Aethersafha**: Theme bridge for consistent AGNOS look and feel
- **MCP Tools**: `tanur_open` (launch with panel), `tanur_chat` (send message), `tanur_status` (connection status), `tanur_models` (list models), `tanur_train` (create training job)
- **Agnoshi Intents**: `tanur open`, `tanur chat <prompt>`, `tanur train <model>`, `tanur pull <model>`, `tanur eval <model>`
- **Marketplace**: Desktop/AI category; sandbox profile allows Unix socket access to `/run/agnos/irfan.sock`, network for remote Irfan, no filesystem access (all files go through Irfan API)

## What Replaces `ifran-desktop`

Tanur supersedes the existing `ifran-desktop` Tauri crate. The mapping:

| `ifran-desktop` Command | Tanur Panel | Notes |
|---|---|---|
| `list_models()` | Models panel | Enhanced with search, filter, cards |
| `get_model(id)` | Model detail view | Full metadata + actions |
| `delete_model(id)` | Models panel delete action | Confirmation dialog |
| `pull_model(repo_id, quant)` | Models panel pull | Progress bar, registry browse |
| `send_message(...)` | Chat panel | Multi-conversation, streaming |
| `get_status()` | System panel | GPU meters, backend status |
| `get_hardware()` | System panel | No longer local — via Irfan API |
| `list_jobs()` | Training panel | With progress streams |
| `create_job(config)` | Training panel | Form-based job creation |
| `cancel_job(id)` | Training panel | |
| `list_sessions()` | RLHF panel | |
| `create_session(...)` | RLHF panel | |
| `get_next_pair(...)` | RLHF panel | Side-by-side comparison UI |
| `submit_annotation(...)` | RLHF panel | One-click A/B/Tie |
| `get_session_stats(...)` | RLHF panel | Progress visualization |
| `export_session(...)` | RLHF panel | |
| *(not in ifran-desktop)* | Experiments panel | **New** |
| *(not in ifran-desktop)* | Evaluation panel | **New** |
| *(not in ifran-desktop)* | Datasets panel | **New** |
| *(not in ifran-desktop)* | RAG panel | **New** |
| *(not in ifran-desktop)* | Fleet panel | **New** |
| *(not in ifran-desktop)* | Distributed training panel | **New** |
| *(not in ifran-desktop)* | Lineage panel | **New** |
| *(not in ifran-desktop)* | Marketplace panel | **New** |

Once Tanur reaches feature parity, `ifran-desktop` can be removed from the Irfan workspace.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `eframe` / `egui` | Native GUI framework (or `iced` — TBD) |
| `reqwest` | HTTP client (Unix socket + TCP) |
| `tokio` | Async runtime |
| `serde` + `serde_json` | API serialization |
| `eventsource-client` | SSE stream parsing |
| `hyper-unix-connector` | Unix socket HTTP transport |
| `chrono` | Timestamps |
| `tracing` | Structured logging |

No murti dependency. No ai-hwaccel dependency. Tanur is a pure client.

## Security

- **No model access**: Tanur never touches model files. All model operations go through Irfan's API.
- **Socket permissions**: `/run/agnos/irfan.sock` is permission-controlled. Tanur runs under user context.
- **No network by default**: Marketplace and remote fleet features require explicit network sandbox permission.
- **No secrets**: API keys for cloud providers live in hoosh's config, not Tanur.

## Roadmap

### Phase 1 — Core Experience
- [ ] `IrfanConnection` with Unix socket + TCP transport
- [ ] Models panel (list, detail, delete, discover)
- [ ] Chat panel (multi-conversation, streaming, model selector)
- [ ] System panel (hardware, health, GPU telemetry)
- [ ] AGNOS theme integration
- [ ] Marketplace recipe
- [ ] Tests: 50+

### Phase 2 — Training Studio
- [ ] Training panel (create, progress stream, checkpoints, metrics, cancel)
- [ ] Experiments panel (create, leaderboard, stop)
- [ ] Evaluation panel (run benchmarks, compare models)
- [ ] Datasets panel (preview, validate, auto-label, augment)
- [ ] Loss curve chart widget
- [ ] Model pull with progress bar (requires Irfan streaming pull endpoint)

### Phase 3 — Advanced
- [ ] RLHF panel (annotation sessions, A/B comparison UI)
- [ ] RAG panel (pipelines, drag-and-drop ingest, query with sources)
- [ ] Distributed training panel (worker assignment, auto-placement, aggregation)
- [ ] Fleet panel (node monitoring, GPU telemetry gauges)
- [ ] Lineage panel (DAG visualization, version tree)
- [ ] Marketplace panel (search, publish, pull from peers)
- [ ] MCP tools + agnoshi intents
- [ ] Remove `ifran-desktop` from Irfan workspace

---

*Last Updated: 2026-03-21*
