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
        let mode_manager = ModeManager::new(initial_mode, true);
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
