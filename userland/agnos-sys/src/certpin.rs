//! TLS Certificate Pinning Interface
//!
//! Provides certificate pinning for outbound TLS connections, primarily for
//! the LLM Gateway's connections to cloud AI providers (OpenAI, Anthropic, Google).
//! Uses SPKI (Subject Public Key Info) SHA-256 hashing, compatible with the HPKP
//! standard (RFC 7469).
//!
//! On non-Linux platforms, `fetch_server_cert` returns `SysError::NotSupported`.

use crate::error::{Result, SysError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Base64 encoder (minimal, standard alphabet with padding)
// ---------------------------------------------------------------------------

const BASE64_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(BASE64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, String> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.chars() {
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("invalid base64 character: {}", c)),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A pinned certificate entry for a specific host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PinnedCert {
    /// Hostname (e.g. "api.openai.com")
    pub host: String,
    /// Primary SPKI SHA-256 pins (base64-encoded). Multiple for rotation.
    pub pin_sha256: Vec<String>,
    /// Optional expiry — after this time the pin entry is considered stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
    /// Backup pins for key rotation (should be pinned before the CA rotates).
    #[serde(default)]
    pub backup_pins: Vec<String>,
}

/// A set of pinned certificates with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CertPinSet {
    /// All pinned certificate entries.
    pub pins: Vec<PinnedCert>,
    /// If `true`, connection MUST be rejected on mismatch.
    /// If `false`, mismatch is only reported (report-only mode).
    pub enforce: bool,
    /// When this pin set was created / last updated.
    pub created_at: DateTime<Utc>,
    /// Schema version for forward compatibility.
    pub version: u32,
}

/// Result of verifying a certificate pin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CertPinResult {
    /// The presented SPKI pin matches a primary or backup pin.
    Valid,
    /// Pin mismatch — potential MITM or CA change.
    PinMismatch {
        host: String,
        expected: Vec<String>,
        actual: String,
    },
    /// The pin entry for this host has expired.
    Expired { host: String },
    /// No pin is configured for this host.
    NoPinConfigured { host: String },
    /// An error occurred during verification.
    Error(String),
}

/// Configuration for the cert pinning subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CertPinConfig {
    /// Path to the JSON pin-set file on disk.
    pub pin_file: PathBuf,
    /// Enforce mode (true) vs report-only (false).
    pub enforce: bool,
    /// Optional URI to POST pin violation reports to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_uri: Option<String>,
    /// Whether pins apply to subdomains of the host as well.
    #[serde(default)]
    pub include_subdomains: bool,
    /// Maximum age in seconds that the pin set is considered fresh.
    #[serde(default = "default_max_age")]
    pub max_age_secs: u64,
}

fn default_max_age() -> u64 {
    2_592_000 // 30 days
}

/// Information extracted from an X.509 certificate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CertInfo {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub not_before: String,
    pub not_after: String,
    /// SHA-256 fingerprint of the full certificate (hex-encoded).
    pub sha256_fingerprint: String,
    /// SHA-256 hash of the Subject Public Key Info (base64-encoded).
    /// This is the value used for pin comparison.
    pub spki_sha256: String,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Compute the SPKI pin from a PEM-encoded certificate.
///
/// Extracts the DER-encoded Subject Public Key Info from the PEM certificate,
/// computes its SHA-256 hash, and returns it as a base64-encoded string
/// (standard HPKP pin format: `sha256/...`).
pub fn compute_spki_pin(cert_pem: &str) -> Result<String> {
    // Extract base64 content between BEGIN/END CERTIFICATE markers
    let der = pem_to_der(cert_pem)?;
    // Parse the TBSCertificate to locate the SubjectPublicKeyInfo field
    let spki_bytes = extract_spki_from_der(&der)?;
    let hash = Sha256::digest(&spki_bytes);
    Ok(base64_encode(&hash))
}

/// Verify a certificate pin against a pin set (pure function).
pub fn verify_pin(host: &str, actual_spki_pin: &str, pin_set: &CertPinSet) -> CertPinResult {
    let entry = pin_set.pins.iter().find(|p| p.host == host);
    let entry = match entry {
        Some(e) => e,
        None => return CertPinResult::NoPinConfigured { host: host.to_string() },
    };

    // Check expiry
    if let Some(expires) = entry.expires {
        if Utc::now() > expires {
            return CertPinResult::Expired { host: host.to_string() };
        }
    }

    // Check primary pins
    if entry.pin_sha256.iter().any(|p| p == actual_spki_pin) {
        return CertPinResult::Valid;
    }

    // Check backup pins
    if entry.backup_pins.iter().any(|p| p == actual_spki_pin) {
        return CertPinResult::Valid;
    }

    let mut expected = entry.pin_sha256.clone();
    expected.extend(entry.backup_pins.clone());
    CertPinResult::PinMismatch {
        host: host.to_string(),
        expected,
        actual: actual_spki_pin.to_string(),
    }
}

/// Load a `CertPinSet` from a JSON file.
pub fn load_pin_set(path: &Path) -> Result<CertPinSet> {
    let data = std::fs::read_to_string(path).map_err(|e| {
        SysError::Unknown(format!("Failed to read pin set file {}: {}", path.display(), e))
    })?;
    serde_json::from_str(&data).map_err(|e| {
        SysError::InvalidArgument(format!("Invalid pin set JSON: {}", e))
    })
}

/// Save a `CertPinSet` to a JSON file.
pub fn save_pin_set(pin_set: &CertPinSet, path: &Path) -> Result<()> {
    let data = serde_json::to_string_pretty(pin_set).map_err(|e| {
        SysError::Unknown(format!("Failed to serialize pin set: {}", e))
    })?;
    std::fs::write(path, data).map_err(|e| {
        SysError::Unknown(format!("Failed to write pin set to {}: {}", path.display(), e))
    })
}

/// Fetch a server's TLS certificate using `openssl s_client`.
///
/// On non-Linux platforms this returns `SysError::NotSupported`.
#[cfg(target_os = "linux")]
pub fn fetch_server_cert(host: &str, port: u16) -> Result<CertInfo> {
    if host.is_empty() {
        return Err(SysError::InvalidArgument("host cannot be empty".into()));
    }
    if host.contains('/') || host.contains(' ') {
        return Err(SysError::InvalidArgument("host contains invalid characters".into()));
    }

    let connect_str = format!("{}:{}", host, port);

    // Get the PEM certificate
    let pem_output = std::process::Command::new("openssl")
        .args(["s_client", "-connect", &connect_str, "-servername", host])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run openssl s_client: {}", e)))?;

    let pem_text = String::from_utf8_lossy(&pem_output.stdout);

    // Extract PEM block
    let begin = pem_text
        .find("-----BEGIN CERTIFICATE-----")
        .ok_or_else(|| SysError::Unknown("No certificate found in openssl output".into()))?;
    let end = pem_text
        .find("-----END CERTIFICATE-----")
        .ok_or_else(|| SysError::Unknown("Malformed certificate in openssl output".into()))?;
    let pem_cert = &pem_text[begin..end + "-----END CERTIFICATE-----".len()];

    // Parse certificate details with openssl x509 (pipe PEM via stdin)
    let mut x509_child = std::process::Command::new("openssl")
        .args(["x509", "-noout", "-subject", "-issuer", "-serial", "-dates", "-fingerprint", "-sha256"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| SysError::Unknown(format!("Failed to spawn openssl x509: {}", e)))?;

    if let Some(mut stdin) = x509_child.stdin.take() {
        use std::io::Write;
        if let Err(e) = stdin.write_all(pem_cert.as_bytes()) {
            tracing::warn!("Failed to write PEM to openssl x509 stdin: {}", e);
        }
    }
    let x509_output = x509_child
        .wait_with_output()
        .map_err(|e| SysError::Unknown(format!("Failed to run openssl x509: {}", e)))?;

    let x509_text = String::from_utf8_lossy(&x509_output.stdout);

    // Get SPKI hash: openssl x509 -pubkey | openssl pkey -pubin -outform DER | sha256sum
    // We use a two-stage pipeline, piping PEM via stdin to avoid shell injection.
    let mut pubkey_child = std::process::Command::new("openssl")
        .args(["x509", "-pubkey", "-noout"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| SysError::Unknown(format!("Failed to spawn openssl x509 -pubkey: {}", e)))?;

    if let Some(mut stdin) = pubkey_child.stdin.take() {
        use std::io::Write;
        if let Err(e) = stdin.write_all(pem_cert.as_bytes()) {
            tracing::warn!("Failed to write PEM to openssl pubkey stdin: {}", e);
        }
    }
    let pubkey_output = pubkey_child
        .wait_with_output()
        .map_err(|e| SysError::Unknown(format!("Failed to get public key: {}", e)))?;

    let mut pkey_child = std::process::Command::new("openssl")
        .args(["pkey", "-pubin", "-outform", "DER"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| SysError::Unknown(format!("Failed to spawn openssl pkey: {}", e)))?;

    if let Some(mut stdin) = pkey_child.stdin.take() {
        use std::io::Write;
        if let Err(e) = stdin.write_all(&pubkey_output.stdout) {
            tracing::warn!("Failed to write pubkey to openssl pkey stdin: {}", e);
        }
    }
    let der_output = pkey_child
        .wait_with_output()
        .map_err(|e| SysError::Unknown(format!("Failed to get DER key: {}", e)))?;

    // Compute SHA-256 of the DER-encoded SPKI directly in Rust
    use sha2::{Sha256, Digest};
    let spki_hash = Sha256::digest(&der_output.stdout);
    let _spki_hex = hex::encode(&spki_hash);

    let spki_bytes = spki_hash.to_vec();
    let spki_b64 = base64_encode(&spki_bytes);

    let mut info = parse_openssl_cert(&x509_text)?;
    info.spki_sha256 = spki_b64;
    Ok(info)
}

#[cfg(not(target_os = "linux"))]
pub fn fetch_server_cert(_host: &str, _port: u16) -> Result<CertInfo> {
    Err(SysError::NotSupported)
}

/// Parse the text output of `openssl x509 -noout -subject -issuer -serial -dates -fingerprint`.
///
/// This is a pure function, suitable for unit testing.
pub fn parse_openssl_cert(output: &str) -> Result<CertInfo> {
    let get_field = |prefix: &str| -> String {
        output
            .lines()
            .find(|l| l.starts_with(prefix))
            .map(|l| l[prefix.len()..].trim().to_string())
            .unwrap_or_default()
    };

    let subject = get_field("subject=");
    let issuer = get_field("issuer=");
    let serial = get_field("serial=");
    let not_before = get_field("notBefore=");
    let not_after = get_field("notAfter=");
    let fingerprint_raw = get_field("sha256 Fingerprint=");
    // Also try uppercase variant produced by some openssl versions
    let fingerprint_raw = if fingerprint_raw.is_empty() {
        get_field("SHA256 Fingerprint=")
    } else {
        fingerprint_raw
    };
    let sha256_fingerprint = fingerprint_raw.replace(':', "").to_lowercase();

    if subject.is_empty() && issuer.is_empty() && serial.is_empty() {
        return Err(SysError::InvalidArgument(
            "Could not parse any certificate fields from openssl output".into(),
        ));
    }

    Ok(CertInfo {
        subject,
        issuer,
        serial,
        not_before,
        not_after,
        sha256_fingerprint,
        spki_sha256: String::new(), // Caller must fill in separately
    })
}

/// Generate an HTTP Public-Key-Pins header value for a given host.
///
/// Returns `None` if no pins are configured for the host.
pub fn generate_pin_header(
    config: &CertPinConfig,
    pin_set: &CertPinSet,
    host: &str,
) -> Option<String> {
    let entry = pin_set.pins.iter().find(|p| p.host == host)?;

    let mut parts: Vec<String> = Vec::new();

    for pin in &entry.pin_sha256 {
        parts.push(format!("pin-sha256=\"{}\"", pin));
    }
    for pin in &entry.backup_pins {
        parts.push(format!("pin-sha256=\"{}\"", pin));
    }

    parts.push(format!("max-age={}", config.max_age_secs));

    if config.include_subdomains {
        parts.push("includeSubDomains".to_string());
    }

    if let Some(ref uri) = config.report_uri {
        parts.push(format!("report-uri=\"{}\"", uri));
    }

    Some(parts.join("; "))
}

/// Validate that a pin string is a valid base64-encoded SHA-256 hash.
///
/// A valid pin is 44 characters of base64 (representing 32 bytes = 256 bits).
pub fn validate_pin_format(pin: &str) -> Result<()> {
    if pin.is_empty() {
        return Err(SysError::InvalidArgument("Pin cannot be empty".into()));
    }

    // base64 of 32 bytes = 44 chars (with padding)
    let decoded = base64_decode(pin).map_err(|e| {
        SysError::InvalidArgument(format!("Pin is not valid base64: {}", e))
    })?;

    if decoded.len() != 32 {
        return Err(SysError::InvalidArgument(format!(
            "Pin decodes to {} bytes, expected 32 (SHA-256)",
            decoded.len()
        )));
    }

    Ok(())
}

/// Return all pinned certs that expire within the next 30 days.
pub fn check_pin_expiry(pin_set: &CertPinSet) -> Vec<PinnedCert> {
    let threshold = Utc::now() + chrono::Duration::days(30);
    pin_set
        .pins
        .iter()
        .filter(|p| {
            if let Some(exp) = p.expires {
                exp <= threshold
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

/// Built-in pins for known AGNOS cloud provider hosts.
///
/// # WARNING: Development-Only Placeholder Pins
///
/// These SPKI hashes are **not production-verified**. They were generated during
/// development and may not match the current certificates served by these providers.
/// Before any production deployment:
///
/// 1. Fetch live SPKI hashes: `openssl s_client -connect <host>:443 | openssl x509 -pubkey -noout | openssl pkey -pubin -outform DER | openssl dgst -sha256 -binary | base64`
/// 2. Replace the pin values below
/// 3. Set `enforce: true`
/// 4. Set `expires` to a reasonable rotation date
///
/// The `enforce: false` default means pin mismatches are **logged but not blocked**.
/// This is intentional for pre-alpha to avoid connectivity breakage from certificate rotations.
pub fn default_agnos_pins() -> CertPinSet {
    tracing::warn!(
        "Using development placeholder certificate pins (enforce=false). \
         Replace with live SPKI hashes before production deployment."
    );

    CertPinSet {
        pins: vec![
            PinnedCert {
                host: "api.openai.com".to_string(),
                pin_sha256: vec![
                    "YZPgTZ+woNCCCIW3LH2CxQeLzB/1m42QcCTBSdgayjs=".to_string(),
                ],
                expires: None,
                backup_pins: vec![
                    "Vjs8r4z+80wjNcr1YKepWQboSIRi63WsWXhIMN+eWys=".to_string(),
                ],
            },
            PinnedCert {
                host: "api.anthropic.com".to_string(),
                pin_sha256: vec![
                    "jQJTbIh0grw0/1TkHSumWb+Fs0Ggogr621gT3PvPKG0=".to_string(),
                ],
                expires: None,
                backup_pins: vec![
                    "C5+lpZ7tcVwmwQIMcRtPbsQtWLABXhQzejna0wHFr8M=".to_string(),
                ],
            },
            PinnedCert {
                host: "generativelanguage.googleapis.com".to_string(),
                pin_sha256: vec![
                    "hxqRlPTu1bMS/0DITB1SSu0vd4u/8l8TPoA4DLBldGc=".to_string(),
                ],
                expires: None,
                backup_pins: vec![
                    "Vjs8r4z+80wjNcr1YKepWQboSIRi63WsWXhIMN+eWys=".to_string(),
                ],
            },
        ],
        enforce: false, // report-only — see docstring above
        created_at: Utc::now(),
        version: 1,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers — PEM / DER / ASN.1 parsing
// ---------------------------------------------------------------------------

/// Decode a PEM block into DER bytes.
fn pem_to_der(pem: &str) -> Result<Vec<u8>> {
    let begin_marker = "-----BEGIN CERTIFICATE-----";
    let end_marker = "-----END CERTIFICATE-----";

    let start = pem
        .find(begin_marker)
        .ok_or_else(|| SysError::InvalidArgument("Missing BEGIN CERTIFICATE marker".into()))?;
    let end = pem
        .find(end_marker)
        .ok_or_else(|| SysError::InvalidArgument("Missing END CERTIFICATE marker".into()))?;

    let b64_content: String = pem[start + begin_marker.len()..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    base64_decode(&b64_content)
        .map_err(|e| SysError::InvalidArgument(format!("Invalid PEM base64: {}", e)))
}

/// Minimal ASN.1 DER parser: read a tag-length-value.
/// Returns `(tag, header_len, value_len)`.
fn read_asn1_tl(data: &[u8]) -> std::result::Result<(u8, usize, usize), String> {
    if data.is_empty() {
        return Err("Empty ASN.1 data".into());
    }
    let tag = data[0];
    if data.len() < 2 {
        return Err("ASN.1 data too short for length".into());
    }
    let first_len = data[1] as usize;
    if first_len < 0x80 {
        Ok((tag, 2, first_len))
    } else {
        let num_bytes = first_len & 0x7F;
        if num_bytes == 0 || num_bytes > 4 {
            return Err(format!("Unsupported ASN.1 length encoding: {} bytes", num_bytes));
        }
        if data.len() < 2 + num_bytes {
            return Err("ASN.1 data too short for multi-byte length".into());
        }
        let mut len: usize = 0;
        for i in 0..num_bytes {
            len = (len << 8) | (data[2 + i] as usize);
        }
        Ok((tag, 2 + num_bytes, len))
    }
}

/// Extract the SubjectPublicKeyInfo (SPKI) field from a DER-encoded X.509 certificate.
///
/// X.509 structure (simplified):
/// ```text
/// SEQUENCE {              -- Certificate
///   SEQUENCE {            -- TBSCertificate
///     [0] EXPLICIT ...    -- version (optional)
///     INTEGER             -- serialNumber
///     SEQUENCE            -- signature algorithm
///     SEQUENCE            -- issuer
///     SEQUENCE            -- validity
///     SEQUENCE            -- subject
///     SEQUENCE {          -- subjectPublicKeyInfo  <-- we want this
///       SEQUENCE          -- algorithm
///       BIT STRING        -- subjectPublicKey
///     }
///     ...
///   }
///   ...
/// }
/// ```
fn extract_spki_from_der(der: &[u8]) -> Result<Vec<u8>> {
    let map_err = |e: String| SysError::InvalidArgument(format!("ASN.1 parse error: {}", e));

    // Outer SEQUENCE (Certificate)
    let (tag, hdr, _len) = read_asn1_tl(der).map_err(map_err)?;
    if tag != 0x30 {
        return Err(SysError::InvalidArgument("Expected SEQUENCE for Certificate".into()));
    }
    let tbs_start = hdr;

    // TBSCertificate SEQUENCE
    let (tag, hdr2, tbs_len) = read_asn1_tl(&der[tbs_start..]).map_err(map_err)?;
    if tag != 0x30 {
        return Err(SysError::InvalidArgument("Expected SEQUENCE for TBSCertificate".into()));
    }

    let mut pos = tbs_start + hdr2;
    let tbs_end = pos + tbs_len;

    // Field 0: optional explicit tag [0] for version
    if pos < tbs_end && der[pos] == 0xA0 {
        let (_tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
        pos += h + l;
    }

    // Field 1: serialNumber (INTEGER)
    let (_tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
    pos += h + l;

    // Field 2: signature algorithm (SEQUENCE)
    let (_tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
    pos += h + l;

    // Field 3: issuer (SEQUENCE)
    let (_tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
    pos += h + l;

    // Field 4: validity (SEQUENCE)
    let (_tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
    pos += h + l;

    // Field 5: subject (SEQUENCE)
    let (_tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
    pos += h + l;

    // Field 6: subjectPublicKeyInfo (SEQUENCE) — this is what we want
    if pos >= tbs_end {
        return Err(SysError::InvalidArgument(
            "Certificate too short: could not find SubjectPublicKeyInfo".into(),
        ));
    }
    let (tag, h, l) = read_asn1_tl(&der[pos..]).map_err(map_err)?;
    if tag != 0x30 {
        return Err(SysError::InvalidArgument(
            "Expected SEQUENCE for SubjectPublicKeyInfo".into(),
        ));
    }

    // Return the entire TLV of the SPKI field (tag + length + value)
    Ok(der[pos..pos + h + l].to_vec())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// A self-signed test certificate generated with:
    /// `openssl req -x509 -newkey rsa:2048 -keyout /dev/null -out - -days 365 -nodes -subj '/CN=test'`
    const TEST_PEM: &str = "-----BEGIN CERTIFICATE-----
MIICpDCCAYwCCQDMRJrfOVMBaTANBgkqhkiG9w0BAQsFADAUMRIwEAYDVQQDDAls
b2NhbGhvc3QwHhcNMjUwMTAxMDAwMDAwWhcNMjYwMTAxMDAwMDAwWjAUMRIwEAYD
VQQDDAlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC7
o4qne60TB3pOYaBy2VDCM2GnYMg9YJKD7GTbfM0kXwMGnbbHk0YBnq0VpMCOBCb
m6PH/0D4PPnBHVGAHaU8PCkR9UoX3UQWbqXOhIF2yFswQL5RfbFNiVN1qKueM1oN
mVVBCAqAL0aGMU7H9cXJEmDJIUCaiLeQyB5+B4o71hDDRLBf4BKMwBTIUTwxBOqR
Hy4Jt3o1dv6kBttXBKRJYFJfXrGJKXxocWP7LNYoYEu/JxqI3MKbYJ3bG6jnFEyB
/z9pV6/qVfXPjEkPMvzTwZ7GRrmGOL4E0WDBpVoE+HF1C0Vi8eT3nfHjsYXHfEjr
S1aNQIGl7+78iBaKfTMBAgMBAAEwDQYJKoZIhvcNAQELBQADggEBAI8bR+b/ee+C
TQzAT6MkjXdNpOc5bsAXjfU0Cqh3ANUHQP39no9yvdCUoQ7eFb7PGlJjpKq1jDZ
smbjZGnibSFlJ9mMXv/jUqLEl9aIQJGsN3Q2c0BPblK3h16CHqNOiaew2uAvDjUJ
9nGtiDiRFwSAhY/OAr4qdVlSJfGxSNGMErKTkKiLqXCb8SGYm2F8WPHX8DQ1bR1q
Ifl4xQmc3hhZVihOjTHpGMhJBJILEl3aJyAFGVYExDR3Ip5k0M1GjSBJPEWmNjhO
2RZNiF7BOJpGBgDOmOPMLtQHANOp5Frgf2tHJfgdkZaOmFj3C+k3G21vrWypqbLq
jBmk0VlYRfk=
-----END CERTIFICATE-----";

    fn make_pin_set() -> CertPinSet {
        CertPinSet {
            pins: vec![
                PinnedCert {
                    host: "api.openai.com".to_string(),
                    pin_sha256: vec!["abc123def456abc123def456abc123de".to_string()],
                    expires: None,
                    backup_pins: vec!["backup_pin_value_32bytes_base64x".to_string()],
                },
                PinnedCert {
                    host: "api.anthropic.com".to_string(),
                    pin_sha256: vec!["pin_anthropic_primary_sha256hash".to_string()],
                    expires: Some(Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap()),
                    backup_pins: vec![],
                },
            ],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        }
    }

    fn make_valid_b64_pin() -> String {
        // 32 zero bytes base64-encoded
        base64_encode(&[0u8; 32])
    }

    // ---- Pin format validation ----

    #[test]
    fn test_validate_pin_format_valid() {
        let pin = make_valid_b64_pin();
        assert!(validate_pin_format(&pin).is_ok());
    }

    #[test]
    fn test_validate_pin_format_empty() {
        assert!(validate_pin_format("").is_err());
    }

    #[test]
    fn test_validate_pin_format_invalid_base64() {
        assert!(validate_pin_format("not!valid@base64").is_err());
    }

    #[test]
    fn test_validate_pin_format_wrong_length() {
        // 16 bytes instead of 32
        let pin = base64_encode(&[0u8; 16]);
        assert!(validate_pin_format(&pin).is_err());
    }

    #[test]
    fn test_validate_pin_format_too_long() {
        let pin = base64_encode(&[0u8; 64]);
        assert!(validate_pin_format(&pin).is_err());
    }

    // ---- verify_pin ----

    #[test]
    fn test_verify_pin_match_primary() {
        let pin_set = make_pin_set();
        let result = verify_pin("api.openai.com", "abc123def456abc123def456abc123de", &pin_set);
        assert_eq!(result, CertPinResult::Valid);
    }

    #[test]
    fn test_verify_pin_match_backup() {
        let pin_set = make_pin_set();
        let result = verify_pin("api.openai.com", "backup_pin_value_32bytes_base64x", &pin_set);
        assert_eq!(result, CertPinResult::Valid);
    }

    #[test]
    fn test_verify_pin_mismatch() {
        let pin_set = make_pin_set();
        let result = verify_pin("api.openai.com", "wrong_pin", &pin_set);
        match result {
            CertPinResult::PinMismatch { host, actual, expected } => {
                assert_eq!(host, "api.openai.com");
                assert_eq!(actual, "wrong_pin");
                assert_eq!(expected.len(), 2); // primary + backup
            }
            other => panic!("Expected PinMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_verify_pin_no_config() {
        let pin_set = make_pin_set();
        let result = verify_pin("unknown.example.com", "some_pin", &pin_set);
        assert_eq!(
            result,
            CertPinResult::NoPinConfigured {
                host: "unknown.example.com".to_string()
            }
        );
    }

    #[test]
    fn test_verify_pin_expired() {
        let pin_set = CertPinSet {
            pins: vec![PinnedCert {
                host: "expired.example.com".to_string(),
                pin_sha256: vec!["some_pin".to_string()],
                expires: Some(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
                backup_pins: vec![],
            }],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        let result = verify_pin("expired.example.com", "some_pin", &pin_set);
        assert_eq!(
            result,
            CertPinResult::Expired {
                host: "expired.example.com".to_string()
            }
        );
    }

    #[test]
    fn test_verify_pin_not_expired_yet() {
        let pin_set = CertPinSet {
            pins: vec![PinnedCert {
                host: "future.example.com".to_string(),
                pin_sha256: vec!["correct_pin".to_string()],
                expires: Some(Utc.with_ymd_and_hms(2099, 12, 31, 23, 59, 59).unwrap()),
                backup_pins: vec![],
            }],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        let result = verify_pin("future.example.com", "correct_pin", &pin_set);
        assert_eq!(result, CertPinResult::Valid);
    }

    // ---- SPKI computation ----

    #[test]
    fn test_compute_spki_pin_valid_pem() {
        let pin = compute_spki_pin(TEST_PEM);
        assert!(pin.is_ok(), "compute_spki_pin failed: {:?}", pin.err());
        let pin = pin.unwrap();
        // Should be base64 of a SHA-256 hash (44 chars with padding)
        assert_eq!(pin.len(), 44, "SPKI pin should be 44 base64 chars, got {}", pin.len());
    }

    #[test]
    fn test_compute_spki_pin_deterministic() {
        let pin1 = compute_spki_pin(TEST_PEM).unwrap();
        let pin2 = compute_spki_pin(TEST_PEM).unwrap();
        assert_eq!(pin1, pin2, "SPKI pin should be deterministic");
    }

    #[test]
    fn test_compute_spki_pin_invalid_pem() {
        let result = compute_spki_pin("not a PEM certificate");
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_spki_pin_empty() {
        let result = compute_spki_pin("");
        assert!(result.is_err());
    }

    // ---- Pin header generation ----

    #[test]
    fn test_generate_pin_header_basic() {
        let pin_set = make_pin_set();
        let config = CertPinConfig {
            pin_file: PathBuf::from("/etc/agnos/pins.json"),
            enforce: true,
            report_uri: None,
            include_subdomains: false,
            max_age_secs: 86400,
        };
        let header = generate_pin_header(&config, &pin_set, "api.openai.com");
        assert!(header.is_some());
        let header = header.unwrap();
        assert!(header.contains("pin-sha256="));
        assert!(header.contains("max-age=86400"));
    }

    #[test]
    fn test_generate_pin_header_with_subdomains_and_report() {
        let pin_set = make_pin_set();
        let config = CertPinConfig {
            pin_file: PathBuf::from("/etc/agnos/pins.json"),
            enforce: true,
            report_uri: Some("https://report.agnos.dev/pins".to_string()),
            include_subdomains: true,
            max_age_secs: 2592000,
        };
        let header = generate_pin_header(&config, &pin_set, "api.openai.com");
        assert!(header.is_some());
        let h = header.unwrap();
        assert!(h.contains("includeSubDomains"));
        assert!(h.contains("report-uri="));
    }

    #[test]
    fn test_generate_pin_header_unknown_host() {
        let pin_set = make_pin_set();
        let config = CertPinConfig {
            pin_file: PathBuf::from("/etc/agnos/pins.json"),
            enforce: true,
            report_uri: None,
            include_subdomains: false,
            max_age_secs: 86400,
        };
        assert!(generate_pin_header(&config, &pin_set, "unknown.com").is_none());
    }

    // ---- Serialization roundtrip ----

    #[test]
    fn test_pin_set_serialization_roundtrip() {
        let original = make_pin_set();
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CertPinSet = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_cert_pin_config_serialization() {
        let config = CertPinConfig {
            pin_file: PathBuf::from("/etc/agnos/pins.json"),
            enforce: true,
            report_uri: Some("https://report.example.com".to_string()),
            include_subdomains: true,
            max_age_secs: 86400,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CertPinConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_cert_info_serialization() {
        let info = CertInfo {
            subject: "CN=test".to_string(),
            issuer: "CN=test".to_string(),
            serial: "01".to_string(),
            not_before: "Jan  1 00:00:00 2025 GMT".to_string(),
            not_after: "Jan  1 00:00:00 2026 GMT".to_string(),
            sha256_fingerprint: "aabbccdd".to_string(),
            spki_sha256: "test_pin".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: CertInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);
    }

    // ---- Expiry checking ----

    #[test]
    fn test_check_pin_expiry_none_expiring() {
        let pin_set = CertPinSet {
            pins: vec![PinnedCert {
                host: "far-future.com".to_string(),
                pin_sha256: vec!["pin".to_string()],
                expires: Some(Utc.with_ymd_and_hms(2099, 12, 31, 0, 0, 0).unwrap()),
                backup_pins: vec![],
            }],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        assert!(check_pin_expiry(&pin_set).is_empty());
    }

    #[test]
    fn test_check_pin_expiry_already_expired() {
        let pin_set = CertPinSet {
            pins: vec![PinnedCert {
                host: "expired.com".to_string(),
                pin_sha256: vec!["pin".to_string()],
                expires: Some(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
                backup_pins: vec![],
            }],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        let expiring = check_pin_expiry(&pin_set);
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].host, "expired.com");
    }

    #[test]
    fn test_check_pin_expiry_no_expiry_set() {
        let pin_set = CertPinSet {
            pins: vec![PinnedCert {
                host: "no-expiry.com".to_string(),
                pin_sha256: vec!["pin".to_string()],
                expires: None,
                backup_pins: vec![],
            }],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        assert!(check_pin_expiry(&pin_set).is_empty());
    }

    // ---- Default pins ----

    #[test]
    fn test_default_agnos_pins_has_known_hosts() {
        let pins = default_agnos_pins();
        assert_eq!(pins.version, 1);
        assert!(!pins.enforce, "Default pins should be report-only");
        let hosts: Vec<&str> = pins.pins.iter().map(|p| p.host.as_str()).collect();
        assert!(hosts.contains(&"api.openai.com"));
        assert!(hosts.contains(&"api.anthropic.com"));
        assert!(hosts.contains(&"generativelanguage.googleapis.com"));
    }

    #[test]
    fn test_default_agnos_pins_have_backup() {
        let pins = default_agnos_pins();
        for entry in &pins.pins {
            assert!(
                !entry.backup_pins.is_empty(),
                "Host {} should have at least one backup pin",
                entry.host
            );
        }
    }

    // ---- OpenSSL output parsing ----

    #[test]
    fn test_parse_openssl_cert_full() {
        let output = "\
subject=CN = api.openai.com
issuer=C = US, O = Let's Encrypt, CN = R3
serial=04A1B2C3D4E5F60718293A4B5C6D7E8F
notBefore=Mar  1 00:00:00 2026 GMT
notAfter=May 30 00:00:00 2026 GMT
SHA256 Fingerprint=AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89";

        let info = parse_openssl_cert(output).unwrap();
        assert_eq!(info.subject, "CN = api.openai.com");
        assert_eq!(info.issuer, "C = US, O = Let's Encrypt, CN = R3");
        assert_eq!(info.serial, "04A1B2C3D4E5F60718293A4B5C6D7E8F");
        assert_eq!(info.not_before, "Mar  1 00:00:00 2026 GMT");
        assert_eq!(info.not_after, "May 30 00:00:00 2026 GMT");
        assert!(!info.sha256_fingerprint.is_empty());
        assert!(!info.sha256_fingerprint.contains(':'));
    }

    #[test]
    fn test_parse_openssl_cert_lowercase_fingerprint() {
        let output = "\
subject=CN = test
issuer=CN = test
serial=01
sha256 Fingerprint=AA:BB:CC:DD";

        let info = parse_openssl_cert(output).unwrap();
        assert_eq!(info.sha256_fingerprint, "aabbccdd");
    }

    #[test]
    fn test_parse_openssl_cert_empty_input() {
        let result = parse_openssl_cert("");
        assert!(result.is_err());
    }

    // ---- Base64 roundtrip ----

    #[test]
    fn test_base64_roundtrip() {
        let data = b"Hello, AGNOS certificate pinning!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_base64_sha256_length() {
        // SHA-256 is 32 bytes, base64 should be 44 chars
        let hash = [0u8; 32];
        let encoded = base64_encode(&hash);
        assert_eq!(encoded.len(), 44);
    }

    // ---- PEM parsing ----

    #[test]
    fn test_pem_to_der_valid() {
        let der = pem_to_der(TEST_PEM);
        assert!(der.is_ok());
        let der = der.unwrap();
        // DER should start with SEQUENCE tag (0x30)
        assert_eq!(der[0], 0x30);
    }

    #[test]
    fn test_pem_to_der_invalid() {
        assert!(pem_to_der("garbage data").is_err());
    }

    // ---- Save/Load roundtrip via temp file ----

    #[test]
    fn test_save_load_pin_set_roundtrip() {
        let original = make_pin_set();
        let dir = std::env::temp_dir();
        let path = dir.join("agnos_test_pin_set.json");

        save_pin_set(&original, &path).unwrap();
        let loaded = load_pin_set(&path).unwrap();
        assert_eq!(original, loaded);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_pin_set_missing_file() {
        let result = load_pin_set(Path::new("/nonexistent/path/pins.json"));
        assert!(result.is_err());
    }

    // ---- Edge cases ----

    #[test]
    fn test_verify_pin_empty_pin_set() {
        let pin_set = CertPinSet {
            pins: vec![],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        let result = verify_pin("any.host", "any_pin", &pin_set);
        assert_eq!(
            result,
            CertPinResult::NoPinConfigured {
                host: "any.host".to_string()
            }
        );
    }

    #[test]
    fn test_cert_pin_result_serialization() {
        let results = vec![
            CertPinResult::Valid,
            CertPinResult::PinMismatch {
                host: "h".into(),
                expected: vec!["e".into()],
                actual: "a".into(),
            },
            CertPinResult::Expired { host: "h".into() },
            CertPinResult::NoPinConfigured { host: "h".into() },
            CertPinResult::Error("err".into()),
        ];
        for r in &results {
            let json = serde_json::to_string(r).unwrap();
            let deser: CertPinResult = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, deser);
        }
    }

    #[test]
    fn test_multiple_primary_pins() {
        let pin_set = CertPinSet {
            pins: vec![PinnedCert {
                host: "multi.example.com".to_string(),
                pin_sha256: vec![
                    "pin_one".to_string(),
                    "pin_two".to_string(),
                    "pin_three".to_string(),
                ],
                expires: None,
                backup_pins: vec![],
            }],
            enforce: true,
            created_at: Utc::now(),
            version: 1,
        };
        assert_eq!(verify_pin("multi.example.com", "pin_two", &pin_set), CertPinResult::Valid);
        assert_eq!(verify_pin("multi.example.com", "pin_three", &pin_set), CertPinResult::Valid);
        assert!(matches!(
            verify_pin("multi.example.com", "pin_four", &pin_set),
            CertPinResult::PinMismatch { .. }
        ));
    }
}
