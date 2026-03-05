// SPDX-License-Identifier: GPL-3.0
//! System-level benchmarks for AGNOS AI Shell.
//!
//! These benchmarks measure end-to-end operations rather than isolated micro-ops:
//!   - Session lifecycle: create -> configure -> execute -> destroy
//!   - Multi-command pipeline: parse + translate N commands (throughput)
//!   - Prompt rendering pipeline: full prompt with all modules
//!   - Intent classification throughput: classify diverse NL inputs
//!   - History search: add N entries then search for patterns
//!   - Explain pipeline: explain N different commands

use std::path::PathBuf;

use ai_shell::config::ShellConfig;
use ai_shell::history::CommandHistory;
use ai_shell::interpreter::Interpreter;
use ai_shell::mode::Mode;
use ai_shell::prompt::{PromptConfig, PromptContext, PromptRenderer};
use ai_shell::security::SecurityContext;
use ai_shell::session::Session;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::runtime::Runtime;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Diverse natural language inputs for intent classification benchmarks.
const NL_INPUTS: &[&str] = &[
    "show me all files in /home",
    "display the contents of /etc/hosts",
    "go to /tmp",
    "create a directory called test",
    "copy file1 to file2",
    "move old.txt to new.txt",
    "show me running processes",
    "show system information",
    "list files",
    "what is the disk usage of /var",
    "search for password in /etc",
    "find files named *.log",
    "kill process 1234",
    "install package htop",
    "how much disk space is left",
    "who am I",
    "show network information",
    "remove /tmp/junk recursively",
    "show the top 10 lines of /var/log/syslog",
    "ls -la",
];

/// Commands for the explain pipeline.
const EXPLAIN_COMMANDS: &[(&str, &[&str])] = &[
    ("ls", &["-la"]),
    ("cat", &["/etc/hosts"]),
    ("cd", &["/tmp"]),
    ("mkdir", &["test"]),
    ("cp", &["a", "b"]),
    ("mv", &["x", "y"]),
    ("rm", &["-rf", "/tmp/test"]),
    ("ps", &["aux"]),
    ("top", &[]),
    ("df", &["-h"]),
    ("du", &["-sh", "."]),
    ("grep", &["-r", "TODO", "."]),
    ("find", &[".", "-name", "*.rs"]),
    ("chmod", &["755", "script.sh"]),
    ("chown", &["root:root", "file"]),
    ("tar", &["-xzf", "archive.tar.gz"]),
];

fn make_temp_config() -> ShellConfig {
    let tmp = std::env::temp_dir().join("agnos_bench");
    ShellConfig {
        default_mode: Mode::Human,
        history_file: tmp.join("bench_history"),
        history_size: 10000,
        output_format: "auto".to_string(),
        ai_enabled: false,
        auto_approve_low_risk: false,
        approval_timeout: 300,
        llm_endpoint: None,
        audit_log: tmp.join("bench_audit.log"),
        show_explanations: false,
        theme: "default".to_string(),
    }
}

fn make_prompt_context() -> PromptContext {
    let mut ctx = PromptContext::new(
        PathBuf::from("/home/bench/project"),
        "bench_user".to_string(),
        "AI-ASSIST".to_string(),
    );
    ctx.last_exit_code = 0;
    ctx.cmd_duration_ms = 3500; // above default 2000ms threshold
    ctx
}

async fn make_history(n: usize) -> CommandHistory {
    let path = PathBuf::from("/tmp/agnos_bench_history_preload");
    let _ = tokio::fs::remove_file(&path).await;
    let mut history = CommandHistory::new(&path).await.unwrap();
    for i in 0..n {
        let cmd = match i % 5 {
            0 => format!("ls -la /path/{}", i),
            1 => format!("git commit -m 'change {}'", i),
            2 => format!("cargo test -p module_{}", i),
            3 => format!("docker build -t image:{}", i),
            _ => format!("grep -r pattern_{} src/", i),
        };
        history.add(&cmd).await.unwrap();
    }
    history
}

// ---------------------------------------------------------------------------
// 1. Session lifecycle: create -> set context -> execute commands -> destroy
// ---------------------------------------------------------------------------

fn bench_session_lifecycle(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/session_lifecycle_create_execute_destroy", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = make_temp_config();
                let security = SecurityContext::new(false).unwrap();
                let mut session = Session::new(config, security, Mode::Human).await.unwrap();

                // Execute several one-shot commands (builtins that don't spawn processes)
                let _ = session.execute_one_shot("help".to_string()).await;
                let _ = session.execute_one_shot("mode".to_string()).await;
                let _ = session.execute_one_shot("history".to_string()).await;

                black_box(&session);
                // Session drops here
            });
        });
    });
}

fn bench_session_create_only(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("system/session_create", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = make_temp_config();
                let security = SecurityContext::new(false).unwrap();
                let session = Session::new(config, security, Mode::Human).await.unwrap();
                black_box(&session);
            });
        });
    });
}

// ---------------------------------------------------------------------------
// 2. Multi-command pipeline: parse + translate N commands (throughput)
// ---------------------------------------------------------------------------

fn bench_parse_translate_pipeline(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("system/parse_translate_pipeline");
    for count in [10, 50, 100] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let input = NL_INPUTS[i % NL_INPUTS.len()];
                    let intent = interpreter.parse(input);
                    // translate may fail for some intents (Unknown, Question, etc.)
                    let _ = black_box(interpreter.translate(&intent));
                }
            });
        });
    }
    group.finish();
}

fn bench_parse_only_pipeline(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("system/parse_only_pipeline");
    for count in [10, 50, 100] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let input = NL_INPUTS[i % NL_INPUTS.len()];
                    black_box(interpreter.parse(input));
                }
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Prompt rendering pipeline: full prompt with all modules enabled
// ---------------------------------------------------------------------------

fn bench_prompt_render_full(c: &mut Criterion) {
    let mut config = PromptConfig::default();
    config.show_ai_mode = true;
    config.show_directory = true;
    config.show_git_status = true;
    config.show_execution_time = true;
    config.show_exit_status = true;
    config.show_context = true;
    config.execution_time_threshold = 0; // always show exec time
    let renderer = PromptRenderer::new(config);
    let ctx = make_prompt_context();

    c.bench_function("system/prompt_render_full_all_modules", |b| {
        b.iter(|| {
            black_box(renderer.render(&ctx));
        });
    });
}

fn bench_prompt_render_minimal(c: &mut Criterion) {
    let mut config = PromptConfig::default();
    config.show_ai_mode = false;
    config.show_directory = true;
    config.show_git_status = false;
    config.show_execution_time = false;
    config.show_exit_status = false;
    config.show_context = false;
    let renderer = PromptRenderer::new(config);
    let ctx = make_prompt_context();

    c.bench_function("system/prompt_render_minimal", |b| {
        b.iter(|| {
            black_box(renderer.render(&ctx));
        });
    });
}

fn bench_prompt_render_right(c: &mut Criterion) {
    let renderer = PromptRenderer::default();
    let mut ctx = make_prompt_context();
    ctx.cmd_duration_ms = 5000;

    c.bench_function("system/prompt_render_right_prompt", |b| {
        b.iter(|| {
            black_box(renderer.render_right(&ctx));
        });
    });
}

fn bench_prompt_render_repeated(c: &mut Criterion) {
    let mut config = PromptConfig::default();
    config.show_context = true;
    config.execution_time_threshold = 0;
    let renderer = PromptRenderer::new(config);

    let mut group = c.benchmark_group("system/prompt_render_repeated");
    for count in [10, 50, 100] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let mut ctx = make_prompt_context();
                    ctx.last_exit_code = (i % 2) as i32;
                    ctx.cmd_duration_ms = (i as u64) * 100;
                    black_box(renderer.render(&ctx));
                }
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Intent classification throughput: classify N diverse NL inputs
// ---------------------------------------------------------------------------

fn bench_intent_classification(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("system/intent_classification");
    for count in [10, 50, 100, 500] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let input = NL_INPUTS[i % NL_INPUTS.len()];
                    black_box(interpreter.parse(input));
                }
            });
        });
    }
    group.finish();
}

fn bench_intent_classification_diverse(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    // Mix of well-formed NL, edge cases, and raw commands
    let diverse_inputs: Vec<&str> = vec![
        "show me all files",
        "",
        "   ",
        "a very long input that doesn't really match any known pattern at all whatsoever",
        "ls",
        "/usr/bin/env python3 -c 'print(1)'",
        "go to /tmp",
        "what is this",
        "how do I list files?",
        "create directory /tmp/bench_test_dir",
        "remove /tmp/old recursively",
        "copy /etc/hosts to /tmp/hosts.bak",
        "move /tmp/a to /tmp/b",
        "show processes",
        "system info please",
    ];

    c.bench_function("system/intent_classification_diverse_15", |b| {
        b.iter(|| {
            for input in &diverse_inputs {
                black_box(interpreter.parse(input));
            }
        });
    });
}

// ---------------------------------------------------------------------------
// 5. History search: add N entries then search for patterns
// ---------------------------------------------------------------------------

fn bench_history_add_and_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/history_add_then_search");
    for count in [100, 500, 1000, 5000] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let path = PathBuf::from("/tmp/agnos_bench_hist_nonexist");
                    let _ = tokio::fs::remove_file(&path).await;
                    let mut history = CommandHistory::new(&path).await.unwrap();

                    // Add N entries
                    for i in 0..n {
                        let cmd = match i % 5 {
                            0 => format!("ls -la /path/{}", i),
                            1 => format!("git commit -m 'change {}'", i),
                            2 => format!("cargo test -p module_{}", i),
                            3 => format!("docker build -t image:{}", i),
                            _ => format!("grep -r pattern_{} src/", i),
                        };
                        history.add(&cmd).await.unwrap();
                    }

                    // Search for patterns
                    let r1 = history.search("git");
                    black_box(r1.len());
                    let r2 = history.search("cargo");
                    black_box(r2.len());
                    let r3 = history.search("nonexistent_pattern");
                    black_box(r3.len());
                });
            });
        });
    }
    group.finish();
}

fn bench_history_search_preloaded(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/history_search_preloaded");
    for count in [100, 1000, 5000] {
        let history = rt.block_on(make_history(count));
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _n| {
            b.iter(|| {
                black_box(history.search("git"));
                black_box(history.search("cargo"));
                black_box(history.search("docker"));
                black_box(history.search("grep"));
                black_box(history.search("nonexistent"));
            });
        });
    }
    group.finish();
}

fn bench_history_get_recent(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("system/history_get_recent");
    for count in [100, 1000, 5000] {
        let history = rt.block_on(make_history(count));
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _n| {
            b.iter(|| {
                black_box(history.get_recent(10));
                black_box(history.get_recent(50));
                black_box(history.get_recent(100));
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 6. Explain pipeline: explain N different commands
// ---------------------------------------------------------------------------

fn bench_explain_pipeline(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let mut group = c.benchmark_group("system/explain_pipeline");
    for count in [5, 16, 50] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                for i in 0..n {
                    let (cmd, args_strs) = EXPLAIN_COMMANDS[i % EXPLAIN_COMMANDS.len()];
                    let args: Vec<String> = args_strs.iter().map(|s| s.to_string()).collect();
                    black_box(interpreter.explain(cmd, &args));
                }
            });
        });
    }
    group.finish();
}

fn bench_explain_all_known_commands(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    c.bench_function("system/explain_all_16_commands", |b| {
        b.iter(|| {
            for (cmd, args_strs) in EXPLAIN_COMMANDS {
                let args: Vec<String> = args_strs.iter().map(|s| s.to_string()).collect();
                black_box(interpreter.explain(cmd, &args));
            }
        });
    });
}

fn bench_explain_unknown_commands(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    let unknown_cmds = [
        "mycustomtool", "zig", "bun", "deno", "nix-build", "podman",
        "wasmtime", "ollama", "kubectl", "terraform",
    ];

    c.bench_function("system/explain_10_unknown_commands", |b| {
        b.iter(|| {
            for cmd in &unknown_cmds {
                black_box(interpreter.explain(cmd, &[]));
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    session_benches,
    bench_session_lifecycle,
    bench_session_create_only
);

criterion_group!(
    pipeline_benches,
    bench_parse_translate_pipeline,
    bench_parse_only_pipeline
);

criterion_group!(
    prompt_benches,
    bench_prompt_render_full,
    bench_prompt_render_minimal,
    bench_prompt_render_right,
    bench_prompt_render_repeated
);

criterion_group!(
    intent_benches,
    bench_intent_classification,
    bench_intent_classification_diverse
);

criterion_group!(
    history_benches,
    bench_history_add_and_search,
    bench_history_search_preloaded,
    bench_history_get_recent
);

criterion_group!(
    explain_benches,
    bench_explain_pipeline,
    bench_explain_all_known_commands,
    bench_explain_unknown_commands
);

criterion_main!(
    session_benches,
    pipeline_benches,
    prompt_benches,
    intent_benches,
    history_benches,
    explain_benches
);
