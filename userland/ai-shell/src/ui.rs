//! User interface components

use anyhow::Result;
use console::{style, Style, Term};
use dialoguer::{theme::ColorfulTheme, Input};

pub struct Ui {
    term: Term,
    theme: ColorfulTheme,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            term: Term::stdout(),
            theme: ColorfulTheme::default(),
        }
    }
    
    /// Read input from user
    pub async fn read_input(&self, prompt: &str) -> Result<Option<String>> {
        print!("{}", prompt);
        std::io::Write::flush(&mut std::io::stdout())?;
        
        let mut input = String::new();
        match std::io::stdin().read_line(&mut input) {
            Ok(0) => Ok(None), // EOF
            Ok(_) => Ok(Some(input.trim().to_string())),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Show welcome message
    pub fn show_welcome(&self) {
        println!("\n{}", style("╔════════════════════════════════════════════════╗").cyan());
        println!("{}", style("║         Welcome to AGNOS AI Shell (agnsh)      ║").cyan().bold());
        println!("{}", style("║                                                ║").cyan());
        println!("{}", style("║   Natural language interface with built-in     ║").cyan());
        println!("{}", style("║   human oversight and security controls        ║").cyan());
        println!("{}", style("╚════════════════════════════════════════════════╝").cyan());
        println!();
        println!("Type {} to see available commands\n", style("help").yellow());
    }
    
    /// Show goodbye message
    pub fn show_goodbye(&self) {
        println!("\n{} Goodbye!\n", style("👋").dim());
    }
    
    /// Show help
    pub fn show_help(&self) {
        println!("\n{}", style("AGNOS AI Shell Commands:").bold().underline());
        println!();
        println!("  {}    - Show this help", style("help").yellow());
        println!("  {}  - Clear screen", style("clear").yellow());
        println!("  {}  - Show command history", style("history").yellow());
        println!("  {}   - Show current mode", style("mode").yellow());
        println!("  {} - Change mode (human/ai/auto/strict)", style("mode <name>").yellow());
        println!("  {}   - Exit the shell", style("exit/quit").yellow());
        println!();
        println!("{}", style("Modes:").bold());
        println!("  👤       - Human: Direct shell control");
        println!("  👤🤖     - AI-Assist: AI helps interpret commands");
        println!("  🤖       - AI-Auto: AI acts autonomously (with limits)");
        println!("  🔒       - Strict: All commands require approval");
        println!();
        println!("{}", style("Safety:").bold());
        println!("  • AI never has root access");
        println!("  • Dangerous commands are blocked");
        println!("  • System modifications require approval");
        println!("  • All actions are logged");
        println!();
    }
    
    /// Show current mode
    pub fn show_mode(&self, mode: &crate::mode::Mode) {
        println!("Current mode: {}", style(mode).bold());
        println!("Description: {}", mode.description());
    }
    
    /// Show command history
    pub fn show_history(&self, history: &crate::history::CommandHistory) {
        println!("\n{}", style("Command History:").bold().underline());
        println!();
        
        for (i, cmd) in history.get_recent(20).iter().enumerate() {
            println!("  {} {}", style(format!("{:3}", i + 1)).dim(), cmd);
        }
        println!();
    }
    
    /// Show AI thinking
    pub fn show_ai_thinking(&self, message: &str) {
        println!("  {} {}", style("🤔").dim(), style(message).dim());
    }
    
    /// Show proposed action
    pub fn show_proposed_action(&self, translation: &crate::interpreter::Translation) {
        println!("\n  {} {}", style("▶").green(), style(&translation.description).bold());
        println!("  {} {} {}", style("Command:").dim(), translation.command, translation.args.join(" "));
        if !translation.explanation.is_empty() {
            println!("  {} {}", style("Explanation:").dim(), translation.explanation);
        }
        println!();
    }
    
    /// Show output
    pub fn show_output(&self, output: &str) {
        println!("{}", output);
    }
    
    /// Show info message
    pub fn show_info(&self, message: &str) {
        println!("  {} {}", style("ℹ").blue(), message);
    }
    
    /// Show warning
    pub fn show_warning(&self, message: &str) {
        println!("  {} {}", style("⚠").yellow(), style(message).yellow());
    }
    
    /// Show error
    pub fn show_error(&self, message: &str) {
        eprintln!("  {} {}", style("✗").red(), style(message).red());
    }
    
    /// Clear screen
    pub fn clear_screen(&self) {
        let _ = self.term.clear_screen();
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
