//! Externalization Gate — Scan-before-send for sandbox egress
//!
//! Inspects outbound data from sandboxed agents before it leaves the system.
//! Blocks transmission if secrets, PII, or sensitive patterns are detected.
//!
//! Complements the credential proxy (which handles inbound auth injection)
//! by guarding the outbound path.
//!
//! Inspired by SecureYeoman's externalization-gate pattern.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::warn;

// ---------------------------------------------------------------------------
// Detection Patterns
// ---------------------------------------------------------------------------

/// A pattern that flags sensitive content in outbound data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivePattern {
    /// Pattern name for audit logs.
    pub name: String,
    /// Regex pattern to match.
    pub pattern: String,
    /// Severity: low, medium, high, critical.
    pub severity: PatternSeverity,
    /// Category for grouping (e.g., "api_key", "pii", "credential", "internal").
    pub category: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for PatternSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Built-in patterns for common sensitive data.
pub fn builtin_patterns() -> Vec<SensitivePattern> {
    vec![
        // API keys
        SensitivePattern {
            name: "OpenAI API Key".into(),
            pattern: r"sk-[a-zA-Z0-9]{20,}".into(),
            severity: PatternSeverity::Critical,
            category: "api_key".into(),
        },
        SensitivePattern {
            name: "Anthropic API Key".into(),
            pattern: r"sk-ant-[a-zA-Z0-9\-]{20,}".into(),
            severity: PatternSeverity::Critical,
            category: "api_key".into(),
        },
        SensitivePattern {
            name: "AWS Access Key".into(),
            pattern: r"AKIA[0-9A-Z]{16}".into(),
            severity: PatternSeverity::Critical,
            category: "credential".into(),
        },
        SensitivePattern {
            name: "AWS Secret Key".into(),
            pattern: r"(?i)aws_secret_access_key\s*[=:]\s*[A-Za-z0-9/+=]{40}".into(),
            severity: PatternSeverity::Critical,
            category: "credential".into(),
        },
        SensitivePattern {
            name: "GitHub Token".into(),
            pattern: r"gh[ps]_[A-Za-z0-9_]{36,}".into(),
            severity: PatternSeverity::Critical,
            category: "api_key".into(),
        },
        SensitivePattern {
            name: "Generic Bearer Token".into(),
            pattern: r"Bearer\s+[A-Za-z0-9\-_.~+/]+=*".into(),
            severity: PatternSeverity::High,
            category: "credential".into(),
        },
        SensitivePattern {
            name: "Private Key Block".into(),
            pattern: r"-----BEGIN\s+(RSA|EC|DSA|OPENSSH|ENCRYPTED)?\s*PRIVATE KEY-----".into(),
            severity: PatternSeverity::Critical,
            category: "credential".into(),
        },
        // PII
        SensitivePattern {
            name: "Email Address".into(),
            pattern: r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}".into(),
            severity: PatternSeverity::Medium,
            category: "pii".into(),
        },
        SensitivePattern {
            name: "SSN (US)".into(),
            pattern: r"\b\d{3}-\d{2}-\d{4}\b".into(),
            severity: PatternSeverity::Critical,
            category: "pii".into(),
        },
        SensitivePattern {
            name: "Credit Card Number".into(),
            pattern: r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13})\b".into(),
            severity: PatternSeverity::Critical,
            category: "pii".into(),
        },
        // Internal
        SensitivePattern {
            name: "AGNOS Audit Hash".into(),
            pattern: r"agnos_audit_hash:[a-f0-9]{64}".into(),
            severity: PatternSeverity::High,
            category: "internal".into(),
        },
        SensitivePattern {
            name: "Internal Socket Path".into(),
            pattern: r"/run/agnos/agents/[a-f0-9\-]+\.sock".into(),
            severity: PatternSeverity::Medium,
            category: "internal".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Gate Decision
// ---------------------------------------------------------------------------

/// Result of scanning outbound data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateDecision {
    /// Whether the data is allowed to leave.
    pub allowed: bool,
    /// Findings that triggered a block (or warnings if allowed).
    pub findings: Vec<GateFinding>,
    /// Data size in bytes.
    pub data_size: usize,
    /// Scan duration in microseconds.
    pub scan_duration_us: u64,
}

/// A single finding from the externalization gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateFinding {
    pub pattern_name: String,
    pub severity: PatternSeverity,
    pub category: String,
    /// Byte offset of the match (approximate).
    pub offset: usize,
    /// Length of the match.
    pub match_len: usize,
    /// Redacted snippet for audit (first/last few chars only).
    pub redacted_snippet: String,
}

// ---------------------------------------------------------------------------
// Externalization Gate
// ---------------------------------------------------------------------------

/// Configuration for the externalization gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalizationGateConfig {
    /// Minimum severity to block (Critical = block only critical, Low = block everything).
    pub block_threshold: PatternSeverity,
    /// Additional custom patterns.
    pub custom_patterns: Vec<SensitivePattern>,
    /// Categories to ignore (e.g., "pii" in dev mode).
    pub ignored_categories: Vec<String>,
    /// Maximum data size to scan (larger payloads are blocked outright).
    pub max_scan_size_bytes: usize,
}

impl Default for ExternalizationGateConfig {
    fn default() -> Self {
        Self {
            block_threshold: PatternSeverity::High,
            custom_patterns: vec![],
            ignored_categories: vec![],
            max_scan_size_bytes: 50 * 1024 * 1024, // 50 MB
        }
    }
}

/// The externalization gate scanner.
#[derive(Debug, Clone)]
pub struct ExternalizationGate {
    config: ExternalizationGateConfig,
    compiled_patterns: Vec<(SensitivePattern, regex::Regex)>,
    /// Stats: total scans, total blocked, total findings.
    stats: GateStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GateStats {
    pub total_scans: u64,
    pub total_blocked: u64,
    pub total_findings: u64,
    pub findings_by_category: HashMap<String, u64>,
}

impl ExternalizationGate {
    pub fn new(config: ExternalizationGateConfig) -> Self {
        let mut all_patterns = builtin_patterns();
        all_patterns.extend(config.custom_patterns.clone());

        let compiled: Vec<_> = all_patterns
            .into_iter()
            .filter(|p| !config.ignored_categories.contains(&p.category))
            .filter_map(|p| {
                regex::Regex::new(&p.pattern)
                    .ok()
                    .map(|r| (p, r))
            })
            .collect();

        Self {
            config,
            compiled_patterns: compiled,
            stats: GateStats::default(),
        }
    }

    /// Scan outbound data. Returns a decision with findings.
    pub fn scan(&mut self, data: &[u8], agent_id: &str) -> GateDecision {
        let start = std::time::Instant::now();
        self.stats.total_scans += 1;

        // Block oversized payloads
        if data.len() > self.config.max_scan_size_bytes {
            self.stats.total_blocked += 1;
            return GateDecision {
                allowed: false,
                findings: vec![GateFinding {
                    pattern_name: "Payload too large".into(),
                    severity: PatternSeverity::High,
                    category: "size".into(),
                    offset: 0,
                    match_len: data.len(),
                    redacted_snippet: format!("{}B exceeds {}B limit", data.len(), self.config.max_scan_size_bytes),
                }],
                data_size: data.len(),
                scan_duration_us: start.elapsed().as_micros() as u64,
            };
        }

        // Convert to string for regex scanning (lossy — binary data gets replaced)
        let text = String::from_utf8_lossy(data);
        let mut findings = Vec::new();
        let threshold = self.config.block_threshold;

        for (pattern, regex) in &self.compiled_patterns {
            for mat in regex.find_iter(&text) {
                let matched_str = mat.as_str();
                let snippet = if matched_str.len() > 8 {
                    format!(
                        "{}...{}",
                        &matched_str[..3],
                        &matched_str[matched_str.len() - 3..]
                    )
                } else {
                    "***".to_string()
                };

                findings.push(GateFinding {
                    pattern_name: pattern.name.clone(),
                    severity: pattern.severity,
                    category: pattern.category.clone(),
                    offset: mat.start(),
                    match_len: mat.len(),
                    redacted_snippet: snippet,
                });
            }
        }

        // Update stats (after pattern iteration completes, releasing the borrow)
        self.stats.total_findings += findings.len() as u64;
        for f in &findings {
            *self
                .stats
                .findings_by_category
                .entry(f.category.clone())
                .or_insert(0) += 1;
        }

        // Determine if we should block
        let should_block = findings
            .iter()
            .any(|f| severity_rank(f.severity) >= severity_rank(threshold));

        if should_block {
            self.stats.total_blocked += 1;
            warn!(
                agent_id = %agent_id,
                findings = findings.len(),
                "Externalization gate BLOCKED outbound data"
            );
        }

        GateDecision {
            allowed: !should_block,
            findings,
            data_size: data.len(),
            scan_duration_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Get gate statistics.
    pub fn stats(&self) -> &GateStats {
        &self.stats
    }

    /// Get pattern count.
    pub fn pattern_count(&self) -> usize {
        self.compiled_patterns.len()
    }
}

fn severity_rank(s: PatternSeverity) -> u8 {
    match s {
        PatternSeverity::Low => 0,
        PatternSeverity::Medium => 1,
        PatternSeverity::High => 2,
        PatternSeverity::Critical => 3,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> ExternalizationGate {
        ExternalizationGate::new(ExternalizationGateConfig::default())
    }

    #[test]
    fn test_clean_data_passes() {
        let mut g = gate();
        let decision = g.scan(b"Hello, this is a normal response.", "agent-1");
        assert!(decision.allowed);
        assert!(decision.findings.is_empty());
    }

    #[test]
    fn test_openai_key_blocked() {
        let mut g = gate();
        let data = b"Here is the key: sk-abcdefghijklmnopqrstuvwxyz123456";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
        assert_eq!(decision.findings.len(), 1);
        assert_eq!(decision.findings[0].category, "api_key");
        assert_eq!(decision.findings[0].severity, PatternSeverity::Critical);
    }

    #[test]
    fn test_anthropic_key_blocked() {
        let mut g = gate();
        let data = b"Use this: sk-ant-api03-abcdefghijklmnopqrstuvwxyz";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
        assert!(decision.findings.iter().any(|f| f.pattern_name.contains("Anthropic")));
    }

    #[test]
    fn test_aws_key_blocked() {
        let mut g = gate();
        let data = b"Access key: AKIAIOSFODNN7EXAMPLE";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
    }

    #[test]
    fn test_private_key_blocked() {
        let mut g = gate();
        let data = b"-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIB...";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
        assert!(decision.findings.iter().any(|f| f.pattern_name.contains("Private Key")));
    }

    #[test]
    fn test_ssn_blocked() {
        let mut g = gate();
        let data = b"SSN: 123-45-6789";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
        assert!(decision.findings.iter().any(|f| f.category == "pii"));
    }

    #[test]
    fn test_email_below_threshold() {
        // Default threshold is High; email is Medium
        let mut g = gate();
        let data = b"Contact: user@example.com";
        let decision = g.scan(data, "agent-1");
        assert!(decision.allowed); // Medium < High threshold
        assert_eq!(decision.findings.len(), 1); // Still reported as finding
    }

    #[test]
    fn test_oversized_payload_blocked() {
        let config = ExternalizationGateConfig {
            max_scan_size_bytes: 100,
            ..Default::default()
        };
        let mut g = ExternalizationGate::new(config);
        let data = vec![0u8; 200];
        let decision = g.scan(&data, "agent-1");
        assert!(!decision.allowed);
    }

    #[test]
    fn test_multiple_findings() {
        let mut g = gate();
        let data = b"Key: sk-abcdefghijklmnopqrstuvwxyz123456 and SSN: 123-45-6789";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
        assert!(decision.findings.len() >= 2);
    }

    #[test]
    fn test_ignored_category() {
        let config = ExternalizationGateConfig {
            ignored_categories: vec!["pii".to_string()],
            ..Default::default()
        };
        let mut g = ExternalizationGate::new(config);
        let data = b"SSN: 123-45-6789";
        let decision = g.scan(data, "agent-1");
        assert!(decision.allowed); // PII category ignored
        assert!(decision.findings.is_empty());
    }

    #[test]
    fn test_stats_tracking() {
        let mut g = gate();
        g.scan(b"clean data", "agent-1");
        g.scan(b"sk-abcdefghijklmnopqrstuvwxyz123456", "agent-2");
        assert_eq!(g.stats().total_scans, 2);
        assert_eq!(g.stats().total_blocked, 1);
        assert!(g.stats().total_findings >= 1);
    }

    #[test]
    fn test_redacted_snippet() {
        let mut g = gate();
        let data = b"sk-abcdefghijklmnopqrstuvwxyz123456";
        let decision = g.scan(data, "agent-1");
        let snippet = &decision.findings[0].redacted_snippet;
        // Should not contain the full key
        assert!(!snippet.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(snippet.contains("..."));
    }

    #[test]
    fn test_github_token_blocked() {
        let mut g = gate();
        let data = b"token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijk";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
    }

    #[test]
    fn test_custom_pattern() {
        let config = ExternalizationGateConfig {
            custom_patterns: vec![SensitivePattern {
                name: "Internal URL".into(),
                pattern: r"https://internal\.corp\.example\.com".into(),
                severity: PatternSeverity::High,
                category: "internal".into(),
            }],
            ..Default::default()
        };
        let mut g = ExternalizationGate::new(config);
        let data = b"Fetching https://internal.corp.example.com/api/data";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed);
    }

    #[test]
    fn test_pattern_severity_display() {
        assert_eq!(format!("{}", PatternSeverity::Low), "low");
        assert_eq!(format!("{}", PatternSeverity::Medium), "medium");
        assert_eq!(format!("{}", PatternSeverity::High), "high");
        assert_eq!(format!("{}", PatternSeverity::Critical), "critical");
    }

    #[test]
    fn test_gate_decision_fields() {
        let mut g = gate();
        let decision = g.scan(b"sk-abcdefghijklmnopqrstuvwxyz123456", "agent-1");
        assert!(!decision.allowed);
        assert!(decision.data_size > 0);
        assert!(decision.scan_duration_us < 1_000_000); // under 1 second
    }

    #[test]
    fn test_pattern_count() {
        let g = gate();
        assert!(g.pattern_count() >= 10); // at least the builtins
    }

    #[test]
    fn test_low_threshold_blocks_email() {
        let config = ExternalizationGateConfig {
            block_threshold: PatternSeverity::Low,
            ..Default::default()
        };
        let mut g = ExternalizationGate::new(config);
        let data = b"Contact: user@example.com";
        let decision = g.scan(data, "agent-1");
        assert!(!decision.allowed); // Low threshold blocks Medium email
    }
}
