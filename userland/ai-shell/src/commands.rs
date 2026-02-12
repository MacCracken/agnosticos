//! Command execution utilities

use anyhow::Result;

/// Split command line into command and arguments
pub fn split_command(line: &str) -> Result<(String, Vec<String>)> {
    let parts: Vec<&str> = shlex::split(line)
        .ok_or_else(|| anyhow::anyhow!("Invalid command syntax"))?;
    
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty command"));
    }
    
    let command = parts[0].clone();
    let args = parts[1..].iter().map(|s| s.to_string()).collect();
    
    Ok((command, args))
}

/// Check if command is a shell builtin
pub fn is_builtin(command: &str) -> bool {
    let builtins = [
        "cd", "exit", "quit", "help", "clear", "mode", "history",
    ];
    
    builtins.contains(&command.to_lowercase().as_str())
}

/// Get description of a builtin command
pub fn builtin_description(command: &str) -> Option<&'static str> {
    match command.to_lowercase().as_str() {
        "cd" => Some("Change directory"),
        "exit" | "quit" => Some("Exit the shell"),
        "help" => Some("Show help information"),
        "clear" => Some("Clear the screen"),
        "mode" => Some("Show or change shell mode"),
        "history" => Some("Show command history"),
        _ => None,
    }
}
