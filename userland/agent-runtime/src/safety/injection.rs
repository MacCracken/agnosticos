//! Safety — Prompt injection detection.

use serde::{Deserialize, Serialize};

/// Result of a prompt injection detection check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionResult {
    pub safe: bool,
    pub confidence: f64,
    pub detected_patterns: Vec<String>,
}

/// Detects common prompt-injection attempts in user or agent input.
///
/// **Limitation:** The current detection relies on baseline substring and
/// heuristic pattern matching. These rules catch common injection
/// templates but are inherently bypassable by adversarial rephrasing,
/// encoding tricks, or novel attack vectors. In production deployments
/// this detector should be supplemented with ML-based classification
/// (e.g., a fine-tuned transformer trained on injection corpora) to
/// achieve robust coverage.
///
/// NOTE: Current detection patterns are baseline substring heuristics intended as a
/// first layer of defense. Production deployments should supplement these with
/// ML-based detection (e.g., classifier trained on prompt injection datasets).
/// A named pattern matcher: label + detection function.
type PatternMatcher = (String, Box<dyn Fn(&str) -> bool + Send + Sync>);

pub struct PromptInjectionDetector {
    /// Pattern label + substring/heuristic pairs.
    patterns: Vec<PatternMatcher>,
}

impl PromptInjectionDetector {
    /// Build a detector with the default set of heuristics.
    pub fn new() -> Self {
        let patterns: Vec<PatternMatcher> = vec![
            (
                "ignore_previous_instructions".into(),
                Box::new(|s: &str| {
                    let l = s.to_lowercase();
                    l.contains("ignore previous instructions")
                        || l.contains("ignore all previous")
                        || l.contains("disregard previous")
                        || l.contains("forget previous instructions")
                        || l.contains("ignore your instructions")
                }),
            ),
            (
                "system_prompt_leak".into(),
                Box::new(|s: &str| {
                    let l = s.to_lowercase();
                    l.contains("system prompt:")
                        || l.contains("system message:")
                        || l.contains("reveal your system prompt")
                        || l.contains("show me your instructions")
                        || l.contains("print your system prompt")
                }),
            ),
            (
                "role_confusion".into(),
                Box::new(|s: &str| {
                    let l = s.to_lowercase();
                    l.contains("you are now")
                        || l.contains("act as a")
                        || l.contains("pretend you are")
                        || l.contains("roleplay as")
                        || l.contains("switch to role")
                }),
            ),
            (
                "excessive_special_chars".into(),
                Box::new(|s: &str| {
                    let char_count = s.chars().count();
                    if char_count < 20 {
                        return false;
                    }
                    let special: usize = s
                        .chars()
                        .filter(|c| {
                            !c.is_alphanumeric() && !c.is_whitespace() && *c != '.' && *c != ','
                        })
                        .count();
                    let ratio = special as f64 / char_count as f64;
                    ratio > 0.4
                }),
            ),
            (
                "base64_payload".into(),
                Box::new(|s: &str| {
                    // Heuristic: long runs of base64 characters with padding
                    let char_count = s.chars().count();
                    if char_count < 40 {
                        return false;
                    }
                    let base64_chars: usize = s
                        .chars()
                        .filter(|c| {
                            c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '='
                        })
                        .count();
                    let ratio = base64_chars as f64 / char_count as f64;
                    ratio > 0.85 && s.contains('=')
                }),
            ),
            (
                "delimiter_injection".into(),
                Box::new(|s: &str| {
                    let l = s.to_lowercase();
                    l.contains("```system") || l.contains("---system") || l.contains("[system]")
                }),
            ),
        ];

        Self { patterns }
    }

    /// Strip Unicode zero-width and directional override characters that can
    /// be used to hide injection patterns from substring matching.
    fn normalize_input(input: &str) -> String {
        input
            .chars()
            .filter(|c| {
                !matches!(
                    *c,
                    '\u{200B}' // Zero-Width Space
                    | '\u{200C}' // Zero-Width Non-Joiner
                    | '\u{200D}' // Zero-Width Joiner
                    | '\u{200E}' // Left-to-Right Mark
                    | '\u{200F}' // Right-to-Left Mark
                    | '\u{202A}' // Left-to-Right Embedding
                    | '\u{202B}' // Right-to-Left Embedding
                    | '\u{202C}' // Pop Directional Formatting
                    | '\u{202D}' // Left-to-Right Override
                    | '\u{202E}' // Right-to-Left Override
                    | '\u{2060}' // Word Joiner
                    | '\u{2061}'..='\u{2064}' // Invisible operators
                    | '\u{FEFF}' // BOM / Zero-Width No-Break Space
                    | '\u{FE00}'..='\u{FE0F}' // Variation selectors
                )
            })
            .collect()
    }

    /// Check a string for prompt-injection patterns.
    pub fn check_input(&self, input: &str) -> InjectionResult {
        // Normalize: strip zero-width/invisible characters before matching
        let normalized = Self::normalize_input(input);
        let mut detected: Vec<String> = Vec::new();

        for (label, check_fn) in &self.patterns {
            if check_fn(&normalized) {
                detected.push(label.clone());
            }
        }

        let confidence = if detected.is_empty() {
            0.0
        } else {
            // More patterns matched = higher confidence
            (detected.len() as f64 * 0.25).min(1.0)
        };

        InjectionResult {
            safe: detected.is_empty(),
            confidence,
            detected_patterns: detected,
        }
    }
}

impl Default for PromptInjectionDetector {
    fn default() -> Self {
        Self::new()
    }
}
