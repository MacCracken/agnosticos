use super::Interpreter;

impl Interpreter {
    /// Get explanation of what a command does
    pub fn explain(&self, command: &str, _args: &[String]) -> String {
        let cmd = command.to_lowercase();

        match cmd.as_str() {
            "ls" => "Lists files and directories".to_string(),
            "cat" => "Displays file contents".to_string(),
            "cd" => "Changes current directory".to_string(),
            "mkdir" => "Creates a new directory".to_string(),
            "cp" => "Copies files or directories".to_string(),
            "mv" => "Moves or renames files".to_string(),
            "rm" => "Removes files or directories (destructive)".to_string(),
            "ps" => "Lists running processes".to_string(),
            "top" => "Shows system resource usage".to_string(),
            "df" => "Shows disk space usage".to_string(),
            "du" => "Shows directory space usage".to_string(),
            "grep" => "Searches for text patterns".to_string(),
            "find" => "Finds files by name or criteria".to_string(),
            _ => format!("Executes the {} command", cmd),
        }
    }
}
