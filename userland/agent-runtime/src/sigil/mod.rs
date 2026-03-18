//! Sigil — System-Wide Trust Verification for AGNOS
//!
//! Unified trust module that extends marketplace signing into a complete
//! trust chain covering boot, agent binaries, configs, and packages.
//! Named after the Latin word for "seal" — sigil seals trust into AGNOS.

pub mod chain;
pub mod policy;
pub mod types;
pub mod verify;

#[cfg(test)]
mod tests;

// Re-export the full public API surface (identical to old sigil.rs).
pub use policy::{RevocationEntry, RevocationList};
pub use types::{
    ArtifactType, SigilStats, TrustCheck, TrustEnforcement, TrustLevel, TrustPolicy,
    TrustedArtifact, VerificationResult,
};
pub use verify::SigilVerifier;
