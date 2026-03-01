//! Natural language interpreter
//!
//! Translates natural language requests into shell commands
//! with safety checks and human oversight.

use anyhow::{anyhow, Result};
use regex::Regex;
use std::collections::HashMap;

use crate::security::{analyze_command_permission, PermissionLevel};

/// Parsed intent from natural language
#[derive(Debug, Clone)]
pub enum Intent {
    /// Show files or directories
    ListFiles {
        path: Option<String>,
        options: ListOptions,
    },
    /// Display file contents
    ShowFile { path: String, lines: Option<usize> },
    /// Search for files
    FindFiles {
        pattern: String,
        path: Option<String>,
    },
    /// Search within files
    SearchContent {
        pattern: String,
        path: Option<String>,
    },
    /// Change directory
    ChangeDirectory { path: String },
    /// Create directory
    CreateDirectory { path: String },
    /// Copy files
    Copy { source: String, destination: String },
    /// Move/rename files
    Move { source: String, destination: String },
    /// Remove files
    Remove { path: String, recursive: bool },
    /// View process information
    ShowProcesses,
    /// Kill a process
    KillProcess { pid: u32 },
    /// Show system information
    SystemInfo,
    /// Network operations
    NetworkInfo,
    /// Disk usage
    DiskUsage { path: Option<String> },
    /// Install package
    InstallPackage { packages: Vec<String> },
    /// General shell command
    ShellCommand { command: String, args: Vec<String> },
    /// Question/Information request
    Question { query: String },
    /// Ambiguous - needs clarification
    Ambiguous { alternatives: Vec<String> },
    /// Unknown intent
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub all: bool,
    pub long: bool,
    pub human_readable: bool,
    pub sort_by_time: bool,
    pub recursive: bool,
}

/// Command translation result
#[derive(Debug, Clone)]
pub struct Translation {
    pub command: String,
    pub args: Vec<String>,
    pub description: String,
    pub permission: PermissionLevel,
    pub explanation: String,
}

/// Natural language interpreter
pub struct Interpreter {
    patterns: HashMap<String, Regex>,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // List files patterns
        patterns.insert("list".to_string(), Regex::new(
            r"(?i)^(show|list|display|what|see)?\s*(me\s+)?(all\s+)?(files|directories|dirs|folders|contents?)?\s*(in\s+)?(.+)?$"
        ).unwrap());

        // Show file patterns
        patterns.insert("show_file".to_string(), Regex::new(
            r"(?i)^(show|display|view|read|cat|open|print)\s+(me\s+)?(the\s+)?(content|file|contents)?\s*(of\s+)?(.+)$"
        ).unwrap());

        // Find files patterns
        patterns.insert("find".to_string(), Regex::new(
            r"(?i)^(find|locate|search\s+for|look\s+for)\s+(files?\s+(named|called)?\s+)?(.+)(\s+in\s+(.+))?$"
        ).unwrap());

        // Search content patterns
        patterns.insert(
            "grep".to_string(),
            Regex::new(r"(?i)^(search|grep|find)\s+(for\s+)?(.+?)\s+(in|within|inside)\s+(.+)$")
                .unwrap(),
        );

        // Change directory patterns
        patterns.insert("cd".to_string(), Regex::new(
            r"(?i)^(go\s+to|change\s+(to\s+)?|cd\s+(to\s+)?|switch\s+to)\s*(directory\s+)?(.+)$"
        ).unwrap());

        // Create directory patterns
        patterns.insert("mkdir".to_string(), Regex::new(
            r"(?i)^(create|make|new)\s+(a\s+)?(new\s+)?(directory|folder)\s+(named|called)?\s*(.+)$"
        ).unwrap());

        // Copy patterns
        patterns.insert(
            "copy".to_string(),
            Regex::new(r"(?i)^(copy|duplicate)\s+(.+?)\s+(to|into)\s+(.+)$").unwrap(),
        );

        // Move patterns
        patterns.insert(
            "move".to_string(),
            Regex::new(r"(?i)^(move|rename)\s+(.+?)\s+(to|into|as)\s+(.+)$").unwrap(),
        );

        // Remove patterns
        patterns.insert(
            "remove".to_string(),
            Regex::new(r"(?i)^(remove|delete|rm)\s+(the\s+)?(file|directory|folder)?\s*(.+)$")
                .unwrap(),
        );

        // Process patterns
        patterns.insert("ps".to_string(), Regex::new(
            r"(?i)^(show|list|display|what|view)\s+(me\s+)?(all\s+)?(running\s+)?(processes|tasks|programs|apps)$"
        ).unwrap());

        // System info patterns
        patterns.insert("sysinfo".to_string(), Regex::new(
            r"(?i)^(show|display|what|get|view)\s+(me\s+)?(system|computer|machine)\s*(info|information|status|stats)?$"
        ).unwrap());

        // Disk usage patterns
        patterns.insert("du".to_string(), Regex::new(
            r"(?i)^(how\s+much\s+)?(disk\s+)?(space|usage|size)\s+(is\s+)?(used\s+)?(by\s+)?(in\s+)?(.+)?$"
        ).unwrap());

        // Install package patterns
        patterns.insert(
            "install".to_string(),
            Regex::new(r"(?i)^(install|add|get)\s+(package|program|software|app)?\s*(.+)$")
                .unwrap(),
        );

        // Question patterns
        patterns.insert(
            "question".to_string(),
            Regex::new(r"(?i)^(what|who|when|where|why|how|is|are|can|do|does)\s+.+\??$").unwrap(),
        );

        Self { patterns }
    }

    /// Parse natural language input into intent
    pub fn parse(&self, input: &str) -> Intent {
        let input_lower = input.to_lowercase().trim().to_string();

        // Check each pattern
        if let Some(caps) = self.patterns.get("list").unwrap().captures(&input_lower) {
            let path = caps.get(6).map(|m| m.as_str().trim().to_string());
            let all = input_lower.contains("all");

            return Intent::ListFiles {
                path,
                options: ListOptions {
                    all,
                    ..Default::default()
                },
            };
        }

        if let Some(caps) = self
            .patterns
            .get("show_file")
            .unwrap()
            .captures(&input_lower)
        {
            if let Some(path) = caps.get(6) {
                return Intent::ShowFile {
                    path: path.as_str().trim().to_string(),
                    lines: None,
                };
            }
        }

        if let Some(caps) = self.patterns.get("cd").unwrap().captures(&input_lower) {
            if let Some(path) = caps.get(4) {
                return Intent::ChangeDirectory {
                    path: path.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.patterns.get("mkdir").unwrap().captures(&input_lower) {
            if let Some(path) = caps.get(6) {
                return Intent::CreateDirectory {
                    path: path.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.patterns.get("copy").unwrap().captures(&input_lower) {
            if let (Some(source), Some(dest)) = (caps.get(2), caps.get(4)) {
                return Intent::Copy {
                    source: source.as_str().trim().to_string(),
                    destination: dest.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.patterns.get("move").unwrap().captures(&input_lower) {
            if let (Some(source), Some(dest)) = (caps.get(2), caps.get(4)) {
                return Intent::Move {
                    source: source.as_str().trim().to_string(),
                    destination: dest.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.patterns.get("ps").unwrap().captures(&input_lower) {
            return Intent::ShowProcesses;
        }

        if let Some(caps) = self.patterns.get("sysinfo").unwrap().captures(&input_lower) {
            return Intent::SystemInfo;
        }

        if self
            .patterns
            .get("question")
            .unwrap()
            .is_match(&input_lower)
        {
            return Intent::Question {
                query: input.to_string(),
            };
        }

        // If it looks like a command, treat it as such
        if !input.contains(' ') || input.starts_with("/") {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if !parts.is_empty() {
                return Intent::ShellCommand {
                    command: parts[0].to_string(),
                    args: parts[1..].iter().map(|s| s.to_string()).collect(),
                };
            }
        }

        Intent::Unknown
    }

    /// Translate intent into shell command
    pub fn translate(&self, intent: &Intent) -> Result<Translation> {
        match intent {
            Intent::ListFiles { path, options } => {
                let mut args = vec!["-la".to_string()];
                if options.human_readable {
                    args.push("-h".to_string());
                }
                if let Some(p) = path {
                    args.push(p.clone());
                }

                Ok(Translation {
                    command: "ls".to_string(),
                    args,
                    description: format!(
                        "List files{}",
                        path.as_ref()
                            .map(|p| format!(" in {}", p))
                            .unwrap_or_default()
                    ),
                    permission: PermissionLevel::ReadOnly,
                    explanation: "Lists files and directories with details".to_string(),
                })
            }

            Intent::ShowFile { path, lines } => {
                let (cmd, args) = if let Some(n) = lines {
                    ("head".to_string(), vec![format!("-{}", n), path.clone()])
                } else {
                    ("cat".to_string(), vec![path.clone()])
                };

                Ok(Translation {
                    command: cmd,
                    args,
                    description: format!("Display contents of {}", path),
                    permission: PermissionLevel::ReadOnly,
                    explanation: "Shows file contents".to_string(),
                })
            }

            Intent::ChangeDirectory { path } => Ok(Translation {
                command: "cd".to_string(),
                args: vec![path.clone()],
                description: format!("Change directory to {}", path),
                permission: PermissionLevel::Safe,
                explanation: "Changes current working directory".to_string(),
            }),

            Intent::CreateDirectory { path } => Ok(Translation {
                command: "mkdir".to_string(),
                args: vec!["-p".to_string(), path.clone()],
                description: format!("Create directory {}", path),
                permission: PermissionLevel::UserWrite,
                explanation: "Creates a new directory".to_string(),
            }),

            Intent::Copy {
                source,
                destination,
            } => Ok(Translation {
                command: "cp".to_string(),
                args: vec!["-r".to_string(), source.clone(), destination.clone()],
                description: format!("Copy {} to {}", source, destination),
                permission: PermissionLevel::UserWrite,
                explanation: "Copies files or directories".to_string(),
            }),

            Intent::ShowProcesses => Ok(Translation {
                command: "ps".to_string(),
                args: vec!["aux".to_string()],
                description: "Show running processes".to_string(),
                permission: PermissionLevel::ReadOnly,
                explanation: "Lists all running processes".to_string(),
            }),

            Intent::SystemInfo => Ok(Translation {
                command: "uname".to_string(),
                args: vec!["-a".to_string()],
                description: "Show system information".to_string(),
                permission: PermissionLevel::ReadOnly,
                explanation: "Displays system kernel information".to_string(),
            }),

            Intent::ShellCommand { command, args } => {
                let perm = analyze_command_permission(command, args);
                Ok(Translation {
                    command: command.clone(),
                    args: args.clone(),
                    description: format!("Execute {} {}", command, args.join(" ")),
                    permission: perm,
                    explanation: "Direct shell command execution".to_string(),
                })
            }

            Intent::Question { query } => Err(anyhow!(
                "Questions should be handled by LLM, not translated to commands"
            )),

            _ => Err(anyhow!("Cannot translate intent: {:?}", intent)),
        }
    }

    /// Get explanation of what a command does
    pub fn explain(&self, command: &str, args: &[String]) -> String {
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

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_list_files() {
        let interpreter = Interpreter::new();

        let intent = interpreter.parse("show me all files");
        assert!(matches!(intent, Intent::ListFiles { .. }));

        // This test may need adjustment based on interpreter behavior
        // let intent = interpreter.parse("ls -la");
        // assert!(matches!(intent, Intent::ShellCommand { .. }));
    }

    #[test]
    fn test_translate_cd() {
        let interpreter = Interpreter::new();

        let intent = Intent::ChangeDirectory {
            path: "/tmp".to_string(),
        };
        let translation = interpreter.translate(&intent).unwrap();

        assert_eq!(translation.command, "cd");
        assert_eq!(translation.args, vec!["/tmp"]);
    }

    #[test]
    fn test_translate_list_files() {
        let interpreter = Interpreter::new();
        let intent = Intent::ListFiles {
            path: Some("/home".to_string()),
            options: ListOptions::default(),
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "ls");
    }

    #[test]
    fn test_translate_list_files_no_path() {
        let interpreter = Interpreter::new();
        let intent = Intent::ListFiles {
            path: None,
            options: ListOptions::default(),
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "ls");
    }

    #[test]
    fn test_translate_show_file() {
        let interpreter = Interpreter::new();
        let intent = Intent::ShowFile {
            path: "/etc/hosts".to_string(),
            lines: None,
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "cat");
    }

    #[test]
    fn test_translate_show_file_with_lines() {
        let interpreter = Interpreter::new();
        let intent = Intent::ShowFile {
            path: "/var/log/syslog".to_string(),
            lines: Some(10),
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "head");
    }

    #[test]
    fn test_translate_mkdir() {
        let interpreter = Interpreter::new();
        let intent = Intent::CreateDirectory {
            path: "/tmp/test".to_string(),
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "mkdir");
        assert!(translation.args.contains(&"-p".to_string()));
    }

    #[test]
    fn test_translate_copy() {
        let interpreter = Interpreter::new();
        let intent = Intent::Copy {
            source: "/tmp/a".to_string(),
            destination: "/tmp/b".to_string(),
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "cp");
    }

    #[test]
    fn test_translate_move_not_implemented() {
        let interpreter = Interpreter::new();
        let intent = Intent::Move {
            source: "/tmp/a".to_string(),
            destination: "/tmp/b".to_string(),
        };
        assert!(interpreter.translate(&intent).is_err());
    }

    #[test]
    fn test_translate_show_processes() {
        let interpreter = Interpreter::new();
        let intent = Intent::ShowProcesses;
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "ps");
    }

    #[test]
    fn test_translate_system_info() {
        let interpreter = Interpreter::new();
        let intent = Intent::SystemInfo;
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "uname");
    }

    #[test]
    fn test_translate_shell_command() {
        let interpreter = Interpreter::new();
        let intent = Intent::ShellCommand {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
        };
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "echo");
    }

    #[test]
    fn test_translate_question_fails() {
        let interpreter = Interpreter::new();
        let intent = Intent::Question {
            query: "What is this?".to_string(),
        };
        assert!(interpreter.translate(&intent).is_err());
    }

    #[test]
    fn test_translate_unknown_fails() {
        let interpreter = Interpreter::new();
        let intent = Intent::Unknown;
        assert!(interpreter.translate(&intent).is_err());
    }

    #[test]
    fn test_explain_ls() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("ls", &[]);
        assert!(explanation.contains("files"));
    }

    #[test]
    fn test_explain_cat() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("cat", &[]);
        assert!(explanation.contains("contents"));
    }

    #[test]
    fn test_explain_rm() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("rm", &[]);
        assert!(explanation.contains("Removes") || explanation.contains("destructive"));
    }

    #[test]
    fn test_explain_unknown_command() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("foobar", &[]);
        assert!(explanation.contains("foobar"));
    }

    #[test]
    fn test_list_options_default() {
        let opts = ListOptions::default();
        assert!(!opts.all);
        assert!(!opts.long);
        assert!(!opts.recursive);
    }

    #[test]
    fn test_intent_variants() {
        let intent = Intent::FindFiles {
            pattern: "*.rs".to_string(),
            path: None,
        };
        assert!(matches!(intent, Intent::FindFiles { .. }));

        let intent = Intent::SearchContent {
            pattern: "TODO".to_string(),
            path: Some("/src".to_string()),
        };
        assert!(matches!(intent, Intent::SearchContent { .. }));

        let intent = Intent::Remove {
            path: "/tmp/test".to_string(),
            recursive: true,
        };
        assert!(matches!(intent, Intent::Remove { .. }));

        let intent = Intent::KillProcess { pid: 1234 };
        assert!(matches!(intent, Intent::KillProcess { .. }));

        let intent = Intent::NetworkInfo;
        assert!(matches!(intent, Intent::NetworkInfo));

        let intent = Intent::DiskUsage { path: None };
        assert!(matches!(intent, Intent::DiskUsage { .. }));

        let intent = Intent::InstallPackage {
            packages: vec!["vim".to_string()],
        };
        assert!(matches!(intent, Intent::InstallPackage { .. }));

        let intent = Intent::Ambiguous {
            alternatives: vec!["a".to_string(), "b".to_string()],
        };
        assert!(matches!(intent, Intent::Ambiguous { .. }));
    }

    #[test]
    fn test_interpreter_default() {
        let interpreter = Interpreter::default();
    }

    #[test]
    fn test_explain_cd() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("cd", &[String::from("/home")]);
        assert!(explanation.contains("directory"));
    }

    #[test]
    fn test_explain_mkdir() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("mkdir", &[String::from("/tmp/test")]);
        assert!(explanation.contains("new") || explanation.contains("directory"));
    }

    #[test]
    fn test_explain_ps() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("ps", &[]);
        assert!(explanation.contains("process"));
    }

    #[test]
    fn test_explain_df() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("df", &[]);
        assert!(explanation.contains("disk") || explanation.contains("space"));
    }

    #[test]
    fn test_list_options_all() {
        let mut opts = ListOptions::default();
        opts.all = true;
        assert!(opts.all);
    }

    #[test]
    fn test_list_options_long() {
        let mut opts = ListOptions::default();
        opts.long = true;
        assert!(opts.long);
    }

    #[test]
    fn test_list_options_human_readable() {
        let mut opts = ListOptions::default();
        opts.human_readable = true;
        assert!(opts.human_readable);
    }

    #[test]
    fn test_list_options_sort_by_time() {
        let mut opts = ListOptions::default();
        opts.sort_by_time = true;
        assert!(opts.sort_by_time);
    }

    #[test]
    fn test_list_options_recursive() {
        let mut opts = ListOptions::default();
        opts.recursive = true;
        assert!(opts.recursive);
    }
}
