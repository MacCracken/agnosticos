//! Command execution utilities

use anyhow::Result;

/// Split command line into command and arguments
pub fn split_command(line: &str) -> Result<(String, Vec<String>)> {
    let parts: Vec<String> =
        shlex::split(line).ok_or_else(|| anyhow::anyhow!("Invalid command syntax"))?;

    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty command"));
    }

    let command = parts[0].clone();
    let args = parts[1..].to_vec();

    Ok((command, args))
}

/// Check if command is a shell builtin
pub fn is_builtin(command: &str) -> bool {
    let builtins = ["cd", "exit", "quit", "help", "clear", "mode", "history"];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_command_simple() {
        let result = split_command("ls -la");
        assert!(result.is_ok());
        let (cmd, args) = result.unwrap();
        assert_eq!(cmd, "ls");
        assert_eq!(args, vec!["-la"]);
    }

    #[test]
    fn test_split_command_with_quotes() {
        let result = split_command("echo \"hello world\"");
        assert!(result.is_ok());
        let (cmd, args) = result.unwrap();
        assert_eq!(cmd, "echo");
        assert_eq!(args, vec!["hello world"]);
    }

    #[test]
    fn test_split_command_empty() {
        let result = split_command("");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_builtin_cd() {
        assert!(is_builtin("cd"));
        assert!(is_builtin("CD"));
    }

    #[test]
    fn test_is_builtin_exit() {
        assert!(is_builtin("exit"));
        assert!(is_builtin("quit"));
    }

    #[test]
    fn test_is_builtin_false() {
        assert!(!is_builtin("ls"));
        assert!(!is_builtin("cat"));
    }

    #[test]
    fn test_builtin_description_cd() {
        assert_eq!(builtin_description("cd"), Some("Change directory"));
    }

    #[test]
    fn test_builtin_description_exit() {
        assert_eq!(builtin_description("exit"), Some("Exit the shell"));
        assert_eq!(builtin_description("quit"), Some("Exit the shell"));
    }

    #[test]
    fn test_builtin_description_help() {
        assert_eq!(builtin_description("help"), Some("Show help information"));
    }

    #[test]
    fn test_builtin_description_unknown() {
        assert_eq!(builtin_description("unknown"), None);
    }
}
