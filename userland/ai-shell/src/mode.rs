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

    #[test]
    fn test_mode_display() {
        assert_eq!(format!("{}", Mode::AiAutonomous), "AI-AUTO");
        assert_eq!(format!("{}", Mode::AiAssisted), "AI-ASSIST");
        assert_eq!(format!("{}", Mode::Human), "HUMAN");
        assert_eq!(format!("{}", Mode::Strict), "STRICT");
    }

    #[test]
    fn test_mode_default() {
        assert_eq!(Mode::default(), Mode::AiAssisted);
    }

    #[test]
    fn test_mode_ai_available() {
        assert!(Mode::AiAutonomous.ai_available());
        assert!(Mode::AiAssisted.ai_available());
        assert!(!Mode::Human.ai_available());
        assert!(Mode::Strict.ai_available());
    }

    #[test]
    fn test_mode_strict_approval() {
        assert!(!Mode::AiAutonomous.strict_approval());
        assert!(!Mode::AiAssisted.strict_approval());
        assert!(!Mode::Human.strict_approval());
        assert!(Mode::Strict.strict_approval());
    }

    #[test]
    fn test_mode_description() {
        assert!(Mode::AiAutonomous.description().contains("autonomous"));
        assert!(Mode::AiAssisted.description().contains("assists"));
        assert!(Mode::Human.description().contains("control"));
        assert!(Mode::Strict.description().contains("approval"));
    }

    #[test]
    fn test_mode_prompt_prefix() {
        assert!(!Mode::AiAutonomous.prompt_prefix().is_empty());
        assert!(!Mode::AiAssisted.prompt_prefix().is_empty());
        assert!(!Mode::Human.prompt_prefix().is_empty());
        assert!(!Mode::Strict.prompt_prefix().is_empty());
    }

    #[test]
    fn test_mode_switching_disabled() {
        let mut manager = ModeManager::new(Mode::Human, false);
        let result = manager.switch(Mode::AiAssisted);
        assert!(result.is_err());
        assert_eq!(manager.current(), &Mode::Human);
    }

    #[test]
    fn test_mode_switching_same_mode_when_disabled() {
        let mut manager = ModeManager::new(Mode::Human, false);
        // Switching to the same mode should succeed even when disabled
        let result = manager.switch(Mode::Human);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mode_revert_no_previous() {
        let mut manager = ModeManager::new(Mode::Human, true);
        let result = manager.revert();
        assert!(result.is_err());
    }

    #[test]
    fn test_mode_toggle_human_to_ai() {
        let mut manager = ModeManager::new(Mode::Human, true);
        manager.toggle();
        assert_eq!(manager.current(), &Mode::AiAssisted);
    }

    #[test]
    fn test_mode_toggle_ai_assisted_to_human() {
        let mut manager = ModeManager::new(Mode::AiAssisted, true);
        manager.toggle();
        assert_eq!(manager.current(), &Mode::Human);
    }

    #[test]
    fn test_mode_toggle_autonomous_to_human() {
        let mut manager = ModeManager::new(Mode::AiAutonomous, true);
        manager.toggle();
        assert_eq!(manager.current(), &Mode::Human);
    }

    #[test]
    fn test_mode_toggle_strict_to_ai_assisted() {
        let mut manager = ModeManager::new(Mode::Strict, true);
        manager.toggle();
        assert_eq!(manager.current(), &Mode::AiAssisted);
    }

    #[test]
    fn test_available_modes() {
        let manager = ModeManager::new(Mode::Human, true);
        let modes = manager.available_modes();
        assert_eq!(modes.len(), 4);
        assert!(modes.contains(&Mode::Human));
        assert!(modes.contains(&Mode::AiAssisted));
        assert!(modes.contains(&Mode::AiAutonomous));
        assert!(modes.contains(&Mode::Strict));
    }

    #[test]
    fn test_mode_serialization() {
        let mode = Mode::AiAssisted;
        let json = serde_json::to_string(&mode).unwrap();
        let deserialized: Mode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, mode);
    }
}
