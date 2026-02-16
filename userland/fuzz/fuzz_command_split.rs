#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // Fuzz the command splitting functionality
        let parts: Vec<&str> = input.split_whitespace().collect();

        // Test basic parsing
        if let Some(cmd) = parts.first() {
            let _ = is_shell_builtin(cmd);
        }
    }
});

fn is_shell_builtin(cmd: &str) -> bool {
    matches!(
        cmd.to_lowercase().as_str(),
        "cd" | "exit" | "quit" | "help" | "clear" | "mode" | "history"
    )
}
