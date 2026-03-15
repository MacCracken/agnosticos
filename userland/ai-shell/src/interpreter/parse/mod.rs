mod core;
mod creative;
mod platforms;
mod tools;

use super::intent::Intent;
use super::Interpreter;

impl Interpreter {
    /// Parse natural language input into intent
    pub fn parse(&self, input: &str) -> Intent {
        let trimmed = input.trim();
        let lowered = trimmed.to_lowercase();
        let input_lower = lowered.as_str();

        // Pipeline detection: "X | Y" or "X then Y"
        // Must be checked first to avoid greedy pattern matches consuming pipe chars
        if trimmed.contains(" | ") || input_lower.contains(" then ") {
            let parts: Vec<String> = if trimmed.contains(" | ") {
                trimmed
                    .split(" | ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                trimmed
                    .split(" then ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            if parts.len() >= 2 {
                return Intent::Pipeline { commands: parts };
            }
        }

        // Check domain-specific parse functions in order of specificity
        if let Some(intent) = platforms::parse_platforms(self, trimmed, input_lower) {
            return intent;
        }
        if let Some(intent) = creative::parse_creative(self, input_lower) {
            return intent;
        }
        if let Some(intent) = tools::parse_tools(self, input_lower) {
            return intent;
        }
        if let Some(intent) = core::parse_core(self, trimmed, input_lower) {
            return intent;
        }

        Intent::Unknown
    }
}
