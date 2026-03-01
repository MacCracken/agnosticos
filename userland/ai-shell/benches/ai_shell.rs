use ai_shell::interpreter::{Intent, Interpreter};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_interpreter_parsing(c: &mut Criterion) {
    let interpreter = Interpreter::new();
    let inputs = vec![
        "show me all files in /home",
        "show the contents of /etc/hosts",
        "list files",
        "cd /tmp",
        "create a directory called test",
        "copy file1 to file2",
        "show me running processes",
        "show system information",
    ];

    c.bench_function("interpreter_parse_simple", |b| {
        b.iter(|| {
            for input in &inputs {
                black_box(interpreter.parse(input));
            }
        });
    });
}

fn benchmark_interpreter_list_files(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    c.bench_function("interpreter_parse_list_files", |b| {
        b.iter(|| interpreter.parse(black_box("show me all files in /home")));
    });
}

fn benchmark_interpreter_cd(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    c.bench_function("interpreter_parse_cd", |b| {
        b.iter(|| interpreter.parse(black_box("go to /tmp")));
    });
}

fn benchmark_interpreter_translate(c: &mut Criterion) {
    let interpreter = Interpreter::new();
    let intent = Intent::ListFiles {
        path: Some("/home".to_string()),
        options: Default::default(),
    };

    c.bench_function("interpreter_translate_list_files", |b| {
        b.iter(|| interpreter.translate(black_box(&intent)));
    });
}

fn benchmark_interpreter_translate_cd(c: &mut Criterion) {
    let interpreter = Interpreter::new();
    let intent = Intent::ChangeDirectory {
        path: "/tmp".to_string(),
    };

    c.bench_function("interpreter_translate_cd", |b| {
        b.iter(|| interpreter.translate(black_box(&intent)));
    });
}

fn benchmark_interpreter_explain(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    c.bench_function("interpreter_explain_ls", |b| {
        b.iter(|| interpreter.explain(black_box("ls"), &black_box(vec![])));
    });

    c.bench_function("interpreter_explain_cat", |b| {
        b.iter(|| {
            interpreter.explain(black_box("cat"), &black_box(vec!["/etc/hosts".to_string()]))
        });
    });

    c.bench_function("interpreter_explain_rm", |b| {
        b.iter(|| {
            interpreter.explain(
                black_box("rm"),
                &black_box(vec!["-rf".to_string(), "/tmp/test".to_string()]),
            )
        });
    });
}

fn benchmark_interpreter_multiple_commands(c: &mut Criterion) {
    let interpreter = Interpreter::new();

    c.bench_function("interpreter_parse_10_commands", |b| {
        b.iter(|| {
            let commands = vec![
                "ls -la",
                "cd /home",
                "cat /etc/passwd",
                "ps aux",
                "mkdir test",
                "cp a b",
                "mv x y",
                "rm file",
                "du -sh",
                "uname -a",
            ];
            for cmd in commands {
                black_box(interpreter.parse(cmd));
            }
        });
    });
}

criterion_group!(
    benches,
    benchmark_interpreter_parsing,
    benchmark_interpreter_list_files,
    benchmark_interpreter_cd,
    benchmark_interpreter_translate,
    benchmark_interpreter_translate_cd,
    benchmark_interpreter_explain,
    benchmark_interpreter_multiple_commands
);
criterion_main!(benches);
