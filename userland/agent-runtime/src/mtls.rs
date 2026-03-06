//! Zero-Trust Agent Networking — mTLS Certificate Lifecycle
//!
//! Models mutual TLS certificate management for agent-to-agent communication.
//! This module handles certificate metadata, issuance, verification, revocation,
//! and rotation — without implementing actual TLS handshakes.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agnos_common::AgentId;

/// An agent's TLS certificate metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCertificate {
    pub agent_id: AgentId,
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub fingerprint_sha256: String,
    pub public_key_hash: String,
}

/// Result of verifying a certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertVerifyResult {
    Valid,
    Expired,
    NotYetValid,
    Revoked,
    UnknownIssuer,
}

/// Policy governing mTLS requirements for agent communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsPolicy {
    pub require_mtls: bool,
    pub allowed_issuers: Vec<String>,
    pub min_key_strength: u32,
    pub auto_rotate_days: u32,
}

impl Default for MtlsPolicy {
    fn default() -> Self {
        Self {
            require_mtls: true,
            allowed_issuers: Vec::new(),
            min_key_strength: 2048,
            auto_rotate_days: 90,
        }
    }
}

/// Information about an established mTLS connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsConnectionInfo {
    pub peer_cert: AgentCertificate,
    pub verified: bool,
    pub cipher_suite: String,
}

/// Certificate authority that manages agent certificates.
pub struct CertificateAuthority {
    ca_name: String,
    certificates: Vec<AgentCertificate>,
    revoked_serials: Vec<String>,
}

impl CertificateAuthority {
    /// Create a new certificate authority.
    pub fn new(ca_name: &str) -> Self {
        Self {
            ca_name: ca_name.to_string(),
            certificates: Vec::new(),
            revoked_serials: Vec::new(),
        }
    }

    /// Issue a new certificate for an agent.
    pub fn issue_certificate(&mut self, agent_id: AgentId, validity_days: u32) -> AgentCertificate {
        let now = Utc::now();
        let serial = Uuid::new_v4().to_string();
        let fingerprint = format!("sha256:{}", Uuid::new_v4().as_simple());
        let pubkey_hash = format!("spki:{}", Uuid::new_v4().as_simple());

        let cert = AgentCertificate {
            agent_id,
            subject: format!("CN=agent-{}", agent_id),
            issuer: self.ca_name.clone(),
            serial,
            not_before: now,
            not_after: now + Duration::days(validity_days as i64),
            fingerprint_sha256: fingerprint,
            public_key_hash: pubkey_hash,
        };

        self.certificates.push(cert.clone());
        cert
    }

    /// Verify a certificate: check expiry, issuer, and revocation status.
    pub fn verify_certificate(&self, cert: &AgentCertificate) -> CertVerifyResult {
        let now = Utc::now();

        if self.is_revoked(&cert.serial) {
            return CertVerifyResult::Revoked;
        }
        if cert.issuer != self.ca_name {
            return CertVerifyResult::UnknownIssuer;
        }
        if now < cert.not_before {
            return CertVerifyResult::NotYetValid;
        }
        if now > cert.not_after {
            return CertVerifyResult::Expired;
        }
        CertVerifyResult::Valid
    }

    /// Revoke a certificate by serial number.
    pub fn revoke_certificate(&mut self, serial: &str) -> bool {
        if self.certificates.iter().any(|c| c.serial == serial) {
            if !self.revoked_serials.contains(&serial.to_string()) {
                self.revoked_serials.push(serial.to_string());
            }
            true
        } else {
            false
        }
    }

    /// Check if a serial number is on the revocation list.
    pub fn is_revoked(&self, serial: &str) -> bool {
        self.revoked_serials.iter().any(|s| s == serial)
    }

    /// Rotate a certificate: issue a new one and revoke the old.
    pub fn rotate_certificate(&mut self, agent_id: AgentId) -> AgentCertificate {
        // Revoke all existing certificates for this agent
        let old_serials: Vec<String> = self.certificates
            .iter()
            .filter(|c| c.agent_id == agent_id && !self.revoked_serials.contains(&c.serial))
            .map(|c| c.serial.clone())
            .collect();

        for serial in old_serials {
            self.revoke_certificate(&serial);
        }

        // Issue a new certificate with default 365-day validity
        self.issue_certificate(agent_id, 365)
    }

    /// Find certificates expiring within the given number of days.
    pub fn certificates_expiring_within(&self, days: u32) -> Vec<&AgentCertificate> {
        let deadline = Utc::now() + Duration::days(days as i64);
        self.certificates
            .iter()
            .filter(|c| {
                !self.is_revoked(&c.serial) && c.not_after <= deadline && c.not_after > Utc::now()
            })
            .collect()
    }

    /// Return all certificates (including revoked).
    pub fn all_certificates(&self) -> Vec<&AgentCertificate> {
        self.certificates.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(n: u8) -> AgentId {
        AgentId(Uuid::from_bytes([n; 16]))
    }

    #[test]
    fn test_issue_certificate() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 365);
        assert_eq!(cert.agent_id, agent(1));
        assert_eq!(cert.issuer, "AGNOS-CA");
        assert!(cert.not_after > cert.not_before);
        assert!(cert.fingerprint_sha256.starts_with("sha256:"));
        assert!(cert.public_key_hash.starts_with("spki:"));
    }

    #[test]
    fn test_verify_valid_certificate() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 365);
        assert_eq!(ca.verify_certificate(&cert), CertVerifyResult::Valid);
    }

    #[test]
    fn test_verify_expired_certificate() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let mut cert = ca.issue_certificate(agent(1), 1);
        // Manually expire it
        cert.not_after = Utc::now() - Duration::days(1);
        assert_eq!(ca.verify_certificate(&cert), CertVerifyResult::Expired);
    }

    #[test]
    fn test_verify_not_yet_valid() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let mut cert = ca.issue_certificate(agent(1), 365);
        cert.not_before = Utc::now() + Duration::days(10);
        assert_eq!(ca.verify_certificate(&cert), CertVerifyResult::NotYetValid);
    }

    #[test]
    fn test_verify_unknown_issuer() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let mut cert = ca.issue_certificate(agent(1), 365);
        cert.issuer = "EVIL-CA".to_string();
        assert_eq!(ca.verify_certificate(&cert), CertVerifyResult::UnknownIssuer);
    }

    #[test]
    fn test_revoke_certificate() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 365);
        assert!(!ca.is_revoked(&cert.serial));
        assert!(ca.revoke_certificate(&cert.serial));
        assert!(ca.is_revoked(&cert.serial));
        assert_eq!(ca.verify_certificate(&cert), CertVerifyResult::Revoked);
    }

    #[test]
    fn test_revoke_nonexistent_serial() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        assert!(!ca.revoke_certificate("nonexistent-serial"));
    }

    #[test]
    fn test_revoke_idempotent() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 365);
        assert!(ca.revoke_certificate(&cert.serial));
        assert!(ca.revoke_certificate(&cert.serial)); // second call still true
        // Only one entry in revoked list
        assert_eq!(ca.revoked_serials.len(), 1);
    }

    #[test]
    fn test_rotate_certificate() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let old_cert = ca.issue_certificate(agent(1), 365);
        let old_serial = old_cert.serial.clone();

        let new_cert = ca.rotate_certificate(agent(1));
        assert_ne!(new_cert.serial, old_serial);
        assert!(ca.is_revoked(&old_serial));
        assert_eq!(ca.verify_certificate(&new_cert), CertVerifyResult::Valid);
    }

    #[test]
    fn test_rotate_multiple_old_certs() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert1 = ca.issue_certificate(agent(1), 365);
        let cert2 = ca.issue_certificate(agent(1), 365);
        let s1 = cert1.serial.clone();
        let s2 = cert2.serial.clone();

        let new_cert = ca.rotate_certificate(agent(1));
        assert!(ca.is_revoked(&s1));
        assert!(ca.is_revoked(&s2));
        assert!(!ca.is_revoked(&new_cert.serial));
    }

    #[test]
    fn test_certificates_expiring_within() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let _long = ca.issue_certificate(agent(1), 365);
        let mut short = ca.issue_certificate(agent(2), 30);
        // Override to expire in 5 days
        short.not_after = Utc::now() + Duration::days(5);
        // Replace in CA's list
        if let Some(c) = ca.certificates.iter_mut().find(|c| c.serial == short.serial) {
            c.not_after = short.not_after;
        }

        let expiring = ca.certificates_expiring_within(10);
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].agent_id, agent(2));
    }

    #[test]
    fn test_all_certificates() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        ca.issue_certificate(agent(1), 365);
        ca.issue_certificate(agent(2), 365);
        ca.issue_certificate(agent(3), 365);
        assert_eq!(ca.all_certificates().len(), 3);
    }

    #[test]
    fn test_certificate_serialization() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 365);
        let json = serde_json::to_string(&cert).unwrap();
        let deserialized: AgentCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_id, cert.agent_id);
        assert_eq!(deserialized.serial, cert.serial);
        assert_eq!(deserialized.issuer, cert.issuer);
    }

    #[test]
    fn test_mtls_policy_default() {
        let policy = MtlsPolicy::default();
        assert!(policy.require_mtls);
        assert!(policy.allowed_issuers.is_empty());
        assert_eq!(policy.min_key_strength, 2048);
        assert_eq!(policy.auto_rotate_days, 90);
    }

    #[test]
    fn test_mtls_connection_info() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 365);
        let info = MtlsConnectionInfo {
            peer_cert: cert.clone(),
            verified: true,
            cipher_suite: "TLS_AES_256_GCM_SHA384".to_string(),
        };
        assert!(info.verified);
        assert_eq!(info.peer_cert.agent_id, agent(1));
    }

    #[test]
    fn test_verify_result_enum_values() {
        // Ensure all variants are distinct
        let variants = [
            CertVerifyResult::Valid,
            CertVerifyResult::Expired,
            CertVerifyResult::NotYetValid,
            CertVerifyResult::Revoked,
            CertVerifyResult::UnknownIssuer,
        ];
        for i in 0..variants.len() {
            for j in (i + 1)..variants.len() {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }

    #[test]
    fn test_ca_empty_initially() {
        let ca = CertificateAuthority::new("Test-CA");
        assert!(ca.all_certificates().is_empty());
        assert!(!ca.is_revoked("anything"));
    }

    #[test]
    fn test_certificate_subject_format() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(5), 30);
        assert!(cert.subject.starts_with("CN=agent-"));
    }

    #[test]
    fn test_certificates_expiring_excludes_revoked() {
        let mut ca = CertificateAuthority::new("AGNOS-CA");
        let cert = ca.issue_certificate(agent(1), 5);
        // This cert expires within 10 days
        let serial = cert.serial.clone();
        ca.revoke_certificate(&serial);
        let expiring = ca.certificates_expiring_within(10);
        // Revoked cert should not appear
        assert!(expiring.iter().all(|c| c.serial != serial));
    }
}
