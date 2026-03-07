//! Runtime Integrity Attestation
//!
//! Periodic verification of file integrity using SHA-256 hashes. Monitors
//! critical system files and agent binaries for tampering, reporting any
//! mismatches against a known-good baseline.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Status of a single integrity measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeasurementStatus {
    /// Not yet verified — awaiting first measurement pass.
    Pending,
    /// Hash matches the expected value.
    Verified,
    /// Hash does not match the expected value.
    Mismatch,
    /// The file was not found on disk.
    FileNotFound,
    /// An error occurred while measuring.
    Error(String),
}

impl PartialEq for MeasurementStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Pending, Self::Pending) => true,
            (Self::Verified, Self::Verified) => true,
            (Self::Mismatch, Self::Mismatch) => true,
            (Self::FileNotFound, Self::FileNotFound) => true,
            (Self::Error(a), Self::Error(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for MeasurementStatus {}

/// A single file integrity measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityMeasurement {
    pub path: PathBuf,
    pub expected_hash: String,
    pub actual_hash: Option<String>,
    pub measured_at: DateTime<Utc>,
    pub status: MeasurementStatus,
}

/// Policy defining which files to monitor.
#[derive(Debug, Clone, Default)]
pub struct IntegrityPolicy {
    pub measurements: Vec<IntegrityMeasurement>,
    pub check_interval_seconds: u64,
    pub enforce: bool,
}

impl IntegrityPolicy {
    /// Register a file to monitor with its expected hash.
    pub fn add_measurement(&mut self, path: PathBuf, expected_hash: String) {
        self.measurements.push(IntegrityMeasurement {
            path,
            expected_hash,
            actual_hash: None,
            measured_at: Utc::now(),
            status: MeasurementStatus::Pending,
        });
    }

    /// Remove a file from monitoring. Returns true if it was present.
    pub fn remove_measurement(&mut self, path: &Path) -> bool {
        let before = self.measurements.len();
        self.measurements.retain(|m| m.path != path);
        self.measurements.len() < before
    }
}

/// Report summarising an integrity verification pass.
#[derive(Debug, Clone)]
pub struct IntegrityReport {
    pub total: usize,
    pub verified: usize,
    pub mismatches: Vec<IntegrityMeasurement>,
    pub errors: Vec<IntegrityMeasurement>,
    pub checked_at: DateTime<Utc>,
}

impl IntegrityReport {
    /// Returns true if no mismatches or errors were found.
    pub fn is_clean(&self) -> bool {
        self.mismatches.is_empty() && self.errors.is_empty()
    }

    /// Human-readable summary string.
    pub fn summary(&self) -> String {
        format!(
            "Integrity check at {}: {}/{} verified, {} mismatches, {} errors",
            self.checked_at,
            self.verified,
            self.total,
            self.mismatches.len(),
            self.errors.len(),
        )
    }
}

/// Verifies file integrity against known-good hashes.
pub struct IntegrityVerifier {
    policy: IntegrityPolicy,
    last_report: Option<IntegrityReport>,
}

impl IntegrityVerifier {
    /// Create a new verifier with the given policy.
    pub fn new(policy: IntegrityPolicy) -> Self {
        Self {
            policy,
            last_report: None,
        }
    }

    /// Replace the current policy, clearing any cached report.
    pub fn set_policy(&mut self, policy: IntegrityPolicy) {
        self.policy = policy;
        self.last_report = None;
    }

    /// Compute the SHA-256 hash of a file's contents.
    pub fn compute_hash(path: &Path) -> anyhow::Result<String> {
        let data = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let result = hasher.finalize();
        Ok(format!("{:x}", result))
    }

    /// Verify a single file against an expected hash.
    pub fn verify_file(&self, path: &Path) -> MeasurementStatus {
        let measurement = self.policy.measurements.iter().find(|m| m.path == path);
        let expected = match measurement {
            Some(m) => &m.expected_hash,
            None => return MeasurementStatus::Error("File not in policy".to_string()),
        };

        match Self::compute_hash(path) {
            Ok(hash) => {
                if hash == *expected {
                    MeasurementStatus::Verified
                } else {
                    MeasurementStatus::Mismatch
                }
            }
            Err(e) => {
                if path.exists() {
                    MeasurementStatus::Error(e.to_string())
                } else {
                    MeasurementStatus::FileNotFound
                }
            }
        }
    }

    /// Verify all files in the policy and produce a report.
    pub fn verify_all(&mut self) -> IntegrityReport {
        let now = Utc::now();
        let mut verified_count = 0;
        let mut mismatches = Vec::new();
        let mut errors = Vec::new();

        for measurement in &self.policy.measurements {
            let (status, actual_hash) = if !measurement.path.exists() {
                (MeasurementStatus::FileNotFound, None)
            } else {
                match Self::compute_hash(&measurement.path) {
                    Ok(hash) => {
                        if hash == measurement.expected_hash {
                            (MeasurementStatus::Verified, Some(hash))
                        } else {
                            (MeasurementStatus::Mismatch, Some(hash))
                        }
                    }
                    Err(e) => (MeasurementStatus::Error(e.to_string()), None),
                }
            };

            let result = IntegrityMeasurement {
                path: measurement.path.clone(),
                expected_hash: measurement.expected_hash.clone(),
                actual_hash,
                measured_at: now,
                status,
            };

            match &result.status {
                MeasurementStatus::Verified => verified_count += 1,
                MeasurementStatus::Mismatch => mismatches.push(result.clone()),
                MeasurementStatus::Pending => { /* not yet measured, skip */ }
                MeasurementStatus::FileNotFound | MeasurementStatus::Error(_) => {
                    errors.push(result.clone());
                }
            }
        }

        let report = IntegrityReport {
            total: self.policy.measurements.len(),
            verified: verified_count,
            mismatches,
            errors,
            checked_at: now,
        };

        self.last_report = Some(report.clone());
        report
    }

    /// Hash a file and add it to the policy as a new baseline measurement.
    pub fn add_baseline(&mut self, path: &Path) -> anyhow::Result<IntegrityMeasurement> {
        let hash = Self::compute_hash(path)?;
        let measurement = IntegrityMeasurement {
            path: path.to_path_buf(),
            expected_hash: hash.clone(),
            actual_hash: Some(hash),
            measured_at: Utc::now(),
            status: MeasurementStatus::Verified,
        };
        self.policy.add_measurement(path.to_path_buf(), measurement.expected_hash.clone());
        Ok(measurement)
    }

    /// Return all files with a Mismatch status from the last report.
    pub fn tampered_files(&self) -> Vec<&IntegrityMeasurement> {
        match &self.last_report {
            Some(report) => report.mismatches.iter().collect(),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_compute_hash_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "known.txt", b"hello world");
        let hash = IntegrityVerifier::compute_hash(&path).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_compute_hash_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "empty.txt", b"");
        let hash = IntegrityVerifier::compute_hash(&path).unwrap();
        // SHA-256 of empty string
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_hash_nonexistent_file() {
        let result = IntegrityVerifier::compute_hash(Path::new("/tmp/this_file_does_not_exist_12345"));
        assert!(result.is_err());
    }

    #[test]
    fn test_policy_add_and_remove() {
        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(PathBuf::from("/tmp/a"), "hash_a".to_string());
        policy.add_measurement(PathBuf::from("/tmp/b"), "hash_b".to_string());
        assert_eq!(policy.measurements.len(), 2);

        assert!(policy.remove_measurement(Path::new("/tmp/a")));
        assert_eq!(policy.measurements.len(), 1);
        assert!(!policy.remove_measurement(Path::new("/tmp/nonexistent")));
    }

    #[test]
    fn test_verify_file_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "test.txt", b"test content");
        let hash = IntegrityVerifier::compute_hash(&path).unwrap();

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(path.clone(), hash);
        let verifier = IntegrityVerifier::new(policy);

        assert_eq!(verifier.verify_file(&path), MeasurementStatus::Verified);
    }

    #[test]
    fn test_verify_file_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "test.txt", b"original");

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(path.clone(), "wrong_hash".to_string());
        let verifier = IntegrityVerifier::new(policy);

        assert_eq!(verifier.verify_file(&path), MeasurementStatus::Mismatch);
    }

    #[test]
    fn test_verify_file_not_found() {
        let path = PathBuf::from("/tmp/integrity_test_nonexistent_file_999");
        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(path.clone(), "hash".to_string());
        let verifier = IntegrityVerifier::new(policy);

        assert_eq!(verifier.verify_file(&path), MeasurementStatus::FileNotFound);
    }

    #[test]
    fn test_verify_file_not_in_policy() {
        let policy = IntegrityPolicy::default();
        let verifier = IntegrityVerifier::new(policy);
        let status = verifier.verify_file(Path::new("/tmp/whatever"));
        match status {
            MeasurementStatus::Error(msg) => assert!(msg.contains("not in policy")),
            _ => panic!("Expected Error for file not in policy"),
        }
    }

    #[test]
    fn test_verify_all_clean() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = create_temp_file(dir.path(), "a.txt", b"alpha");
        let p2 = create_temp_file(dir.path(), "b.txt", b"beta");

        let h1 = IntegrityVerifier::compute_hash(&p1).unwrap();
        let h2 = IntegrityVerifier::compute_hash(&p2).unwrap();

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(p1, h1);
        policy.add_measurement(p2, h2);

        let mut verifier = IntegrityVerifier::new(policy);
        let report = verifier.verify_all();
        assert!(report.is_clean());
        assert_eq!(report.total, 2);
        assert_eq!(report.verified, 2);
    }

    #[test]
    fn test_verify_all_with_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = create_temp_file(dir.path(), "good.txt", b"good");
        let p2 = create_temp_file(dir.path(), "bad.txt", b"original");

        let h1 = IntegrityVerifier::compute_hash(&p1).unwrap();

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(p1, h1);
        policy.add_measurement(p2.clone(), "fake_hash".to_string());

        let mut verifier = IntegrityVerifier::new(policy);
        let report = verifier.verify_all();
        assert!(!report.is_clean());
        assert_eq!(report.verified, 1);
        assert_eq!(report.mismatches.len(), 1);
        assert_eq!(report.mismatches[0].path, p2);
    }

    #[test]
    fn test_verify_all_with_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = create_temp_file(dir.path(), "exists.txt", b"data");
        let h1 = IntegrityVerifier::compute_hash(&p1).unwrap();

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(p1, h1);
        policy.add_measurement(PathBuf::from("/tmp/missing_integrity_test_999"), "hash".to_string());

        let mut verifier = IntegrityVerifier::new(policy);
        let report = verifier.verify_all();
        assert!(!report.is_clean());
        assert_eq!(report.verified, 1);
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn test_add_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "baseline.txt", b"baseline content");

        let policy = IntegrityPolicy::default();
        let mut verifier = IntegrityVerifier::new(policy);
        let measurement = verifier.add_baseline(&path).unwrap();

        assert_eq!(measurement.status, MeasurementStatus::Verified);
        assert!(measurement.actual_hash.is_some());
        assert_eq!(measurement.expected_hash, measurement.actual_hash.unwrap());

        // Now verify should pass
        assert_eq!(verifier.verify_file(&path), MeasurementStatus::Verified);
    }

    #[test]
    fn test_add_baseline_nonexistent() {
        let policy = IntegrityPolicy::default();
        let mut verifier = IntegrityVerifier::new(policy);
        let result = verifier.add_baseline(Path::new("/tmp/no_such_file_integrity_12345"));
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_files_empty_before_verify() {
        let policy = IntegrityPolicy::default();
        let verifier = IntegrityVerifier::new(policy);
        assert!(verifier.tampered_files().is_empty());
    }

    #[test]
    fn test_tampered_files_after_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "tamper.txt", b"original");

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(path.clone(), "wrong_hash".to_string());

        let mut verifier = IntegrityVerifier::new(policy);
        verifier.verify_all();

        let tampered = verifier.tampered_files();
        assert_eq!(tampered.len(), 1);
        assert_eq!(tampered[0].path, path);
    }

    #[test]
    fn test_detect_file_modification() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_temp_file(dir.path(), "mutable.txt", b"version 1");
        let original_hash = IntegrityVerifier::compute_hash(&path).unwrap();

        let mut policy = IntegrityPolicy::default();
        policy.add_measurement(path.clone(), original_hash);

        let mut verifier = IntegrityVerifier::new(policy);

        // First check should pass
        let report = verifier.verify_all();
        assert!(report.is_clean());

        // Modify the file
        std::fs::write(&path, b"version 2").unwrap();

        // Second check should detect tampering
        let report = verifier.verify_all();
        assert!(!report.is_clean());
        assert_eq!(report.mismatches.len(), 1);
    }

    #[test]
    fn test_report_summary_format() {
        let report = IntegrityReport {
            total: 5,
            verified: 3,
            mismatches: vec![],
            errors: vec![],
            checked_at: Utc::now(),
        };
        let summary = report.summary();
        assert!(summary.contains("3/5 verified"));
        assert!(summary.contains("0 mismatches"));
        assert!(summary.contains("0 errors"));
    }

    #[test]
    fn test_measurement_status_equality() {
        assert_eq!(MeasurementStatus::Verified, MeasurementStatus::Verified);
        assert_eq!(MeasurementStatus::Mismatch, MeasurementStatus::Mismatch);
        assert_eq!(MeasurementStatus::FileNotFound, MeasurementStatus::FileNotFound);
        assert_eq!(
            MeasurementStatus::Error("a".to_string()),
            MeasurementStatus::Error("a".to_string())
        );
        assert_ne!(MeasurementStatus::Verified, MeasurementStatus::Mismatch);
        assert_ne!(
            MeasurementStatus::Error("a".to_string()),
            MeasurementStatus::Error("b".to_string())
        );
    }

    #[test]
    fn test_integrity_measurement_serialization() {
        let m = IntegrityMeasurement {
            path: PathBuf::from("/usr/bin/test"),
            expected_hash: "abc123".to_string(),
            actual_hash: Some("abc123".to_string()),
            measured_at: Utc::now(),
            status: MeasurementStatus::Verified,
        };
        let json = serde_json::to_string(&m).unwrap();
        let deserialized: IntegrityMeasurement = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.path, m.path);
        assert_eq!(deserialized.expected_hash, m.expected_hash);
    }
}
