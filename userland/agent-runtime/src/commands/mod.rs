//! Command implementations for the agent runtime CLI.

pub mod agent;
pub mod daemon;
pub mod package;
pub mod service;

/// Truncate a string to `max` characters, appending an ellipsis if truncated.
pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{}…", truncated)
    }
}
