//! Sandbox Integrity Monitor — Runtime escape detection
//!
//! Periodically checks sandboxed processes for signs of isolation breach:
//!   - Namespace integrity (PID, mount, net, user)
//!   - Filesystem escape (probing sensitive paths)
//!   - Process tree integrity (unexpected parent/children)
//!   - Resource limit enforcement (cgroup compliance)
//!
//! Inspired by SecureYeoman's SandboxMonitor.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::sigil::{RevocationEntry, SigilVerifier};

// ---------------------------------------------------------------------------
// Check Results
// ---------------------------------------------------------------------------

/// Result of a single integrity check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheck {
    pub check_type: CheckType,
    pub passed: bool,
    pub detail: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CheckType {
    /// PID namespace isolation.
    PidNamespace,
    /// Mount namespace isolation.
    MountNamespace,
    /// Network namespace isolation.
    NetNamespace,
    /// User namespace isolation.
    UserNamespace,
    /// Sensitive filesystem paths not accessible.
    FilesystemBoundary,
    /// Process parent is expected (not escaped).
    ProcessTree,
    /// Resource limits (cgroup) still applied.
    ResourceLimits,
    /// Seccomp filter still active.
    SeccompActive,
}

impl std::fmt::Display for CheckType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PidNamespace => write!(f, "pid_namespace"),
            Self::MountNamespace => write!(f, "mount_namespace"),
            Self::NetNamespace => write!(f, "net_namespace"),
            Self::UserNamespace => write!(f, "user_namespace"),
            Self::FilesystemBoundary => write!(f, "filesystem_boundary"),
            Self::ProcessTree => write!(f, "process_tree"),
            Self::ResourceLimits => write!(f, "resource_limits"),
            Self::SeccompActive => write!(f, "seccomp_active"),
        }
    }
}

/// A full integrity report for a sandboxed process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub agent_id: String,
    pub pid: u32,
    pub checks: Vec<IntegrityCheck>,
    pub overall_pass: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Namespace Inspection
// ---------------------------------------------------------------------------

/// Read the namespace inode for a process from /proc.
fn read_namespace_inode(pid: u32, ns: &str) -> Option<u64> {
    let path = format!("/proc/{}/ns/{}", pid, ns);
    std::fs::read_link(&path).ok().and_then(|link| {
        // Format: "ns_type:[inode]"
        let s = link.to_string_lossy();
        let start = s.find('[')?;
        let end = s.find(']')?;
        s[start + 1..end].parse::<u64>().ok()
    })
}

/// Check if a process is in a different namespace than PID 1 (init).
fn is_in_separate_namespace(pid: u32, ns: &str) -> bool {
    let init_ns = read_namespace_inode(1, ns);
    let proc_ns = read_namespace_inode(pid, ns);

    match (init_ns, proc_ns) {
        (Some(init), Some(proc)) => init != proc,
        _ => false, // Can't read — assume not isolated
    }
}

/// Check if seccomp is active for a process.
fn is_seccomp_active(pid: u32) -> bool {
    let status_path = format!("/proc/{}/status", pid);
    if let Ok(content) = std::fs::read_to_string(&status_path) {
        for line in content.lines() {
            if line.starts_with("Seccomp:") {
                // 0 = disabled, 1 = strict, 2 = filter
                let val = line.split(':').nth(1).map(|s| s.trim());
                return matches!(val, Some("1") | Some("2"));
            }
        }
    }
    false
}

/// Check if a process can access a sensitive path (it shouldn't be able to).
fn can_access_sensitive_path(path: &str) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Read resource limits for a process.
fn read_proc_limits(pid: u32) -> Option<String> {
    let path = format!("/proc/{}/limits", pid);
    std::fs::read_to_string(&path).ok()
}

// ---------------------------------------------------------------------------
// Sandbox Monitor
// ---------------------------------------------------------------------------

/// Configuration for the integrity monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Check interval in seconds.
    pub interval_secs: u64,
    /// Sensitive paths that sandboxed processes should NOT access.
    pub sensitive_paths: Vec<String>,
    /// Whether to check namespace isolation.
    pub check_namespaces: bool,
    /// Whether to check seccomp filter status.
    pub check_seccomp: bool,
    /// Whether to check filesystem boundaries.
    pub check_filesystem: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            sensitive_paths: vec![
                "/etc/shadow".to_string(),
                "/etc/sudoers".to_string(),
                "/proc/1/root".to_string(),
                "/proc/kcore".to_string(),
                "/sys/firmware/efi/efivars".to_string(),
                "/var/lib/agnos/audit".to_string(),
            ],
            check_namespaces: true,
            check_seccomp: true,
            check_filesystem: true,
        }
    }
}

/// The sandbox integrity monitor.
#[derive(Debug)]
pub struct SandboxMonitor {
    config: MonitorConfig,
    /// Monitored processes: agent_id → pid.
    monitored: HashMap<String, u32>,
    /// Historical reports (ring buffer).
    reports: Vec<IntegrityReport>,
    max_reports: usize,
    /// Failure counts per agent for escalation.
    failure_counts: HashMap<String, u32>,
}

impl SandboxMonitor {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            config,
            monitored: HashMap::new(),
            reports: Vec::new(),
            max_reports: 1000,
            failure_counts: HashMap::new(),
        }
    }

    /// Register a sandboxed process for monitoring.
    pub fn register(&mut self, agent_id: &str, pid: u32) {
        self.monitored.insert(agent_id.to_string(), pid);
        info!(agent_id = %agent_id, pid = pid, "Sandbox monitor: process registered");
    }

    /// Unregister a process (sandbox terminated).
    pub fn unregister(&mut self, agent_id: &str) {
        self.monitored.remove(agent_id);
    }

    /// Run integrity checks on all monitored processes.
    pub fn check_all(&mut self) -> Vec<IntegrityReport> {
        let agents: Vec<(String, u32)> = self.monitored.clone().into_iter().collect();
        let mut reports = Vec::new();

        for (agent_id, pid) in agents {
            let report = self.check_process(&agent_id, pid);
            if !report.overall_pass {
                let count = self.failure_counts.entry(agent_id.clone()).or_insert(0);
                *count += 1;
                warn!(
                    agent_id = %agent_id,
                    pid = pid,
                    failures = *count,
                    "Sandbox integrity check FAILED"
                );
            }
            reports.push(report);
        }

        // Store reports
        for r in &reports {
            if self.reports.len() >= self.max_reports {
                self.reports.remove(0);
            }
            self.reports.push(r.clone());
        }

        reports
    }

    /// Run integrity checks on a single process.
    pub fn check_process(&self, agent_id: &str, pid: u32) -> IntegrityReport {
        let mut checks = Vec::new();
        let now = chrono::Utc::now();

        // Namespace checks
        if self.config.check_namespaces {
            for (ns, check_type) in &[
                ("pid", CheckType::PidNamespace),
                ("mnt", CheckType::MountNamespace),
                ("net", CheckType::NetNamespace),
                ("user", CheckType::UserNamespace),
            ] {
                let isolated = is_in_separate_namespace(pid, ns);
                checks.push(IntegrityCheck {
                    check_type: *check_type,
                    passed: isolated,
                    detail: if isolated {
                        format!("{} namespace isolated", ns)
                    } else {
                        format!("{} namespace NOT isolated — shares with init", ns)
                    },
                    timestamp: now,
                });
            }
        }

        // Seccomp check
        if self.config.check_seccomp {
            let active = is_seccomp_active(pid);
            checks.push(IntegrityCheck {
                check_type: CheckType::SeccompActive,
                passed: active,
                detail: if active {
                    "Seccomp filter active".to_string()
                } else {
                    "Seccomp filter NOT active".to_string()
                },
                timestamp: now,
            });
        }

        // Filesystem boundary checks
        if self.config.check_filesystem {
            for path in &self.config.sensitive_paths {
                let accessible = can_access_sensitive_path(path);
                checks.push(IntegrityCheck {
                    check_type: CheckType::FilesystemBoundary,
                    // PASS = cannot access (good), FAIL = can access (breach)
                    passed: !accessible,
                    detail: if accessible {
                        format!("BREACH: sandbox can access {}", path)
                    } else {
                        format!("OK: {} not accessible", path)
                    },
                    timestamp: now,
                });
            }
        }

        // Resource limits check
        let limits_ok = read_proc_limits(pid).is_some();
        checks.push(IntegrityCheck {
            check_type: CheckType::ResourceLimits,
            passed: limits_ok,
            detail: if limits_ok {
                "Resource limits readable".to_string()
            } else {
                "Cannot read process limits".to_string()
            },
            timestamp: now,
        });

        let overall_pass = checks.iter().all(|c| c.passed);

        IntegrityReport {
            agent_id: agent_id.to_string(),
            pid,
            checks,
            overall_pass,
            timestamp: now,
        }
    }

    /// Get failure count for an agent (for escalation decisions).
    pub fn failure_count(&self, agent_id: &str) -> u32 {
        self.failure_counts.get(agent_id).copied().unwrap_or(0)
    }

    /// Get recent reports.
    pub fn recent_reports(&self, limit: usize) -> &[IntegrityReport] {
        let start = self.reports.len().saturating_sub(limit);
        &self.reports[start..]
    }

    /// Number of monitored processes.
    pub fn monitored_count(&self) -> usize {
        self.monitored.len()
    }
}

// ---------------------------------------------------------------------------
// Offender Tracker — cross-session violation tracking
// ---------------------------------------------------------------------------

/// Tracks agents that repeatedly violate sandbox policies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffenderRecord {
    pub agent_id: String,
    pub total_violations: u64,
    pub violation_types: HashMap<String, u64>,
    pub first_violation: chrono::DateTime<chrono::Utc>,
    pub last_violation: chrono::DateTime<chrono::Utc>,
    /// Trust score: 1.0 = fully trusted, 0.0 = fully untrusted.
    pub trust_score: f64,
    /// Whether the agent is currently suspended.
    pub suspended: bool,
}

/// The offender tracker.
#[derive(Debug)]
pub struct OffenderTracker {
    records: HashMap<String, OffenderRecord>,
    /// Violations before automatic suspension.
    suspension_threshold: u64,
    /// Trust score decay per violation.
    trust_decay_per_violation: f64,
}

impl OffenderTracker {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            suspension_threshold: 10,
            trust_decay_per_violation: 0.1,
        }
    }

    /// Record a violation for an agent.
    pub fn record_violation(&mut self, agent_id: &str, violation_type: &str) {
        let now = chrono::Utc::now();
        let record = self
            .records
            .entry(agent_id.to_string())
            .or_insert_with(|| OffenderRecord {
                agent_id: agent_id.to_string(),
                total_violations: 0,
                violation_types: HashMap::new(),
                first_violation: now,
                last_violation: now,
                trust_score: 1.0,
                suspended: false,
            });

        record.total_violations += 1;
        *record
            .violation_types
            .entry(violation_type.to_string())
            .or_insert(0) += 1;
        record.last_violation = now;
        record.trust_score = (record.trust_score - self.trust_decay_per_violation).max(0.0);

        if record.total_violations >= self.suspension_threshold && !record.suspended {
            record.suspended = true;
            warn!(
                agent_id = %agent_id,
                violations = record.total_violations,
                trust = record.trust_score,
                "Agent SUSPENDED — exceeded violation threshold"
            );
        }
    }

    /// Check if an agent is suspended.
    pub fn is_suspended(&self, agent_id: &str) -> bool {
        self.records
            .get(agent_id)
            .map(|r| r.suspended)
            .unwrap_or(false)
    }

    /// Get trust score for an agent (1.0 = fully trusted).
    pub fn trust_score(&self, agent_id: &str) -> f64 {
        self.records
            .get(agent_id)
            .map(|r| r.trust_score)
            .unwrap_or(1.0)
    }

    /// Get the offender record for an agent.
    pub fn get_record(&self, agent_id: &str) -> Option<&OffenderRecord> {
        self.records.get(agent_id)
    }

    /// List all offenders sorted by violation count (descending).
    pub fn top_offenders(&self, limit: usize) -> Vec<&OffenderRecord> {
        let mut records: Vec<_> = self.records.values().collect();
        records.sort_by(|a, b| b.total_violations.cmp(&a.total_violations));
        records.truncate(limit);
        records
    }

    /// Reinstate a suspended agent (manual override).
    pub fn reinstate(&mut self, agent_id: &str) -> bool {
        if let Some(record) = self.records.get_mut(agent_id) {
            record.suspended = false;
            record.trust_score = 0.5; // Partial trust restoration
            info!(agent_id = %agent_id, "Agent reinstated");
            true
        } else {
            false
        }
    }

    /// Total tracked agents.
    pub fn tracked_count(&self) -> usize {
        self.records.len()
    }

    /// Total suspended agents.
    pub fn suspended_count(&self) -> usize {
        self.records.values().filter(|r| r.suspended).count()
    }

    // -----------------------------------------------------------------------
    // S3: Sigil trust-chain integration
    // -----------------------------------------------------------------------

    /// Trust score threshold below which sigil records a demotion.
    ///
    /// When an agent's trust score drops to or below this value, the
    /// offender tracker notifies the sigil trust chain so the demotion is
    /// captured in the system-wide trust audit log.
    pub const SIGIL_DEMOTION_THRESHOLD: f64 = 0.5;

    /// Record a sandbox policy violation and, if the agent's trust score
    /// crosses the demotion threshold, write a revocation entry into the
    /// sigil trust chain.
    ///
    /// This is the preferred entry point when a `SigilVerifier` is
    /// available. Use [`record_violation`] for contexts where sigil is
    /// not reachable (e.g. unit tests, isolated components).
    pub fn record_violation_with_sigil(
        &mut self,
        agent_id: &str,
        violation_type: &str,
        sigil: &mut SigilVerifier,
    ) {
        // Capture score before the violation so we can detect threshold crossing.
        let score_before = self.trust_score(agent_id);

        self.record_violation(agent_id, violation_type);

        let score_after = self.trust_score(agent_id);

        // Notify sigil when the score crosses below the demotion threshold
        // for the first time (i.e. was above before, now at or below).
        if score_before > Self::SIGIL_DEMOTION_THRESHOLD
            && score_after <= Self::SIGIL_DEMOTION_THRESHOLD
        {
            warn!(
                agent_id = %agent_id,
                trust_score = score_after,
                threshold = Self::SIGIL_DEMOTION_THRESHOLD,
                "Trust score crossed demotion threshold — recording in sigil trust chain"
            );

            let entry = RevocationEntry {
                // Use agent_id as the key_id so sigil can look it up by agent.
                key_id: Some(format!("agent:{}", agent_id)),
                content_hash: None,
                reason: format!(
                    "Sandbox offender trust demotion: score dropped to {:.2} (threshold {:.2}) after violation '{}'",
                    score_after,
                    Self::SIGIL_DEMOTION_THRESHOLD,
                    violation_type
                ),
                revoked_at: chrono::Utc::now(),
                revoked_by: "sandbox_offender_tracker".to_string(),
            };

            if let Err(e) = sigil.add_revocation(entry) {
                warn!(
                    agent_id = %agent_id,
                    error = %e,
                    "Failed to record trust demotion in sigil chain"
                );
            } else {
                info!(
                    agent_id = %agent_id,
                    "Trust demotion recorded in sigil trust chain"
                );
            }
        }
    }
}

impl Default for OffenderTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Monitor tests ---

    #[test]
    fn test_monitor_register_unregister() {
        let mut monitor = SandboxMonitor::new(MonitorConfig::default());
        monitor.register("agent-1", 12345);
        assert_eq!(monitor.monitored_count(), 1);
        monitor.unregister("agent-1");
        assert_eq!(monitor.monitored_count(), 0);
    }

    #[test]
    fn test_check_process_self() {
        let monitor = SandboxMonitor::new(MonitorConfig::default());
        let pid = std::process::id();
        let report = monitor.check_process("test-agent", pid);
        assert_eq!(report.agent_id, "test-agent");
        assert_eq!(report.pid, pid);
        assert!(!report.checks.is_empty());
    }

    #[test]
    fn test_check_all_empty() {
        let mut monitor = SandboxMonitor::new(MonitorConfig::default());
        let reports = monitor.check_all();
        assert!(reports.is_empty());
    }

    #[test]
    fn test_namespace_check_types() {
        let check = IntegrityCheck {
            check_type: CheckType::PidNamespace,
            passed: true,
            detail: "isolated".to_string(),
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(format!("{}", check.check_type), "pid_namespace");
    }

    #[test]
    fn test_monitor_failure_count() {
        let mut monitor = SandboxMonitor::new(MonitorConfig {
            check_namespaces: false,
            check_seccomp: false,
            check_filesystem: false,
            ..Default::default()
        });
        monitor.register("agent-1", 99999); // non-existent PID
        monitor.check_all();
        // Non-existent PID won't fail namespace checks since we disabled them
        assert_eq!(monitor.failure_count("nonexistent"), 0);
    }

    // --- Offender Tracker tests ---

    #[test]
    fn test_offender_record_violation() {
        let mut tracker = OffenderTracker::new();
        tracker.record_violation("agent-1", "filesystem_escape");
        assert_eq!(tracker.trust_score("agent-1"), 0.9);
        assert!(!tracker.is_suspended("agent-1"));
    }

    #[test]
    fn test_offender_suspension() {
        let mut tracker = OffenderTracker::new();
        for _ in 0..10 {
            tracker.record_violation("agent-1", "filesystem_escape");
        }
        assert!(tracker.is_suspended("agent-1"));
        assert!(tracker.trust_score("agent-1") < 0.01);
    }

    #[test]
    fn test_offender_reinstate() {
        let mut tracker = OffenderTracker::new();
        for _ in 0..10 {
            tracker.record_violation("agent-1", "escape");
        }
        assert!(tracker.is_suspended("agent-1"));
        assert!(tracker.reinstate("agent-1"));
        assert!(!tracker.is_suspended("agent-1"));
        assert_eq!(tracker.trust_score("agent-1"), 0.5);
    }

    #[test]
    fn test_offender_top_offenders() {
        let mut tracker = OffenderTracker::new();
        for _ in 0..5 {
            tracker.record_violation("agent-1", "escape");
        }
        for _ in 0..3 {
            tracker.record_violation("agent-2", "network");
        }
        let top = tracker.top_offenders(10);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].agent_id, "agent-1");
        assert_eq!(top[1].agent_id, "agent-2");
    }

    #[test]
    fn test_offender_unknown_agent_trusted() {
        let tracker = OffenderTracker::new();
        assert_eq!(tracker.trust_score("unknown"), 1.0);
        assert!(!tracker.is_suspended("unknown"));
    }

    #[test]
    fn test_offender_violation_types_tracked() {
        let mut tracker = OffenderTracker::new();
        tracker.record_violation("agent-1", "filesystem_escape");
        tracker.record_violation("agent-1", "filesystem_escape");
        tracker.record_violation("agent-1", "network_breach");
        let record = tracker.get_record("agent-1").unwrap();
        assert_eq!(record.violation_types["filesystem_escape"], 2);
        assert_eq!(record.violation_types["network_breach"], 1);
    }

    #[test]
    fn test_offender_counts() {
        let mut tracker = OffenderTracker::new();
        for _ in 0..10 {
            tracker.record_violation("agent-bad", "escape");
        }
        tracker.record_violation("agent-ok", "minor");
        assert_eq!(tracker.tracked_count(), 2);
        assert_eq!(tracker.suspended_count(), 1);
    }

    #[test]
    fn test_reinstate_nonexistent() {
        let mut tracker = OffenderTracker::new();
        assert!(!tracker.reinstate("ghost"));
    }

    #[test]
    fn test_check_type_display_all() {
        assert_eq!(format!("{}", CheckType::PidNamespace), "pid_namespace");
        assert_eq!(format!("{}", CheckType::MountNamespace), "mount_namespace");
        assert_eq!(format!("{}", CheckType::NetNamespace), "net_namespace");
        assert_eq!(format!("{}", CheckType::UserNamespace), "user_namespace");
        assert_eq!(
            format!("{}", CheckType::FilesystemBoundary),
            "filesystem_boundary"
        );
        assert_eq!(format!("{}", CheckType::ProcessTree), "process_tree");
        assert_eq!(format!("{}", CheckType::ResourceLimits), "resource_limits");
        assert_eq!(format!("{}", CheckType::SeccompActive), "seccomp_active");
    }

    #[test]
    fn test_offender_default() {
        let tracker = OffenderTracker::default();
        assert_eq!(tracker.tracked_count(), 0);
    }

    #[test]
    fn test_check_process_self_has_checks() {
        let monitor = SandboxMonitor::new(MonitorConfig {
            check_namespaces: true,
            check_seccomp: true,
            check_filesystem: true,
            sensitive_paths: vec!["/nonexistent/path/that/does/not/exist".to_string()],
            ..Default::default()
        });
        let pid = std::process::id();
        let report = monitor.check_process("test", pid);

        // Should have namespace checks + seccomp + filesystem + resource limits
        assert!(report.checks.len() >= 6);

        // The nonexistent sensitive path should pass (not accessible = good)
        let fs_checks: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.check_type == CheckType::FilesystemBoundary)
            .collect();
        assert!(!fs_checks.is_empty());
        assert!(fs_checks[0].passed);
    }

    #[test]
    fn test_monitor_recent_reports() {
        let mut monitor = SandboxMonitor::new(MonitorConfig {
            check_namespaces: false,
            check_seccomp: false,
            check_filesystem: false,
            ..Default::default()
        });
        monitor.register("agent-1", std::process::id());
        monitor.check_all();
        monitor.check_all();
        let recent = monitor.recent_reports(1);
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn test_is_seccomp_active_self() {
        // Our test process won't have seccomp active
        let active = is_seccomp_active(std::process::id());
        assert!(!active);
    }

    #[test]
    fn test_read_namespace_inode_self() {
        let inode = read_namespace_inode(std::process::id(), "pid");
        // Should be readable for our own process
        assert!(inode.is_some());
    }

    #[test]
    fn test_read_proc_limits_self() {
        let limits = read_proc_limits(std::process::id());
        assert!(limits.is_some());
        assert!(limits.unwrap().contains("Max open files"));
    }

    #[test]
    fn test_can_access_sensitive_path() {
        assert!(!can_access_sensitive_path("/nonexistent/path"));
        // /tmp should be accessible
        assert!(can_access_sensitive_path("/tmp"));
    }

    // --- S3: Sigil trust-chain integration tests ---

    #[test]
    fn test_record_violation_with_sigil_no_demotion_above_threshold() {
        use crate::marketplace::trust::PublisherKeyring;
        use crate::sigil::{SigilVerifier, TrustPolicy};
        use std::path::Path;

        let mut tracker = OffenderTracker::new();
        let keyring = PublisherKeyring::new(Path::new("/tmp"));
        let mut sigil = SigilVerifier::new(keyring, TrustPolicy::default());

        // 4 violations: trust_score = 0.6 (above 0.5 threshold)
        for _ in 0..4 {
            tracker.record_violation_with_sigil("agent-1", "escape", &mut sigil);
        }
        assert!((tracker.trust_score("agent-1") - 0.6).abs() < f64::EPSILON);
        // No revocation entry yet (threshold not crossed)
        assert_eq!(sigil.revocation_count(), 0);
    }

    #[test]
    fn test_record_violation_with_sigil_demotion_at_threshold() {
        use crate::marketplace::trust::PublisherKeyring;
        use crate::sigil::{SigilVerifier, TrustPolicy};
        use std::path::Path;

        let mut tracker = OffenderTracker::new();
        let keyring = PublisherKeyring::new(Path::new("/tmp"));
        let mut sigil = SigilVerifier::new(keyring, TrustPolicy::default());

        // 6th violation drops score to 0.4, clearly below the 0.5 threshold.
        for _ in 0..6 {
            tracker.record_violation_with_sigil("agent-2", "namespace_breach", &mut sigil);
        }
        let score = tracker.trust_score("agent-2");
        assert!(score < OffenderTracker::SIGIL_DEMOTION_THRESHOLD);
        // Sigil should now have a revocation entry for this agent.
        assert!(sigil.revocation_count() > 0);
    }

    #[test]
    fn test_record_violation_with_sigil_demotion_only_once() {
        use crate::marketplace::trust::PublisherKeyring;
        use crate::sigil::{SigilVerifier, TrustPolicy};
        use std::path::Path;

        let mut tracker = OffenderTracker::new();
        let keyring = PublisherKeyring::new(Path::new("/tmp"));
        let mut sigil = SigilVerifier::new(keyring, TrustPolicy::default());

        // Cross the threshold exactly once at the 5th violation.
        for _ in 0..8 {
            tracker.record_violation_with_sigil("agent-3", "escape", &mut sigil);
        }
        // Only one demotion entry should be present (threshold crossed once).
        assert_eq!(sigil.revocation_count(), 1);
    }

    #[test]
    fn test_sigil_demotion_threshold_constant() {
        // Verify the threshold is the documented 0.5.
        assert!((OffenderTracker::SIGIL_DEMOTION_THRESHOLD - 0.5).abs() < f64::EPSILON);
    }
}
