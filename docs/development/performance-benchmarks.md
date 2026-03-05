# AGNOS Performance Benchmarks

This document describes the benchmark infrastructure, how to run and interpret
benchmarks, current performance targets, and the plan for CI regression tracking.

## Overview

AGNOS uses [Criterion.rs](https://github.com/bheisler/criterion.rs) (v0.5) for
all Rust benchmarks. Benchmarks live in the `benches/` directory of each
workspace crate and are compiled as separate binaries with `harness = false`.

There are two tiers of benchmarks:

| Tier | Purpose | Example |
|------|---------|---------|
| **Micro** | Measure individual operations (ID generation, serialization, parsing) | `agent_id_generation`, `inference_request_serialize` |
| **System** | Measure end-to-end flows across multiple subsystems | `system/agent_lifecycle_create_register_unregister`, `system/ipc_messagebus_roundtrip` |

## Running Benchmarks

### Run all benchmarks

```bash
cargo bench
```

### Run benchmarks for a single crate

```bash
cargo bench -p agent_runtime
cargo bench -p agnos_common
cargo bench -p ai_shell
cargo bench -p llm_gateway
```

### Run a specific benchmark by name filter

```bash
cargo bench -- "system/agent"
cargo bench -- "interpreter_parse"
```

### Run benchmarks without plotting (faster, no gnuplot dependency)

```bash
cargo bench -- --noplot
```

### Save a baseline for later comparison

```bash
cargo bench -- --save-baseline before-optimization
# ... make changes ...
cargo bench -- --baseline before-optimization
```

## Performance Targets (Alpha)

These targets must be met before the Q2 2026 Alpha release. Values marked
"current" were measured on an AMD Ryzen 9 7950X / 64 GB DDR5 development
machine and may vary on different hardware.

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Boot time (kernel + userland init) | < 10 s | not yet measured | pending |
| Agent spawn time (create + register) | < 500 ms | ~300 ms | on track |
| Shell response time (parse + translate) | < 100 ms | ~50 ms | on track |
| Memory overhead (idle, 0 agents) | < 2 GB | ~1.2 GB | on track |
| IPC round-trip (MessageBus send_to) | < 1 ms | ~20 us | on track |
| Orchestrator submit (single task) | < 5 ms | ~50 us | on track |
| Memory reserve+release cycle | < 1 ms | ~10 us | on track |

## Benchmark Index

### agnos-common (8 micro-benchmarks)

File: `userland/agnos-common/benches/agnos_common.rs`

| Name | What it measures |
|------|-----------------|
| `agent_id_new` | UUID v4 agent ID generation |
| `agent_id_to_string` | AgentId -> String conversion |
| `agent_id_display` | AgentId Display formatting |
| `inference_request_serialize` | InferenceRequest -> JSON |
| `inference_request_deserialize` | JSON -> InferenceRequest |
| `agent_config_default` | AgentConfig::default() |
| `agent_config_new` | AgentConfig construction with fields |
| `serde_json_to_string` | AgentConfig JSON serialization |
| `serde_json_to_vec` | AgentConfig JSON-to-bytes |

### agent-runtime micro (11 micro-benchmarks)

File: `userland/agent-runtime/benches/bench.rs`

| Name | What it measures |
|------|-----------------|
| `agent_id_generation` | AgentId::new() |
| `agent_config_creation` | AgentConfig struct creation |
| `orchestrator_task_creation` | Task struct creation with UUID |
| `agent_handle_clone` | Clone of AgentHandle |
| `task_priority_ordering` | Vec of TaskPriority variants |
| `task_result_serialize` | TaskResult -> JSON |
| `task_payload_json_parse` | JSON -> serde_json::Value |
| `agent_status_clone` | AgentStatus clone |
| `resource_usage_default` | ResourceUsage::default() |
| `agent_handle_creation` | Full AgentHandle construction |
| `task_result_creation` | Full TaskResult construction |

### agent-runtime system (10 system-level benchmarks)

File: `userland/agent-runtime/benches/system_bench.rs`

| Name | What it measures |
|------|-----------------|
| `system/agent_lifecycle_create_register_unregister` | Full agent create -> registry register -> unregister |
| `system/agent_create` | Agent::new() with config (includes IPC channel setup) |
| `system/registry_register_unregister/{1,10,50}` | N agents register + unregister throughput |
| `system/ipc_messagebus_roundtrip` | Subscribe 2 agents, send A->B, send B->A |
| `system/ipc_broadcast/{1,5,20}` | Publish to N subscribers via MessageBus |
| `system/orchestrator_submit/{1,10,100}` | Submit N tasks, measure throughput |
| `system/orchestrator_submit_mixed_priorities_50` | Submit 50 tasks across 5 priority levels |
| `system/orchestrator_submit_and_result` | Submit task, store result, retrieve result |
| `system/resource_memory_reserve_release` | Reserve 64 MB + release cycle |
| `system/resource_memory_concurrent/{1,5,20}` | N concurrent 1 MB reserves via tokio::spawn |

### ai-shell (10 micro-benchmarks)

File: `userland/ai-shell/benches/ai_shell.rs`

| Name | What it measures |
|------|-----------------|
| `interpreter_parse_simple` | Parse 8 natural-language commands |
| `interpreter_parse_list_files` | Parse "show me all files in /home" |
| `interpreter_parse_cd` | Parse "go to /tmp" |
| `interpreter_translate_list_files` | Translate ListFiles intent to command |
| `interpreter_translate_cd` | Translate ChangeDirectory intent |
| `interpreter_explain_ls` | Explain "ls" command |
| `interpreter_explain_cat` | Explain "cat /etc/hosts" |
| `interpreter_explain_rm` | Explain "rm -rf /tmp/test" |
| `interpreter_parse_10_commands` | Parse 10 mixed shell commands |

### llm-gateway (7 micro-benchmarks)

File: `userland/llm-gateway/benches/llm_gateway.rs`

| Name | What it measures |
|------|-----------------|
| `cache_key_generation` | Format-based cache key from InferenceRequest |
| `hashmap_insert_100` | Insert 100 String->String pairs |
| `hashmap_lookup_100` | Look up 100 keys |
| `json_parse_small` | Parse 3 JSON request strings |
| `token_usage_default` | TokenUsage::default() |
| `token_usage_calculation` | Token arithmetic |
| `response_serialize` | InferenceResponse -> JSON |

**Total: 46 benchmarks** (36 micro + 10 system)

## Interpreting Results

Criterion outputs reports to `target/criterion/`. Each benchmark gets a
directory containing:

- `report/index.html` -- HTML report with violin plots and regression analysis
- `estimates.json` -- machine-readable timing estimates (mean, median, std dev)
- `new/` and `base/` -- raw sample data for current and baseline runs

Key values to look at:

- **mean** -- average time per iteration
- **std dev** -- lower is better; high values indicate inconsistent performance
- **change** -- percentage change vs. the saved baseline (shown as
  "No change", "Improvement", or "Regression")

Criterion uses statistical hypothesis testing. A benchmark is flagged as a
regression only when the change exceeds the noise threshold (default 5%).

## Adding New Benchmarks

1. Add a benchmark function in the appropriate `benches/*.rs` file. Follow the
   naming convention: `benchmark_<what>` for micro-benchmarks, `bench_<what>`
   for system-level.

2. Use `black_box()` around values that must not be optimized away.

3. For async operations, create a `tokio::runtime::Runtime` outside the
   benchmark closure and use `rt.block_on()` inside `b.iter()`.

4. Add the function to the relevant `criterion_group!()` macro.

5. If creating a new bench file, add a `[[bench]]` section to `Cargo.toml`:
   ```toml
   [[bench]]
   name = "my_bench"
   harness = false
   ```

6. Update the benchmark index table in this document.

7. Run `cargo bench -- "<your_bench_name>"` to verify.

## CI Regression Tracking Plan

The CI pipeline (`.github/workflows/ci.yml`) includes a benchmark step that:

1. Runs `cargo bench -- --noplot` on every push to `main` and `develop` and on
   every pull request.
2. The step is configured with `continue-on-error: true` so benchmark
   fluctuations do not block merges.
3. Benchmark output is uploaded as a CI artifact for manual review.

### Future improvements (post-Alpha)

- Store baseline results in a dedicated `gh-pages` branch or S3 bucket.
- Use [criterion-compare-action](https://github.com/boa-dev/criterion-compare-action)
  to post benchmark diffs as PR comments.
- Set hard regression thresholds (e.g., > 15% slowdown = CI failure) once
  baselines are stable.
- Add integration benchmarks that measure full boot-to-agent-ready time in a
  VM or container.

## Coverage Gate

The CI pipeline enforces a minimum code coverage threshold using cargo-tarpaulin:

```
cargo tarpaulin --fail-under 65
```

The current threshold is **65%** (actual coverage: ~62%). This will be raised
incrementally toward the Alpha target of **80%**.
