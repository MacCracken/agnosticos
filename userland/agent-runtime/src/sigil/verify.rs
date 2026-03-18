//! Verification logic — SigilVerifier implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use tracing::{debug, info, warn};

use crate::integrity::{IntegrityPolicy, IntegrityReport, IntegrityVerifier};
use crate::marketplace::trust::{
    hash_data, key_id_from_verifying_key, sign_data, verify_signature, PublisherKeyring,
};

use super::policy::{RevocationEntry, RevocationList};
use super::types::{
    ArtifactType, SigilStats, TrustCheck, TrustEnforcement, TrustLevel, TrustPolicy,
    TrustedArtifact, VerificationResult,
};

// ---------------------------------------------------------------------------
// SigilVerifier — the main trust engine
// ---------------------------------------------------------------------------

/// System-wide trust verification engine.
///
/// Combines Ed25519 signing from the marketplace keyring with file-level
/// integrity checks and a revocation list to provide unified trust
/// verification across the entire OS.
pub struct SigilVerifier {
    /// Publisher keyring for signature verification.
    keyring: PublisherKeyring,
    /// Active trust policy.
    policy: TrustPolicy,
    /// Revocation list.
    revocations: RevocationList,
    /// File integrity verifier (used by boot chain verification and future
    /// periodic integrity sweeps).
    integrity: IntegrityVerifier,
    /// Trust store keyed by content hash.
    trust_store: HashMap<String, TrustedArtifact>,
}

impl SigilVerifier {
    /// Create a new verifier with the given keyring and policy.
    pub fn new(keyring: PublisherKeyring, policy: TrustPolicy) -> Self {
        let integrity = IntegrityVerifier::new(IntegrityPolicy::default());
        info!(
            enforcement = %policy.enforcement,
            minimum_trust = %policy.minimum_trust_level,
            "Sigil trust verifier initialised"
        );
        Self {
            keyring,
            policy,
            revocations: RevocationList::new(),
            integrity,
            trust_store: HashMap::new(),
        }
    }

    /// Verify an artifact on disk.
    ///
    /// Reads the file, computes its hash, checks the trust store for a
    /// known signature, validates against the revocation list, and returns
    /// a detailed `VerificationResult`.
    pub fn verify_artifact(
        &self,
        path: &Path,
        artifact_type: ArtifactType,
    ) -> Result<VerificationResult> {
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read artifact: {}", path.display()))?;

        let content_hash = hash_data(&data);
        let mut checks: Vec<TrustCheck> = Vec::new();
        let mut trust_level = TrustLevel::Unverified;
        let now = Utc::now();

        // --- Check: file exists and is readable ---
        checks.push(TrustCheck {
            name: "file_readable".to_string(),
            passed: true,
            detail: format!("File read successfully: {}", path.display()),
        });

        // --- Check: hash integrity (if previously registered) ---
        let stored = self.trust_store.get(&content_hash);
        if let Some(artifact) = stored {
            trust_level = artifact.trust_level;
            checks.push(TrustCheck {
                name: "trust_store".to_string(),
                passed: true,
                detail: format!(
                    "Artifact found in trust store with level {}",
                    artifact.trust_level
                ),
            });
        } else {
            checks.push(TrustCheck {
                name: "trust_store".to_string(),
                passed: false,
                detail: "Artifact not found in trust store".to_string(),
            });
        }

        // --- Check: signature verification ---
        let sig_check = if let Some(artifact) = stored {
            if let (Some(sig), Some(key_id)) = (&artifact.signature, &artifact.signer_key_id) {
                match self.keyring.get_current_key(key_id) {
                    Some(kv) => match kv.verifying_key() {
                        Ok(vk) => match verify_signature(&data, sig, &vk) {
                            Ok(()) => {
                                if trust_level < TrustLevel::Verified {
                                    trust_level = TrustLevel::Verified;
                                }
                                TrustCheck {
                                    name: "signature".to_string(),
                                    passed: true,
                                    detail: format!("Signature verified with key {}", key_id),
                                }
                            }
                            Err(e) => {
                                trust_level = TrustLevel::Unverified;
                                TrustCheck {
                                    name: "signature".to_string(),
                                    passed: false,
                                    detail: format!("Signature verification failed: {}", e),
                                }
                            }
                        },
                        Err(e) => TrustCheck {
                            name: "signature".to_string(),
                            passed: false,
                            detail: format!("Failed to decode verifying key: {}", e),
                        },
                    },
                    None => TrustCheck {
                        name: "signature".to_string(),
                        passed: false,
                        detail: format!("Signer key {} not found in keyring", key_id),
                    },
                }
            } else {
                TrustCheck {
                    name: "signature".to_string(),
                    passed: false,
                    detail: "Artifact has no signature".to_string(),
                }
            }
        } else {
            TrustCheck {
                name: "signature".to_string(),
                passed: false,
                detail: "Artifact not in trust store; no signature to check".to_string(),
            }
        };
        checks.push(sig_check);

        // --- Check: revocation ---
        if self.policy.revocation_check {
            let key_id = stored.and_then(|a| a.signer_key_id.as_deref());
            let revoked = self.check_revocation(key_id, &content_hash);
            if revoked {
                trust_level = TrustLevel::Revoked;
                warn!(
                    path = %path.display(),
                    hash = %content_hash,
                    "Artifact or signing key is revoked"
                );
            }
            checks.push(TrustCheck {
                name: "revocation".to_string(),
                passed: !revoked,
                detail: if revoked {
                    "Artifact or signing key is revoked".to_string()
                } else {
                    "Not revoked".to_string()
                },
            });
        }

        // --- Check: trust level meets policy ---
        let meets_policy = trust_level >= self.policy.minimum_trust_level;
        checks.push(TrustCheck {
            name: "policy".to_string(),
            passed: meets_policy,
            detail: format!(
                "Trust level {} {} minimum {}",
                trust_level,
                if meets_policy { "meets" } else { "below" },
                self.policy.minimum_trust_level
            ),
        });

        // Determine overall pass/fail based on enforcement mode
        let all_critical_passed = meets_policy && trust_level != TrustLevel::Revoked;
        let passed = match self.policy.enforcement {
            TrustEnforcement::Strict => all_critical_passed,
            TrustEnforcement::Permissive => {
                if !all_critical_passed {
                    warn!(
                        path = %path.display(),
                        trust_level = %trust_level,
                        "Permissive mode: allowing artifact below minimum trust"
                    );
                }
                trust_level != TrustLevel::Revoked
            }
            TrustEnforcement::AuditOnly => {
                if !all_critical_passed {
                    warn!(
                        path = %path.display(),
                        trust_level = %trust_level,
                        "Audit-only: artifact would be blocked under strict policy"
                    );
                }
                // Revoked artifacts must NEVER pass regardless of enforcement mode
                trust_level != TrustLevel::Revoked
            }
        };

        let artifact = TrustedArtifact {
            path: path.to_path_buf(),
            artifact_type,
            content_hash,
            signature: stored.and_then(|a| a.signature.clone()),
            signer_key_id: stored.and_then(|a| a.signer_key_id.clone()),
            trust_level,
            verified_at: Some(now),
            metadata: stored.map(|a| a.metadata.clone()).unwrap_or_default(),
        };

        debug!(
            path = %path.display(),
            trust_level = %trust_level,
            passed = passed,
            checks = checks.len(),
            "Artifact verification complete"
        );

        Ok(VerificationResult {
            artifact,
            passed,
            checks,
            verified_at: now,
        })
    }

    /// Convenience method: verify an agent binary.
    ///
    /// In addition to standard artifact verification, checks that the file
    /// has execute permission and that unsigned agents are allowed by policy.
    pub fn verify_agent_binary(&self, path: &Path) -> Result<VerificationResult> {
        // Early-return if policy says not to verify on execute
        if !self.policy.verify_on_execute {
            debug!(
                path = %path.display(),
                "Skipping agent binary verification (verify_on_execute=false)"
            );
            let data = std::fs::read(path)
                .with_context(|| format!("Failed to read artifact: {}", path.display()))?;
            let content_hash = hash_data(&data);
            let now = Utc::now();
            return Ok(VerificationResult {
                artifact: TrustedArtifact {
                    path: path.to_path_buf(),
                    artifact_type: ArtifactType::AgentBinary,
                    content_hash,
                    signature: None,
                    signer_key_id: None,
                    trust_level: TrustLevel::Unverified,
                    verified_at: Some(now),
                    metadata: HashMap::new(),
                },
                passed: true,
                checks: vec![TrustCheck {
                    name: "skipped".to_string(),
                    passed: true,
                    detail: "Verification skipped: verify_on_execute is disabled".to_string(),
                }],
                verified_at: now,
            });
        }

        let mut result = self.verify_artifact(path, ArtifactType::AgentBinary)?;

        // Check execute permission
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(path)
                .with_context(|| format!("Failed to stat {}", path.display()))?;
            let executable = meta.permissions().mode() & 0o111 != 0;
            result.checks.push(TrustCheck {
                name: "execute_permission".to_string(),
                passed: executable,
                detail: if executable {
                    "File has execute permission".to_string()
                } else {
                    "File lacks execute permission".to_string()
                },
            });
            if !executable {
                result.passed = false;
            }
        }

        // Unsigned agent check — blocks in both Strict and Permissive when
        // allow_unsigned_agents is false.
        if result.artifact.signature.is_none() && !self.policy.allow_unsigned_agents {
            result.checks.push(TrustCheck {
                name: "unsigned_agent".to_string(),
                passed: false,
                detail: "Unsigned agent binaries are not allowed by policy".to_string(),
            });
            if self.policy.enforcement == TrustEnforcement::Strict
                || self.policy.enforcement == TrustEnforcement::Permissive
            {
                result.passed = false;
            }
        }

        Ok(result)
    }

    /// Verify a package file (`.ark` or `.deb`).
    ///
    /// If `expected_hash` is provided, the file's hash must match it exactly.
    pub fn verify_package(
        &self,
        path: &Path,
        expected_hash: Option<&str>,
    ) -> Result<VerificationResult> {
        // Early-return if policy says not to verify on install
        if !self.policy.verify_on_install {
            debug!(
                path = %path.display(),
                "Skipping package verification (verify_on_install=false)"
            );
            let data = std::fs::read(path)
                .with_context(|| format!("Failed to read artifact: {}", path.display()))?;
            let content_hash = hash_data(&data);
            let now = Utc::now();
            return Ok(VerificationResult {
                artifact: TrustedArtifact {
                    path: path.to_path_buf(),
                    artifact_type: ArtifactType::Package,
                    content_hash,
                    signature: None,
                    signer_key_id: None,
                    trust_level: TrustLevel::Unverified,
                    verified_at: Some(now),
                    metadata: HashMap::new(),
                },
                passed: true,
                checks: vec![TrustCheck {
                    name: "skipped".to_string(),
                    passed: true,
                    detail: "Verification skipped: verify_on_install is disabled".to_string(),
                }],
                verified_at: now,
            });
        }

        let mut result = self.verify_artifact(path, ArtifactType::Package)?;

        if let Some(expected) = expected_hash {
            let matches = result.artifact.content_hash == expected;
            result.checks.push(TrustCheck {
                name: "expected_hash".to_string(),
                passed: matches,
                detail: if matches {
                    format!("Content hash matches expected {}", expected)
                } else {
                    format!(
                        "Hash mismatch: got {} expected {}",
                        result.artifact.content_hash, expected
                    )
                },
            });
            if !matches {
                result.passed = false;
            }
        }

        Ok(result)
    }

    /// Sign an artifact and register it in the trust store.
    ///
    /// Reads the file, computes the SHA-256 hash, signs the hash with the
    /// given key, and stores the result.
    pub fn sign_artifact(
        &mut self,
        path: &Path,
        signing_key: &SigningKey,
        artifact_type: ArtifactType,
    ) -> Result<TrustedArtifact> {
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read artifact for signing: {}", path.display()))?;

        let content_hash = hash_data(&data);
        let signature = sign_data(&data, signing_key);
        let vk = signing_key.verifying_key();
        let key_id = key_id_from_verifying_key(&vk);
        let now = Utc::now();

        // Determine trust level based on whether the key is in the keyring
        // and currently valid. Unknown signers get Community trust.
        let trust_level = match self.keyring.get_current_key(&key_id) {
            Some(_) => TrustLevel::Verified,
            None => {
                warn!(
                    key_id = %key_id,
                    "Signing key not found in keyring; assigning Community trust"
                );
                TrustLevel::Community
            }
        };

        let artifact = TrustedArtifact {
            path: path.to_path_buf(),
            artifact_type,
            content_hash: content_hash.clone(),
            signature: Some(signature),
            signer_key_id: Some(key_id.clone()),
            trust_level,
            verified_at: Some(now),
            metadata: HashMap::new(),
        };

        info!(
            path = %path.display(),
            key_id = %key_id,
            hash = %content_hash,
            "Artifact signed and registered"
        );

        self.trust_store.insert(content_hash, artifact.clone());
        Ok(artifact)
    }

    /// Register a pre-built `TrustedArtifact` in the trust store.
    ///
    /// `SystemCore` trust level is not allowed through this method — it will
    /// be downgraded to `Verified`. Use `register_system_core()` for
    /// system-critical components.
    pub fn register_trusted(&mut self, mut artifact: TrustedArtifact) {
        if artifact.trust_level == TrustLevel::SystemCore {
            warn!(
                path = %artifact.path.display(),
                hash = %artifact.content_hash,
                "SystemCore trust level not allowed via register_trusted; downgrading to Verified"
            );
            artifact.trust_level = TrustLevel::Verified;
        }
        debug!(
            path = %artifact.path.display(),
            hash = %artifact.content_hash,
            trust_level = %artifact.trust_level,
            "Artifact registered in trust store"
        );
        self.trust_store
            .insert(artifact.content_hash.clone(), artifact);
    }

    /// Register a system-core artifact in the trust store.
    ///
    /// This is the only path to `SystemCore` trust. In the future this may
    /// require additional attestation (e.g. TPM measurement).
    pub fn register_system_core(&mut self, artifact: TrustedArtifact) {
        debug!(
            path = %artifact.path.display(),
            hash = %artifact.content_hash,
            "SystemCore artifact registered in trust store"
        );
        let mut art = artifact;
        art.trust_level = TrustLevel::SystemCore;
        self.trust_store.insert(art.content_hash.clone(), art);
    }

    /// Check whether a key or artifact hash has been revoked.
    ///
    /// Returns `true` if revoked.
    pub fn check_revocation(&self, key_id: Option<&str>, content_hash: &str) -> bool {
        if self.revocations.is_artifact_revoked(content_hash) {
            return true;
        }
        if let Some(kid) = key_id {
            if self.revocations.is_key_revoked(kid) {
                return true;
            }
        }
        false
    }

    /// Return the number of entries in the revocation list.
    pub fn revocation_count(&self) -> usize {
        self.revocations.len()
    }

    /// Add a revocation entry.
    pub fn add_revocation(&mut self, entry: RevocationEntry) -> Result<()> {
        info!(
            key_id = ?entry.key_id,
            content_hash = ?entry.content_hash,
            reason = %entry.reason,
            "Revocation added"
        );
        self.revocations.add(entry)
    }

    /// Look up the trust level for a content hash. Returns `Unverified` if
    /// the hash is not in the trust store.
    pub fn trust_level_for(&self, content_hash: &str) -> TrustLevel {
        self.trust_store
            .get(content_hash)
            .map(|a| a.trust_level)
            .unwrap_or(TrustLevel::Unverified)
    }

    /// Return a reference to the active trust policy.
    pub fn policy(&self) -> &TrustPolicy {
        &self.policy
    }

    /// Verify a list of boot-critical component paths.
    ///
    /// Builds an `IntegrityPolicy` from the trust store entries for the
    /// given paths and runs a full integrity check.
    pub fn verify_boot_chain(&mut self, components: &[PathBuf]) -> Result<IntegrityReport> {
        use super::chain::verify_boot_chain_impl;
        verify_boot_chain_impl(
            &self.policy,
            &mut self.integrity,
            &self.trust_store,
            components,
        )
    }

    /// Save the trust store to a JSON file on disk.
    pub fn save_trust_store(&self, path: &Path) -> Result<()> {
        let artifacts: Vec<&TrustedArtifact> = self.trust_store.values().collect();
        let json =
            serde_json::to_string_pretty(&artifacts).context("Failed to serialize trust store")?;
        std::fs::write(path, json)
            .with_context(|| format!("Failed to write trust store to {}", path.display()))?;
        info!(path = %path.display(), count = artifacts.len(), "Trust store saved");
        Ok(())
    }

    /// Load trust store entries from a JSON file on disk.
    ///
    /// Returns the number of entries loaded. Existing entries with the same
    /// content hash are overwritten.
    pub fn load_trust_store(&mut self, path: &Path) -> Result<usize> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read trust store from {}", path.display()))?;
        let artifacts: Vec<TrustedArtifact> =
            serde_json::from_str(&json).context("Failed to deserialize trust store")?;
        let count = artifacts.len();
        for artifact in artifacts {
            self.trust_store
                .insert(artifact.content_hash.clone(), artifact);
        }
        info!(path = %path.display(), count = count, "Trust store loaded");
        Ok(count)
    }

    /// Save the revocation list to a JSON file on disk.
    pub fn save_revocations(&self, path: &Path) -> Result<()> {
        let json = self.revocations.to_json()?;
        std::fs::write(path, json)
            .with_context(|| format!("Failed to write revocations to {}", path.display()))?;
        info!(path = %path.display(), count = self.revocations.len(), "Revocations saved");
        Ok(())
    }

    /// Load revocation entries from a JSON file on disk.
    ///
    /// Returns the number of entries loaded. Entries are appended to the
    /// existing revocation list.
    pub fn load_revocations(&mut self, path: &Path) -> Result<usize> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read revocations from {}", path.display()))?;
        let loaded = RevocationList::from_json(&json)?;
        let count = loaded.len();
        for entry in loaded.entries {
            self.revocations.add(entry)?;
        }
        info!(path = %path.display(), count = count, "Revocations loaded");
        Ok(count)
    }

    /// Compute summary statistics for the trust store.
    pub fn stats(&self) -> SigilStats {
        let total_artifacts = self.trust_store.len();
        let verified_count = self
            .trust_store
            .values()
            .filter(|a| a.verified_at.is_some())
            .count();
        let revoked_count = self
            .trust_store
            .values()
            .filter(|a| a.trust_level == TrustLevel::Revoked)
            .count();

        let mut trust_level_counts: HashMap<TrustLevel, usize> = HashMap::new();
        for artifact in self.trust_store.values() {
            *trust_level_counts.entry(artifact.trust_level).or_insert(0) += 1;
        }

        SigilStats {
            total_artifacts,
            verified_count,
            revoked_count,
            trust_level_counts,
        }
    }
}
