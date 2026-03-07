//! Sigil — System-Wide Trust Verification for AGNOS
//!
//! Unified trust module that extends marketplace signing into a complete
//! trust chain covering boot, agent binaries, configs, and packages.
//! Named after the Latin word for "seal" — sigil seals trust into AGNOS.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::integrity::{IntegrityPolicy, IntegrityReport, IntegrityVerifier};
use crate::marketplace::trust::{
    hash_data, key_id_from_verifying_key, sign_data, verify_signature, PublisherKeyring,
};

// ---------------------------------------------------------------------------
// Trust levels
// ---------------------------------------------------------------------------

/// Trust level assigned to an artifact or component.
///
/// Ordered from highest trust to lowest. `SystemCore` is the most trusted,
/// `Revoked` the least.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Core OS component — signed by the AGNOS project key, measured at boot.
    SystemCore,
    /// Third-party artifact whose signature has been verified against a
    /// trusted publisher key.
    Verified,
    /// Community-contributed artifact with a valid signature but from a
    /// publisher that is not in the curated keyring.
    Community,
    /// Artifact with no signature or unknown signer.
    Unverified,
    /// Artifact or key that has been explicitly revoked.
    Revoked,
}

impl TrustLevel {
    /// Numeric rank for ordering (higher = more trusted).
    fn rank(self) -> u8 {
        match self {
            Self::SystemCore => 4,
            Self::Verified => 3,
            Self::Community => 2,
            Self::Unverified => 1,
            Self::Revoked => 0,
        }
    }
}

impl fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemCore => write!(f, "SystemCore"),
            Self::Verified => write!(f, "Verified"),
            Self::Community => write!(f, "Community"),
            Self::Unverified => write!(f, "Unverified"),
            Self::Revoked => write!(f, "Revoked"),
        }
    }
}

impl PartialOrd for TrustLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TrustLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

// ---------------------------------------------------------------------------
// Enforcement mode
// ---------------------------------------------------------------------------

/// How strictly the trust policy is enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustEnforcement {
    /// Block any artifact that does not meet the minimum trust level.
    Strict,
    /// Allow artifacts below the minimum trust level with a warning.
    Permissive,
    /// Log violations but never block — useful during migration.
    AuditOnly,
}

impl fmt::Display for TrustEnforcement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict => write!(f, "Strict"),
            Self::Permissive => write!(f, "Permissive"),
            Self::AuditOnly => write!(f, "AuditOnly"),
        }
    }
}

// ---------------------------------------------------------------------------
// Artifact types
// ---------------------------------------------------------------------------

/// Classification of a trusted artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactType {
    /// An agent binary (executed by the runtime).
    AgentBinary,
    /// A core system binary (part of the OS).
    SystemBinary,
    /// A configuration file.
    Config,
    /// An `.ark` or `.deb` package.
    Package,
    /// A boot-critical component (kernel, initramfs, etc.).
    BootComponent,
}

impl fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentBinary => write!(f, "AgentBinary"),
            Self::SystemBinary => write!(f, "SystemBinary"),
            Self::Config => write!(f, "Config"),
            Self::Package => write!(f, "Package"),
            Self::BootComponent => write!(f, "BootComponent"),
        }
    }
}

// ---------------------------------------------------------------------------
// Trust policy
// ---------------------------------------------------------------------------

/// Configurable trust policy controlling verification behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPolicy {
    /// How violations are handled.
    pub enforcement: TrustEnforcement,
    /// Minimum trust level required for an artifact to be accepted.
    pub minimum_trust_level: TrustLevel,
    /// Whether unsigned agent binaries are allowed to execute.
    pub allow_unsigned_agents: bool,
    /// Verify boot-critical components on startup.
    pub verify_on_boot: bool,
    /// Verify packages before installation.
    pub verify_on_install: bool,
    /// Verify agent binaries before execution.
    pub verify_on_execute: bool,
    /// Check the revocation list during verification.
    pub revocation_check: bool,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        Self {
            enforcement: TrustEnforcement::Strict,
            minimum_trust_level: TrustLevel::Verified,
            allow_unsigned_agents: false,
            verify_on_boot: true,
            verify_on_install: true,
            verify_on_execute: true,
            revocation_check: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Trusted artifact
// ---------------------------------------------------------------------------

/// An artifact that has been registered in the trust store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedArtifact {
    /// Filesystem path of the artifact.
    pub path: PathBuf,
    /// What kind of artifact this is.
    pub artifact_type: ArtifactType,
    /// SHA-256 hash of the artifact's contents.
    pub content_hash: String,
    /// Ed25519 signature bytes (if signed).
    pub signature: Option<Vec<u8>>,
    /// Key ID of the signer (if signed).
    pub signer_key_id: Option<String>,
    /// Determined trust level.
    pub trust_level: TrustLevel,
    /// When the artifact was last verified.
    pub verified_at: Option<DateTime<Utc>>,
    /// Arbitrary metadata (e.g. version, publisher name).
    pub metadata: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Verification result
// ---------------------------------------------------------------------------

/// Outcome of verifying a single artifact.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// The artifact that was verified.
    pub artifact: TrustedArtifact,
    /// Whether the artifact passed all checks.
    pub passed: bool,
    /// Individual checks that were performed.
    pub checks: Vec<TrustCheck>,
    /// When verification occurred.
    pub verified_at: DateTime<Utc>,
}

/// A single check performed during verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustCheck {
    /// Short name of the check (e.g. "signature", "revocation").
    pub name: String,
    /// Whether this check passed.
    pub passed: bool,
    /// Human-readable detail about the outcome.
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Revocation
// ---------------------------------------------------------------------------

/// A single revocation entry — either a key or a specific artifact hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationEntry {
    /// Revoke all artifacts signed by this key.
    pub key_id: Option<String>,
    /// Revoke a specific artifact by its content hash.
    pub content_hash: Option<String>,
    /// Reason for revocation.
    pub reason: String,
    /// When the revocation was created.
    pub revoked_at: DateTime<Utc>,
    /// Identity of the revoker.
    pub revoked_by: String,
}

/// A list of revoked keys and artifact hashes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevocationList {
    entries: Vec<RevocationEntry>,
}

impl RevocationList {
    /// Create an empty revocation list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a revocation entry.
    ///
    /// At least one of `key_id` or `content_hash` must be `Some`,
    /// otherwise an error is returned.
    pub fn add(&mut self, entry: RevocationEntry) -> Result<()> {
        if entry.key_id.is_none() && entry.content_hash.is_none() {
            anyhow::bail!("RevocationEntry must have at least one of key_id or content_hash set");
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Check whether a key ID has been revoked.
    pub fn is_key_revoked(&self, key_id: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.key_id.as_deref() == Some(key_id))
    }

    /// Check whether an artifact content hash has been revoked.
    pub fn is_artifact_revoked(&self, content_hash: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.content_hash.as_deref() == Some(content_hash))
    }

    /// Number of entries in the revocation list.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the revocation list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.entries).context("Failed to serialize revocation list")
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        let entries: Vec<RevocationEntry> =
            serde_json::from_str(json).context("Failed to deserialize revocation list")?;
        Ok(Self { entries })
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Summary statistics from the trust store.
#[derive(Debug, Clone)]
pub struct SigilStats {
    /// Total artifacts in the trust store.
    pub total_artifacts: usize,
    /// How many have been verified at least once.
    pub verified_count: usize,
    /// How many are currently revoked.
    pub revoked_count: usize,
    /// Breakdown by trust level.
    pub trust_level_counts: HashMap<TrustLevel, usize>,
}

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

    /// Verify a list of boot-critical component paths.
    ///
    /// Builds an `IntegrityPolicy` from the trust store entries for the
    /// given paths and runs a full integrity check.
    pub fn verify_boot_chain(&mut self, components: &[PathBuf]) -> Result<IntegrityReport> {
        // Early-return if policy says not to verify on boot
        if !self.policy.verify_on_boot {
            debug!("Skipping boot chain verification (verify_on_boot=false)");
            return Ok(IntegrityReport {
                total: components.len(),
                verified: components.len(),
                mismatches: Vec::new(),
                errors: Vec::new(),
                checked_at: Utc::now(),
            });
        }

        let mut policy = IntegrityPolicy::default();
        policy.enforce = true;

        // Build a path-based index into the trust store so we can look up
        // the expected baseline hash even when the file has been tampered.
        let path_index: HashMap<&Path, &TrustedArtifact> = self
            .trust_store
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

            policy.add_measurement(component.clone(), expected);
        }

        self.integrity.set_policy(policy);
        let report = self.integrity.verify_all();

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

    /// Return a reference to the active trust policy.
    pub fn policy(&self) -> &TrustPolicy {
        &self.policy
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

    /// Helper: create a temp dir with a file inside.
    fn temp_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    /// Helper: make a file executable on Unix.
    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }

    /// Helper: generate a keyring with one key already added.
    fn keyring_with_key(
        dir: &Path,
    ) -> (
        PublisherKeyring,
        SigningKey,
        ed25519_dalek::VerifyingKey,
        String,
    ) {
        use crate::marketplace::trust::{generate_keypair, KeyVersion};

        let (sk, vk, kid) = generate_keypair();
        let mut kr = PublisherKeyring::new(dir);
        kr.add_key(KeyVersion {
            key_id: kid.clone(),
            valid_from: Utc::now() - chrono::Duration::hours(1),
            valid_until: None,
            public_key_hex: {
                // Inline hex encode (same logic as trust.rs)
                vk.to_bytes().iter().map(|b| format!("{:02x}", b)).collect()
            },
        });
        (kr, sk, vk, kid)
    }

    // -----------------------------------------------------------------------
    // TrustLevel
    // -----------------------------------------------------------------------

    #[test]
    fn trust_level_ordering() {
        assert!(TrustLevel::SystemCore > TrustLevel::Verified);
        assert!(TrustLevel::Verified > TrustLevel::Community);
        assert!(TrustLevel::Community > TrustLevel::Unverified);
        assert!(TrustLevel::Unverified > TrustLevel::Revoked);
    }

    #[test]
    fn trust_level_display() {
        assert_eq!(TrustLevel::SystemCore.to_string(), "SystemCore");
        assert_eq!(TrustLevel::Verified.to_string(), "Verified");
        assert_eq!(TrustLevel::Community.to_string(), "Community");
        assert_eq!(TrustLevel::Unverified.to_string(), "Unverified");
        assert_eq!(TrustLevel::Revoked.to_string(), "Revoked");
    }

    #[test]
    fn trust_level_equality() {
        assert_eq!(TrustLevel::Verified, TrustLevel::Verified);
        assert_ne!(TrustLevel::Verified, TrustLevel::Community);
    }

    // -----------------------------------------------------------------------
    // TrustPolicy
    // -----------------------------------------------------------------------

    #[test]
    fn trust_policy_defaults() {
        let p = TrustPolicy::default();
        assert_eq!(p.enforcement, TrustEnforcement::Strict);
        assert_eq!(p.minimum_trust_level, TrustLevel::Verified);
        assert!(!p.allow_unsigned_agents);
        assert!(p.verify_on_boot);
        assert!(p.verify_on_install);
        assert!(p.verify_on_execute);
        assert!(p.revocation_check);
    }

    // -----------------------------------------------------------------------
    // TrustEnforcement
    // -----------------------------------------------------------------------

    #[test]
    fn trust_enforcement_variants() {
        assert_eq!(TrustEnforcement::Strict.to_string(), "Strict");
        assert_eq!(TrustEnforcement::Permissive.to_string(), "Permissive");
        assert_eq!(TrustEnforcement::AuditOnly.to_string(), "AuditOnly");
    }

    // -----------------------------------------------------------------------
    // ArtifactType
    // -----------------------------------------------------------------------

    #[test]
    fn artifact_type_variants() {
        assert_eq!(ArtifactType::AgentBinary.to_string(), "AgentBinary");
        assert_eq!(ArtifactType::SystemBinary.to_string(), "SystemBinary");
        assert_eq!(ArtifactType::Config.to_string(), "Config");
        assert_eq!(ArtifactType::Package.to_string(), "Package");
        assert_eq!(ArtifactType::BootComponent.to_string(), "BootComponent");
    }

    // -----------------------------------------------------------------------
    // Verify artifact — valid signature
    // -----------------------------------------------------------------------

    #[test]
    fn verify_artifact_with_valid_signature() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "agent.bin", b"trusted binary");

        let (kr, sk, _vk, kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        // Sign and register
        let artifact = verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();
        assert_eq!(artifact.trust_level, TrustLevel::Verified);
        assert_eq!(artifact.signer_key_id.as_deref(), Some(kid.as_str()));

        // Verify
        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "signature" && c.passed));
    }

    // -----------------------------------------------------------------------
    // Verify artifact — no signature, strict → fail
    // -----------------------------------------------------------------------

    #[test]
    fn verify_unsigned_artifact_strict_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "unsigned.bin", b"no sig");

        let kr = PublisherKeyring::new(dir.path());
        let verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(!result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Unverified);
    }

    // -----------------------------------------------------------------------
    // Verify artifact — no signature, permissive → pass with lower trust
    // -----------------------------------------------------------------------

    #[test]
    fn verify_unsigned_artifact_permissive_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "unsigned.bin", b"no sig");

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.enforcement = TrustEnforcement::Permissive;
        let verifier = SigilVerifier::new(kr, policy);

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        // Permissive allows non-revoked artifacts even below minimum trust
        assert!(result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Unverified);
    }

    // -----------------------------------------------------------------------
    // Verify artifact — wrong signature
    // -----------------------------------------------------------------------

    #[test]
    fn verify_artifact_wrong_signature() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "bad_sig.bin", b"content");
        let content_hash = hash_data(b"content");

        let (kr, _sk, _vk, kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        // Register with a bogus signature
        verifier.register_trusted(TrustedArtifact {
            path: path.clone(),
            artifact_type: ArtifactType::AgentBinary,
            content_hash: content_hash.clone(),
            signature: Some(vec![0u8; 64]),
            signer_key_id: Some(kid),
            trust_level: TrustLevel::Verified,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        // Signature check should fail
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "signature" && !c.passed));
    }

    // -----------------------------------------------------------------------
    // Revoked key
    // -----------------------------------------------------------------------

    #[test]
    fn verify_revoked_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "rev.bin", b"revoked key content");

        let (kr, sk, _vk, kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();

        // Revoke the key
        verifier
            .add_revocation(RevocationEntry {
                key_id: Some(kid.clone()),
                content_hash: None,
                reason: "Compromised".to_string(),
                revoked_at: Utc::now(),
                revoked_by: "admin".to_string(),
            })
            .unwrap();

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(!result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Revoked);
    }

    // -----------------------------------------------------------------------
    // Revoked artifact hash
    // -----------------------------------------------------------------------

    #[test]
    fn verify_revoked_artifact_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "rev_hash.bin", b"revoked hash content");
        let content_hash = hash_data(b"revoked hash content");

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier
            .sign_artifact(&path, &sk, ArtifactType::Config)
            .unwrap();

        verifier
            .add_revocation(RevocationEntry {
                key_id: None,
                content_hash: Some(content_hash),
                reason: "Malicious config".to_string(),
                revoked_at: Utc::now(),
                revoked_by: "audit-bot".to_string(),
            })
            .unwrap();

        let result = verifier
            .verify_artifact(&path, ArtifactType::Config)
            .unwrap();
        assert!(!result.passed);
    }

    // -----------------------------------------------------------------------
    // RevocationList
    // -----------------------------------------------------------------------

    #[test]
    fn revocation_list_add_check() {
        let mut rl = RevocationList::new();
        assert!(rl.is_empty());
        assert_eq!(rl.len(), 0);

        rl.add(RevocationEntry {
            key_id: Some("key1".to_string()),
            content_hash: None,
            reason: "test".to_string(),
            revoked_at: Utc::now(),
            revoked_by: "tester".to_string(),
        })
        .unwrap();
        assert_eq!(rl.len(), 1);
        assert!(rl.is_key_revoked("key1"));
        assert!(!rl.is_key_revoked("key2"));
        assert!(!rl.is_artifact_revoked("somehash"));
    }

    #[test]
    fn revocation_list_artifact_revoked() {
        let mut rl = RevocationList::new();
        rl.add(RevocationEntry {
            key_id: None,
            content_hash: Some("abc123".to_string()),
            reason: "bad".to_string(),
            revoked_at: Utc::now(),
            revoked_by: "admin".to_string(),
        })
        .unwrap();
        assert!(rl.is_artifact_revoked("abc123"));
        assert!(!rl.is_artifact_revoked("def456"));
    }

    #[test]
    fn revocation_list_serialize_deserialize() {
        let mut rl = RevocationList::new();
        rl.add(RevocationEntry {
            key_id: Some("k1".to_string()),
            content_hash: Some("h1".to_string()),
            reason: "compromised".to_string(),
            revoked_at: Utc::now(),
            revoked_by: "root".to_string(),
        })
        .unwrap();

        let json = rl.to_json().unwrap();
        let recovered = RevocationList::from_json(&json).unwrap();
        assert_eq!(recovered.len(), 1);
        assert!(recovered.is_key_revoked("k1"));
        assert!(recovered.is_artifact_revoked("h1"));
    }

    #[test]
    fn revocation_list_empty() {
        let rl = RevocationList::new();
        assert!(rl.is_empty());
        assert!(!rl.is_key_revoked("anything"));
        assert!(!rl.is_artifact_revoked("anything"));
        let json = rl.to_json().unwrap();
        let recovered = RevocationList::from_json(&json).unwrap();
        assert!(recovered.is_empty());
    }

    // -----------------------------------------------------------------------
    // Sign and verify roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn sign_and_verify_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "roundtrip.bin", b"roundtrip data");

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let artifact = verifier
            .sign_artifact(&path, &sk, ArtifactType::SystemBinary)
            .unwrap();
        assert!(artifact.signature.is_some());
        assert!(artifact.signer_key_id.is_some());

        let result = verifier
            .verify_artifact(&path, ArtifactType::SystemBinary)
            .unwrap();
        assert!(result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Verified);
    }

    // -----------------------------------------------------------------------
    // Trust store registration and lookup
    // -----------------------------------------------------------------------

    #[test]
    fn trust_store_register_and_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let hash = "deadbeef".to_string();
        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/opt/agent/bin"),
            artifact_type: ArtifactType::AgentBinary,
            content_hash: hash.clone(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Community,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });

        assert_eq!(verifier.trust_level_for(&hash), TrustLevel::Community);
        assert_eq!(verifier.trust_level_for("unknown"), TrustLevel::Unverified);
    }

    // -----------------------------------------------------------------------
    // verify_agent_binary with real temp file
    // -----------------------------------------------------------------------

    #[test]
    fn verify_agent_binary_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "my_agent", b"agent code");
        #[cfg(unix)]
        make_executable(&path);

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();

        let result = verifier.verify_agent_binary(&path).unwrap();
        assert!(result.passed);
        #[cfg(unix)]
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "execute_permission" && c.passed));
    }

    // -----------------------------------------------------------------------
    // verify_package — matching hash
    // -----------------------------------------------------------------------

    #[test]
    fn verify_package_matching_hash() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"package data v1.0";
        let path = temp_file(dir.path(), "pkg.ark", content);
        let expected_hash = hash_data(content);

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier
            .sign_artifact(&path, &sk, ArtifactType::Package)
            .unwrap();

        let result = verifier
            .verify_package(&path, Some(&expected_hash))
            .unwrap();
        assert!(result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "expected_hash" && c.passed));
    }

    // -----------------------------------------------------------------------
    // verify_package — mismatched hash
    // -----------------------------------------------------------------------

    #[test]
    fn verify_package_mismatched_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "pkg.ark", b"real content");

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier
            .sign_artifact(&path, &sk, ArtifactType::Package)
            .unwrap();

        let result = verifier
            .verify_package(&path, Some("wrong_hash_value"))
            .unwrap();
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "expected_hash" && !c.passed));
    }

    // -----------------------------------------------------------------------
    // verify_boot_chain — all clean
    // -----------------------------------------------------------------------

    #[test]
    fn verify_boot_chain_clean() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = temp_file(dir.path(), "vmlinuz", b"kernel image");
        let p2 = temp_file(dir.path(), "initramfs", b"initramfs image");

        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let report = verifier.verify_boot_chain(&[p1, p2]).unwrap();
        assert!(report.is_clean());
        assert_eq!(report.total, 2);
        assert_eq!(report.verified, 2);
    }

    // -----------------------------------------------------------------------
    // verify_boot_chain — tampered file
    // -----------------------------------------------------------------------

    #[test]
    fn verify_boot_chain_tampered() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = temp_file(dir.path(), "vmlinuz", b"kernel");
        let hash1 = hash_data(b"kernel");

        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        // Register baseline via register_system_core (SystemCore trust)
        verifier.register_system_core(TrustedArtifact {
            path: p1.clone(),
            artifact_type: ArtifactType::BootComponent,
            content_hash: hash1.clone(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::SystemCore,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });

        // Tamper
        std::fs::write(&p1, b"modified kernel").unwrap();

        let report = verifier.verify_boot_chain(&[p1]).unwrap();
        assert!(!report.is_clean());
        assert_eq!(report.mismatches.len(), 1);
    }

    // -----------------------------------------------------------------------
    // SigilStats accuracy
    // -----------------------------------------------------------------------

    #[test]
    fn sigil_stats_accuracy() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/a"),
            artifact_type: ArtifactType::AgentBinary,
            content_hash: "h1".to_string(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Verified,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });
        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/b"),
            artifact_type: ArtifactType::Config,
            content_hash: "h2".to_string(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Revoked,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });
        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/c"),
            artifact_type: ArtifactType::Package,
            content_hash: "h3".to_string(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Verified,
            verified_at: None,
            metadata: HashMap::new(),
        });

        let stats = verifier.stats();
        assert_eq!(stats.total_artifacts, 3);
        assert_eq!(stats.verified_count, 2); // two have verified_at
        assert_eq!(stats.revoked_count, 1);
        assert_eq!(
            *stats.trust_level_counts.get(&TrustLevel::Verified).unwrap(),
            2
        );
        assert_eq!(
            *stats.trust_level_counts.get(&TrustLevel::Revoked).unwrap(),
            1
        );
    }

    // -----------------------------------------------------------------------
    // Policy enforcement: strict blocks unverified
    // -----------------------------------------------------------------------

    #[test]
    fn policy_strict_blocks_unverified() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "unverified.bin", b"data");

        let kr = PublisherKeyring::new(dir.path());
        let verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(!result.passed);
    }

    // -----------------------------------------------------------------------
    // Policy enforcement: audit-only logs but allows
    // -----------------------------------------------------------------------

    #[test]
    fn policy_audit_only_allows() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "audit.bin", b"data");

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.enforcement = TrustEnforcement::AuditOnly;
        let verifier = SigilVerifier::new(kr, policy);

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(result.passed);
        // Policy check itself reports not-met
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "policy" && !c.passed));
    }

    // -----------------------------------------------------------------------
    // Policy enforcement: permissive allows with warning
    // -----------------------------------------------------------------------

    #[test]
    fn policy_permissive_allows_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "perm.bin", b"data");

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.enforcement = TrustEnforcement::Permissive;
        let verifier = SigilVerifier::new(kr, policy);

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Unverified);
    }

    // -----------------------------------------------------------------------
    // TrustedArtifact serialization
    // -----------------------------------------------------------------------

    #[test]
    fn trusted_artifact_serialization() {
        let artifact = TrustedArtifact {
            path: PathBuf::from("/opt/agent"),
            artifact_type: ArtifactType::AgentBinary,
            content_hash: "abc".to_string(),
            signature: Some(vec![1, 2, 3]),
            signer_key_id: Some("key1".to_string()),
            trust_level: TrustLevel::Verified,
            verified_at: Some(Utc::now()),
            metadata: {
                let mut m = HashMap::new();
                m.insert("version".to_string(), "1.0".to_string());
                m
            },
        };

        let json = serde_json::to_string(&artifact).unwrap();
        let recovered: TrustedArtifact = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered.content_hash, "abc");
        assert_eq!(recovered.trust_level, TrustLevel::Verified);
        assert_eq!(recovered.metadata.get("version").unwrap(), "1.0");
    }

    // -----------------------------------------------------------------------
    // VerificationResult checks detail
    // -----------------------------------------------------------------------

    #[test]
    fn verification_result_checks_detail() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "detail.bin", b"test");

        let kr = PublisherKeyring::new(dir.path());
        let verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let result = verifier
            .verify_artifact(&path, ArtifactType::Config)
            .unwrap();

        // Should have at least: file_readable, trust_store, signature, revocation, policy
        assert!(result.checks.len() >= 4);
        for check in &result.checks {
            assert!(!check.name.is_empty());
            assert!(!check.detail.is_empty());
        }
    }

    // -----------------------------------------------------------------------
    // Multiple artifacts in trust store
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_artifacts_in_trust_store() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        for i in 0..5 {
            verifier.register_trusted(TrustedArtifact {
                path: PathBuf::from(format!("/artifact_{}", i)),
                artifact_type: ArtifactType::AgentBinary,
                content_hash: format!("hash_{}", i),
                signature: None,
                signer_key_id: None,
                trust_level: TrustLevel::Verified,
                verified_at: Some(Utc::now()),
                metadata: HashMap::new(),
            });
        }

        let stats = verifier.stats();
        assert_eq!(stats.total_artifacts, 5);
        for i in 0..5 {
            assert_eq!(
                verifier.trust_level_for(&format!("hash_{}", i)),
                TrustLevel::Verified
            );
        }
    }

    // -----------------------------------------------------------------------
    // Revocation after trust
    // -----------------------------------------------------------------------

    #[test]
    fn revocation_after_trust() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "trusted_then_revoked.bin", b"trusted");

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let artifact = verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();

        // Initially trusted
        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(result.passed);

        // Revoke by hash
        verifier
            .add_revocation(RevocationEntry {
                key_id: None,
                content_hash: Some(artifact.content_hash.clone()),
                reason: "Supply chain compromise".to_string(),
                revoked_at: Utc::now(),
                revoked_by: "security-team".to_string(),
            })
            .unwrap();

        // Now should fail
        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        assert!(!result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Revoked);
    }

    // -----------------------------------------------------------------------
    // Sign artifact registers in trust store
    // -----------------------------------------------------------------------

    #[test]
    fn sign_artifact_registers_in_trust_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "to_sign.bin", b"sign me");
        let content_hash = hash_data(b"sign me");

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        assert_eq!(
            verifier.trust_level_for(&content_hash),
            TrustLevel::Unverified
        );

        verifier
            .sign_artifact(&path, &sk, ArtifactType::Config)
            .unwrap();

        assert_eq!(
            verifier.trust_level_for(&content_hash),
            TrustLevel::Verified
        );
    }

    // -----------------------------------------------------------------------
    // check_revocation helper
    // -----------------------------------------------------------------------

    #[test]
    fn check_revocation_direct() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        assert!(!verifier.check_revocation(Some("k1"), "h1"));

        verifier
            .add_revocation(RevocationEntry {
                key_id: Some("k1".to_string()),
                content_hash: None,
                reason: "test".to_string(),
                revoked_at: Utc::now(),
                revoked_by: "test".to_string(),
            })
            .unwrap();

        assert!(verifier.check_revocation(Some("k1"), "h1"));
        assert!(!verifier.check_revocation(Some("k2"), "h1"));
        assert!(!verifier.check_revocation(None, "h1"));
    }

    // -----------------------------------------------------------------------
    // Policy accessor
    // -----------------------------------------------------------------------

    #[test]
    fn policy_accessor() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.enforcement = TrustEnforcement::Permissive;
        let verifier = SigilVerifier::new(kr, policy);

        assert_eq!(verifier.policy().enforcement, TrustEnforcement::Permissive);
    }

    // -----------------------------------------------------------------------
    // Agent binary without execute permission
    // -----------------------------------------------------------------------

    #[cfg(unix)]
    #[test]
    fn verify_agent_binary_no_execute_permission() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "no_exec", b"agent code");
        // Do NOT make executable

        let (kr, sk, _vk, _kid) = keyring_with_key(dir.path());
        let mut policy = TrustPolicy::default();
        policy.allow_unsigned_agents = true;
        let mut verifier = SigilVerifier::new(kr, policy);

        verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();

        let result = verifier.verify_agent_binary(&path).unwrap();
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "execute_permission" && !c.passed));
    }

    // -----------------------------------------------------------------------
    // AUDIT FIX TESTS
    // -----------------------------------------------------------------------

    // CRITICAL 1: AuditOnly mode blocks revoked artifacts
    #[test]
    fn audit_only_blocks_revoked_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "revoked_audit.bin", b"audit revoked");

        let (kr, sk, _vk, kid) = keyring_with_key(dir.path());
        let mut policy = TrustPolicy::default();
        policy.enforcement = TrustEnforcement::AuditOnly;
        let mut verifier = SigilVerifier::new(kr, policy);

        verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();

        // Revoke the signing key
        verifier
            .add_revocation(RevocationEntry {
                key_id: Some(kid),
                content_hash: None,
                reason: "Key compromised".to_string(),
                revoked_at: Utc::now(),
                revoked_by: "admin".to_string(),
            })
            .unwrap();

        let result = verifier
            .verify_artifact(&path, ArtifactType::AgentBinary)
            .unwrap();
        // Even in AuditOnly, revoked must NOT pass
        assert!(!result.passed);
        assert_eq!(result.artifact.trust_level, TrustLevel::Revoked);
    }

    // CRITICAL 2: verify_on_execute=false skips verification
    #[test]
    fn verify_on_execute_false_skips_verification() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "skip_exec.bin", b"skip me");

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.verify_on_execute = false;
        let verifier = SigilVerifier::new(kr, policy);

        let result = verifier.verify_agent_binary(&path).unwrap();
        assert!(result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "skipped" && c.passed));
    }

    // CRITICAL 2: verify_on_install=false skips verification
    #[test]
    fn verify_on_install_false_skips_verification() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "skip_pkg.ark", b"skip pkg");

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.verify_on_install = false;
        let verifier = SigilVerifier::new(kr, policy);

        let result = verifier.verify_package(&path, Some("wrong_hash")).unwrap();
        // Should pass even with wrong expected hash because verification is skipped
        assert!(result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "skipped" && c.passed));
    }

    // CRITICAL 2: verify_on_boot=false skips verification
    #[test]
    fn verify_on_boot_false_skips_verification() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = temp_file(dir.path(), "vmlinuz", b"kernel");

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.verify_on_boot = false;
        let mut verifier = SigilVerifier::new(kr, policy);

        let report = verifier.verify_boot_chain(&[p1]).unwrap();
        assert!(report.is_clean());
    }

    // HIGH 1: sign_artifact with unknown key gets Community trust
    #[test]
    fn sign_artifact_unknown_key_gets_community_trust() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "community.bin", b"community data");

        // Create a keyring but do NOT add the signing key to it
        let kr = PublisherKeyring::new(dir.path());
        let sk = SigningKey::generate(&mut rand::rngs::OsRng);
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let artifact = verifier
            .sign_artifact(&path, &sk, ArtifactType::AgentBinary)
            .unwrap();
        assert_eq!(artifact.trust_level, TrustLevel::Community);
    }

    // HIGH 2: register_trusted downgrades SystemCore to Verified
    #[test]
    fn register_trusted_downgrades_system_core() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let hash = "sys_core_hash".to_string();
        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/boot/vmlinuz"),
            artifact_type: ArtifactType::BootComponent,
            content_hash: hash.clone(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::SystemCore,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });

        // Should have been downgraded to Verified
        assert_eq!(verifier.trust_level_for(&hash), TrustLevel::Verified);
    }

    // HIGH 2: register_system_core keeps SystemCore trust
    #[test]
    fn register_system_core_keeps_trust() {
        let dir = tempfile::tempdir().unwrap();
        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        let hash = "sys_core_hash2".to_string();
        verifier.register_system_core(TrustedArtifact {
            path: PathBuf::from("/boot/vmlinuz"),
            artifact_type: ArtifactType::BootComponent,
            content_hash: hash.clone(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Verified, // even if lower is passed
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });

        // Should be forced to SystemCore
        assert_eq!(verifier.trust_level_for(&hash), TrustLevel::SystemCore);
    }

    // HIGH 3: save/load trust store roundtrip
    #[test]
    fn save_load_trust_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("trust_store.json");

        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/opt/agent1"),
            artifact_type: ArtifactType::AgentBinary,
            content_hash: "hash_a".to_string(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Verified,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });
        verifier.register_trusted(TrustedArtifact {
            path: PathBuf::from("/opt/agent2"),
            artifact_type: ArtifactType::Config,
            content_hash: "hash_b".to_string(),
            signature: None,
            signer_key_id: None,
            trust_level: TrustLevel::Community,
            verified_at: Some(Utc::now()),
            metadata: HashMap::new(),
        });

        verifier.save_trust_store(&store_path).unwrap();

        // Load into a fresh verifier
        let kr2 = PublisherKeyring::new(dir.path());
        let mut verifier2 = SigilVerifier::new(kr2, TrustPolicy::default());
        let count = verifier2.load_trust_store(&store_path).unwrap();

        assert_eq!(count, 2);
        assert_eq!(verifier2.trust_level_for("hash_a"), TrustLevel::Verified);
        assert_eq!(verifier2.trust_level_for("hash_b"), TrustLevel::Community);
    }

    // HIGH 3: save/load revocations roundtrip
    #[test]
    fn save_load_revocations_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let rev_path = dir.path().join("revocations.json");

        let kr = PublisherKeyring::new(dir.path());
        let mut verifier = SigilVerifier::new(kr, TrustPolicy::default());

        verifier
            .add_revocation(RevocationEntry {
                key_id: Some("key_x".to_string()),
                content_hash: None,
                reason: "test revocation".to_string(),
                revoked_at: Utc::now(),
                revoked_by: "admin".to_string(),
            })
            .unwrap();

        verifier.save_revocations(&rev_path).unwrap();

        // Load into a fresh verifier
        let kr2 = PublisherKeyring::new(dir.path());
        let mut verifier2 = SigilVerifier::new(kr2, TrustPolicy::default());
        let count = verifier2.load_revocations(&rev_path).unwrap();

        assert_eq!(count, 1);
        assert!(verifier2.check_revocation(Some("key_x"), "any_hash"));
    }

    // HIGH 5: RevocationEntry with both None rejected
    #[test]
    fn revocation_entry_both_none_rejected() {
        let mut rl = RevocationList::new();
        let result = rl.add(RevocationEntry {
            key_id: None,
            content_hash: None,
            reason: "invalid entry".to_string(),
            revoked_at: Utc::now(),
            revoked_by: "test".to_string(),
        });
        assert!(result.is_err());
        assert!(rl.is_empty());
    }

    // MEDIUM: unsigned agent blocked in Permissive mode when allow_unsigned=false
    #[test]
    fn unsigned_agent_blocked_in_permissive_when_disallowed() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_file(dir.path(), "unsigned_perm.bin", b"unsigned agent");
        #[cfg(unix)]
        make_executable(&path);

        let kr = PublisherKeyring::new(dir.path());
        let mut policy = TrustPolicy::default();
        policy.enforcement = TrustEnforcement::Permissive;
        policy.allow_unsigned_agents = false;
        let verifier = SigilVerifier::new(kr, policy);

        let result = verifier.verify_agent_binary(&path).unwrap();
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "unsigned_agent" && !c.passed));
    }
}
