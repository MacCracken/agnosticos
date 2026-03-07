//! Post-Quantum Cryptography (PQC) — Hybrid Classical + PQC Key Exchange & Signatures
//!
//! Implements NIST-standardized post-quantum algorithms (ML-KEM / CRYSTALS-Kyber
//! and ML-DSA / CRYSTALS-Dilithium) alongside classical Ed25519/X25519 for
//! defense-in-depth during the quantum transition period.
//!
//! # Architecture
//!
//! All PQC operations are behind thin wrapper functions so that swapping the
//! current SHA-256-based simulation for real `ml-kem` / `ml-dsa` crate
//! implementations is a one-function-per-operation change.
//!
//! # Simulation Notice
//!
//! The actual post-quantum primitives are **simulated** using SHA-256 key
//! derivation. This is clearly marked throughout. The type signatures,
//! interfaces, key sizes, and hybrid combiners are production-correct so that
//! the swap to real PQC crates is mechanical.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Inline hex helpers (same pattern as marketplace/trust.rs)
// ---------------------------------------------------------------------------

mod hex {
    #[allow(dead_code)]
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[allow(dead_code)]
    pub fn decode(s: &str) -> Result<Vec<u8>, anyhow::Error> {
        if s.len() % 2 != 0 {
            return Err(anyhow::anyhow!("Hex string has odd length"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| anyhow::anyhow!("Invalid hex: {}", e))
            })
            .collect()
    }
}

// ===========================================================================
// PQC Algorithm Definitions
// ===========================================================================

/// NIST-standardized post-quantum algorithms supported by AGNOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PqcAlgorithm {
    /// ML-KEM-768 (CRYSTALS-Kyber): NIST security level 3 KEM.
    MlKem768,
    /// ML-KEM-1024 (CRYSTALS-Kyber): NIST security level 5 KEM.
    MlKem1024,
    /// ML-DSA-44 (CRYSTALS-Dilithium): NIST security level 2 signatures.
    MlDsa44,
    /// ML-DSA-65 (CRYSTALS-Dilithium): NIST security level 3 signatures.
    MlDsa65,
    /// ML-DSA-87 (CRYSTALS-Dilithium): NIST security level 5 signatures.
    MlDsa87,
}

impl PqcAlgorithm {
    /// NIST security level (1–5).
    pub fn security_level(self) -> u8 {
        match self {
            Self::MlKem768 => 3,
            Self::MlKem1024 => 5,
            Self::MlDsa44 => 2,
            Self::MlDsa65 => 3,
            Self::MlDsa87 => 5,
        }
    }

    /// Public key size in bytes (per FIPS 203 / FIPS 204).
    pub fn public_key_size(self) -> usize {
        match self {
            Self::MlKem768 => 1184,
            Self::MlKem1024 => 1568,
            Self::MlDsa44 => 1312,
            Self::MlDsa65 => 1952,
            Self::MlDsa87 => 2592,
        }
    }

    /// Secret (private) key size in bytes.
    pub fn secret_key_size(self) -> usize {
        match self {
            Self::MlKem768 => 2400,
            Self::MlKem1024 => 3168,
            Self::MlDsa44 => 2560,
            Self::MlDsa65 => 4032,
            Self::MlDsa87 => 4896,
        }
    }

    /// Ciphertext size in bytes (KEM only).
    pub fn ciphertext_size(self) -> Option<usize> {
        match self {
            Self::MlKem768 => Some(1088),
            Self::MlKem1024 => Some(1568),
            _ => None,
        }
    }

    /// Signature size in bytes (DSA only).
    pub fn signature_size(self) -> Option<usize> {
        match self {
            Self::MlDsa44 => Some(2420),
            Self::MlDsa65 => Some(3309),
            Self::MlDsa87 => Some(4627),
            _ => None,
        }
    }

    /// Whether this algorithm is a KEM (key encapsulation mechanism).
    pub fn is_kem(self) -> bool {
        matches!(self, Self::MlKem768 | Self::MlKem1024)
    }

    /// Whether this algorithm is a digital signature scheme.
    pub fn is_signature(self) -> bool {
        matches!(self, Self::MlDsa44 | Self::MlDsa65 | Self::MlDsa87)
    }
}

impl fmt::Display for PqcAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MlKem768 => write!(f, "ML-KEM-768"),
            Self::MlKem1024 => write!(f, "ML-KEM-1024"),
            Self::MlDsa44 => write!(f, "ML-DSA-44"),
            Self::MlDsa65 => write!(f, "ML-DSA-65"),
            Self::MlDsa87 => write!(f, "ML-DSA-87"),
        }
    }
}

// ===========================================================================
// Simulated PQC Primitives
// ===========================================================================
//
// Each function in this section is a drop-in replacement target. When real
// ml-kem / ml-dsa crates are added, only these functions change.

/// SIMULATED: Generate an ML-KEM keypair.
/// Replace with `ml_kem::MlKem768::generate()` (or MlKem1024) when available.
fn sim_kem_keygen(algorithm: PqcAlgorithm) -> (Vec<u8>, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let mut seed = [0u8; 64];
    rng.fill_bytes(&mut seed);

    let pk_size = algorithm.public_key_size();
    let sk_size = algorithm.secret_key_size();

    // Derive deterministic-length key material from the seed via iterated SHA-256.
    let public_key = expand_hash(&seed, b"pqc-kem-pk", pk_size);
    // Secret key embeds the seed so decapsulation can recover the shared secret.
    let mut secret_key = Vec::with_capacity(sk_size);
    secret_key.extend_from_slice(&seed);
    secret_key.extend_from_slice(&expand_hash(&seed, b"pqc-kem-sk", sk_size - seed.len()));
    secret_key.truncate(sk_size);

    (public_key, secret_key)
}

/// SIMULATED: ML-KEM encapsulate — produce (ciphertext, shared_secret) given a public key.
/// Replace with `ml_kem::MlKem768::encapsulate(pk)` when available.
fn sim_kem_encapsulate(algorithm: PqcAlgorithm, public_key: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let mut ephemeral = [0u8; 32];
    rng.fill_bytes(&mut ephemeral);

    let ct_size = algorithm.ciphertext_size().unwrap_or(1088);

    // Ciphertext = hash(ephemeral || pk), expanded to ct_size.
    let mut ct_input = Vec::new();
    ct_input.extend_from_slice(&ephemeral);
    ct_input.extend_from_slice(public_key);
    let ciphertext = expand_hash(&ct_input, b"pqc-kem-ct", ct_size);

    // Shared secret = SHA-256(ephemeral || pk), 32 bytes.
    let shared_secret = sha256_concat(&[&ephemeral, public_key]);

    // Embed ephemeral in ciphertext so decapsulate can recover (simulation only).
    let mut ct_with_ephemeral = Vec::with_capacity(32 + ciphertext.len());
    ct_with_ephemeral.extend_from_slice(&ephemeral);
    ct_with_ephemeral.extend_from_slice(&ciphertext[32..]);

    (ct_with_ephemeral, shared_secret)
}

/// SIMULATED: ML-KEM decapsulate — recover shared_secret from ciphertext + secret key.
/// Replace with `ml_kem::MlKem768::decapsulate(sk, ct)` when available.
fn sim_kem_decapsulate(secret_key: &[u8], ciphertext: &[u8], public_key: &[u8]) -> Vec<u8> {
    // In simulation the ephemeral is the first 32 bytes of the ciphertext,
    // and we re-derive the shared secret the same way encapsulate did.
    let ephemeral = &ciphertext[..32.min(ciphertext.len())];
    let _ = secret_key; // Real impl would use sk; we use pk for the hash.
    sha256_concat(&[ephemeral, public_key])
}

/// SIMULATED: Generate an ML-DSA keypair.
/// Replace with `ml_dsa::MlDsa65::generate()` (or MlDsa44/MlDsa87) when available.
fn sim_dsa_keygen(algorithm: PqcAlgorithm) -> (Vec<u8>, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let mut seed = [0u8; 64];
    rng.fill_bytes(&mut seed);

    let pk_size = algorithm.public_key_size();
    let sk_size = algorithm.secret_key_size();

    let public_key = expand_hash(&seed, b"pqc-dsa-pk", pk_size);
    let mut secret_key = Vec::with_capacity(sk_size);
    secret_key.extend_from_slice(&seed);
    secret_key.extend_from_slice(&expand_hash(&seed, b"pqc-dsa-sk", sk_size - seed.len()));
    secret_key.truncate(sk_size);

    (public_key, secret_key)
}

/// SIMULATED: ML-DSA sign — produce a signature over `message` with `secret_key`.
/// Replace with `ml_dsa::MlDsa65::sign(sk, msg)` when available.
fn sim_dsa_sign(algorithm: PqcAlgorithm, secret_key: &[u8], message: &[u8]) -> Vec<u8> {
    let sig_size = algorithm.signature_size().unwrap_or(3309);

    // Signature = HMAC-like construction: H(sk || msg), expanded to sig_size.
    let mut input = Vec::new();
    input.extend_from_slice(secret_key);
    input.extend_from_slice(message);
    expand_hash(&input, b"pqc-dsa-sig", sig_size)
}

/// SIMULATED: ML-DSA verify — check signature over `message` with `public_key`.
/// Replace with `ml_dsa::MlDsa65::verify(pk, msg, sig)` when available.
fn sim_dsa_verify(
    algorithm: PqcAlgorithm,
    _public_key: &[u8],
    secret_key: &[u8],
    message: &[u8],
    signature: &[u8],
) -> bool {
    // Re-derive what the signature should be and compare.
    // In the simulation we need the secret key to verify (not realistic, but
    // the public API hides this by keeping the keypair together).
    let expected = sim_dsa_sign(algorithm, secret_key, message);
    constant_time_eq(&expected, signature)
}

// ---------------------------------------------------------------------------
// Hash helpers
// ---------------------------------------------------------------------------

/// SHA-256 of the concatenation of multiple slices.
fn sha256_concat(parts: &[&[u8]]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().to_vec()
}

/// Expand a seed into `len` bytes via iterated SHA-256 (counter mode).
fn expand_hash(seed: &[u8], domain: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut counter: u32 = 0;
    while out.len() < len {
        let mut hasher = Sha256::new();
        hasher.update(domain);
        hasher.update(seed);
        hasher.update(counter.to_le_bytes());
        out.extend_from_slice(&hasher.finalize());
        counter += 1;
    }
    out.truncate(len);
    out
}

/// Constant-time equality comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

// ===========================================================================
// Hybrid KEM
// ===========================================================================

/// Hybrid KEM keypair combining X25519 (classical) and ML-KEM (post-quantum).
#[derive(Clone, Serialize, Deserialize)]
pub struct HybridKemKeypair {
    pub algorithm: PqcAlgorithm,
    /// Classical X25519 public key (32 bytes).
    pub classical_public: Vec<u8>,
    /// Classical X25519 secret key (32 bytes).
    #[serde(default, skip_serializing)]
    pub classical_secret: Vec<u8>,
    /// ML-KEM public key.
    pub pqc_public: Vec<u8>,
    /// ML-KEM secret key.
    #[serde(default, skip_serializing)]
    pub pqc_secret: Vec<u8>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for HybridKemKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HybridKemKeypair")
            .field("algorithm", &self.algorithm)
            .field("classical_public_len", &self.classical_public.len())
            .field("pqc_public_len", &self.pqc_public.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Result of a hybrid encapsulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridEncapsulation {
    /// Classical X25519 ciphertext (ephemeral public key, 32 bytes).
    pub classical_ciphertext: Vec<u8>,
    /// ML-KEM ciphertext.
    pub pqc_ciphertext: Vec<u8>,
    /// Combined shared secret: SHA-256(classical_ss || pqc_ss).
    pub combined_shared_secret: Vec<u8>,
}

/// Generate a hybrid KEM keypair.
pub fn generate_kem_keypair(algorithm: PqcAlgorithm) -> Result<HybridKemKeypair> {
    if !algorithm.is_kem() {
        bail!(
            "Algorithm {} is not a KEM; use a signing algorithm for signatures",
            algorithm
        );
    }

    debug!("Generating hybrid KEM keypair with {}", algorithm);

    // Classical: X25519 keypair (simulated via random bytes; real impl would
    // use x25519-dalek).
    // SIMULATED: Replace with x25519_dalek::StaticSecret::random_from_rng()
    let mut classical_secret = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut classical_secret);
    let classical_public = sha256_concat(&[&classical_secret, b"x25519-pub"])[..32].to_vec();

    // Post-quantum: ML-KEM keypair.
    let (pqc_public, pqc_secret) = sim_kem_keygen(algorithm);

    info!(
        "Generated hybrid KEM keypair: {} (classical 32B + PQC {}B public key)",
        algorithm,
        pqc_public.len()
    );

    Ok(HybridKemKeypair {
        algorithm,
        classical_public,
        classical_secret,
        pqc_public,
        pqc_secret,
        created_at: Utc::now(),
    })
}

/// Encapsulate a shared secret to a recipient's hybrid KEM public key.
///
/// Both the classical and PQC shared secrets are derived independently and
/// then combined via HKDF-like construction: `SHA-256(classical_ss || pqc_ss)`.
pub fn encapsulate(recipient: &HybridKemKeypair) -> Result<HybridEncapsulation> {
    debug!("Encapsulating with hybrid KEM ({})", recipient.algorithm);

    // Classical: X25519 encapsulate.
    // SIMULATED: Replace with x25519_dalek DH exchange.
    let mut ephemeral_secret = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut ephemeral_secret);
    let classical_ciphertext =
        sha256_concat(&[&ephemeral_secret, b"x25519-eph-pub"])[..32].to_vec();
    // Derive classical shared secret from the ephemeral public (ciphertext) and
    // recipient's public key so that decapsulate can reconstruct the same value.
    let classical_ss = sha256_concat(&[
        &classical_ciphertext,
        &recipient.classical_public,
        b"x25519-ss",
    ]);

    // Post-quantum: ML-KEM encapsulate.
    let (pqc_ciphertext, pqc_ss) = sim_kem_encapsulate(recipient.algorithm, &recipient.pqc_public);

    // Combine: HKDF-like — SHA-256(classical_ss || pqc_ss).
    let combined = sha256_concat(&[&classical_ss, &pqc_ss]);

    info!(
        "Hybrid encapsulation complete: {}B classical ct + {}B PQC ct",
        classical_ciphertext.len(),
        pqc_ciphertext.len()
    );

    Ok(HybridEncapsulation {
        classical_ciphertext,
        pqc_ciphertext,
        combined_shared_secret: combined,
    })
}

/// Decapsulate a shared secret using a hybrid KEM keypair.
pub fn decapsulate(
    keypair: &HybridKemKeypair,
    encapsulation: &HybridEncapsulation,
) -> Result<Vec<u8>> {
    debug!("Decapsulating with hybrid KEM ({})", keypair.algorithm);

    // Classical: X25519 decapsulate.
    // SIMULATED: Replace with x25519_dalek DH exchange.
    // Derive classical shared secret from the ephemeral public (ciphertext) and
    // our own public key — mirrors the derivation in encapsulate().
    let classical_ss = sha256_concat(&[
        &encapsulation.classical_ciphertext,
        &keypair.classical_public,
        b"x25519-ss",
    ]);

    // Post-quantum: ML-KEM decapsulate.
    let pqc_ss = sim_kem_decapsulate(
        &keypair.pqc_secret,
        &encapsulation.pqc_ciphertext,
        &keypair.pqc_public,
    );

    // Combine the same way encapsulate did.
    let combined = sha256_concat(&[&classical_ss, &pqc_ss]);

    info!("Hybrid decapsulation complete");

    Ok(combined)
}

// ===========================================================================
// Hybrid Signatures
// ===========================================================================

/// Hybrid signing keypair combining Ed25519 (classical) and ML-DSA (post-quantum).
#[derive(Clone, Serialize, Deserialize)]
pub struct HybridSigningKeypair {
    pub algorithm: PqcAlgorithm,
    /// Ed25519 signing key bytes (32 bytes).
    #[serde(default, skip_serializing)]
    pub classical_signing_key: Vec<u8>,
    /// Ed25519 verifying key bytes (32 bytes).
    pub classical_verifying_key: Vec<u8>,
    /// ML-DSA public key.
    pub pqc_public: Vec<u8>,
    /// ML-DSA secret key.
    #[serde(default, skip_serializing)]
    pub pqc_secret: Vec<u8>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for HybridSigningKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HybridSigningKeypair")
            .field("algorithm", &self.algorithm)
            .field(
                "classical_verifying_key_len",
                &self.classical_verifying_key.len(),
            )
            .field("pqc_public_len", &self.pqc_public.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// A hybrid signature containing both classical and post-quantum components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSignature {
    /// Ed25519 signature (64 bytes).
    pub classical_sig: Vec<u8>,
    /// ML-DSA signature.
    pub pqc_sig: Vec<u8>,
    /// Algorithm used for the PQC component.
    pub algorithm: PqcAlgorithm,
}

/// Generate a hybrid signing keypair.
pub fn generate_signing_keypair(algorithm: PqcAlgorithm) -> Result<HybridSigningKeypair> {
    if !algorithm.is_signature() {
        bail!(
            "Algorithm {} is not a signature scheme; use a KEM algorithm for key exchange",
            algorithm
        );
    }

    debug!("Generating hybrid signing keypair with {}", algorithm);

    // Classical: Ed25519 keypair.
    let ed_signing_key = SigningKey::generate(&mut rand::thread_rng());
    let ed_verifying_key = ed_signing_key.verifying_key();

    // Post-quantum: ML-DSA keypair.
    let (pqc_public, pqc_secret) = sim_dsa_keygen(algorithm);

    info!(
        "Generated hybrid signing keypair: {} (Ed25519 + PQC {}B public key)",
        algorithm,
        pqc_public.len()
    );

    Ok(HybridSigningKeypair {
        algorithm,
        classical_signing_key: ed_signing_key.to_bytes().to_vec(),
        classical_verifying_key: ed_verifying_key.to_bytes().to_vec(),
        pqc_public,
        pqc_secret,
        created_at: Utc::now(),
    })
}

/// Sign a message with a hybrid signing keypair.
///
/// Both Ed25519 and ML-DSA signatures are produced over the same message.
pub fn hybrid_sign(keypair: &HybridSigningKeypair, message: &[u8]) -> Result<HybridSignature> {
    debug!("Hybrid signing with {}", keypair.algorithm);

    // Classical: Ed25519 sign.
    let sk_bytes: [u8; 32] = keypair
        .classical_signing_key
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 signing key must be 32 bytes"))?;
    let ed_signing_key = SigningKey::from_bytes(&sk_bytes);
    let classical_sig = ed_signing_key.sign(message).to_bytes().to_vec();

    // Post-quantum: ML-DSA sign.
    // SIMULATED: Replace with ml_dsa::MlDsa65::sign(&keypair.pqc_secret, message)
    let pqc_sig = sim_dsa_sign(keypair.algorithm, &keypair.pqc_secret, message);

    info!(
        "Hybrid signature produced: {}B Ed25519 + {}B {}",
        classical_sig.len(),
        pqc_sig.len(),
        keypair.algorithm
    );

    Ok(HybridSignature {
        classical_sig,
        pqc_sig,
        algorithm: keypair.algorithm,
    })
}

/// Verify a hybrid signature. Both the classical and PQC signatures must be
/// valid (AND logic — not OR).
pub fn hybrid_verify(
    public_key: &HybridSigningKeypair,
    message: &[u8],
    signature: &HybridSignature,
) -> Result<bool> {
    debug!("Hybrid verification with {}", signature.algorithm);

    // Classical: Ed25519 verify.
    let vk_bytes: [u8; 32] = public_key
        .classical_verifying_key
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 verifying key must be 32 bytes"))?;
    let ed_verifying_key = VerifyingKey::from_bytes(&vk_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid Ed25519 verifying key: {}", e))?;

    let sig_bytes: [u8; 64] = signature
        .classical_sig
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 signature must be 64 bytes"))?;
    let ed_signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    let classical_valid = ed_verifying_key.verify(message, &ed_signature).is_ok();

    // Post-quantum: ML-DSA verify.
    // SIMULATED: Replace with ml_dsa::MlDsa65::verify(&public_key.pqc_public, message, &sig.pqc_sig)
    let pqc_valid = sim_dsa_verify(
        signature.algorithm,
        &public_key.pqc_public,
        &public_key.pqc_secret,
        message,
        &signature.pqc_sig,
    );

    if !classical_valid {
        warn!("Hybrid verification failed: Ed25519 signature invalid");
    }
    if !pqc_valid {
        warn!(
            "Hybrid verification failed: {} signature invalid",
            signature.algorithm
        );
    }

    Ok(classical_valid && pqc_valid)
}

// ===========================================================================
// PQC Key Store
// ===========================================================================

/// Type of key stored in the key store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    Kem,
    Signing,
}

impl fmt::Display for KeyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kem => write!(f, "KEM"),
            Self::Signing => write!(f, "Signing"),
        }
    }
}

/// Summary info for a stored key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    pub key_id: String,
    pub algorithm: PqcAlgorithm,
    pub created_at: DateTime<Utc>,
    pub key_type: KeyType,
}

/// Persistent store for PQC key material.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PqcKeyStore {
    kem_keys: HashMap<String, HybridKemKeypair>,
    signing_keys: HashMap<String, HybridSigningKeypair>,
}

impl PqcKeyStore {
    /// Create a new empty key store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a KEM keypair. Returns an error if the key_id already exists.
    pub fn add_kem_keypair(
        &mut self,
        key_id: impl Into<String>,
        keypair: HybridKemKeypair,
    ) -> Result<()> {
        let key_id = key_id.into();
        if self.kem_keys.contains_key(&key_id) || self.signing_keys.contains_key(&key_id) {
            bail!("Key ID '{}' already exists in the store", key_id);
        }
        debug!("Adding KEM keypair: {}", key_id);
        self.kem_keys.insert(key_id, keypair);
        Ok(())
    }

    /// Add a signing keypair. Returns an error if the key_id already exists.
    pub fn add_signing_keypair(
        &mut self,
        key_id: impl Into<String>,
        keypair: HybridSigningKeypair,
    ) -> Result<()> {
        let key_id = key_id.into();
        if self.kem_keys.contains_key(&key_id) || self.signing_keys.contains_key(&key_id) {
            bail!("Key ID '{}' already exists in the store", key_id);
        }
        debug!("Adding signing keypair: {}", key_id);
        self.signing_keys.insert(key_id, keypair);
        Ok(())
    }

    /// Retrieve a KEM keypair by ID.
    pub fn get_kem_keypair(&self, key_id: &str) -> Option<&HybridKemKeypair> {
        self.kem_keys.get(key_id)
    }

    /// Retrieve a signing keypair by ID.
    pub fn get_signing_keypair(&self, key_id: &str) -> Option<&HybridSigningKeypair> {
        self.signing_keys.get(key_id)
    }

    /// List all keys in the store.
    pub fn list_keys(&self) -> Vec<KeyInfo> {
        let mut keys: Vec<KeyInfo> = Vec::new();
        for (id, kp) in &self.kem_keys {
            keys.push(KeyInfo {
                key_id: id.clone(),
                algorithm: kp.algorithm,
                created_at: kp.created_at,
                key_type: KeyType::Kem,
            });
        }
        for (id, kp) in &self.signing_keys {
            keys.push(KeyInfo {
                key_id: id.clone(),
                algorithm: kp.algorithm,
                created_at: kp.created_at,
                key_type: KeyType::Signing,
            });
        }
        keys.sort_by(|a, b| a.key_id.cmp(&b.key_id));
        keys
    }

    /// Remove a key by ID (either KEM or signing).
    pub fn remove_key(&mut self, key_id: &str) -> Result<()> {
        if self.kem_keys.remove(key_id).is_some() {
            info!("Removed KEM key: {}", key_id);
            return Ok(());
        }
        if self.signing_keys.remove(key_id).is_some() {
            info!("Removed signing key: {}", key_id);
            return Ok(());
        }
        bail!("Key ID '{}' not found in the store", key_id)
    }

    /// Number of keys in the store.
    pub fn len(&self) -> usize {
        self.kem_keys.len() + self.signing_keys.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Save the key store to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize key store: {}", e))?;
        std::fs::write(path, json).map_err(|e| {
            anyhow::anyhow!("Failed to write key store to {}: {}", path.display(), e)
        })?;
        info!(
            "Key store saved to {} ({} keys)",
            path.display(),
            self.len()
        );
        Ok(())
    }

    /// Load a key store from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path).map_err(|e| {
            anyhow::anyhow!("Failed to read key store from {}: {}", path.display(), e)
        })?;
        let store: Self = serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize key store: {}", e))?;
        info!(
            "Key store loaded from {} ({} keys)",
            path.display(),
            store.len()
        );
        Ok(store)
    }
}

// ===========================================================================
// PQC Configuration
// ===========================================================================

/// Operating mode for PQC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PqcMode {
    /// Classical cryptography only — no PQC operations.
    Disabled,
    /// Hybrid mode: both classical and PQC (default, recommended for transition).
    Hybrid,
    /// PQC only — classical algorithms disabled (future, post-transition).
    PqcOnly,
}

impl Default for PqcMode {
    fn default() -> Self {
        Self::Hybrid
    }
}

impl fmt::Display for PqcMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "Disabled"),
            Self::Hybrid => write!(f, "Hybrid"),
            Self::PqcOnly => write!(f, "PQC-Only"),
        }
    }
}

/// PQC migration and transition configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PqcConfig {
    /// Operating mode.
    pub mode: PqcMode,
    /// Default KEM algorithm for new keypairs.
    pub kem_algorithm: PqcAlgorithm,
    /// Default signing algorithm for new keypairs.
    pub signing_algorithm: PqcAlgorithm,
    /// Require PQC keys for agent-to-agent communication.
    pub require_pqc_for_agents: bool,
    /// Require PQC keys for inter-node federation.
    pub require_pqc_for_federation: bool,
}

impl Default for PqcConfig {
    fn default() -> Self {
        Self {
            mode: PqcMode::Hybrid,
            kem_algorithm: PqcAlgorithm::MlKem768,
            signing_algorithm: PqcAlgorithm::MlDsa65,
            require_pqc_for_agents: false,
            require_pqc_for_federation: false,
        }
    }
}

impl PqcConfig {
    /// Create a new config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check whether PQC operations are active (Hybrid or PqcOnly).
    pub fn is_pqc_active(&self) -> bool {
        self.mode != PqcMode::Disabled
    }

    /// Check whether classical operations are active (Disabled or Hybrid).
    pub fn is_classical_active(&self) -> bool {
        self.mode != PqcMode::PqcOnly
    }
}

// ===========================================================================
// Migration Status
// ===========================================================================

/// Tracks the progress of PQC migration across the system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PqcMigrationStatus {
    /// Number of agents that have PQC keypairs provisioned.
    pub agents_with_pqc_keys: usize,
    /// Number of agents still using classical-only keys.
    pub agents_without_pqc_keys: usize,
    /// Number of federation nodes that support PQC.
    pub federation_nodes_pqc_ready: usize,
    /// Total number of federation nodes.
    pub federation_nodes_total: usize,
    /// Timestamp of the last status check.
    pub last_checked: Option<DateTime<Utc>>,
}

impl PqcMigrationStatus {
    /// Create a new empty migration status.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether all agents and federation nodes have migrated to PQC.
    pub fn migration_complete(&self) -> bool {
        self.agents_without_pqc_keys == 0
            && self.federation_nodes_pqc_ready >= self.federation_nodes_total
            && self.agents_with_pqc_keys > 0
    }

    /// Percentage of agents that have PQC keys (0–100).
    pub fn agent_migration_percent(&self) -> f64 {
        let total = self.agents_with_pqc_keys + self.agents_without_pqc_keys;
        if total == 0 {
            return 0.0;
        }
        (self.agents_with_pqc_keys as f64 / total as f64) * 100.0
    }

    /// Percentage of federation nodes that are PQC-ready (0–100).
    pub fn federation_migration_percent(&self) -> f64 {
        if self.federation_nodes_total == 0 {
            return 0.0;
        }
        (self.federation_nodes_pqc_ready as f64 / self.federation_nodes_total as f64) * 100.0
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    // -----------------------------------------------------------------------
    // Algorithm properties
    // -----------------------------------------------------------------------

    #[test]
    fn test_algorithm_display() {
        assert_eq!(PqcAlgorithm::MlKem768.to_string(), "ML-KEM-768");
        assert_eq!(PqcAlgorithm::MlKem1024.to_string(), "ML-KEM-1024");
        assert_eq!(PqcAlgorithm::MlDsa44.to_string(), "ML-DSA-44");
        assert_eq!(PqcAlgorithm::MlDsa65.to_string(), "ML-DSA-65");
        assert_eq!(PqcAlgorithm::MlDsa87.to_string(), "ML-DSA-87");
    }

    #[test]
    fn test_algorithm_security_levels() {
        assert_eq!(PqcAlgorithm::MlKem768.security_level(), 3);
        assert_eq!(PqcAlgorithm::MlKem1024.security_level(), 5);
        assert_eq!(PqcAlgorithm::MlDsa44.security_level(), 2);
        assert_eq!(PqcAlgorithm::MlDsa65.security_level(), 3);
        assert_eq!(PqcAlgorithm::MlDsa87.security_level(), 5);
    }

    #[test]
    fn test_algorithm_key_sizes() {
        // ML-KEM public key sizes per FIPS 203.
        assert_eq!(PqcAlgorithm::MlKem768.public_key_size(), 1184);
        assert_eq!(PqcAlgorithm::MlKem1024.public_key_size(), 1568);
        // ML-KEM secret key sizes.
        assert_eq!(PqcAlgorithm::MlKem768.secret_key_size(), 2400);
        assert_eq!(PqcAlgorithm::MlKem1024.secret_key_size(), 3168);
    }

    #[test]
    fn test_algorithm_dsa_key_sizes() {
        assert_eq!(PqcAlgorithm::MlDsa44.public_key_size(), 1312);
        assert_eq!(PqcAlgorithm::MlDsa65.public_key_size(), 1952);
        assert_eq!(PqcAlgorithm::MlDsa87.public_key_size(), 2592);
        assert_eq!(PqcAlgorithm::MlDsa44.secret_key_size(), 2560);
        assert_eq!(PqcAlgorithm::MlDsa65.secret_key_size(), 4032);
        assert_eq!(PqcAlgorithm::MlDsa87.secret_key_size(), 4896);
    }

    #[test]
    fn test_algorithm_ciphertext_sizes() {
        assert_eq!(PqcAlgorithm::MlKem768.ciphertext_size(), Some(1088));
        assert_eq!(PqcAlgorithm::MlKem1024.ciphertext_size(), Some(1568));
        assert_eq!(PqcAlgorithm::MlDsa44.ciphertext_size(), None);
        assert_eq!(PqcAlgorithm::MlDsa65.ciphertext_size(), None);
    }

    #[test]
    fn test_algorithm_signature_sizes() {
        assert_eq!(PqcAlgorithm::MlKem768.signature_size(), None);
        assert_eq!(PqcAlgorithm::MlDsa44.signature_size(), Some(2420));
        assert_eq!(PqcAlgorithm::MlDsa65.signature_size(), Some(3309));
        assert_eq!(PqcAlgorithm::MlDsa87.signature_size(), Some(4627));
    }

    #[test]
    fn test_algorithm_is_kem() {
        assert!(PqcAlgorithm::MlKem768.is_kem());
        assert!(PqcAlgorithm::MlKem1024.is_kem());
        assert!(!PqcAlgorithm::MlDsa44.is_kem());
        assert!(!PqcAlgorithm::MlDsa65.is_kem());
        assert!(!PqcAlgorithm::MlDsa87.is_kem());
    }

    #[test]
    fn test_algorithm_is_signature() {
        assert!(!PqcAlgorithm::MlKem768.is_signature());
        assert!(!PqcAlgorithm::MlKem1024.is_signature());
        assert!(PqcAlgorithm::MlDsa44.is_signature());
        assert!(PqcAlgorithm::MlDsa65.is_signature());
        assert!(PqcAlgorithm::MlDsa87.is_signature());
    }

    #[test]
    fn test_algorithm_serialization() {
        let alg = PqcAlgorithm::MlKem768;
        let json = serde_json::to_string(&alg).unwrap();
        let deserialized: PqcAlgorithm = serde_json::from_str(&json).unwrap();
        assert_eq!(alg, deserialized);
    }

    // -----------------------------------------------------------------------
    // KEM keypair generation
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_kem_keypair_768() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        assert_eq!(kp.algorithm, PqcAlgorithm::MlKem768);
        assert_eq!(kp.classical_public.len(), 32);
        assert_eq!(kp.classical_secret.len(), 32);
        assert_eq!(kp.pqc_public.len(), 1184);
        assert_eq!(kp.pqc_secret.len(), 2400);
    }

    #[test]
    fn test_generate_kem_keypair_1024() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem1024).unwrap();
        assert_eq!(kp.algorithm, PqcAlgorithm::MlKem1024);
        assert_eq!(kp.pqc_public.len(), 1568);
        assert_eq!(kp.pqc_secret.len(), 3168);
    }

    #[test]
    fn test_generate_kem_keypair_wrong_algorithm() {
        let result = generate_kem_keypair(PqcAlgorithm::MlDsa65);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a KEM"));
    }

    #[test]
    fn test_kem_keypair_uniqueness() {
        let kp1 = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let kp2 = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        assert_ne!(kp1.classical_public, kp2.classical_public);
        assert_ne!(kp1.pqc_public, kp2.pqc_public);
    }

    // -----------------------------------------------------------------------
    // KEM encapsulation / decapsulation
    // -----------------------------------------------------------------------

    #[test]
    fn test_kem_encapsulate_produces_output() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let enc = encapsulate(&kp).unwrap();
        assert_eq!(enc.classical_ciphertext.len(), 32);
        assert!(!enc.pqc_ciphertext.is_empty());
        assert_eq!(enc.combined_shared_secret.len(), 32);
    }

    #[test]
    fn test_kem_decapsulate_produces_output() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let enc = encapsulate(&kp).unwrap();
        let ss = decapsulate(&kp, &enc).unwrap();
        assert_eq!(ss.len(), 32);
    }

    #[test]
    fn test_kem_encapsulate_1024() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem1024).unwrap();
        let enc = encapsulate(&kp).unwrap();
        assert_eq!(enc.combined_shared_secret.len(), 32);
    }

    #[test]
    fn test_kem_encapsulate_decapsulate_roundtrip() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let enc = encapsulate(&kp).unwrap();
        let ss = decapsulate(&kp, &enc).unwrap();
        assert_eq!(
            enc.combined_shared_secret, ss,
            "Encapsulated and decapsulated shared secrets must match"
        );
    }

    #[test]
    fn test_kem_encapsulate_decapsulate_roundtrip_1024() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem1024).unwrap();
        let enc = encapsulate(&kp).unwrap();
        let ss = decapsulate(&kp, &enc).unwrap();
        assert_eq!(
            enc.combined_shared_secret, ss,
            "Encapsulated and decapsulated shared secrets must match (ML-KEM-1024)"
        );
    }

    #[test]
    fn test_kem_shared_secret_not_zero() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let enc = encapsulate(&kp).unwrap();
        assert_ne!(enc.combined_shared_secret, vec![0u8; 32]);
    }

    // -----------------------------------------------------------------------
    // Signing keypair generation
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_signing_keypair_dsa44() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa44).unwrap();
        assert_eq!(kp.algorithm, PqcAlgorithm::MlDsa44);
        assert_eq!(kp.classical_signing_key.len(), 32);
        assert_eq!(kp.classical_verifying_key.len(), 32);
        assert_eq!(kp.pqc_public.len(), 1312);
        assert_eq!(kp.pqc_secret.len(), 2560);
    }

    #[test]
    fn test_generate_signing_keypair_dsa65() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        assert_eq!(kp.algorithm, PqcAlgorithm::MlDsa65);
        assert_eq!(kp.pqc_public.len(), 1952);
        assert_eq!(kp.pqc_secret.len(), 4032);
    }

    #[test]
    fn test_generate_signing_keypair_dsa87() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa87).unwrap();
        assert_eq!(kp.algorithm, PqcAlgorithm::MlDsa87);
        assert_eq!(kp.pqc_public.len(), 2592);
        assert_eq!(kp.pqc_secret.len(), 4896);
    }

    #[test]
    fn test_generate_signing_keypair_wrong_algorithm() {
        let result = generate_signing_keypair(PqcAlgorithm::MlKem768);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a signature"));
    }

    #[test]
    fn test_signing_keypair_uniqueness() {
        let kp1 = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let kp2 = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        assert_ne!(kp1.classical_signing_key, kp2.classical_signing_key);
        assert_ne!(kp1.pqc_public, kp2.pqc_public);
    }

    // -----------------------------------------------------------------------
    // Hybrid sign / verify
    // -----------------------------------------------------------------------

    #[test]
    fn test_sign_verify_roundtrip() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = b"hello AGNOS post-quantum world";
        let sig = hybrid_sign(&kp, msg).unwrap();
        let valid = hybrid_verify(&kp, msg, &sig).unwrap();
        assert!(valid);
    }

    #[test]
    fn test_sign_verify_roundtrip_dsa44() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa44).unwrap();
        let msg = b"ML-DSA-44 test";
        let sig = hybrid_sign(&kp, msg).unwrap();
        assert!(hybrid_verify(&kp, msg, &sig).unwrap());
    }

    #[test]
    fn test_sign_verify_roundtrip_dsa87() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa87).unwrap();
        let msg = b"ML-DSA-87 test";
        let sig = hybrid_sign(&kp, msg).unwrap();
        assert!(hybrid_verify(&kp, msg, &sig).unwrap());
    }

    #[test]
    fn test_sign_verify_empty_message() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = b"";
        let sig = hybrid_sign(&kp, msg).unwrap();
        assert!(hybrid_verify(&kp, msg, &sig).unwrap());
    }

    #[test]
    fn test_verify_fails_tampered_message() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = b"original message";
        let sig = hybrid_sign(&kp, msg).unwrap();
        let valid = hybrid_verify(&kp, b"tampered message", &sig).unwrap();
        assert!(!valid);
    }

    #[test]
    fn test_verify_fails_tampered_classical_sig() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = b"test message";
        let mut sig = hybrid_sign(&kp, msg).unwrap();
        // Tamper with the classical (Ed25519) signature.
        sig.classical_sig[0] ^= 0xFF;
        let valid = hybrid_verify(&kp, msg, &sig).unwrap();
        assert!(!valid, "Should fail when classical sig is tampered");
    }

    #[test]
    fn test_verify_fails_tampered_pqc_sig() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = b"test message";
        let mut sig = hybrid_sign(&kp, msg).unwrap();
        // Tamper with the PQC (ML-DSA) signature.
        sig.pqc_sig[0] ^= 0xFF;
        let valid = hybrid_verify(&kp, msg, &sig).unwrap();
        assert!(!valid, "Should fail when PQC sig is tampered");
    }

    #[test]
    fn test_verify_fails_wrong_keypair() {
        let kp1 = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let kp2 = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = b"test message";
        let sig = hybrid_sign(&kp1, msg).unwrap();
        let valid = hybrid_verify(&kp2, msg, &sig).unwrap();
        assert!(!valid, "Should fail with different keypair");
    }

    #[test]
    fn test_signature_sizes() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let sig = hybrid_sign(&kp, b"test").unwrap();
        assert_eq!(sig.classical_sig.len(), 64); // Ed25519
        assert_eq!(sig.pqc_sig.len(), 3309); // ML-DSA-65
        assert_eq!(sig.algorithm, PqcAlgorithm::MlDsa65);
    }

    #[test]
    fn test_signature_sizes_dsa44() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa44).unwrap();
        let sig = hybrid_sign(&kp, b"test").unwrap();
        assert_eq!(sig.pqc_sig.len(), 2420);
    }

    #[test]
    fn test_signature_sizes_dsa87() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa87).unwrap();
        let sig = hybrid_sign(&kp, b"test").unwrap();
        assert_eq!(sig.pqc_sig.len(), 4627);
    }

    // -----------------------------------------------------------------------
    // Key Store CRUD
    // -----------------------------------------------------------------------

    #[test]
    fn test_keystore_new_empty() {
        let store = PqcKeyStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.list_keys().is_empty());
    }

    #[test]
    fn test_keystore_add_kem() {
        let mut store = PqcKeyStore::new();
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        store.add_kem_keypair("kem-1", kp).unwrap();
        assert_eq!(store.len(), 1);
        assert!(store.get_kem_keypair("kem-1").is_some());
    }

    #[test]
    fn test_keystore_add_signing() {
        let mut store = PqcKeyStore::new();
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        store.add_signing_keypair("sig-1", kp).unwrap();
        assert_eq!(store.len(), 1);
        assert!(store.get_signing_keypair("sig-1").is_some());
    }

    #[test]
    fn test_keystore_duplicate_key_id() {
        let mut store = PqcKeyStore::new();
        let kp1 = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let kp2 = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        store.add_kem_keypair("dup", kp1).unwrap();
        let result = store.add_kem_keypair("dup", kp2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_keystore_duplicate_across_types() {
        let mut store = PqcKeyStore::new();
        let kem_kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let sig_kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        store.add_kem_keypair("shared-id", kem_kp).unwrap();
        let result = store.add_signing_keypair("shared-id", sig_kp);
        assert!(result.is_err());
    }

    #[test]
    fn test_keystore_get_nonexistent() {
        let store = PqcKeyStore::new();
        assert!(store.get_kem_keypair("nope").is_none());
        assert!(store.get_signing_keypair("nope").is_none());
    }

    #[test]
    fn test_keystore_remove() {
        let mut store = PqcKeyStore::new();
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        store.add_kem_keypair("remove-me", kp).unwrap();
        assert_eq!(store.len(), 1);
        store.remove_key("remove-me").unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn test_keystore_remove_signing() {
        let mut store = PqcKeyStore::new();
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        store.add_signing_keypair("sig-rm", kp).unwrap();
        store.remove_key("sig-rm").unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn test_keystore_remove_nonexistent() {
        let mut store = PqcKeyStore::new();
        let result = store.remove_key("ghost");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_keystore_list_keys() {
        let mut store = PqcKeyStore::new();
        let kem_kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let sig_kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        store.add_kem_keypair("aaa-kem", kem_kp).unwrap();
        store.add_signing_keypair("bbb-sig", sig_kp).unwrap();

        let keys = store.list_keys();
        assert_eq!(keys.len(), 2);
        // Sorted by key_id.
        assert_eq!(keys[0].key_id, "aaa-kem");
        assert_eq!(keys[0].key_type, KeyType::Kem);
        assert_eq!(keys[0].algorithm, PqcAlgorithm::MlKem768);
        assert_eq!(keys[1].key_id, "bbb-sig");
        assert_eq!(keys[1].key_type, KeyType::Signing);
        assert_eq!(keys[1].algorithm, PqcAlgorithm::MlDsa65);
    }

    // -----------------------------------------------------------------------
    // Key Store persistence
    // -----------------------------------------------------------------------

    #[test]
    fn test_keystore_save_load_roundtrip() {
        let mut store = PqcKeyStore::new();
        let kem_kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let sig_kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        store.add_kem_keypair("kem-persist", kem_kp).unwrap();
        store.add_signing_keypair("sig-persist", sig_kp).unwrap();

        let tmp = NamedTempFile::new().unwrap();
        store.save(tmp.path()).unwrap();

        let loaded = PqcKeyStore::load(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.get_kem_keypair("kem-persist").is_some());
        assert!(loaded.get_signing_keypair("sig-persist").is_some());
    }

    #[test]
    fn test_keystore_load_nonexistent_file() {
        let result = PqcKeyStore::load(Path::new("/tmp/does-not-exist-pqc-test.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_keystore_save_load_empty() {
        let store = PqcKeyStore::new();
        let tmp = NamedTempFile::new().unwrap();
        store.save(tmp.path()).unwrap();
        let loaded = PqcKeyStore::load(tmp.path()).unwrap();
        assert!(loaded.is_empty());
    }

    // -----------------------------------------------------------------------
    // Config
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_defaults() {
        let cfg = PqcConfig::default();
        assert_eq!(cfg.mode, PqcMode::Hybrid);
        assert_eq!(cfg.kem_algorithm, PqcAlgorithm::MlKem768);
        assert_eq!(cfg.signing_algorithm, PqcAlgorithm::MlDsa65);
        assert!(!cfg.require_pqc_for_agents);
        assert!(!cfg.require_pqc_for_federation);
    }

    #[test]
    fn test_config_is_pqc_active() {
        let mut cfg = PqcConfig::new();
        cfg.mode = PqcMode::Hybrid;
        assert!(cfg.is_pqc_active());
        cfg.mode = PqcMode::PqcOnly;
        assert!(cfg.is_pqc_active());
        cfg.mode = PqcMode::Disabled;
        assert!(!cfg.is_pqc_active());
    }

    #[test]
    fn test_config_is_classical_active() {
        let mut cfg = PqcConfig::new();
        cfg.mode = PqcMode::Hybrid;
        assert!(cfg.is_classical_active());
        cfg.mode = PqcMode::Disabled;
        assert!(cfg.is_classical_active());
        cfg.mode = PqcMode::PqcOnly;
        assert!(!cfg.is_classical_active());
    }

    #[test]
    fn test_config_serialization() {
        let cfg = PqcConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: PqcConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mode, cfg.mode);
        assert_eq!(deserialized.kem_algorithm, cfg.kem_algorithm);
        assert_eq!(deserialized.signing_algorithm, cfg.signing_algorithm);
    }

    #[test]
    fn test_pqc_mode_display() {
        assert_eq!(PqcMode::Disabled.to_string(), "Disabled");
        assert_eq!(PqcMode::Hybrid.to_string(), "Hybrid");
        assert_eq!(PqcMode::PqcOnly.to_string(), "PQC-Only");
    }

    #[test]
    fn test_pqc_mode_default() {
        assert_eq!(PqcMode::default(), PqcMode::Hybrid);
    }

    // -----------------------------------------------------------------------
    // Migration status
    // -----------------------------------------------------------------------

    #[test]
    fn test_migration_status_empty() {
        let status = PqcMigrationStatus::new();
        assert!(!status.migration_complete());
        assert_eq!(status.agent_migration_percent(), 0.0);
        assert_eq!(status.federation_migration_percent(), 0.0);
    }

    #[test]
    fn test_migration_status_partial() {
        let status = PqcMigrationStatus {
            agents_with_pqc_keys: 5,
            agents_without_pqc_keys: 5,
            federation_nodes_pqc_ready: 2,
            federation_nodes_total: 4,
            last_checked: Some(Utc::now()),
        };
        assert!(!status.migration_complete());
        assert!((status.agent_migration_percent() - 50.0).abs() < f64::EPSILON);
        assert!((status.federation_migration_percent() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_migration_status_complete() {
        let status = PqcMigrationStatus {
            agents_with_pqc_keys: 10,
            agents_without_pqc_keys: 0,
            federation_nodes_pqc_ready: 3,
            federation_nodes_total: 3,
            last_checked: Some(Utc::now()),
        };
        assert!(status.migration_complete());
        assert!((status.agent_migration_percent() - 100.0).abs() < f64::EPSILON);
        assert!((status.federation_migration_percent() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_migration_not_complete_with_zero_agents() {
        let status = PqcMigrationStatus {
            agents_with_pqc_keys: 0,
            agents_without_pqc_keys: 0,
            federation_nodes_pqc_ready: 1,
            federation_nodes_total: 1,
            last_checked: None,
        };
        // No agents at all — migration_complete requires agents_with_pqc_keys > 0.
        assert!(!status.migration_complete());
    }

    #[test]
    fn test_migration_serialization() {
        let status = PqcMigrationStatus {
            agents_with_pqc_keys: 7,
            agents_without_pqc_keys: 3,
            federation_nodes_pqc_ready: 2,
            federation_nodes_total: 5,
            last_checked: Some(Utc::now()),
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: PqcMigrationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agents_with_pqc_keys, 7);
        assert_eq!(deserialized.agents_without_pqc_keys, 3);
    }

    // -----------------------------------------------------------------------
    // Key type display
    // -----------------------------------------------------------------------

    #[test]
    fn test_key_type_display() {
        assert_eq!(KeyType::Kem.to_string(), "KEM");
        assert_eq!(KeyType::Signing.to_string(), "Signing");
    }

    // -----------------------------------------------------------------------
    // Hex helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_hex_roundtrip() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let encoded = hex::encode(&data);
        assert_eq!(encoded, "deadbeef");
        let decoded = hex::decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_decode_odd_length() {
        let result = hex::decode("abc");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_constant_time_eq_equal() {
        let a = vec![1, 2, 3, 4];
        assert!(constant_time_eq(&a, &a));
    }

    #[test]
    fn test_constant_time_eq_different() {
        let a = vec![1, 2, 3, 4];
        let b = vec![1, 2, 3, 5];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        let a = vec![1, 2, 3];
        let b = vec![1, 2, 3, 4];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn test_expand_hash_deterministic() {
        let seed = b"test seed";
        let a = expand_hash(seed, b"domain", 64);
        let b = expand_hash(seed, b"domain", 64);
        assert_eq!(a, b);
    }

    #[test]
    fn test_expand_hash_different_domains() {
        let seed = b"test seed";
        let a = expand_hash(seed, b"domain-a", 32);
        let b = expand_hash(seed, b"domain-b", 32);
        assert_ne!(a, b);
    }

    #[test]
    fn test_sign_large_message() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let msg = vec![0x42u8; 1_000_000]; // 1 MB message
        let sig = hybrid_sign(&kp, &msg).unwrap();
        assert!(hybrid_verify(&kp, &msg, &sig).unwrap());
    }

    #[test]
    fn test_hybrid_keypair_debug_no_secrets() {
        let kp = generate_kem_keypair(PqcAlgorithm::MlKem768).unwrap();
        let debug_str = format!("{:?}", kp);
        // Debug output should NOT contain secret key material.
        assert!(!debug_str.contains("classical_secret"));
        assert!(!debug_str.contains("pqc_secret"));
    }

    #[test]
    fn test_signing_keypair_debug_no_secrets() {
        let kp = generate_signing_keypair(PqcAlgorithm::MlDsa65).unwrap();
        let debug_str = format!("{:?}", kp);
        assert!(!debug_str.contains("classical_signing_key"));
        assert!(!debug_str.contains("pqc_secret"));
    }
}
