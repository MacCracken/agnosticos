//! Natural language interpreter
//!
//! Translates natural language requests into shell commands
//! with safety checks and human oversight.

mod explain;
pub mod intent;
mod parse;
pub(crate) mod patterns;
mod translate;

#[cfg(test)]
mod tests;

use regex::Regex;
use std::collections::HashMap;

pub use intent::{Intent, ListOptions, Translation};

/// Natural language interpreter
pub struct Interpreter {
    patterns: &'static HashMap<String, Regex>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            patterns: &patterns::PATTERNS,
        }
    }

    /// Try to capture against a named pattern. Returns None if the pattern
    /// is missing from the map (defensive) or if it doesn't match.
    fn try_captures<'a>(&'a self, name: &str, input: &'a str) -> Option<regex::Captures<'a>> {
        self.patterns.get(name)?.captures(input)
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
