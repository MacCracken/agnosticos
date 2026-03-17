//! Phylax external security bridge.
//!
//! Extends Phylax's native YARA/entropy scanning with external security
//! tool integration. When Phylax detects a threat, it can forward findings
//! to SecureYeoman's security tools for deeper analysis and remediation.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// An external security scanner that Phylax can delegate to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExternalScanner {
    /// Unique scanner identifier.
    pub id: String,
    /// Human-readable name (e.g. "SecureYeoman Security Tools").
    pub name: String,
    /// HTTP endpoint for scan requests.
    pub endpoint: String,
    /// API key for authentication (optional).
    pub api_key: Option<String>,
    /// Scanner capabilities (e.g. ["cve_lookup", "dlp_scan", "network_scan"]).
    pub capabilities: Vec<String>,
    /// Timeout for scan requests in seconds.
    pub timeout_secs: u64,
    /// Whether this scanner is enabled.
    pub enabled: bool,
    /// When this scanner was registered.
    pub registered_at: DateTime<Utc>,
    /// Last successful scan timestamp.
    pub last_used: Option<DateTime<Utc>>,
}

/// A unified scan finding that combines native and external results.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnifiedFinding {
    /// Source of the finding ("phylax", "secureyeoman", etc).
    pub source: String,
    /// Severity level.
    pub severity: FindingSeverity,
    /// Finding category.
    pub category: String,
    /// Short description.
    pub summary: String,
    /// Detailed description.
    pub details: String,
    /// Affected target (file path, agent ID, etc).
    pub target: String,
    /// Recommended remediation action.
    pub remediation: Option<String>,
    /// When this finding was produced.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FindingSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Request sent to an external scanner.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExternalScanRequest {
    /// What to scan.
    pub target: String,
    /// Scan type (e.g. "cve_lookup", "dlp_scan", "full").
    pub scan_type: String,
    /// Native Phylax findings to augment.
    pub native_findings: Vec<UnifiedFinding>,
    /// Additional context.
    pub context: HashMap<String, String>,
}

/// Response from an external scanner.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExternalScanResponse {
    /// Scanner ID that produced this response.
    pub scanner_id: String,
    /// Findings from the external scanner.
    pub findings: Vec<UnifiedFinding>,
    /// Whether the scan completed successfully.
    pub success: bool,
    /// Error message if scan failed.
    pub error: Option<String>,
    /// Scan duration in milliseconds.
    pub duration_ms: u64,
}

/// Manages external scanner registration and dispatch.
#[derive(Debug, Clone)]
pub struct PhylaxBridge {
    scanners: Arc<RwLock<HashMap<String, ExternalScanner>>>,
}

impl PhylaxBridge {
    pub fn new() -> Self {
        Self {
            scanners: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an external scanner.
    pub async fn register_scanner(&self, scanner: ExternalScanner) {
        let mut scanners = self.scanners.write().await;
        scanners.insert(scanner.id.clone(), scanner);
    }

    /// Remove an external scanner.
    pub async fn remove_scanner(&self, id: &str) -> bool {
        let mut scanners = self.scanners.write().await;
        scanners.remove(id).is_some()
    }

    /// List all registered scanners.
    pub async fn list_scanners(&self) -> Vec<ExternalScanner> {
        let scanners = self.scanners.read().await;
        scanners.values().cloned().collect()
    }

    /// Find scanners with a specific capability.
    pub async fn find_by_capability(&self, capability: &str) -> Vec<ExternalScanner> {
        let scanners = self.scanners.read().await;
        scanners
            .values()
            .filter(|s| s.enabled && s.capabilities.contains(&capability.to_string()))
            .cloned()
            .collect()
    }

    /// Dispatch a scan request to all capable external scanners.
    /// Returns a merged list of findings from all scanners.
    pub async fn dispatch_scan(&self, request: &ExternalScanRequest) -> Vec<ExternalScanResponse> {
        let scanners = self.find_by_capability(&request.scan_type).await;
        let mut responses = Vec::new();

        for scanner in &scanners {
            match self.call_scanner(scanner, request).await {
                Ok(response) => responses.push(response),
                Err(err) => {
                    responses.push(ExternalScanResponse {
                        scanner_id: scanner.id.clone(),
                        findings: vec![],
                        success: false,
                        error: Some(err),
                        duration_ms: 0,
                    });
                }
            }
        }

        responses
    }

    /// Merge native Phylax findings with external scanner findings.
    pub fn merge_findings(
        native: Vec<UnifiedFinding>,
        external_responses: &[ExternalScanResponse],
    ) -> Vec<UnifiedFinding> {
        let mut all = native;
        for response in external_responses {
            if response.success {
                all.extend(response.findings.clone());
            }
        }
        // Sort by severity (critical first)
        all.sort_by(|a, b| severity_rank(&a.severity).cmp(&severity_rank(&b.severity)));
        all
    }

    /// Call a single external scanner via HTTP.
    async fn call_scanner(
        &self,
        scanner: &ExternalScanner,
        request: &ExternalScanRequest,
    ) -> Result<ExternalScanResponse, String> {
        let client = reqwest::Client::new();
        let mut req = client
            .post(&scanner.endpoint)
            .json(request)
            .timeout(std::time::Duration::from_secs(scanner.timeout_secs));

        if let Some(key) = &scanner.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let start = std::time::Instant::now();
        let res = req
            .send()
            .await
            .map_err(|e| format!("Scanner '{}' request failed: {}", scanner.id, e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !res.status().is_success() {
            return Err(format!(
                "Scanner '{}' returned HTTP {}",
                scanner.id,
                res.status()
            ));
        }

        let mut response: ExternalScanResponse = res
            .json()
            .await
            .map_err(|e| format!("Scanner '{}' invalid response: {}", scanner.id, e))?;

        response.duration_ms = duration_ms;
        Ok(response)
    }
}

impl Default for PhylaxBridge {
    fn default() -> Self {
        Self::new()
    }
}

fn severity_rank(severity: &FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Critical => 0,
        FindingSeverity::High => 1,
        FindingSeverity::Medium => 2,
        FindingSeverity::Low => 3,
        FindingSeverity::Info => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_scanner(id: &str, capabilities: Vec<&str>) -> ExternalScanner {
        ExternalScanner {
            id: id.into(),
            name: format!("Scanner {}", id),
            endpoint: format!("http://localhost:9999/scan/{}", id),
            api_key: None,
            capabilities: capabilities.into_iter().map(String::from).collect(),
            timeout_secs: 30,
            enabled: true,
            registered_at: Utc::now(),
            last_used: None,
        }
    }

    fn sample_finding(source: &str, severity: FindingSeverity) -> UnifiedFinding {
        UnifiedFinding {
            source: source.into(),
            severity,
            category: "test".into(),
            summary: "Test finding".into(),
            details: "Details".into(),
            target: "/tmp/test".into(),
            remediation: None,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let bridge = PhylaxBridge::new();
        bridge
            .register_scanner(sample_scanner("sy", vec!["cve_lookup", "dlp_scan"]))
            .await;
        bridge
            .register_scanner(sample_scanner("custom", vec!["network_scan"]))
            .await;

        let scanners = bridge.list_scanners().await;
        assert_eq!(scanners.len(), 2);
    }

    #[tokio::test]
    async fn test_find_by_capability() {
        let bridge = PhylaxBridge::new();
        bridge
            .register_scanner(sample_scanner("sy", vec!["cve_lookup", "dlp_scan"]))
            .await;
        bridge
            .register_scanner(sample_scanner("custom", vec!["network_scan"]))
            .await;

        let cve_scanners = bridge.find_by_capability("cve_lookup").await;
        assert_eq!(cve_scanners.len(), 1);
        assert_eq!(cve_scanners[0].id, "sy");

        let net_scanners = bridge.find_by_capability("network_scan").await;
        assert_eq!(net_scanners.len(), 1);
        assert_eq!(net_scanners[0].id, "custom");

        let none = bridge.find_by_capability("nonexistent").await;
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn test_disabled_scanner_excluded() {
        let bridge = PhylaxBridge::new();
        let mut scanner = sample_scanner("sy", vec!["cve_lookup"]);
        scanner.enabled = false;
        bridge.register_scanner(scanner).await;

        let found = bridge.find_by_capability("cve_lookup").await;
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn test_remove_scanner() {
        let bridge = PhylaxBridge::new();
        bridge
            .register_scanner(sample_scanner("sy", vec!["cve_lookup"]))
            .await;

        assert!(bridge.remove_scanner("sy").await);
        assert!(!bridge.remove_scanner("sy").await);
        assert!(bridge.list_scanners().await.is_empty());
    }

    #[test]
    fn test_merge_findings_sorted_by_severity() {
        let native = vec![
            sample_finding("phylax", FindingSeverity::Low),
            sample_finding("phylax", FindingSeverity::Critical),
        ];
        let external = vec![ExternalScanResponse {
            scanner_id: "sy".into(),
            findings: vec![
                sample_finding("sy", FindingSeverity::High),
                sample_finding("sy", FindingSeverity::Medium),
            ],
            success: true,
            error: None,
            duration_ms: 100,
        }];

        let merged = PhylaxBridge::merge_findings(native, &external);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].severity, FindingSeverity::Critical);
        assert_eq!(merged[1].severity, FindingSeverity::High);
        assert_eq!(merged[2].severity, FindingSeverity::Medium);
        assert_eq!(merged[3].severity, FindingSeverity::Low);
    }

    #[test]
    fn test_merge_skips_failed_responses() {
        let native = vec![sample_finding("phylax", FindingSeverity::Info)];
        let external = vec![ExternalScanResponse {
            scanner_id: "failed".into(),
            findings: vec![sample_finding(
                "should_be_ignored",
                FindingSeverity::Critical,
            )],
            success: false,
            error: Some("timeout".into()),
            duration_ms: 0,
        }];

        let merged = PhylaxBridge::merge_findings(native, &external);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, "phylax");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(severity_rank(&FindingSeverity::Critical) < severity_rank(&FindingSeverity::High));
        assert!(severity_rank(&FindingSeverity::High) < severity_rank(&FindingSeverity::Medium));
        assert!(severity_rank(&FindingSeverity::Medium) < severity_rank(&FindingSeverity::Low));
        assert!(severity_rank(&FindingSeverity::Low) < severity_rank(&FindingSeverity::Info));
    }
}
