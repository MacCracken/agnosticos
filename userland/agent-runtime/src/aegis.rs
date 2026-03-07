//! Aegis — System Security Daemon for AGNOS
//!
//! Unified security coordination that ties together sandboxing, anomaly
//! detection, integrity monitoring, threat assessment, and quarantine.
//! Named after the Greek shield of Zeus — aegis protects the system.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ThreatLevel
// ---------------------------------------------------------------------------

/// Severity classification for security events and findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThreatLevel {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl ThreatLevel {
    /// Numeric rank — lower number means higher severity.
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

impl PartialOrd for ThreatLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ThreatLevel {
    /// More severe threats compare as *less* (come first in sorted order).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl fmt::Display for ThreatLevel {
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
// SecurityEventType
// ---------------------------------------------------------------------------

/// Category of a security event reported to Aegis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityEventType {
    IntegrityViolation,
    AnomalousBehavior,
    UnauthorizedAccess,
    SandboxEscape,
    NetworkViolation,
    TrustViolation,
    FileSystemViolation,
    ResourceExhaustion,
    MaliciousPayload,
    PolicyViolation,
}

impl fmt::Display for SecurityEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::IntegrityViolation => "integrity_violation",
            Self::AnomalousBehavior => "anomalous_behavior",
            Self::UnauthorizedAccess => "unauthorized_access",
            Self::SandboxEscape => "sandbox_escape",
            Self::NetworkViolation => "network_violation",
            Self::TrustViolation => "trust_violation",
            Self::FileSystemViolation => "filesystem_violation",
            Self::ResourceExhaustion => "resource_exhaustion",
            Self::MaliciousPayload => "malicious_payload",
            Self::PolicyViolation => "policy_violation",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// SecurityEvent
// ---------------------------------------------------------------------------

/// A single security event recorded by Aegis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: SecurityEventType,
    /// Which subsystem reported this event (e.g. "integrity", "sandbox", "learning").
    pub source: String,
    /// The agent involved, if any.
    pub agent_id: Option<String>,
    pub threat_level: ThreatLevel,
    pub description: String,
    pub metadata: HashMap<String, String>,
    pub resolved: bool,
}

impl SecurityEvent {
    /// Create a new unresolved security event with a fresh UUID.
    pub fn new(
        event_type: SecurityEventType,
        source: impl Into<String>,
        agent_id: Option<String>,
        threat_level: ThreatLevel,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            source: source.into(),
            agent_id,
            threat_level,
            description: description.into(),
            metadata: HashMap::new(),
            resolved: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Quarantine
// ---------------------------------------------------------------------------

/// Record of a quarantined agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineEntry {
    pub agent_id: String,
    pub reason: String,
    pub quarantined_at: DateTime<Utc>,
    pub threat_level: ThreatLevel,
    /// Event IDs that led to this quarantine.
    pub events: Vec<String>,
    /// If set, the agent will be auto-released after this time.
    pub auto_release_at: Option<DateTime<Utc>>,
}

/// Action to take when quarantining an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuarantineAction {
    Suspend,
    Terminate,
    Isolate,
    RateLimit,
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// What triggered a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanType {
    OnInstall,
    OnExecute,
    Periodic,
    Manual,
}

/// A single finding within a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub severity: ThreatLevel,
    pub category: String,
    pub description: String,
    pub recommendation: String,
}

/// Result of a security scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanResult {
    pub scanned_at: DateTime<Utc>,
    /// What was scanned — file path, agent ID, package name, etc.
    pub target: String,
    pub scan_type: ScanType,
    pub findings: Vec<SecurityFinding>,
    pub clean: bool,
}

// ---------------------------------------------------------------------------
// AegisConfig
// ---------------------------------------------------------------------------

/// Configuration for the Aegis security daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AegisConfig {
    pub scan_on_install: bool,
    pub scan_on_execute: bool,
    pub periodic_scan_interval_secs: u64,
    pub quarantine_on_critical: bool,
    pub quarantine_on_high: bool,
    pub max_events: usize,
    pub auto_release_timeout_secs: Option<u64>,
}

impl Default for AegisConfig {
    fn default() -> Self {
        Self {
            scan_on_install: true,
            scan_on_execute: true,
            periodic_scan_interval_secs: 3600,
            quarantine_on_critical: true,
            quarantine_on_high: true,
            max_events: 10_000,
            auto_release_timeout_secs: None,
        }
    }
}

// ---------------------------------------------------------------------------
// AegisStats
// ---------------------------------------------------------------------------

/// Aggregate statistics from the Aegis daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AegisStats {
    pub total_events: usize,
    pub unresolved_events: usize,
    pub quarantined_agents: usize,
    pub scans_completed: usize,
    pub threat_counts: HashMap<ThreatLevel, usize>,
}

// ---------------------------------------------------------------------------
// AegisSecurityDaemon
// ---------------------------------------------------------------------------

/// Unified security coordination daemon.
///
/// Collects security events from subsystems, enforces quarantine policies,
/// performs agent/package scans, and tracks threat statistics.
pub struct AegisSecurityDaemon {
    config: AegisConfig,
    events: Vec<SecurityEvent>,
    quarantine: HashMap<String, QuarantineEntry>,
    scan_history: Vec<SecurityScanResult>,
    threat_counts: HashMap<ThreatLevel, usize>,
}

impl AegisSecurityDaemon {
    /// Create a new Aegis daemon with the given configuration.
    pub fn new(config: AegisConfig) -> Self {
        info!("Aegis security daemon initialising");
        Self {
            config,
            events: Vec::new(),
            quarantine: HashMap::new(),
            scan_history: Vec::new(),
            threat_counts: HashMap::new(),
        }
    }

    /// Report a security event.
    ///
    /// Records the event, updates threat counts, and evaluates quarantine
    /// policy. Returns a [`QuarantineAction`] if the event triggers a
    /// quarantine for the associated agent.
    pub fn report_event(&mut self, event: SecurityEvent) -> Option<QuarantineAction> {
        info!(
            event_id = %event.id,
            threat = %event.threat_level,
            event_type = %event.event_type,
            "Aegis: security event reported"
        );

        let threat = event.threat_level;
        let agent_id = event.agent_id.clone();
        let event_id = event.id.clone();
        let description = event.description.clone();

        // Update threat counts.
        *self.threat_counts.entry(threat).or_insert(0) += 1;

        // Store the event (pruning oldest if at capacity).
        self.events.push(event);
        self.prune_events();

        // Evaluate quarantine policy.
        let should_quarantine = match threat {
            ThreatLevel::Critical => self.config.quarantine_on_critical,
            ThreatLevel::High => self.config.quarantine_on_high,
            _ => false,
        };

        if should_quarantine {
            match agent_id {
                Some(ref aid) => {
                    if let Some(q) = self.quarantine.get_mut(aid) {
                        // Already quarantined — link event, escalate if more severe.
                        debug!(agent_id = %aid, "Aegis: agent already quarantined, linking event");
                        q.events.push(event_id);
                        if threat < q.threat_level {
                            q.threat_level = threat;
                        }
                    } else {
                        warn!(agent_id = %aid, threat = %threat, "Aegis: auto-quarantining agent");
                        let entry = self.build_quarantine_entry(aid, &description, threat, Some(&event_id));
                        self.quarantine.insert(aid.clone(), entry);
                    }

                    let action = match threat {
                        ThreatLevel::Critical => QuarantineAction::Terminate,
                        ThreatLevel::High => QuarantineAction::Suspend,
                        _ => QuarantineAction::Isolate,
                    };
                    return Some(action);
                }
                None => {
                    warn!(
                        threat = %threat,
                        event_id = %event_id,
                        "Aegis: quarantine-severity event has no agent_id — cannot quarantine"
                    );
                }
            }
        }

        None
    }

    /// Scan an agent binary for basic security heuristics.
    /// Respects `config.scan_on_execute` — returns a clean result if disabled.
    pub fn scan_agent(&mut self, agent_id: &str, binary_path: &Path) -> SecurityScanResult {
        if !self.config.scan_on_execute {
            debug!(agent_id = %agent_id, "Aegis: agent scan skipped (disabled in config)");
            return SecurityScanResult {
                scanned_at: Utc::now(),
                target: format!("agent:{}", agent_id),
                scan_type: ScanType::OnExecute,
                findings: Vec::new(),
                clean: true,
            };
        }
        debug!(agent_id = %agent_id, path = %binary_path.display(), "Aegis: scanning agent binary");

        let mut findings = Vec::new();

        // Check that the file exists.
        if !binary_path.exists() {
            findings.push(SecurityFinding {
                severity: ThreatLevel::High,
                category: "missing_binary".into(),
                description: format!("Binary not found at {}", binary_path.display()),
                recommendation: "Verify the agent installation is complete.".into(),
            });
        } else {
            match std::fs::metadata(binary_path) {
                Ok(meta) => {
                    if meta.len() == 0 {
                        findings.push(SecurityFinding {
                            severity: ThreatLevel::Medium,
                            category: "empty_binary".into(),
                            description: "Agent binary is empty (0 bytes).".into(),
                            recommendation: "Re-install the agent.".into(),
                        });
                    }

                    // Warn about world-writable.
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mode = meta.permissions().mode();
                        if mode & 0o002 != 0 {
                            findings.push(SecurityFinding {
                                severity: ThreatLevel::High,
                                category: "world_writable".into(),
                                description: "Agent binary is world-writable.".into(),
                                recommendation: "Fix permissions: chmod o-w on the binary.".into(),
                            });
                        }
                    }
                }
                Err(e) => {
                    findings.push(SecurityFinding {
                        severity: ThreatLevel::Medium,
                        category: "unreadable_metadata".into(),
                        description: format!("Cannot read file metadata: {}", e),
                        recommendation: "Check file permissions and ownership.".into(),
                    });
                }
            }
        }

        let clean = findings.is_empty();
        let result = SecurityScanResult {
            scanned_at: Utc::now(),
            target: format!("agent:{}", agent_id),
            scan_type: ScanType::OnExecute,
            findings,
            clean,
        };

        self.scan_history.push(result.clone());
        result
    }

    /// Scan a package archive before installation.
    /// Respects `config.scan_on_install` — returns a clean result if disabled.
    pub fn scan_package(&mut self, package_path: &Path) -> SecurityScanResult {
        if !self.config.scan_on_install {
            debug!(path = %package_path.display(), "Aegis: package scan skipped (disabled in config)");
            return SecurityScanResult {
                scanned_at: Utc::now(),
                target: format!("package:{}", package_path.display()),
                scan_type: ScanType::OnInstall,
                findings: Vec::new(),
                clean: true,
            };
        }
        debug!(path = %package_path.display(), "Aegis: scanning package");

        let mut findings = Vec::new();

        if !package_path.exists() {
            findings.push(SecurityFinding {
                severity: ThreatLevel::High,
                category: "missing_package".into(),
                description: format!("Package not found at {}", package_path.display()),
                recommendation: "Verify download completed successfully.".into(),
            });
        } else {
            match std::fs::metadata(package_path) {
                Ok(meta) => {
                    if meta.len() == 0 {
                        findings.push(SecurityFinding {
                            severity: ThreatLevel::Medium,
                            category: "empty_package".into(),
                            description: "Package file is empty (0 bytes).".into(),
                            recommendation: "Re-download the package.".into(),
                        });
                    }

                    // Very large packages get an informational flag.
                    if meta.len() > 500 * 1024 * 1024 {
                        findings.push(SecurityFinding {
                            severity: ThreatLevel::Low,
                            category: "large_package".into(),
                            description: format!(
                                "Package is very large ({:.1} MB).",
                                meta.len() as f64 / (1024.0 * 1024.0)
                            ),
                            recommendation: "Review package contents for unnecessary data.".into(),
                        });
                    }
                }
                Err(e) => {
                    findings.push(SecurityFinding {
                        severity: ThreatLevel::Medium,
                        category: "unreadable_metadata".into(),
                        description: format!("Cannot read package metadata: {}", e),
                        recommendation: "Check file permissions and download integrity.".into(),
                    });
                }
            }
        }

        let clean = findings.is_empty();
        let result = SecurityScanResult {
            scanned_at: Utc::now(),
            target: format!("package:{}", package_path.display()),
            scan_type: ScanType::OnInstall,
            findings,
            clean,
        };

        self.scan_history.push(result.clone());
        result
    }

    /// Quarantine an agent by ID. If already quarantined, escalates the threat
    /// level if the new level is more severe, and appends the reason.
    pub fn quarantine_agent(
        &mut self,
        agent_id: &str,
        reason: &str,
        threat_level: ThreatLevel,
    ) -> QuarantineEntry {
        if let Some(existing) = self.quarantine.get_mut(agent_id) {
            debug!(agent_id = %agent_id, "Aegis: agent already quarantined, updating");
            if threat_level < existing.threat_level {
                existing.threat_level = threat_level;
            }
            existing.reason = format!("{}; {}", existing.reason, reason);
            return existing.clone();
        }
        warn!(agent_id = %agent_id, threat = %threat_level, "Aegis: quarantining agent");
        let entry = self.build_quarantine_entry(agent_id, reason, threat_level, None);
        self.quarantine.insert(agent_id.to_string(), entry.clone());
        entry
    }

    /// Release an agent from quarantine. Returns `true` if the agent was
    /// quarantined and has now been released. Records the release in the
    /// event log.
    pub fn release_agent(&mut self, agent_id: &str) -> bool {
        let removed = self.quarantine.remove(agent_id).is_some();
        if removed {
            info!(agent_id = %agent_id, "Aegis: agent released from quarantine");
        }
        removed
    }

    /// Check whether an agent is currently quarantined.
    pub fn is_quarantined(&self, agent_id: &str) -> bool {
        self.quarantine.contains_key(agent_id)
    }

    /// Get the quarantine record for an agent, if any.
    pub fn get_quarantine(&self, agent_id: &str) -> Option<&QuarantineEntry> {
        self.quarantine.get(agent_id)
    }

    /// List all currently quarantined agents.
    pub fn quarantined_agents(&self) -> Vec<&QuarantineEntry> {
        self.quarantine.values().collect()
    }

    /// Return all events associated with a given agent.
    pub fn events_for_agent(&self, agent_id: &str) -> Vec<&SecurityEvent> {
        self.events
            .iter()
            .filter(|e| e.agent_id.as_deref() == Some(agent_id))
            .collect()
    }

    /// Return all events matching the specified threat level.
    pub fn events_by_threat(&self, threat_level: ThreatLevel) -> Vec<&SecurityEvent> {
        self.events
            .iter()
            .filter(|e| e.threat_level == threat_level)
            .collect()
    }

    /// Return the most recent `count` events.
    pub fn recent_events(&self, count: usize) -> Vec<&SecurityEvent> {
        let start = self.events.len().saturating_sub(count);
        self.events[start..].iter().collect()
    }

    /// Return all unresolved events.
    pub fn unresolved_events(&self) -> Vec<&SecurityEvent> {
        self.events.iter().filter(|e| !e.resolved).collect()
    }

    /// Mark an event as resolved. Returns `true` if the event was found and
    /// marked, `false` if the ID does not exist.
    pub fn resolve_event(&mut self, event_id: &str) -> bool {
        if let Some(ev) = self.events.iter_mut().find(|e| e.id == event_id) {
            ev.resolved = true;
            debug!(event_id = %event_id, "Aegis: event resolved");
            true
        } else {
            false
        }
    }

    /// Current threat count summary.
    pub fn threat_summary(&self) -> HashMap<ThreatLevel, usize> {
        self.threat_counts.clone()
    }

    /// Aggregate statistics.
    pub fn stats(&self) -> AegisStats {
        AegisStats {
            total_events: self.events.len(),
            unresolved_events: self.events.iter().filter(|e| !e.resolved).count(),
            quarantined_agents: self.quarantine.len(),
            scans_completed: self.scan_history.len(),
            threat_counts: self.threat_counts.clone(),
        }
    }

    /// Check for agents whose auto-release timeout has expired and release
    /// them. Returns the list of released agent IDs.
    pub fn check_auto_releases(&mut self) -> Vec<String> {
        let now = Utc::now();
        let released: Vec<String> = self
            .quarantine
            .iter()
            .filter_map(|(id, entry)| {
                entry
                    .auto_release_at
                    .filter(|&release_at| now >= release_at)
                    .map(|_| id.clone())
            })
            .collect();

        for id in &released {
            info!(agent_id = %id, "Aegis: auto-releasing agent (timeout expired)");
            self.quarantine.remove(id);
        }

        released
    }

    // -- internal helpers ---------------------------------------------------

    fn build_quarantine_entry(
        &self,
        agent_id: &str,
        reason: &str,
        threat_level: ThreatLevel,
        event_id: Option<&str>,
    ) -> QuarantineEntry {
        let events = match event_id {
            Some(id) => vec![id.to_string()],
            None => Vec::new(),
        };

        let auto_release_at = self
            .config
            .auto_release_timeout_secs
            .map(|secs| Utc::now() + Duration::seconds(secs as i64));

        QuarantineEntry {
            agent_id: agent_id.to_string(),
            reason: reason.to_string(),
            quarantined_at: Utc::now(),
            threat_level,
            events,
            auto_release_at,
        }
    }

    /// Prune the oldest events when above the configured capacity.
    fn prune_events(&mut self) {
        if self.events.len() > self.config.max_events {
            let excess = self.events.len() - self.config.max_events;
            self.events.drain(..excess);
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn default_daemon() -> AegisSecurityDaemon {
        AegisSecurityDaemon::new(AegisConfig::default())
    }

    fn make_event(
        event_type: SecurityEventType,
        threat: ThreatLevel,
        agent_id: Option<&str>,
    ) -> SecurityEvent {
        SecurityEvent::new(
            event_type,
            "test",
            agent_id.map(String::from),
            threat,
            "test event",
        )
    }

    // -- ThreatLevel --------------------------------------------------------

    #[test]
    fn threat_level_display() {
        assert_eq!(ThreatLevel::Critical.to_string(), "CRITICAL");
        assert_eq!(ThreatLevel::High.to_string(), "HIGH");
        assert_eq!(ThreatLevel::Medium.to_string(), "MEDIUM");
        assert_eq!(ThreatLevel::Low.to_string(), "LOW");
        assert_eq!(ThreatLevel::Info.to_string(), "INFO");
    }

    #[test]
    fn threat_level_ordering() {
        assert!(ThreatLevel::Critical < ThreatLevel::High);
        assert!(ThreatLevel::High < ThreatLevel::Medium);
        assert!(ThreatLevel::Medium < ThreatLevel::Low);
        assert!(ThreatLevel::Low < ThreatLevel::Info);
    }

    #[test]
    fn threat_level_sort() {
        let mut levels = vec![
            ThreatLevel::Low,
            ThreatLevel::Critical,
            ThreatLevel::Info,
            ThreatLevel::High,
            ThreatLevel::Medium,
        ];
        levels.sort();
        assert_eq!(
            levels,
            vec![
                ThreatLevel::Critical,
                ThreatLevel::High,
                ThreatLevel::Medium,
                ThreatLevel::Low,
                ThreatLevel::Info,
            ]
        );
    }

    // -- SecurityEventType --------------------------------------------------

    #[test]
    fn security_event_type_all_variants() {
        let variants = vec![
            SecurityEventType::IntegrityViolation,
            SecurityEventType::AnomalousBehavior,
            SecurityEventType::UnauthorizedAccess,
            SecurityEventType::SandboxEscape,
            SecurityEventType::NetworkViolation,
            SecurityEventType::TrustViolation,
            SecurityEventType::FileSystemViolation,
            SecurityEventType::ResourceExhaustion,
            SecurityEventType::MaliciousPayload,
            SecurityEventType::PolicyViolation,
        ];
        assert_eq!(variants.len(), 10);
        // Verify Display works on each.
        for v in &variants {
            assert!(!v.to_string().is_empty());
        }
    }

    // -- AegisConfig --------------------------------------------------------

    #[test]
    fn config_defaults() {
        let cfg = AegisConfig::default();
        assert!(cfg.scan_on_install);
        assert!(cfg.scan_on_execute);
        assert_eq!(cfg.periodic_scan_interval_secs, 3600);
        assert!(cfg.quarantine_on_critical);
        assert!(cfg.quarantine_on_high);
        assert_eq!(cfg.max_events, 10_000);
        assert!(cfg.auto_release_timeout_secs.is_none());
    }

    // -- Event creation and reporting ---------------------------------------

    #[test]
    fn create_and_report_event() {
        let mut daemon = default_daemon();
        let event = make_event(SecurityEventType::PolicyViolation, ThreatLevel::Low, None);
        let action = daemon.report_event(event);
        assert!(action.is_none());
        assert_eq!(daemon.events.len(), 1);
    }

    #[test]
    fn event_has_uuid_and_timestamp() {
        let event = SecurityEvent::new(
            SecurityEventType::IntegrityViolation,
            "integrity",
            None,
            ThreatLevel::Medium,
            "test",
        );
        assert!(!event.id.is_empty());
        assert!(event.timestamp <= Utc::now());
        assert!(!event.resolved);
    }

    #[test]
    fn event_metadata() {
        let mut event = make_event(SecurityEventType::NetworkViolation, ThreatLevel::Medium, None);
        event.metadata.insert("ip".into(), "10.0.0.1".into());
        event.metadata.insert("port".into(), "443".into());
        assert_eq!(event.metadata.len(), 2);
        assert_eq!(event.metadata.get("ip").unwrap(), "10.0.0.1");
    }

    // -- Auto-quarantine ----------------------------------------------------

    #[test]
    fn auto_quarantine_on_critical() {
        let mut daemon = default_daemon();
        let event = make_event(
            SecurityEventType::SandboxEscape,
            ThreatLevel::Critical,
            Some("agent-1"),
        );
        let action = daemon.report_event(event);
        assert_eq!(action, Some(QuarantineAction::Terminate));
        assert!(daemon.is_quarantined("agent-1"));
    }

    #[test]
    fn auto_quarantine_on_high() {
        let mut daemon = default_daemon();
        let event = make_event(
            SecurityEventType::UnauthorizedAccess,
            ThreatLevel::High,
            Some("agent-2"),
        );
        let action = daemon.report_event(event);
        assert_eq!(action, Some(QuarantineAction::Suspend));
        assert!(daemon.is_quarantined("agent-2"));
    }

    #[test]
    fn no_quarantine_on_medium() {
        let mut daemon = default_daemon();
        let event = make_event(
            SecurityEventType::AnomalousBehavior,
            ThreatLevel::Medium,
            Some("agent-3"),
        );
        let action = daemon.report_event(event);
        assert!(action.is_none());
        assert!(!daemon.is_quarantined("agent-3"));
    }

    #[test]
    fn no_quarantine_on_low() {
        let mut daemon = default_daemon();
        let event = make_event(
            SecurityEventType::ResourceExhaustion,
            ThreatLevel::Low,
            Some("agent-4"),
        );
        let action = daemon.report_event(event);
        assert!(action.is_none());
        assert!(!daemon.is_quarantined("agent-4"));
    }

    #[test]
    fn no_quarantine_without_agent_id() {
        let mut daemon = default_daemon();
        let event = make_event(SecurityEventType::SandboxEscape, ThreatLevel::Critical, None);
        let action = daemon.report_event(event);
        assert!(action.is_none());
    }

    // -- Quarantine and release ---------------------------------------------

    #[test]
    fn quarantine_and_release_agent() {
        let mut daemon = default_daemon();
        daemon.quarantine_agent("agent-a", "test quarantine", ThreatLevel::High);
        assert!(daemon.is_quarantined("agent-a"));
        assert!(daemon.release_agent("agent-a"));
        assert!(!daemon.is_quarantined("agent-a"));
    }

    #[test]
    fn release_non_quarantined_agent() {
        let mut daemon = default_daemon();
        assert!(!daemon.release_agent("nonexistent"));
    }

    #[test]
    fn get_quarantine_entry() {
        let mut daemon = default_daemon();
        daemon.quarantine_agent("agent-b", "suspicious", ThreatLevel::Medium);
        let entry = daemon.get_quarantine("agent-b").unwrap();
        assert_eq!(entry.agent_id, "agent-b");
        assert_eq!(entry.reason, "suspicious");
        assert_eq!(entry.threat_level, ThreatLevel::Medium);
    }

    #[test]
    fn quarantined_agents_list() {
        let mut daemon = default_daemon();
        daemon.quarantine_agent("a1", "reason1", ThreatLevel::High);
        daemon.quarantine_agent("a2", "reason2", ThreatLevel::Critical);
        let list = daemon.quarantined_agents();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn quarantine_links_events() {
        let mut daemon = default_daemon();
        // First critical event triggers quarantine.
        let e1 = make_event(
            SecurityEventType::MaliciousPayload,
            ThreatLevel::Critical,
            Some("agent-x"),
        );
        daemon.report_event(e1);
        // Second critical event links to existing quarantine.
        let e2 = make_event(
            SecurityEventType::SandboxEscape,
            ThreatLevel::Critical,
            Some("agent-x"),
        );
        daemon.report_event(e2);

        let q = daemon.get_quarantine("agent-x").unwrap();
        assert_eq!(q.events.len(), 2);
    }

    // -- Event queries ------------------------------------------------------

    #[test]
    fn events_for_agent_filtering() {
        let mut daemon = default_daemon();
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Low,
            Some("agent-f"),
        ));
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Low,
            Some("other"),
        ));
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Info,
            Some("agent-f"),
        ));
        assert_eq!(daemon.events_for_agent("agent-f").len(), 2);
    }

    #[test]
    fn events_by_threat_filtering() {
        let mut daemon = default_daemon();
        daemon.report_event(make_event(
            SecurityEventType::IntegrityViolation,
            ThreatLevel::Medium,
            None,
        ));
        daemon.report_event(make_event(
            SecurityEventType::IntegrityViolation,
            ThreatLevel::Low,
            None,
        ));
        daemon.report_event(make_event(
            SecurityEventType::IntegrityViolation,
            ThreatLevel::Medium,
            None,
        ));
        assert_eq!(daemon.events_by_threat(ThreatLevel::Medium).len(), 2);
        assert_eq!(daemon.events_by_threat(ThreatLevel::Low).len(), 1);
    }

    #[test]
    fn recent_events_returns_correct_count() {
        let mut daemon = default_daemon();
        for _ in 0..5 {
            daemon.report_event(make_event(
                SecurityEventType::PolicyViolation,
                ThreatLevel::Info,
                None,
            ));
        }
        assert_eq!(daemon.recent_events(3).len(), 3);
        assert_eq!(daemon.recent_events(10).len(), 5);
    }

    #[test]
    fn unresolved_events_filtering() {
        let mut daemon = default_daemon();
        let e1 = make_event(SecurityEventType::PolicyViolation, ThreatLevel::Low, None);
        let id1 = e1.id.clone();
        daemon.report_event(e1);
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Info,
            None,
        ));
        daemon.resolve_event(&id1);
        assert_eq!(daemon.unresolved_events().len(), 1);
    }

    #[test]
    fn resolve_event_marks_resolved() {
        let mut daemon = default_daemon();
        let event = make_event(SecurityEventType::NetworkViolation, ThreatLevel::Medium, None);
        let id = event.id.clone();
        daemon.report_event(event);
        assert!(daemon.resolve_event(&id));
        assert!(daemon.events.iter().find(|e| e.id == id).unwrap().resolved);
    }

    #[test]
    fn resolve_nonexistent_event() {
        let mut daemon = default_daemon();
        assert!(!daemon.resolve_event("does-not-exist"));
    }

    // -- Scanning -----------------------------------------------------------

    #[test]
    fn scan_agent_missing_binary() {
        let mut daemon = default_daemon();
        let result = daemon.scan_agent("agent-m", Path::new("/nonexistent/binary"));
        assert!(!result.clean);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, ThreatLevel::High);
        assert_eq!(result.scan_type, ScanType::OnExecute);
    }

    #[test]
    fn scan_agent_existing_binary() {
        let mut daemon = default_daemon();
        // Use Cargo.toml as a known-to-exist file.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = daemon.scan_agent("agent-ok", &path);
        assert!(result.clean);
        assert!(result.target.contains("agent-ok"));
    }

    #[test]
    fn scan_package_missing() {
        let mut daemon = default_daemon();
        let result = daemon.scan_package(Path::new("/nonexistent/package.tar.gz"));
        assert!(!result.clean);
        assert_eq!(result.scan_type, ScanType::OnInstall);
    }

    #[test]
    fn scan_package_existing() {
        let mut daemon = default_daemon();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = daemon.scan_package(&path);
        assert!(result.clean);
    }

    // -- Stats and summaries ------------------------------------------------

    #[test]
    fn threat_summary_accuracy() {
        let mut daemon = default_daemon();
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Low,
            None,
        ));
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Low,
            None,
        ));
        daemon.report_event(make_event(
            SecurityEventType::PolicyViolation,
            ThreatLevel::Medium,
            None,
        ));

        let summary = daemon.threat_summary();
        assert_eq!(summary.get(&ThreatLevel::Low), Some(&2));
        assert_eq!(summary.get(&ThreatLevel::Medium), Some(&1));
        assert_eq!(summary.get(&ThreatLevel::Critical), None);
    }

    #[test]
    fn stats_accuracy() {
        let mut daemon = default_daemon();
        let e = make_event(SecurityEventType::PolicyViolation, ThreatLevel::Low, None);
        let id = e.id.clone();
        daemon.report_event(e);
        daemon.report_event(make_event(
            SecurityEventType::TrustViolation,
            ThreatLevel::Info,
            None,
        ));
        daemon.resolve_event(&id);
        daemon.quarantine_agent("q1", "test", ThreatLevel::High);
        daemon.scan_agent("s1", Path::new("/nonexistent"));

        let stats = daemon.stats();
        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.unresolved_events, 1);
        assert_eq!(stats.quarantined_agents, 1);
        assert_eq!(stats.scans_completed, 1);
    }

    #[test]
    fn empty_daemon_stats() {
        let daemon = default_daemon();
        let stats = daemon.stats();
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.unresolved_events, 0);
        assert_eq!(stats.quarantined_agents, 0);
        assert_eq!(stats.scans_completed, 0);
        assert!(stats.threat_counts.is_empty());
    }

    // -- Multiple events for same agent -------------------------------------

    #[test]
    fn multiple_events_same_agent() {
        let mut daemon = default_daemon();
        for _ in 0..5 {
            daemon.report_event(make_event(
                SecurityEventType::AnomalousBehavior,
                ThreatLevel::Low,
                Some("agent-multi"),
            ));
        }
        assert_eq!(daemon.events_for_agent("agent-multi").len(), 5);
    }

    // -- Auto-release -------------------------------------------------------

    #[test]
    fn auto_release_timeout_expired() {
        let mut daemon = default_daemon();
        daemon.quarantine_agent("agent-ar", "test", ThreatLevel::Medium);
        // Manually set auto_release_at to the past.
        if let Some(entry) = daemon.quarantine.get_mut("agent-ar") {
            entry.auto_release_at = Some(Utc::now() - Duration::seconds(10));
        }
        let released = daemon.check_auto_releases();
        assert_eq!(released, vec!["agent-ar".to_string()]);
        assert!(!daemon.is_quarantined("agent-ar"));
    }

    #[test]
    fn auto_release_timeout_not_expired() {
        let mut daemon = default_daemon();
        daemon.quarantine_agent("agent-nr", "test", ThreatLevel::Medium);
        if let Some(entry) = daemon.quarantine.get_mut("agent-nr") {
            entry.auto_release_at = Some(Utc::now() + Duration::seconds(3600));
        }
        let released = daemon.check_auto_releases();
        assert!(released.is_empty());
        assert!(daemon.is_quarantined("agent-nr"));
    }

    #[test]
    fn auto_release_no_timeout_set() {
        let mut daemon = default_daemon();
        daemon.quarantine_agent("agent-nt", "test", ThreatLevel::Medium);
        let released = daemon.check_auto_releases();
        assert!(released.is_empty());
        assert!(daemon.is_quarantined("agent-nt"));
    }

    // -- SecurityFinding and ScanType ---------------------------------------

    #[test]
    fn security_finding_creation() {
        let finding = SecurityFinding {
            severity: ThreatLevel::High,
            category: "test_category".into(),
            description: "test finding".into(),
            recommendation: "fix it".into(),
        };
        assert_eq!(finding.severity, ThreatLevel::High);
        assert_eq!(finding.category, "test_category");
    }

    #[test]
    fn scan_type_variants() {
        let types = vec![
            ScanType::OnInstall,
            ScanType::OnExecute,
            ScanType::Periodic,
            ScanType::Manual,
        ];
        assert_eq!(types.len(), 4);
        assert_ne!(ScanType::OnInstall, ScanType::Manual);
    }

    #[test]
    fn quarantine_action_variants() {
        let actions = vec![
            QuarantineAction::Suspend,
            QuarantineAction::Terminate,
            QuarantineAction::Isolate,
            QuarantineAction::RateLimit,
        ];
        assert_eq!(actions.len(), 4);
        assert_ne!(QuarantineAction::Suspend, QuarantineAction::Terminate);
    }

    // -- Max events limit ---------------------------------------------------

    #[test]
    fn max_events_prunes_oldest() {
        let config = AegisConfig {
            max_events: 5,
            ..AegisConfig::default()
        };
        let mut daemon = AegisSecurityDaemon::new(config);

        let mut ids = Vec::new();
        for i in 0..8 {
            let mut event = make_event(SecurityEventType::PolicyViolation, ThreatLevel::Info, None);
            event.description = format!("event-{}", i);
            ids.push(event.id.clone());
            daemon.report_event(event);
        }

        // Should have exactly 5 events.
        assert_eq!(daemon.events.len(), 5);
        // Oldest (0, 1, 2) should be gone; 3..7 remain.
        assert!(daemon.events.iter().all(|e| {
            let n: usize = e.description.strip_prefix("event-").unwrap().parse().unwrap();
            n >= 3
        }));
    }

    // -- Config with auto release -------------------------------------------

    #[test]
    fn config_auto_release_populates_quarantine_entry() {
        let config = AegisConfig {
            auto_release_timeout_secs: Some(300),
            ..AegisConfig::default()
        };
        let mut daemon = AegisSecurityDaemon::new(config);
        let entry = daemon.quarantine_agent("agent-auto", "test", ThreatLevel::Medium);
        assert!(entry.auto_release_at.is_some());
        // Should be roughly 300s in the future.
        let diff = entry.auto_release_at.unwrap() - Utc::now();
        assert!(diff.num_seconds() >= 298 && diff.num_seconds() <= 302);
    }

    // --- Audit fix tests ---

    #[test]
    fn quarantine_agent_does_not_overwrite_existing() {
        let mut daemon = AegisSecurityDaemon::new(AegisConfig::default());
        daemon.quarantine_agent("a1", "first reason", ThreatLevel::High);
        let updated = daemon.quarantine_agent("a1", "second reason", ThreatLevel::Critical);
        // Should escalate to Critical and append reason.
        assert_eq!(updated.threat_level, ThreatLevel::Critical);
        assert!(updated.reason.contains("first reason"));
        assert!(updated.reason.contains("second reason"));
    }

    #[test]
    fn quarantine_agent_no_downgrade() {
        let mut daemon = AegisSecurityDaemon::new(AegisConfig::default());
        daemon.quarantine_agent("a1", "critical issue", ThreatLevel::Critical);
        let updated = daemon.quarantine_agent("a1", "lesser issue", ThreatLevel::Medium);
        // Should stay at Critical (more severe).
        assert_eq!(updated.threat_level, ThreatLevel::Critical);
    }

    #[test]
    fn scan_disabled_by_config() {
        let config = AegisConfig {
            scan_on_install: false,
            scan_on_execute: false,
            ..AegisConfig::default()
        };
        let mut daemon = AegisSecurityDaemon::new(config);
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent");
        std::fs::write(&agent, b"binary").unwrap();

        let result = daemon.scan_agent("test", &agent);
        assert!(result.clean);

        let pkg = dir.path().join("pkg.ark");
        std::fs::write(&pkg, b"package").unwrap();
        let result = daemon.scan_package(&pkg);
        assert!(result.clean);
    }

    #[test]
    fn scan_empty_binary_flagged() {
        let mut daemon = AegisSecurityDaemon::new(AegisConfig::default());
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty");
        std::fs::write(&empty, b"").unwrap();
        let result = daemon.scan_agent("test", &empty);
        assert!(!result.clean);
        assert!(result.findings.iter().any(|f| f.category == "empty_binary"));
    }

    #[cfg(unix)]
    #[test]
    fn scan_world_writable_flagged() {
        use std::os::unix::fs::PermissionsExt;
        let mut daemon = AegisSecurityDaemon::new(AegisConfig::default());
        let dir = tempfile::tempdir().unwrap();
        let ww = dir.path().join("writable");
        std::fs::write(&ww, b"data").unwrap();
        std::fs::set_permissions(&ww, std::fs::Permissions::from_mode(0o666)).unwrap();
        let result = daemon.scan_agent("test", &ww);
        assert!(!result.clean);
        assert!(result.findings.iter().any(|f| f.category == "world_writable"));
    }

    #[test]
    fn critical_event_no_agent_id_no_panic() {
        let mut daemon = AegisSecurityDaemon::new(AegisConfig::default());
        let event = SecurityEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: SecurityEventType::SandboxEscape,
            source: "test".into(),
            agent_id: None,
            threat_level: ThreatLevel::Critical,
            description: "escape detected".into(),
            metadata: HashMap::new(),
            resolved: false,
        };
        // Should not panic, should return None (no agent to quarantine).
        let action = daemon.report_event(event);
        assert!(action.is_none());
        // Event should still be recorded.
        assert_eq!(daemon.stats().total_events, 1);
    }
}
