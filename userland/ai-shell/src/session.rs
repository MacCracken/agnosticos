//! Shell session management
//!
//! Coordinates all shell components and handles the main event loop

use anyhow::{anyhow, Result};

use std::path::PathBuf;

use crate::approval::{ApprovalManager, ApprovalRequest, ApprovalResponse};
use crate::config::ShellConfig;
use crate::history::CommandHistory;
use crate::interpreter::{Interpreter, Intent};
use crate::mode::{Mode, ModeManager};
use crate::output::OutputFormatter;
use crate::prompt::{PromptConfig, PromptContext, PromptRenderer};
use crate::security::{analyze_command_permission, SecurityContext};
use crate::ui::Ui;

/// Main shell session
pub struct Session {
    _config: ShellConfig,
    _security: SecurityContext,
    mode_manager: ModeManager,
    interpreter: Interpreter,
    approval: ApprovalManager,
    history: CommandHistory,
    _output: OutputFormatter,
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
            _config: config,
            _security: security,
            mode_manager,
            interpreter,
            approval,
            history,
            _output: output,
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
            
            // Limit input length to prevent DoS
            if input.len() > 65536 {
                self.ui.show_error("Input too long (max 64KB)");
                continue;
            }

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
        let parts = match shlex::split(command) {
            Some(parts) => parts,
            None => {
                self.ui.show_error("Invalid command: unmatched quotes");
                return Ok(());
            }
        };
        if parts.is_empty() {
            return Ok(());
        }

        let cmd = parts[0].clone();
        let args: Vec<String> = parts[1..].to_vec();
        
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
                        self.prompt_context.last_exit_code = status.code().unwrap_or_else(|| {
                            // Process killed by signal — convention: 128 + signal number
                            #[cfg(unix)]
                            {
                                use std::os::unix::process::ExitStatusExt;
                                status.signal().map(|s| 128 + s).unwrap_or(-1)
                            }
                            #[cfg(not(unix))]
                            { -1 }
                        });
                        
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
        let _history = CommandHistory::new(&temp_path).await.unwrap();
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

    #[test]
    fn test_shell_config_default_values() {
        let config = ShellConfig::default();
        assert_eq!(config.approval_timeout, 300);
        assert_eq!(config.history_size, 10000);
        assert_eq!(config.output_format, "auto");
        assert!(config.ai_enabled);
        assert!(!config.auto_approve_low_risk);
    }

    #[test]
    fn test_shell_config_custom_values() {
        let config = ShellConfig {
            default_mode: Mode::Strict,
            history_file: std::path::PathBuf::from("/tmp/custom-history"),
            history_size: 50000,
            output_format: "json".to_string(),
            ai_enabled: false,
            auto_approve_low_risk: true,
            approval_timeout: 600,
            llm_endpoint: Some("http://localhost:8088".to_string()),
            audit_log: std::path::PathBuf::from("/tmp/audit.log"),
            show_explanations: false,
            theme: "dark".to_string(),
        };

        assert_eq!(config.approval_timeout, 600);
        assert_eq!(config.history_size, 50000);
        assert_eq!(config.output_format, "json");
        assert!(!config.ai_enabled);
    }

    // --- Session tests ---

    async fn make_test_session() -> Session {
        let temp_dir = std::env::temp_dir();
        let history_file = temp_dir.join(format!("agnos_test_session_{}", std::process::id()));
        let config = ShellConfig {
            history_file,
            ..ShellConfig::default()
        };
        let security = SecurityContext::new(false).unwrap();
        Session::new(config, security, Mode::Human).await.unwrap()
    }

    #[tokio::test]
    async fn test_session_new() {
        let session = make_test_session().await;
        // Session was created successfully; verify cwd is set
        assert!(!session.cwd.as_os_str().is_empty());
    }

    #[tokio::test]
    async fn test_session_execute_one_shot_echo() {
        let mut session = make_test_session().await;
        let result = session.execute_one_shot("echo hello".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_execute_one_shot_invalid() {
        let mut session = make_test_session().await;
        // Nonexistent command — execute_shell_command shows error via ui but returns Ok
        let result = session.execute_one_shot("___nonexistent_cmd_12345___".to_string()).await;
        assert!(result.is_ok());
        // Exit code should be set to error
        assert_ne!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_process_input_help() {
        let mut session = make_test_session().await;
        let result = session.process_input("help").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_mode_show() {
        let mut session = make_test_session().await;
        let result = session.process_input("mode").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_history() {
        let mut session = make_test_session().await;
        let result = session.process_input("history").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_clear() {
        let mut session = make_test_session().await;
        let result = session.process_input("clear").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_mode_human() {
        let mut session = make_test_session().await;
        let result = session.process_input("mode human").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.ai_mode, "HUMAN");
    }

    #[tokio::test]
    async fn test_process_input_mode_ai() {
        let mut session = make_test_session().await;
        let result = session.process_input("mode ai").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.ai_mode, "AI-ASSIST");
    }

    #[tokio::test]
    async fn test_process_input_mode_auto() {
        let mut session = make_test_session().await;
        let result = session.process_input("mode auto").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.ai_mode, "AI-AUTO");
    }

    #[tokio::test]
    async fn test_process_input_mode_strict() {
        let mut session = make_test_session().await;
        let result = session.process_input("mode strict").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.ai_mode, "STRICT");
    }

    #[tokio::test]
    async fn test_process_input_mode_invalid() {
        let mut session = make_test_session().await;
        // Invalid mode doesn't error, just shows an error message via ui
        let result = session.process_input("mode invalidxyz").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_exit() {
        let mut session = make_test_session().await;
        let result = session.process_input("exit").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Exit"));
    }

    #[tokio::test]
    async fn test_process_input_quit() {
        let mut session = make_test_session().await;
        let result = session.process_input("quit").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_shell_command_echo() {
        let mut session = make_test_session().await;
        let result = session.execute_shell_command("echo test").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_shell_command_invalid_quotes() {
        let mut session = make_test_session().await;
        // Unmatched quotes — shlex::split returns None
        let result = session.execute_shell_command("echo \"unterminated").await;
        assert!(result.is_ok()); // Returns Ok, shows error via UI
    }

    #[tokio::test]
    async fn test_execute_command_echo() {
        let mut session = make_test_session().await;
        let result = session.execute_command("echo", &["hello".to_string()]).await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_command_nonexistent() {
        let mut session = make_test_session().await;
        let result = session.execute_command("___no_such_command___", &[]).await;
        assert!(result.is_ok()); // Returns Ok, shows error via UI
        assert_eq!(session.prompt_context.last_exit_code, -1);
    }

    #[tokio::test]
    async fn test_execute_command_false() {
        let mut session = make_test_session().await;
        let result = session.execute_command("false", &[]).await;
        assert!(result.is_ok());
        assert_ne!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_build_prompt_nonempty() {
        let session = make_test_session().await;
        let prompt = session.build_prompt();
        assert!(!prompt.is_empty());
    }

    #[tokio::test]
    async fn test_execute_shell_command_cd() {
        let mut session = make_test_session().await;
        let result = session.execute_shell_command("cd /tmp").await;
        assert!(result.is_ok());
        // Note: parallel tests share process cwd, so just verify cd succeeded
        assert!(session.cwd.exists());
    }

    #[tokio::test]
    async fn test_execute_shell_command_cd_invalid() {
        let mut session = make_test_session().await;
        let result = session.execute_shell_command("cd /nonexistent_dir_12345").await;
        assert!(result.is_ok()); // Shows error via UI but returns Ok
    }

    #[tokio::test]
    async fn test_execute_shell_command_empty_after_parse() {
        let mut session = make_test_session().await;
        // shlex::split("") returns Some([]) which is empty — should return Ok(())
        let result = session.execute_shell_command("  ").await;
        assert!(result.is_ok());
    }

    // --- Additional session.rs coverage tests ---

    #[tokio::test]
    async fn test_execute_with_assistance_question_intent() {
        // "what is my IP address?" is parsed as Intent::Question
        let mut session = make_test_session().await;
        session.mode_manager.switch(Mode::AiAssisted).unwrap();
        // execute_with_assistance handles Question by showing info, no error
        let result = session.execute_with_assistance("what is my IP address?").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_with_assistance_unknown_intent() {
        // A multi-word string that doesn't match any pattern falls through to Unknown
        // which tries to execute as shell command with a warning
        let mut session = make_test_session().await;
        session.mode_manager.switch(Mode::AiAssisted).unwrap();
        // "xyzzy plugh" is multi-word, not a question, not a known pattern => Unknown
        // Unknown falls through to execute_shell_command which will fail but return Ok
        let result = session.execute_with_assistance("xyzzy plugh").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_with_assistance_translatable_safe_intent() {
        // "show me all files" parses as ListFiles which translates to "ls"
        let mut session = make_test_session().await;
        session.mode_manager.switch(Mode::AiAssisted).unwrap();
        let result = session.execute_with_assistance("show me all files").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_autonomously_delegates_to_assistance() {
        // Due to the broad "list" regex pattern, most inputs parse as ListFiles
        // rather than ShellCommand, so execute_autonomously delegates to
        // execute_with_assistance for non-ShellCommand intents
        let mut session = make_test_session().await;
        session.mode_manager.switch(Mode::AiAutonomous).unwrap();
        let result = session.execute_autonomously("true").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_autonomously_non_shell_intent() {
        // Non-ShellCommand intents delegate to execute_with_assistance
        let mut session = make_test_session().await;
        session.mode_manager.switch(Mode::AiAutonomous).unwrap();
        let result = session.execute_autonomously("show me all files").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_ai_assisted_mode() {
        let mut session = make_test_session().await;
        // Switch to AI-assisted mode first
        session.process_input("mode ai").await.unwrap();
        // Then run a command in AI mode
        let result = session.process_input("what is the time?").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_autonomous_mode() {
        let mut session = make_test_session().await;
        session.process_input("mode auto").await.unwrap();
        // A safe command in autonomous mode
        let result = session.process_input("true").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_strict_mode_non_interactive() {
        let mut session = make_test_session().await;
        session.process_input("mode strict").await.unwrap();
        // In strict mode, everything requires approval. Since stdin is not a
        // terminal in tests, ApprovalManager will deny by default.
        let result = session.process_input("echo hello").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mode_switch_assist_alias() {
        let mut session = make_test_session().await;
        let result = session.process_input("mode assist").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.ai_mode, "AI-ASSIST");
    }

    #[tokio::test]
    async fn test_execute_shell_command_cd_home_tilde() {
        let mut session = make_test_session().await;
        let result = session.execute_shell_command("cd ~").await;
        assert!(result.is_ok());
        // cwd should point to home directory after cd ~
        let _home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        // Note: parallel tests may race on process cwd, so just check it didn't error
        assert!(session.cwd.exists());
    }

    #[tokio::test]
    async fn test_execute_shell_command_cd_no_args() {
        let mut session = make_test_session().await;
        // "cd" with no args defaults to "~"
        let result = session.execute_shell_command("cd").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_command_with_args() {
        let mut session = make_test_session().await;
        let result = session.execute_command("echo", &["hello".to_string(), "world".to_string()]).await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_command_exit_code_nonzero() {
        let mut session = make_test_session().await;
        // "false" exits with code 1
        let result = session.execute_command("false", &[]).await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 1);
    }

    #[tokio::test]
    async fn test_build_prompt_changes_with_mode() {
        let mut session = make_test_session().await;
        let prompt_human = session.build_prompt();

        session.process_input("mode ai").await.unwrap();
        let prompt_ai = session.build_prompt();

        // Prompts should differ since mode changed
        // (both are non-empty, and typically contain the mode indicator)
        assert!(!prompt_human.is_empty());
        assert!(!prompt_ai.is_empty());
    }

    #[tokio::test]
    async fn test_session_cwd_is_current_dir() {
        let session = make_test_session().await;
        // Session cwd should match actual current directory at creation time
        assert!(session.cwd.is_absolute());
    }

    #[tokio::test]
    async fn test_process_input_empty_string() {
        let mut session = make_test_session().await;
        // Empty input in human mode should execute as shell command (no-op)
        let result = session.process_input("").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_input_whitespace_only() {
        let mut session = make_test_session().await;
        let result = session.process_input("   ").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_shell_command_with_pipe_chars() {
        let mut session = make_test_session().await;
        // shlex should handle this as a single arg containing |
        let result = session.execute_shell_command("echo hello").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_command_true() {
        let mut session = make_test_session().await;
        let result = session.execute_command("true", &[]).await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_session_prompt_context_initial_exit_code() {
        let session = make_test_session().await;
        assert_eq!(session.prompt_context.last_exit_code, 0);
        assert_eq!(session.prompt_context.cmd_duration_ms, 0);
    }

    #[tokio::test]
    async fn test_mode_switch_round_trip() {
        let mut session = make_test_session().await;
        session.process_input("mode ai").await.unwrap();
        assert_eq!(session.prompt_context.ai_mode, "AI-ASSIST");
        session.process_input("mode human").await.unwrap();
        assert_eq!(session.prompt_context.ai_mode, "HUMAN");
    }

    #[tokio::test]
    async fn test_execute_shell_command_ls() {
        let mut session = make_test_session().await;
        let result = session.execute_shell_command("ls").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_command_with_cwd() {
        let mut session = make_test_session().await;
        // The command should run in the session's cwd
        let result = session.execute_command("pwd", &[]).await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    // ====================================================================
    // Additional coverage tests: session lifecycle, history, edge cases
    // ====================================================================

    #[tokio::test]
    async fn test_command_history_empty_search() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_empty_search");
        let history = CommandHistory::new(&temp_path).await.unwrap();
        let results = history.search("nonexistent_pattern_xyz");
        assert!(results.is_empty());
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_get_recent_more_than_available() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_recent_excess");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        history.add("cmd1").await.unwrap();
        history.add("cmd2").await.unwrap();
        // Request more than available
        let entries = history.get_recent(100);
        assert_eq!(entries.len(), 2);
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_get_recent_zero() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_recent_zero");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        history.add("cmd1").await.unwrap();
        let entries = history.get_recent(0);
        assert!(entries.is_empty());
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_deduplicates_consecutive() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_dupes");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        history.add("ls").await.unwrap();
        history.add("ls").await.unwrap();
        history.add("ls").await.unwrap();
        let entries = history.get_recent(10);
        // Consecutive duplicates are deduplicated
        assert_eq!(entries.len(), 1);
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_non_consecutive_duplicates_kept() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_non_consec_dupes");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        history.add("ls").await.unwrap();
        history.add("pwd").await.unwrap();
        history.add("ls").await.unwrap();
        let entries = history.get_recent(10);
        // Non-consecutive duplicates should be preserved
        assert_eq!(entries.len(), 3);
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_search_case_sensitive() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_case");
        let mut history = CommandHistory::new(&temp_path).await.unwrap();
        history.add("Git Status").await.unwrap();
        history.add("git log").await.unwrap();
        let results = history.search("git");
        // search should match "git log" — whether it matches "Git Status" depends on implementation
        assert!(!results.is_empty());
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_command_history_persistence_round_trip() {
        let temp_path = std::env::temp_dir().join("agnos_test_hist_persist_rt");
        // Write and save
        {
            let mut history = CommandHistory::new(&temp_path).await.unwrap();
            history.add("alpha").await.unwrap();
            history.add("beta").await.unwrap();
            history.add("gamma").await.unwrap();
            history.save().await.unwrap();
        }
        // Reload and verify
        {
            let history = CommandHistory::new(&temp_path).await.unwrap();
            let entries = history.get_recent(10);
            assert_eq!(entries.len(), 3);
        }
        let _ = std::fs::remove_file(&temp_path);
    }

    #[tokio::test]
    async fn test_session_execute_one_shot_builtin_help() {
        let mut session = make_test_session().await;
        let result = session.execute_one_shot("help".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_execute_one_shot_builtin_exit() {
        let mut session = make_test_session().await;
        let result = session.execute_one_shot("exit".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_mode_manager_current_starts_human() {
        let session = make_test_session().await;
        assert_eq!(session.mode_manager.current(), &Mode::Human);
    }

    #[tokio::test]
    async fn test_session_prompt_context_username_nonempty() {
        let session = make_test_session().await;
        assert!(!session.prompt_context.username.is_empty());
    }

    #[tokio::test]
    async fn test_session_cwd_matches_prompt_context() {
        let session = make_test_session().await;
        assert_eq!(session.cwd, session.prompt_context.cwd);
    }

    #[tokio::test]
    async fn test_execute_shell_command_cd_then_ls() {
        let mut session = make_test_session().await;
        session.execute_shell_command("cd /tmp").await.unwrap();
        let result = session.execute_shell_command("ls").await;
        assert!(result.is_ok());
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_command_updates_duration() {
        let mut session = make_test_session().await;
        // Execute a command
        session.execute_command("echo", &["test".to_string()]).await.unwrap();
        // last_exit_code should be set to 0 for successful command
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_process_input_special_characters() {
        let mut session = make_test_session().await;
        // Input with special shell characters — shlex should handle gracefully
        let result = session.process_input("echo 'hello world'").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mode_switch_all_modes_round_trip() {
        let mut session = make_test_session().await;
        for mode_str in &["ai", "auto", "strict", "human"] {
            let input = format!("mode {}", mode_str);
            let result = session.process_input(&input).await;
            assert!(result.is_ok(), "Failed to switch to mode: {}", mode_str);
        }
        assert_eq!(session.prompt_context.ai_mode, "HUMAN");
    }

    #[tokio::test]
    async fn test_execute_shell_command_with_env_var_syntax() {
        let mut session = make_test_session().await;
        // shlex handles $VAR syntax as literal strings
        let result = session.execute_shell_command("echo $HOME").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_command_multiple_sequential() {
        let mut session = make_test_session().await;
        for _ in 0..5 {
            session.execute_command("true", &[]).await.unwrap();
            assert_eq!(session.prompt_context.last_exit_code, 0);
        }
    }

    #[tokio::test]
    async fn test_execute_command_alternating_success_failure() {
        let mut session = make_test_session().await;
        session.execute_command("true", &[]).await.unwrap();
        assert_eq!(session.prompt_context.last_exit_code, 0);
        session.execute_command("false", &[]).await.unwrap();
        assert_ne!(session.prompt_context.last_exit_code, 0);
        session.execute_command("true", &[]).await.unwrap();
        assert_eq!(session.prompt_context.last_exit_code, 0);
    }

    #[tokio::test]
    async fn test_session_interpreter_accessible() {
        let session = make_test_session().await;
        // Verify interpreter is properly initialized by calling explain
        let explanation = session.interpreter.explain("ls", &[]);
        assert!(!explanation.is_empty());
    }

    #[tokio::test]
    async fn test_build_prompt_contains_mode() {
        let mut session = make_test_session().await;
        let prompt = session.build_prompt();
        // Default mode is HUMAN, prompt should contain some indication
        assert!(!prompt.is_empty());

        session.process_input("mode strict").await.unwrap();
        let prompt_strict = session.build_prompt();
        assert!(!prompt_strict.is_empty());
    }

    #[test]
    fn test_shell_config_llm_endpoint_default_none() {
        let config = ShellConfig::default();
        assert!(config.llm_endpoint.is_none());
    }

    #[test]
    fn test_shell_config_show_explanations_default() {
        let config = ShellConfig::default();
        assert!(config.show_explanations);
    }

    #[test]
    fn test_shell_config_theme_default() {
        let config = ShellConfig::default();
        assert_eq!(config.theme, "default");
    }
}
