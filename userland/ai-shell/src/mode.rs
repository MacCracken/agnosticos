//! Shell mode management
//!
//! Controls whether the shell is in AI autonomous mode,
//! human mode, or collaborative mode.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Shell operating modes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// AI acts autonomously within constraints
    AiAutonomous,
    /// AI assists human user
    AiAssisted,
    /// Human user in control
    Human,
    /// Strict mode - all actions require approval
    Strict,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::AiAssisted
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::AiAutonomous => write!(f, "AI-AUTO"),
            Mode::AiAssisted => write!(f, "AI-ASSIST"),
            Mode::Human => write!(f, "HUMAN"),
            Mode::Strict => write!(f, "STRICT"),
        }
    }
}

impl Mode {
    /// Check if AI is allowed to execute commands autonomously
    pub fn ai_autonomous(&self) -> bool {
        matches!(self, Mode::AiAutonomous)
    }
    
    /// Check if AI assistance is available
    pub fn ai_available(&self) -> bool {
        !matches!(self, Mode::Human)
    }
    
    /// Check if strict approval is required
    pub fn strict_approval(&self) -> bool {
        matches!(self, Mode::Strict)
    }
    
    /// Get description of mode
    pub fn description(&self) -> &'static str {
        match self {
            Mode::AiAutonomous => "AI acts autonomously within safety constraints",
            Mode::AiAssisted => "AI assists human with suggestions and explanations",
            Mode::Human => "Human user has full control, no AI assistance",
            Mode::Strict => "All actions require explicit human approval",
        }
    }
    
    /// Get prompt prefix for this mode
    pub fn prompt_prefix(&self) -> &'static str {
        match self {
            Mode::AiAutonomous => "🤖",
            Mode::AiAssisted => "👤🤖",
            Mode::Human => "👤",
            Mode::Strict => "🔒",
        }
    }
}

/// Mode manager for tracking and switching modes
pub struct ModeManager {
    current: Mode,
    previous: Option<Mode>,
    allow_switching: bool,
}

impl ModeManager {
    pub fn new(initial: Mode, allow_switching: bool) -> Self {
        Self {
            current: initial,
            previous: None,
            allow_switching,
        }
    }
    
    /// Get current mode
    pub fn current(&self) -> &Mode {
        &self.current
    }
    
    /// Switch to a new mode
    pub fn switch(&mut self, mode: Mode) -> Result<()> {
        if !self.allow_switching && self.current != mode {
            return Err(anyhow::anyhow!("Mode switching is disabled"));
        }
        
        self.previous = Some(self.current.clone());
        self.current = mode;
        
        Ok(())
    }
    
    /// Switch back to previous mode
    pub fn revert(&mut self) -> Result<()> {
        if let Some(prev) = self.previous.take() {
            self.current = prev;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No previous mode to revert to"))
        }
    }
    
    /// Toggle between AI and human modes
    pub fn toggle(&mut self) {
        let new_mode = match self.current {
            Mode::Human => Mode::AiAssisted,
            Mode::AiAssisted => Mode::Human,
            Mode::AiAutonomous => Mode::Human,
            Mode::Strict => Mode::AiAssisted,
        };
        
        self.previous = Some(self.current.clone());
        self.current = new_mode;
    }
    
    /// List all available modes
    pub fn available_modes(&self) -> Vec<Mode> {
        vec![
            Mode::Human,
            Mode::AiAssisted,
            Mode::AiAutonomous,
            Mode::Strict,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mode_switching() {
        let mut manager = ModeManager::new(Mode::Human, true);
        
        assert_eq!(manager.current(), &Mode::Human);
        
        manager.switch(Mode::AiAssisted).unwrap();
        assert_eq!(manager.current(), &Mode::AiAssisted);
        
        manager.revert().unwrap();
        assert_eq!(manager.current(), &Mode::Human);
    }
    
    #[test]
    fn test_ai_permissions() {
        assert!(Mode::AiAutonomous.ai_autonomous());
        assert!(!Mode::AiAssisted.ai_autonomous());
        assert!(!Mode::Human.ai_autonomous());
        assert!(!Mode::Strict.ai_autonomous());
    }
}
