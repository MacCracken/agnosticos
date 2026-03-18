//! Trust chain — boot chain verification.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{info, warn};

use crate::integrity::{IntegrityPolicy, IntegrityReport, IntegrityVerifier};
use crate::marketplace::trust::hash_data;

use super::types::{TrustPolicy, TrustedArtifact};

/// Verify a list of boot-critical component paths.
///
/// Builds an `IntegrityPolicy` from the trust store entries for the
/// given paths and runs a full integrity check.
///
/// This is extracted as a standalone function to keep `SigilVerifier`
/// focused on artifact-level verification. Call via
/// `SigilVerifier::verify_boot_chain` which delegates here.
pub(super) fn verify_boot_chain_impl(
    policy: &TrustPolicy,
    integrity: &mut IntegrityVerifier,
    trust_store: &HashMap<String, TrustedArtifact>,
    components: &[PathBuf],
) -> Result<IntegrityReport> {
    // Early-return if policy says not to verify on boot
    if !policy.verify_on_boot {
        tracing::debug!("Skipping boot chain verification (verify_on_boot=false)");
        return Ok(IntegrityReport {
            total: components.len(),
            verified: components.len(),
            mismatches: Vec::new(),
            errors: Vec::new(),
            checked_at: Utc::now(),
        });
    }

    let mut ip = IntegrityPolicy {
        enforce: true,
        ..IntegrityPolicy::default()
    };

    // Build a path-based index into the trust store so we can look up
    // the expected baseline hash even when the file has been tampered.
    let path_index: HashMap<&std::path::Path, &TrustedArtifact> = trust_store
        .values()
        .map(|a| (a.path.as_path(), a))
        .collect();

    for component in components {
        let expected = if let Some(artifact) = path_index.get(component.as_path()) {
            // Use the trusted baseline hash.
            artifact.content_hash.clone()
        } else {
            // No baseline — compute fresh hash (first-time measurement).
            let data = std::fs::read(component).with_context(|| {
                format!("Failed to read boot component: {}", component.display())
            })?;
            hash_data(&data)
        };

        ip.add_measurement(component.clone(), expected);
    }

    integrity.set_policy(ip);
    let report = integrity.verify_all();

    if report.is_clean() {
        info!(count = components.len(), "Boot chain verification passed");
    } else {
        warn!(
            mismatches = report.mismatches.len(),
            errors = report.errors.len(),
            "Boot chain verification FAILED"
        );
    }

    Ok(report)
}
