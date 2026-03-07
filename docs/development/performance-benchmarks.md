# Performance Benchmarks

> **Last Updated**: 2026-03-07

## Overview

AGNOS includes a benchmark suite built on [Criterion.rs](https://github.com/bheisler/criterion.rs) (v0.5) to measure and track performance across four core userland packages. Benchmarks live in `benches/` directories alongside their crates and are compiled as separate binaries (`harness = false`).

The suite is split into two tiers:

| Tier | Purpose | Count |
|------|---------|-------|
| **Micro-benchmarks** | Isolated operations -- ID generation, serialization, parsing | 36 |
| **System-level benchmarks** | End-to-end flows with async I/O, concurrency, and real subsystem interaction | 10 |

**Total: 46 benchmarks** across 5 bench files in 4 packages.

## Running Benchmarks

### All benchmarks

```bash
cargo bench --workspace
```

### Per-package

```bash
cargo bench -p agnos-common
cargo bench -p agent_runtime
cargo bench -p ai_shell
cargo bench -p llm_gateway
```

### Filter by name

```bash
cargo bench -p agent_runtime -- "agent_id"
cargo bench -p agent_runtime -- "system/"
```

### Save and compare baselines

```bash
cargo bench -- --save-baseline before-change
# ... make changes ...
cargo bench -- --baseline before-change
```

### Fast CI mode (no plots, shorter measurement)

```bash
cargo bench -- --noplot --warm-up-time 1 --measurement-time 3
```

Reports are written to `target/criterion/`. Open `target/criterion/report/index.html` for the overview dashboard.

## Micro-Benchmarks

### agnos-common (9 benchmarks)

File: [`userland/agnos-common/benches/agnos_common.rs`](../../userland/agnos-common/benches/agnos_common.rs)

| Benchmark | What it measures |
|---|---|
| `agent_id_new` | `AgentId::new()` -- UUID v4 generation |
| `agent_id_to_string` | `AgentId` to `String` conversion |
| `agent_id_display` | `AgentId` via `Display` trait (format!) |
| `inference_request_serialize` | `InferenceRequest` to JSON string via serde |
| `inference_request_deserialize` | JSON string to `InferenceRequest` via serde |
| `agent_config_default` | `AgentConfig::default()` construction |
| `agent_config_new` | Full `AgentConfig` with `AgentType::Service` + permissions |
| `serde_json_to_string` | `AgentConfig` serialized to JSON `String` |
| `serde_json_to_vec` | `AgentConfig` serialized to JSON `Vec<u8>` |

### agent-runtime (11 benchmarks)

File: [`userland/agent-runtime/benches/bench.rs`](../../userland/agent-runtime/benches/bench.rs)

| Benchmark | What it measures |
|---|---|
| `agent_id_generation` | `AgentId::new()` via `black_box` |
| `agent_config_creation` | Full `AgentConfig` struct construction |
| `orchestrator_task_creation` | `Task` struct with UUID, chrono timestamp, JSON payload |
| `agent_handle_clone` | Clone cost of `AgentHandle` (String + DateTime + ResourceUsage) |
| `task_priority_ordering` | Priority enum vector creation (Critical through Background) |
| `task_result_serialize` | `TaskResult` to JSON serialization |
| `task_payload_json_parse` | Nested JSON payload deserialization to `serde_json::Value` |
| `agent_status_clone` | `AgentStatus` enum clone |
| `resource_usage_default` | `ResourceUsage::default()` construction |
| `agent_handle_creation` | Full `AgentHandle` construction with UUID + timestamp |
| `task_result_creation` | Full `TaskResult` construction with UUID + JSON result |

### ai-shell (9 benchmarks)

File: [`userland/ai-shell/benches/ai_shell.rs`](../../userland/ai-shell/benches/ai_shell.rs)

| Benchmark | What it measures |
|---|---|
| `interpreter_parse_simple` | Parse 8 natural-language commands (list, cd, copy, processes, etc.) |
| `interpreter_parse_list_files` | Single parse: "show me all files in /home" |
| `interpreter_parse_cd` | Single parse: "go to /tmp" |
| `interpreter_translate_list_files` | Translate `Intent::ListFiles` to shell command string |
| `interpreter_translate_cd` | Translate `Intent::ChangeDirectory` to shell command string |
| `interpreter_explain_ls` | Explain `ls` command (no args) |
| `interpreter_explain_cat` | Explain `cat /etc/hosts` command |
| `interpreter_explain_rm` | Explain `rm -rf /tmp/test` command |
| `interpreter_parse_10_commands` | Parse 10 mixed shell commands sequentially |

### llm-gateway (7 benchmarks)

File: [`userland/llm-gateway/benches/llm_gateway.rs`](../../userland/llm-gateway/benches/llm_gateway.rs)

| Benchmark | What it measures |
|---|---|
| `cache_key_generation` | Format-string cache key from model + prompt + temperature + top_p + max_tokens |
| `hashmap_insert_100` | Insert 100 String key-value pairs into a fresh HashMap |
| `hashmap_lookup_100` | Look up 100 keys in a pre-populated 100-entry HashMap |
| `json_parse_small` | Deserialize 3 JSON strings to `InferenceRequest` |
| `token_usage_default` | `TokenUsage::default()` construction |
| `token_usage_calculation` | Set prompt/completion tokens and sum total |
| `response_serialize` | `InferenceResponse` to JSON serialization |

## System-Level Benchmarks

### agent-runtime (10 benchmarks)

File: [`userland/agent-runtime/benches/system_bench.rs`](../../userland/agent-runtime/benches/system_bench.rs)

These benchmarks use a Tokio runtime and exercise real async code paths including `Arc<RwLock>` contention, mpsc channels, and `tokio::spawn` concurrency.

**Agent lifecycle**

| Benchmark | What it measures |
|---|---|
| `system/agent_lifecycle_create_register_unregister` | Full cycle: `Agent::new()` + registry register + unregister |
| `system/agent_create` | `Agent::new()` only (includes IPC channel setup) |
| `system/registry_register_unregister/{1,10,50}` | Register + unregister N agents; throughput scaling via `Throughput::Elements` |

**IPC**

| Benchmark | What it measures |
|---|---|
| `system/ipc_messagebus_roundtrip` | Subscribe 2 agents, send A->B via `send_to`, then B->A; measures full round-trip |
| `system/ipc_broadcast/{1,5,20}` | Publish one message to N subscribers; measures fan-out cost |

**Orchestrator**

| Benchmark | What it measures |
|---|---|
| `system/orchestrator_submit/{1,10,100}` | Submit N Normal-priority tasks; measures queue throughput |
| `system/orchestrator_submit_mixed_priorities_50` | Submit 50 tasks across all 5 priority levels (Critical, High, Normal, Low, Background) |
| `system/orchestrator_submit_and_result` | Submit task + store `TaskResult` + retrieve result by ID |

**Resource management**

| Benchmark | What it measures |
|---|---|
| `system/resource_memory_reserve_release` | Reserve 64 MB then release via `ResourceManager`; measures lock cycle |
| `system/resource_memory_concurrent/{1,5,20}` | N concurrent `tokio::spawn` tasks each reserving 1 MB; measures contention scaling |

Parameterized benchmarks (those with `/{N}` suffixes) use Criterion `BenchmarkGroup` with `Throughput::Elements` so reports display ops/sec scaling curves.

### llm-gateway (planned)

The following system-level benchmarks are planned for the llm-gateway package:

- **Cache throughput** -- sustained insert + lookup under concurrent load
- **Cache hit/miss ratio** -- measure lookup latency for hits vs. misses
- **Token accounting** -- concurrent token usage accumulation across requests
- **Provider selection** -- health-aware fallback with `ProviderHealth` retry logic
- **Inference pipeline** -- end-to-end: request parse, cache check, provider call, response format
- **Cache cleanup** -- eviction performance under memory pressure

### desktop-environment (micro-benchmarks)

File: `userland/desktop-environment/benches/` (Criterion suite)

The desktop-environment benchmark suite covers compositor and rendering performance:

| Benchmark | What it measures |
|---|---|
| `frame_render_empty` | Baseline frame render with no surfaces |
| `frame_render_surfaces` | Frame render with N active surfaces |
| `surface_commit` | Wayland surface commit processing |
| `damage_tracking` | Damage region calculation and merge |
| `accessibility_tree_build` | AccessibilityTree construction from surface tree |
| `theme_bridge_convert` | AGNOS theme to Flutter ThemeData conversion |
| `plugin_host_dispatch` | Plugin event dispatch latency |
| `high_contrast_render` | High-contrast mode rendering overhead |

### ai-shell (planned)

The following system-level benchmarks are planned for the ai-shell package:

- **Session lifecycle** -- create session, execute commands, teardown
- **Multi-command pipeline** -- throughput of N commands in sequence
- **Prompt rendering** -- latency of prompt string construction
- **Intent classification** -- batch classification accuracy and speed
- **History search** -- search across large session history
- **Explain pipeline** -- end-to-end: parse command, translate, generate explanation

## Performance Targets

Key performance indicators for alpha readiness (Q2 2026):

| Metric | Target | Current Estimate | Status |
|---|---|---|---|
| Agent spawn time (create + register) | < 500 ms | ~300 ms | On track |
| Shell response time (local parse + translate) | < 100 ms | ~50 ms | On track |
| Memory overhead (idle, 0 agents) | < 2 GB | ~1.2 GB | On track |
| IPC round-trip (MessageBus send_to) | < 1 ms | ~20 us | On track |
| Cache lookup (HashMap get) | < 100 us | Sub-100 us | On track |
| Orchestrator submit (single task) | < 5 ms | ~50 us | On track |
| Memory reserve + release cycle | < 1 ms | ~10 us | On track |
| Boot time (kernel + userland init) | < 10 s | N/A | Requires hardware |

Current estimates were measured on an AMD Ryzen 9 7950X / 64 GB DDR5 development machine. These will be replaced with CI-tracked medians once automated regression tracking is in place.

## CI Integration

### Current state

- Benchmarks run locally via `cargo bench`.
- Criterion writes HTML reports to `target/criterion/` with automatic comparison against the previous run.
- The CI pipeline (`.github/workflows/ci.yml`) includes a benchmark step with `continue-on-error: true` so fluctuations do not block merges.
- Benchmark output is uploaded as a CI artifact for manual review.

### Planned: Criterion HTML report archiving

Archive Criterion reports as CI artifacts on every merge to `main`. This provides a browsable history of performance changes without external tooling.

### Planned: bencher.dev integration

Track benchmark results over time with automated regression alerts:

```yaml
- name: Run benchmarks
  run: cargo bench --workspace -- --output-format bencher | tee results.txt
- name: Upload to Bencher
  uses: bencherdev/bencher@v0.4
  with:
    command: bencher run --project agnos --file results.txt
```

### Planned: Regression thresholds

Once baselines are stable, configure hard thresholds -- flag CI failure when any benchmark regresses by more than 15% compared to the rolling baseline.

### Tips for reliable CI results

- Use dedicated runners or reduce sample size (`--sample-size 20`) to minimize noise on shared runners.
- Run benchmarks in a separate CI job from unit tests to avoid resource contention.
- Use `--warm-up-time 1 --measurement-time 3` for faster CI cycles when absolute numbers are less important than relative change detection.

## Contributing

### Adding a micro-benchmark

1. Open the relevant `benches/*.rs` file for the package.
2. Add a function following the established pattern:
   ```rust
   fn benchmark_my_operation(c: &mut Criterion) {
       c.bench_function("my_operation_name", |b| {
           b.iter(|| {
               black_box(my_operation());
           });
       });
   }
   ```
3. Register it in the `criterion_group!` macro at the bottom of the file.
4. Run `cargo bench -p <package> -- "my_operation_name"` to verify.
5. Update the benchmark tables in this document.

### Adding a system-level benchmark

1. Add the benchmark to the appropriate `benches/system_bench.rs` file. If one does not exist for the package, create it and add a `[[bench]]` entry in `Cargo.toml`:
   ```toml
   [[bench]]
   name = "system_bench"
   harness = false
   ```
2. Use `tokio::runtime::Runtime::new().unwrap()` outside the closure and `rt.block_on(async { ... })` inside `b.iter()`.
3. For parameterized benchmarks, use `BenchmarkGroup` with `Throughput::Elements`:
   ```rust
   let mut group = c.benchmark_group("system/my_group");
   for n in [1, 10, 100] {
       group.throughput(Throughput::Elements(n as u64));
       group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
           b.iter(|| { /* ... */ });
       });
   }
   group.finish();
   ```
4. Prefix all system benchmark names with `system/` for easy filtering.

### Naming conventions

| Category | Pattern | Example |
|---|---|---|
| Micro-benchmark | `<noun>_<operation>` | `agent_id_new`, `task_result_serialize` |
| System benchmark | `system/<subsystem>_<operation>` | `system/ipc_broadcast`, `system/orchestrator_submit` |
| Parameterized | auto-appended `/{N}` via `BenchmarkId` | `system/registry_register_unregister/50` |
