//! Starship-style prompt for AGNOS AI Shell
//!
//! Provides a fast, customizable, cross-shell compatible prompt
//! with AGNOS-specific features like AI mode indicators

use ansi_term::{Color, Style};
use anyhow::Result;
use std::path::PathBuf;
use std::time::SystemTime;

/// Prompt configuration (starship-compatible)
#[derive(Debug, Clone)]
pub struct PromptConfig {
    /// Prompt format string
    pub format: String,
    /// Show AI mode indicator
    pub show_ai_mode: bool,
    /// Show execution time for slow commands
    pub show_execution_time: bool,
    /// Show git status
    pub show_git_status: bool,
    /// Show username/hostname
    pub show_context: bool,
    /// Show current directory
    pub show_directory: bool,
    /// Show exit status of last command
    pub show_exit_status: bool,
    /// Show battery level
    pub show_battery: bool,
    /// Time threshold for showing execution time (ms)
    pub execution_time_threshold: u64,
    /// Character set for prompt
    pub character_set: CharacterSet,
    /// Prompt style (powerline, plain, etc.)
    pub style: PromptStyle,
}

#[derive(Debug, Clone)]
pub enum CharacterSet {
    Unicode,
    ASCII,
    Minimal,
}

#[derive(Debug, Clone)]
pub enum PromptStyle {
    Powerline,
    Plain,
    Minimal,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            format: "$ai_mode$directory$git_branch$character".to_string(),
            show_ai_mode: true,
            show_execution_time: true,
            show_git_status: true,
            show_context: false,
            show_directory: true,
            show_exit_status: true,
            show_battery: false,
            execution_time_threshold: 2000, // 2 seconds
            character_set: CharacterSet::Unicode,
            style: PromptStyle::Powerline,
        }
    }
}

/// Prompt module trait for individual prompt components
pub trait PromptModule {
    fn render(&self, context: &PromptContext) -> Option<String>;
    fn name(&self) -> &'static str;
}

/// Context for prompt rendering
pub struct PromptContext {
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub username: String,
    pub hostname: String,
    pub ai_mode: String,
    pub last_exit_code: i32,
    pub cmd_duration_ms: u64,
    pub start_time: SystemTime,
}

impl PromptContext {
    pub fn new(cwd: PathBuf, username: String, ai_mode: String) -> Self {
        Self {
            cwd,
            home: dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
            username,
            hostname: Self::get_hostname(),
            ai_mode,
            last_exit_code: 0,
            cmd_duration_ms: 0,
            start_time: SystemTime::now(),
        }
    }
    
    fn get_hostname() -> String {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// AI Mode indicator module
pub struct AiModeModule;

impl PromptModule for AiModeModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        if context.ai_mode.is_empty() {
            return None;
        }
        
        let (icon, color) = match context.ai_mode.as_str() {
            "AI-AUTO" => ("🤖", Color::Purple),
            "AI-ASSIST" => ("👤🤖", Color::Cyan),
            "HUMAN" => ("👤", Color::Green),
            "STRICT" => ("🔒", Color::Red),
            _ => ("●", Color::White),
        };
        
        let style = Style::new().fg(color).bold();
        Some(format!("{} ", style.paint(icon)))
    }
    
    fn name(&self) -> &'static str {
        "ai_mode"
    }
}

/// Directory module
pub struct DirectoryModule;

impl PromptModule for DirectoryModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        let path = &context.cwd;
        let display_path = if path.starts_with(&context.home) {
            let shortened = path.strip_prefix(&context.home).ok()?;
            PathBuf::from("~").join(shortened)
        } else {
            path.clone()
        };
        
        let path_str = display_path.to_string_lossy();
        let style = Style::new().fg(Color::Blue).bold();
        
        Some(format!("{} ", style.paint(path_str.to_string())))
    }
    
    fn name(&self) -> &'static str {
        "directory"
    }
}

/// Git branch module
pub struct GitBranchModule;

impl PromptModule for GitBranchModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        // Check if we're in a git repository
        let mut current = context.cwd.clone();
        
        loop {
            let git_dir = current.join(".git");
            if git_dir.exists() {
                // Try to read branch name
                let head_file = git_dir.join("HEAD");
                if let Ok(content) = std::fs::read_to_string(&head_file) {
                    let branch = content.trim().strip_prefix("ref: refs/heads/")
                        .unwrap_or(content.trim());
                    
                    let style = Style::new().fg(Color::Yellow);
                    return Some(format!("{} ", style.paint(format!("({})", branch))));
                }
                break;
            }
            
            if !current.pop() {
                break;
            }
        }
        
        None
    }
    
    fn name(&self) -> &'static str {
        "git_branch"
    }
}

/// Execution time module
pub struct ExecutionTimeModule {
    threshold_ms: u64,
}

impl ExecutionTimeModule {
    pub fn new(threshold_ms: u64) -> Self {
        Self { threshold_ms }
    }
}

impl PromptModule for ExecutionTimeModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        if context.cmd_duration_ms < self.threshold_ms {
            return None;
        }
        
        let duration = if context.cmd_duration_ms < 1000 {
            format!("{}ms", context.cmd_duration_ms)
        } else {
            let secs = context.cmd_duration_ms / 1000;
            let ms = context.cmd_duration_ms % 1000;
            format!("{}.{:03}s", secs, ms)
        };
        
        let style = Style::new().fg(Color::Yellow).dimmed();
        Some(format!("{} ", style.paint(format!("took {}", duration))))
    }
    
    fn name(&self) -> &'static str {
        "execution_time"
    }
}

/// Exit status module
pub struct ExitStatusModule;

impl PromptModule for ExitStatusModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        if context.last_exit_code == 0 {
            return None;
        }
        
        let style = Style::new().fg(Color::Red).bold();
        Some(format!("{} ", style.paint(format!("✗ {}", context.last_exit_code))))
    }
    
    fn name(&self) -> &'static str {
        "exit_status"
    }
}

/// Character/prompt symbol module
pub struct CharacterModule;

impl PromptModule for CharacterModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        let (symbol, color) = if context.last_exit_code == 0 {
            ("❯", Color::Green)
        } else {
            ("❯", Color::Red)
        };
        
        let style = Style::new().fg(color).bold();
        Some(format!("{} ", style.paint(symbol)))
    }
    
    fn name(&self) -> &'static str {
        "character"
    }
}

/// Username/hostname module
pub struct ContextModule;

impl PromptModule for ContextModule {
    fn render(&self, context: &PromptContext) -> Option<String> {
        let user_style = Style::new().fg(Color::Yellow);
        let host_style = Style::new().fg(Color::Green);
        let sep_style = Style::new().dimmed();
        
        Some(format!("{}{}{} ",
            user_style.paint(&context.username),
            sep_style.paint("@"),
            host_style.paint(&context.hostname)
        ))
    }
    
    fn name(&self) -> &'static str {
        "context"
    }
}

/// Main prompt renderer
pub struct PromptRenderer {
    config: PromptConfig,
    modules: Vec<Box<dyn PromptModule>>,
}

impl PromptRenderer {
    pub fn new(config: PromptConfig) -> Self {
        let mut renderer = Self {
            config,
            modules: Vec::new(),
        };
        
        renderer.register_default_modules();
        renderer
    }
    
    fn register_default_modules(&mut self) {
        if self.config.show_ai_mode {
            self.modules.push(Box::new(AiModeModule));
        }
        
        if self.config.show_context {
            self.modules.push(Box::new(ContextModule));
        }
        
        if self.config.show_directory {
            self.modules.push(Box::new(DirectoryModule));
        }
        
        if self.config.show_git_status {
            self.modules.push(Box::new(GitBranchModule));
        }
        
        if self.config.show_execution_time {
            self.modules.push(Box::new(ExecutionTimeModule::new(
                self.config.execution_time_threshold
            )));
        }
        
        if self.config.show_exit_status {
            self.modules.push(Box::new(ExitStatusModule));
        }
        
        self.modules.push(Box::new(CharacterModule));
    }
    
    /// Register a custom module
    pub fn register_module(&mut self, module: Box<dyn PromptModule>) {
        self.modules.push(module);
    }
    
    /// Render the full prompt
    pub fn render(&self, context: &PromptContext) -> String {
        let mut parts = Vec::new();
        
        for module in &self.modules {
            if let Some(rendered) = module.render(context) {
                parts.push(rendered);
            }
        }
        
        parts.join("")
    }
    
    /// Render right-side prompt (RPROMPT style)
    pub fn render_right(&self, context: &PromptContext) -> Option<String> {
        // Placeholder for right-side prompt (time, etc.)
        None
    }
}

impl Default for PromptRenderer {
    fn default() -> Self {
        Self::new(PromptConfig::default())
    }
}

/// Parse format string and extract module names
pub fn parse_format(format: &str) -> Vec<&str> {
    format.split('$')
        .skip(1) // Skip empty first element
        .map(|s| s.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or(s))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_format() {
        let modules = parse_format("$ai_mode$directory$git_branch$character");
        assert_eq!(modules, vec!["ai_mode", "directory", "git_branch", "character"]);
    }
    
    #[test]
    fn test_directory_module() {
        let module = DirectoryModule;
        let ctx = PromptContext::new(
            PathBuf::from("/home/user/projects"),
            "user".to_string(),
            "HUMAN".to_string()
        );
        
        let rendered = module.render(&ctx);
        assert!(rendered.is_some());
    }
    
    #[test]
    fn test_ai_mode_rendering() {
        let module = AiModeModule;

        let ctx_auto = PromptContext::new(
            PathBuf::from("/"),
            "user".to_string(),
            "AI-AUTO".to_string()
        );
        let rendered = module.render(&ctx_auto);
        assert!(rendered.is_some());
        assert!(rendered.unwrap().contains('🤖'));
    }

    #[test]
    fn test_ai_mode_all_modes() {
        let module = AiModeModule;

        // AI-ASSIST
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "AI-ASSIST".into());
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("👤🤖"));

        // HUMAN
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("👤"));

        // STRICT
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "STRICT".into());
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("🔒"));

        // Unknown
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "CUSTOM".into());
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("●"));
    }

    #[test]
    fn test_ai_mode_empty_returns_none() {
        let module = AiModeModule;
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "".into());
        assert!(module.render(&ctx).is_none());
    }

    #[test]
    fn test_ai_mode_name() {
        assert_eq!(AiModeModule.name(), "ai_mode");
    }

    #[test]
    fn test_directory_module_home_abbreviation() {
        let module = DirectoryModule;
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/testuser"));
        let sub = home.join("projects");
        let ctx = PromptContext::new(sub, "u".into(), "HUMAN".into());
        let rendered = module.render(&ctx).unwrap();
        assert!(rendered.contains("~"));
    }

    #[test]
    fn test_directory_module_absolute_path() {
        let module = DirectoryModule;
        let ctx = PromptContext::new(PathBuf::from("/etc/nginx"), "u".into(), "HUMAN".into());
        let rendered = module.render(&ctx).unwrap();
        assert!(rendered.contains("/etc/nginx"));
    }

    #[test]
    fn test_directory_module_name() {
        assert_eq!(DirectoryModule.name(), "directory");
    }

    #[test]
    fn test_git_branch_non_git_dir() {
        let module = GitBranchModule;
        let ctx = PromptContext::new(PathBuf::from("/tmp"), "u".into(), "HUMAN".into());
        assert!(module.render(&ctx).is_none());
    }

    #[test]
    fn test_git_branch_name() {
        assert_eq!(GitBranchModule.name(), "git_branch");
    }

    #[test]
    fn test_execution_time_below_threshold() {
        let module = ExecutionTimeModule::new(2000);
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.cmd_duration_ms = 500;
        assert!(module.render(&ctx).is_none());
    }

    #[test]
    fn test_execution_time_above_threshold_ms() {
        let module = ExecutionTimeModule::new(100);
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.cmd_duration_ms = 500;
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("500ms"));
    }

    #[test]
    fn test_execution_time_seconds_format() {
        let module = ExecutionTimeModule::new(100);
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.cmd_duration_ms = 2500;
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("2.500s"));
    }

    #[test]
    fn test_execution_time_name() {
        assert_eq!(ExecutionTimeModule::new(100).name(), "execution_time");
    }

    #[test]
    fn test_exit_status_zero_returns_none() {
        let module = ExitStatusModule;
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.last_exit_code = 0;
        assert!(module.render(&ctx).is_none());
    }

    #[test]
    fn test_exit_status_nonzero() {
        let module = ExitStatusModule;
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.last_exit_code = 127;
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("127"));
    }

    #[test]
    fn test_exit_status_name() {
        assert_eq!(ExitStatusModule.name(), "exit_status");
    }

    #[test]
    fn test_character_success() {
        let module = CharacterModule;
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.last_exit_code = 0;
        let r = module.render(&ctx).unwrap();
        // Green color ANSI for exit code 0
        assert!(r.contains("❯"));
    }

    #[test]
    fn test_character_failure() {
        let module = CharacterModule;
        let mut ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        ctx.last_exit_code = 1;
        let r = module.render(&ctx).unwrap();
        // Red color ANSI for non-zero exit
        assert!(r.contains("❯"));
    }

    #[test]
    fn test_character_name() {
        assert_eq!(CharacterModule.name(), "character");
    }

    #[test]
    fn test_context_module_renders() {
        let module = ContextModule;
        let ctx = PromptContext::new(PathBuf::from("/"), "alice".into(), "HUMAN".into());
        let r = module.render(&ctx).unwrap();
        assert!(r.contains("alice"));
        assert!(r.contains("@"));
    }

    #[test]
    fn test_context_module_name() {
        assert_eq!(ContextModule.name(), "context");
    }

    #[test]
    fn test_prompt_renderer_new() {
        let config = PromptConfig::default();
        let renderer = PromptRenderer::new(config);
        // Should have modules registered
        assert!(!renderer.modules.is_empty());
    }

    #[test]
    fn test_prompt_renderer_render() {
        let renderer = PromptRenderer::default();
        let ctx = PromptContext::new(PathBuf::from("/tmp"), "user".into(), "HUMAN".into());
        let output = renderer.render(&ctx);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_prompt_renderer_render_right_returns_none() {
        let renderer = PromptRenderer::default();
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        assert!(renderer.render_right(&ctx).is_none());
    }

    #[test]
    fn test_prompt_config_defaults() {
        let config = PromptConfig::default();
        assert!(config.show_ai_mode);
        assert!(config.show_execution_time);
        assert!(config.show_git_status);
        assert!(!config.show_context);
        assert!(config.show_directory);
        assert!(config.show_exit_status);
        assert!(!config.show_battery);
        assert_eq!(config.execution_time_threshold, 2000);
        assert!(config.format.contains("$ai_mode"));
    }

    #[test]
    fn test_parse_format_empty() {
        let modules = parse_format("");
        assert!(modules.is_empty());
    }

    #[test]
    fn test_parse_format_single() {
        let modules = parse_format("$directory");
        assert_eq!(modules, vec!["directory"]);
    }

    #[test]
    fn test_prompt_renderer_default() {
        let renderer = PromptRenderer::default();
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "AI-AUTO".into());
        let output = renderer.render(&ctx);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_prompt_renderer_register_module() {
        let mut renderer = PromptRenderer::new(PromptConfig::default());
        let count_before = renderer.modules.len();
        renderer.register_module(Box::new(ContextModule));
        assert_eq!(renderer.modules.len(), count_before + 1);
    }

    #[test]
    fn test_prompt_context_hostname_populated() {
        let ctx = PromptContext::new(PathBuf::from("/"), "u".into(), "HUMAN".into());
        assert!(!ctx.hostname.is_empty());
    }
}
