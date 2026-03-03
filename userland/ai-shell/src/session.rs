//! Shell session management
//!
//! Coordinates all shell components and handles the main event loop

use anyhow::{anyhow, Result};
use std::io::{self, Write};
use std::path::PathBuf;

use crate::approval::{ApprovalManager, ApprovalRequest, ApprovalResponse};
use crate::config::ShellConfig;
use crate::history::CommandHistory;
use crate::interpreter::{Interpreter, Intent};
use crate::mode::{Mode, ModeManager};
use crate::output::OutputFormatter;
use crate::prompt::{PromptConfig, PromptContext, PromptRenderer};
use crate::security::{analyze_command_permission, PermissionLevel, SecurityContext};
use crate::ui::Ui;

/// Main shell session
pub struct Session {
    config: ShellConfig,
    security: SecurityContext,
    mode_manager: ModeManager,
    interpreter: Interpreter,
    approval: ApprovalManager,
    history: CommandHistory,
    output: OutputFormatter,
    ui: Ui,
    cwd: PathBuf,
    prompt_renderer: PromptRenderer,
    prompt_context: PromptContext,
}

impl Session {
    pub async fn new(
        config: ShellConfig,
        security: SecurityContext,
        initial_mode: Mode,
    ) -> Result<Self> {
        let mode_manager = ModeManager::new(initial_mode.clone(), true);
        let interpreter = Interpreter::new();
        let approval = ApprovalManager::new();
        let history = CommandHistory::new(&config.history_file).await?;
        let output = OutputFormatter::new(&config.output_format);
        let ui = Ui::new();
        let cwd = std::env::current_dir()?;
        
        // Initialize starship-style prompt
        let prompt_config = PromptConfig::default();
        let prompt_renderer = PromptRenderer::new(prompt_config);
        let prompt_context = PromptContext::new(
            cwd.clone(),
            security.username().to_string(),
            initial_mode.to_string(),
        );
        
        Ok(Self {
            config,
            security,
            mode_manager,
            interpreter,
            approval,
            history,
            output,
            ui,
            cwd,
            prompt_renderer,
            prompt_context,
        })
    }
    
    /// Run the interactive shell loop
    pub async fn run_interactive(&mut self) -> Result<()> {
        self.ui.show_welcome();
        
        loop {
            // Show prompt
            let prompt = self.build_prompt();
            
            // Get input
            let input = match self.ui.read_input(&prompt).await? {
                Some(line) => line,
                None => break, // EOF
            };
            
            // Skip empty input
            if input.trim().is_empty() {
                continue;
            }
            
            // Add to history
            self.history.add(&input).await?;
            
            // Process input
            if let Err(e) = self.process_input(&input).await {
                self.ui.show_error(&e.to_string());
            }
        }
        
        self.ui.show_goodbye();
        Ok(())
    }
    
    /// Execute one-shot command
    pub async fn execute_one_shot(&mut self, command: String) -> Result<()> {
        self.process_input(&command).await
    }
    
    /// Process a single input line
    async fn process_input(&mut self, input: &str) -> Result<()> {
        // Check for special commands
        if let Some(result) = self.handle_builtin(input).await? {
            return result;
        }
        
        // Parse input based on current mode
        match self.mode_manager.current() {
            Mode::Human => {
                // Direct shell execution in human mode
                self.execute_shell_command(input).await?;
            }
            Mode::AiAssisted => {
                // AI assists with interpretation
                self.execute_with_assistance(input).await?;
            }
            Mode::AiAutonomous => {
                // AI can act autonomously within constraints
                self.execute_autonomously(input).await?;
            }
            Mode::Strict => {
                // Everything requires approval
                self.execute_with_approval(input).await?;
            }
        }
        
        Ok(())
    }
    
    /// Handle builtin commands
    async fn handle_builtin(&mut self, input: &str) -> Result<Option<Result<()>>> {
        let cmd = input.trim().to_lowercase();
        
        match cmd.as_str() {
            "exit" | "quit" => {
                return Ok(Some(Err(anyhow!("Exit requested"))));
            }
            "help" => {
                self.ui.show_help();
                return Ok(Some(Ok(())));
            }
            "clear" => {
                self.ui.clear_screen();
                return Ok(Some(Ok(())));
            }
            "mode" => {
                self.ui.show_mode(self.mode_manager.current());
                return Ok(Some(Ok(())));
            }
            "history" => {
                self.ui.show_history(&self.history);
                return Ok(Some(Ok(())));
            }
            _ if cmd.starts_with("mode ") => {
                let new_mode = cmd.strip_prefix("mode ").unwrap();
                match new_mode {
                    "human" => {
                        self.mode_manager.switch(Mode::Human)?;
                        self.prompt_context.ai_mode = Mode::Human.to_string();
                    }
                    "ai" | "assist" => {
                        self.mode_manager.switch(Mode::AiAssisted)?;
                        self.prompt_context.ai_mode = Mode::AiAssisted.to_string();
                    }
                    "auto" => {
                        self.mode_manager.switch(Mode::AiAutonomous)?;
                        self.prompt_context.ai_mode = Mode::AiAutonomous.to_string();
                    }
                    "strict" => {
                        self.mode_manager.switch(Mode::Strict)?;
                        self.prompt_context.ai_mode = Mode::Strict.to_string();
                    }
                    _ => {
                        self.ui.show_error(&format!("Unknown mode: {}", new_mode));
                    }
                }
                return Ok(Some(Ok(())));
            }
            _ => {}
        }
        
        Ok(None)
    }
    
    /// Execute with AI assistance
    async fn execute_with_assistance(&mut self, input: &str) -> Result<()> {
        // Parse intent
        let intent = self.interpreter.parse(input);
        
        // Show AI interpretation
        self.ui.show_ai_thinking(&format!("Parsed intent: {:?}", intent));
        
        match intent {
            Intent::Question { query } => {
                // Answer question using LLM
                self.ui.show_info("Let me help you with that...");
                // TODO: Call LLM for explanation
                self.ui.show_info(&format!("You asked: {}", query));
            }
            Intent::Unknown => {
                // Try to execute as shell command with warning
                self.ui.show_warning("I didn't understand. Executing as shell command.");
                self.execute_shell_command(input).await?;
            }
            _ => {
                // Translate to command
                match self.interpreter.translate(&intent) {
                    Ok(translation) => {
                        // Show what we'll do
                        self.ui.show_proposed_action(&translation);
                        
                        // Check if approval needed
                        if translation.permission.requires_approval() {
                            let request = ApprovalRequest::Command {
                                command: translation.command.clone(),
                                args: translation.args.clone(),
                                reason: translation.explanation.clone(),
                                risk_level: crate::approval::RiskLevel::from_permission(&translation.permission),
                            };
                            
                            match self.approval.request(&request).await? {
                                ApprovalResponse::Approved | ApprovalResponse::ApprovedOnce => {
                                    self.execute_command(&translation.command, &translation.args).await?;
                                }
                                ApprovalResponse::Denied | ApprovalResponse::DenyAndBlock => {
                                    self.ui.show_info("Action cancelled by user");
                                }
                                _ => {}
                            }
                        } else {
                            // Safe to execute
                            self.execute_command(&translation.command, &translation.args).await?;
                        }
                    }
                    Err(e) => {
                        self.ui.show_error(&format!("Cannot translate: {}", e));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Execute autonomously (AI mode)
    async fn execute_autonomously(&mut self, input: &str) -> Result<()> {
        let intent = self.interpreter.parse(input);
        
        match intent {
            Intent::ShellCommand { command, args } => {
                // Check permissions
                let perm = analyze_command_permission(&command, &args);
                
                if !perm.ai_allowed() {
                    self.ui.show_error("This command is blocked in AI mode");
                    return Ok(());
                }
                
                if perm.requires_approval() {
                    self.ui.show_info("This requires approval...");
                    return self.execute_with_assistance(input).await;
                }
                
                // Safe to execute
                self.execute_command(&command, &args).await?;
            }
            _ => {
                return self.execute_with_assistance(input).await;
            }
        }
        
        Ok(())
    }
    
    /// Execute with mandatory approval
    async fn execute_with_approval(&mut self, input: &str) -> Result<()> {
        let request = ApprovalRequest::Command {
            command: input.to_string(),
            args: vec![],
            reason: "Strict mode requires approval for all commands".to_string(),
            risk_level: crate::approval::RiskLevel::Medium,
        };
        
        match self.approval.request(&request).await? {
            ApprovalResponse::Approved | ApprovalResponse::ApprovedOnce => {
                self.execute_shell_command(input).await?;
            }
            _ => {
                self.ui.show_info("Command cancelled");
            }
        }
        
        Ok(())
    }
    
    /// Execute raw shell command
    async fn execute_shell_command(&mut self, command: &str) -> Result<()> {
        // Parse command
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }
        
        let cmd = parts[0].to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        
        // Check for cd (special handling)
        if cmd == "cd" {
            let path = args.get(0).map(|s| s.as_str()).unwrap_or("~");
            let expanded = shellexpand::tilde(path);
            match std::env::set_current_dir(&*expanded) {
                Ok(_) => {
                    self.cwd = std::env::current_dir()?;
                    self.prompt_context.cwd = self.cwd.clone();
                }
                Err(e) => {
                    self.ui.show_error(&format!("cd: {}", e));
                }
            }
            return Ok(());
        }
        
        self.execute_command(&cmd, &args).await
    }
    
    /// Execute a command with proper formatting
    async fn execute_command(&mut self, cmd: &str, args: &[String]) -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command;
        
        let mut command = Command::new(cmd);
        command.args(args)
            .current_dir(&self.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        match command.spawn() {
            Ok(mut child) => {
                match child.wait().await {
                    Ok(status) => {
                        // Track exit code for prompt
                        self.prompt_context.last_exit_code = status.code().unwrap_or(-1);
                        
                        if let Some(stdout) = child.stdout.take() {
                            let reader = tokio::io::BufReader::new(stdout);
                            use tokio::io::AsyncBufReadExt;
                            let mut lines = reader.lines();
                            while let Some(line) = lines.next_line().await? {
                                self.ui.show_output(&line);
                            }
                        }
                        
                        if !status.success() {
                            if let Some(stderr) = child.stderr.take() {
                                let reader = tokio::io::BufReader::new(stderr);
                                use tokio::io::AsyncBufReadExt;
                                let mut lines = reader.lines();
                                while let Some(line) = lines.next_line().await? {
                                    self.ui.show_error(&line);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        self.ui.show_error(&format!("Failed to wait for process: {}", e));
                        self.prompt_context.last_exit_code = -1;
                    }
                }
            }
            Err(e) => {
                self.ui.show_error(&format!("Failed to execute '{}': {}", cmd, e));
                self.prompt_context.last_exit_code = -1;
            }
        }
        
        Ok(())
    }
    
    /// Build the shell prompt using starship-style renderer
    fn build_prompt(&self) -> String {
        self.prompt_renderer.render(&self.prompt_context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::PermissionLevel;

    #[tokio::test]
    async fn test_prompt_context_new() {
        let cwd = std::path::PathBuf::from("/home/user");
        let context = PromptContext::new(
            cwd.clone(),
            "testuser".to_string(),
            "human".to_string(),
        );
        assert_eq!(context.cwd, cwd);
        assert_eq!(context.username, "testuser");
        assert_eq!(context.ai_mode, "human");
        assert_eq!(context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_prompt_context_new_fields() {
        let cwd = std::path::PathBuf::from("/home/user");
        let context = PromptContext::new(
            cwd.clone(),
            "testuser".to_string(),
            "human".to_string(),
        );
        assert_eq!(context.cwd, cwd);
        assert_eq!(context.username, "testuser");
        assert_eq!(context.ai_mode, "human");
        assert_eq!(context.last_exit_code, 0);
        assert_eq!(context.cmd_duration_ms, 0);
    }

    #[tokio::test]
    async fn test_prompt_context_update_cwd() {
        let cwd = std::path::PathBuf::from("/home/user");
        let mut context = PromptContext::new(
            cwd.clone(),
            "testuser".to_string(),
            "human".to_string(),
        );
        let new_cwd = std::path::PathBuf::from("/tmp");
        context.cwd = new_cwd.clone();
        assert_eq!(context.cwd, new_cwd);
    }

    #[test]
    fn test_shell_config_default() {
        let config = ShellConfig::default();
        assert!(config.history_file.to_string_lossy().ends_with(".agnsh_history"));
        assert_eq!(config.output_format, "auto");
        assert!(config.ai_enabled);
    }

    #[test]
    fn test_shell_config_custom() {
        let config = ShellConfig {
            default_mode: Mode::Human,
            history_file: PathBuf::from("/custom/path/history"),
            history_size: 5000,
            output_format: "json".to_string(),
            ai_enabled: true,
            auto_approve_low_risk: false,
            approval_timeout: 600,
            llm_endpoint: None,
            audit_log: PathBuf::from("/custom/path/audit.log"),
            show_explanations: true,
            theme: "dark".to_string(),
        };
        assert_eq!(config.history_file, PathBuf::from("/custom/path/history"));
        assert_eq!(config.output_format, "json");
        assert!(config.ai_enabled);
        assert_eq!(config.history_size, 5000);
    }

    #[tokio::test]
    async fn test_command_history_new() {
        let temp_path = std::env::temp_dir().join("agnos_test_history");
        let history = CommandHistory::new(&temp_path).await.unwrap();
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_add_and_get() {
        let temp_path = std::env::temp_dir().join("agnos_test_history2");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        
        history.add("ls -la").await.unwrap();
        history.add("pwd").await.unwrap();
        
        let entries = history.get_recent(10);
        assert_eq!(entries.len(), 2);
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_search() {
        let temp_path = std::env::temp_dir().join("agnos_test_history3");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        
        history.add("ls -la").await.unwrap();
        history.add("git status").await.unwrap();
        history.add("git log").await.unwrap();
        
        let results = history.search("git");
        assert_eq!(results.len(), 2);
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_save() {
        let temp_path = std::env::temp_dir().join("agnos_test_history4");
        
        {
            let mut history = CommandHistory::new(&temp_path).await.unwrap();
            history.add("command1").await.unwrap();
            history.add("command2").await.unwrap();
            history.save().await.unwrap();
        }
        
        {
            let history = CommandHistory::new(&temp_path).await.unwrap();
            let entries = history.get_recent(10);
            assert_eq!(entries.len(), 2);
        }
        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_interpreter_new() {
        let interpreter = Interpreter::new();
        assert!(!interpreter.explain("ls", &[]).is_empty());
    }

    #[test]
    fn test_interpreter_parse_shell_command() {
        let interpreter = Interpreter::new();
        let intent = interpreter.parse("ls -la /home");
        
        match intent {
            Intent::ShellCommand { command, args } => {
                assert_eq!(command, "ls");
                assert_eq!(args, vec!["-la", "/home"]);
            }
            _ => {}
        }
    }

    #[test]
    fn test_interpreter_parse_question() {
        let interpreter = Interpreter::new();
        let intent = interpreter.parse("what is my IP address?");
        
        match intent {
            Intent::Question { query } => {
                assert!(query.contains("IP"));
            }
            _ => {}
        }
    }

    #[test]
    fn test_interpreter_parse_unknown() {
        let interpreter = Interpreter::new();
        let intent = interpreter.parse("show me the files");
        
        assert!(matches!(intent, Intent::ListFiles { .. }));
    }

    #[test]
    fn test_interpreter_translate_shell_command() {
        let interpreter = Interpreter::new();
        let intent = Intent::ShellCommand {
            command: "ls".to_string(),
            args: vec!["-la".to_string()],
        };
        
        let translation = interpreter.translate(&intent).unwrap();
        assert_eq!(translation.command, "ls");
    }

    #[test]
    fn test_interpreter_explain() {
        let interpreter = Interpreter::new();
        let explanation = interpreter.explain("ls", &["-la".to_string()]);
        assert!(!explanation.is_empty());
    }

    #[test]
    fn test_mode_manager_new() {
        let manager = ModeManager::new(Mode::Human, false);
        assert_eq!(manager.current(), &Mode::Human);
    }

    #[test]
    fn test_mode_manager_switch() {
        let mut manager = ModeManager::new(Mode::Human, true);
        
        manager.switch(Mode::AiAssisted).unwrap();
        assert_eq!(manager.current(), &Mode::AiAssisted);
        
        manager.switch(Mode::Strict).unwrap();
        assert_eq!(manager.current(), &Mode::Strict);
    }

    #[test]
    fn test_mode_manager_switch_invalid() {
        let mut manager = ModeManager::new(Mode::Human, false);
        
        let result = manager.switch(Mode::Human);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mode_manager_revert() {
        let mut manager = ModeManager::new(Mode::Human, true);
        
        manager.switch(Mode::AiAssisted).unwrap();
        manager.revert().unwrap();
        assert_eq!(manager.current(), &Mode::Human);
    }

    #[test]
    fn test_mode_manager_toggle() {
        let mut manager = ModeManager::new(Mode::Human, true);
        manager.toggle();
        assert_eq!(manager.current(), &Mode::AiAssisted);
        manager.toggle();
        assert_eq!(manager.current(), &Mode::Human);
    }

    #[test]
    fn test_mode_manager_available_modes() {
        let manager = ModeManager::new(Mode::Human, true);
        let modes = manager.available_modes();
        assert!(modes.len() >= 3);
    }

    #[test]
    fn test_mode_display() {
        let human = Mode::Human.to_string();
        let ai = Mode::AiAssisted.to_string();
        let auto = Mode::AiAutonomous.to_string();
        let strict = Mode::Strict.to_string();
        
        assert_eq!(human, "HUMAN");
        assert_eq!(ai, "AI-ASSIST");
        assert_eq!(auto, "AI-AUTO");
        assert_eq!(strict, "STRICT");
    }

    #[test]
    fn test_mode_ai_autonomous() {
        assert!(Mode::AiAutonomous.ai_autonomous());
        assert!(!Mode::AiAssisted.ai_autonomous());
        assert!(!Mode::Human.ai_autonomous());
        assert!(!Mode::Strict.ai_autonomous());
    }

    #[test]
    fn test_mode_ai_available() {
        assert!(Mode::AiAssisted.ai_available());
        assert!(Mode::AiAutonomous.ai_available());
        assert!(!Mode::Human.ai_available());
        assert!(Mode::Strict.ai_available());
    }

    #[test]
    fn test_mode_strict_approval() {
        assert!(Mode::Strict.strict_approval());
        assert!(!Mode::Human.strict_approval());
        assert!(!Mode::AiAssisted.strict_approval());
    }

    #[test]
    fn test_mode_description() {
        assert!(!Mode::Human.description().is_empty());
        assert!(!Mode::AiAssisted.description().is_empty());
        assert!(!Mode::AiAutonomous.description().is_empty());
        assert!(!Mode::Strict.description().is_empty());
    }

    #[test]
    fn test_mode_prompt_prefix() {
        assert!(!Mode::Human.prompt_prefix().is_empty());
        assert!(!Mode::AiAssisted.prompt_prefix().is_empty());
    }

    #[test]
    fn test_security_context_new() {
        let context = SecurityContext::new(false).unwrap();
        assert!(!context.username().is_empty());
    }

    #[test]
    fn test_security_context_is_root() {
        let context = SecurityContext::new(false).unwrap();
        let _ = context.is_root();
    }

    #[test]
    fn test_security_context_is_restricted() {
        let unrestricted = SecurityContext::new(false).unwrap();
        assert!(!unrestricted.is_restricted());
        
        let restricted = SecurityContext::new(true).unwrap();
        assert!(restricted.is_restricted());
    }

    #[test]
    fn test_security_context_username() {
        let context = SecurityContext::new(false).unwrap();
        let username = context.username();
        assert!(!username.is_empty());
    }

    #[test]
    fn test_security_context_can_escalate() {
        let context = SecurityContext::new(false).unwrap();
        let _ = context.can_escalate();
    }

    #[test]
    fn test_analyze_command_permission_safe() {
        let perm = analyze_command_permission("cd", &[]);
        assert!(matches!(perm, PermissionLevel::Safe));
    }

    #[test]
    fn test_permission_level_ai_allowed() {
        assert!(PermissionLevel::Safe.ai_allowed());
        assert!(PermissionLevel::ReadOnly.ai_allowed());
        assert!(PermissionLevel::UserWrite.ai_allowed());
        assert!(PermissionLevel::SystemWrite.ai_allowed());
        assert!(PermissionLevel::Admin.ai_allowed());
        assert!(!PermissionLevel::Blocked.ai_allowed());
    }

    #[test]
    fn test_analyze_command_permission_readonly() {
        let perm = analyze_command_permission("ls", &[]);
        assert!(matches!(perm, PermissionLevel::ReadOnly));
        
        let perm = analyze_command_permission("cat", &["file.txt".to_string()]);
        assert!(matches!(perm, PermissionLevel::ReadOnly));
    }

    #[test]
    fn test_analyze_command_permission_pwd() {
        let perm = analyze_command_permission("pwd", &[]);
        assert!(matches!(perm, PermissionLevel::ReadOnly));
    }

    #[test]
    fn test_permission_level_requires_approval() {
        assert!(!PermissionLevel::Safe.requires_approval());
        assert!(!PermissionLevel::ReadOnly.requires_approval());
        assert!(!PermissionLevel::UserWrite.requires_approval());
        assert!(PermissionLevel::SystemWrite.requires_approval());
        assert!(PermissionLevel::Admin.requires_approval());
        assert!(PermissionLevel::Blocked.requires_approval());
    }

    #[test]
    fn test_output_formatter_new() {
        let formatter = OutputFormatter::new("pretty");
        assert!(!formatter.format("test").is_empty());
    }

    #[test]
    fn test_output_formatter_format_plain() {
        let formatter = OutputFormatter::new("plain");
        let output = formatter.format("test output");
        assert!(output.contains("test output"));
    }

    #[test]
    fn test_output_formatter_format_json() {
        let formatter = OutputFormatter::new("json");
        let output = formatter.format("test output");
        assert!(output.contains("test"));
    }

    #[test]
    fn test_output_formatter_default() {
        let formatter = OutputFormatter::default();
        assert!(!formatter.format("test").is_empty());
    }

    #[tokio::test]
    async fn test_shell_config_with_timeout() {
        let config = ShellConfig::default();
        assert_eq!(config.approval_timeout, 300);
    }

    #[tokio::test]
    async fn test_shell_config_with_history_size() {
        let config = ShellConfig::default();
        assert_eq!(config.history_size, 10000);
    }
}
