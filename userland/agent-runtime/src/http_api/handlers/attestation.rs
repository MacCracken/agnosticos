use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Attestation response returned by GET /v1/attestation.
///
/// Contains TPM PCR values, boot event log, sy-agnos-release metadata,
/// and an HMAC signature over the measurements. When TPM is not available,
/// returns `{ "tpm_available": false }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationResponse {
    /// Whether a TPM device was detected at boot.
    pub tpm_available: bool,
    /// Whether boot measurements were successfully extended into PCRs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<bool>,
    /// Current PCR values (8, 9, 10) read from the TPM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcr_values: Option<Vec<PcrValue>>,
    /// Boot event log entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_log: Option<serde_json::Value>,
    /// Contents of /etc/sy-agnos-release.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_metadata: Option<serde_json::Value>,
    /// HMAC-SHA256 signature over the concatenated PCR values, keyed by
    /// a host-unique machine-id. Allows SY to detect replay/tampering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// A single PCR value returned in the attestation response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcrValue {
    pub index: u32,
    pub bank: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /v1/attestation — return signed TPM boot measurements.
///
/// Reads:
///   - /var/log/agnos/tpm-event-log.json  (boot event log)
///   - /etc/sy-agnos-release              (release metadata)
///   - tpm2_pcrread sha256:8,9,10         (current PCR values)
///
/// If TPM is not available, returns `{ "tpm_available": false }`.
pub async fn attestation_handler(State(_state): State<ApiState>) -> impl IntoResponse {
    // Read the event log
    let event_log = read_event_log();

    // Check if TPM was available at boot
    let tpm_available = event_log
        .as_ref()
        .and_then(|v| v.get("tpm_available"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !tpm_available {
        return (
            StatusCode::OK,
            Json(AttestationResponse {
                tpm_available: false,
                measured: None,
                pcr_values: None,
                event_log: None,
                release_metadata: None,
                signature: None,
            }),
        );
    }

    let measured = event_log
        .as_ref()
        .and_then(|v| v.get("measured"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Read current PCR values via tpm2_pcrread
    let pcr_values = read_pcr_values().await;

    // Read release metadata
    let release_metadata = read_release_metadata();

    // Compute HMAC signature over PCR values for tamper detection
    let signature = compute_attestation_signature(&pcr_values);

    (
        StatusCode::OK,
        Json(AttestationResponse {
            tpm_available: true,
            measured: Some(measured),
            pcr_values: if pcr_values.is_empty() {
                None
            } else {
                Some(pcr_values)
            },
            event_log,
            release_metadata,
            signature,
        }),
    )
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read the TPM event log from /var/log/agnos/tpm-event-log.json.
fn read_event_log() -> Option<serde_json::Value> {
    let content = std::fs::read_to_string("/var/log/agnos/tpm-event-log.json").ok()?;
    serde_json::from_str(&content).ok()
}

/// Read /etc/sy-agnos-release as JSON.
fn read_release_metadata() -> Option<serde_json::Value> {
    let content = std::fs::read_to_string("/etc/sy-agnos-release").ok()?;
    serde_json::from_str(&content).ok()
}

/// Read PCR 8, 9, 10 values via tpm2_pcrread.
async fn read_pcr_values() -> Vec<PcrValue> {
    // Try tpm2_pcrread first
    let output = tokio::process::Command::new("tpm2_pcrread")
        .arg("sha256:8,9,10")
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => parse_pcr_output(&String::from_utf8_lossy(&out.stdout)),
        _ => {
            // Fallback: try /sys/kernel/security/tpm0/pcrs (older kernels)
            read_pcr_from_sysfs().unwrap_or_default()
        }
    }
}

/// Parse tpm2_pcrread output into PcrValue structs.
///
/// Expected format:
/// ```text
///   sha256:
///     8 : 0xABCDEF...
///     9 : 0x012345...
///    10 : 0x6789AB...
/// ```
fn parse_pcr_output(output: &str) -> Vec<PcrValue> {
    let mut values = Vec::new();

    for index in [8, 9, 10] {
        let pattern = format!("{} :", index);
        let alt_pattern = format!("{}:", index);

        let hex_value = output
            .lines()
            .find(|line| {
                let trimmed = line.trim();
                trimmed.starts_with(&pattern) || trimmed.starts_with(&alt_pattern)
            })
            .and_then(|line| line.split(':').nth(1))
            .map(|v| v.trim().trim_start_matches("0x").to_lowercase())
            .unwrap_or_default();

        if !hex_value.is_empty() {
            values.push(PcrValue {
                index,
                bank: "sha256".to_string(),
                value: hex_value,
            });
        }
    }

    values
}

/// Fallback: read PCRs from /sys/kernel/security/tpm0/pcrs (legacy).
fn read_pcr_from_sysfs() -> Option<Vec<PcrValue>> {
    let content = std::fs::read_to_string("/sys/kernel/security/tpm0/pcrs").ok()?;
    let mut values = Vec::new();

    for index in [8u32, 9, 10] {
        let prefix = format!("PCR-{:02}:", index);
        if let Some(line) = content.lines().find(|l| l.starts_with(&prefix)) {
            if let Some(hex) = line.split(':').nth(1) {
                values.push(PcrValue {
                    index,
                    bank: "sha1".to_string(),
                    value: hex.trim().to_lowercase(),
                });
            }
        }
    }

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

/// Compute HMAC-SHA256 over concatenated PCR values using a host-unique key.
///
/// The key is derived from `/etc/machine-id` combined with the dm-verity root
/// hash from `/etc/agnos/verity-root-hash` (if available). This makes the key
/// unique per host AND per verified root filesystem, preventing forgery across
/// different installations.
///
/// Long-term fix: seal the HMAC key into the TPM (TPM2_Seal) so it cannot be
/// extracted by user-space at all. This is planned for post-beta.
fn compute_attestation_signature(pcr_values: &[PcrValue]) -> Option<String> {
    // Use machine-id as base HMAC key (unique per host, stable across reboots)
    let machine_id = std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if machine_id.is_empty() || pcr_values.is_empty() {
        return None;
    }

    // Append the dm-verity root hash if available, making the key
    // machine-specific + image-specific (not just /etc/machine-id which is
    // world-readable and predictable).
    let verity_hash = std::fs::read_to_string("/etc/agnos/verity-root-hash")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let key_material = if verity_hash.is_empty() {
        machine_id
    } else {
        format!("{}:{}", machine_id, verity_hash)
    };

    // Concatenate all PCR values as the message
    let message: String = pcr_values.iter().map(|p| p.value.as_str()).collect();

    Some(hmac_sha256_hex(key_material.as_bytes(), message.as_bytes()))
}

/// Compute HMAC-SHA256 using the `hmac` and `sha2` crates.
fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(message);
    let result = mac.finalize();
    let bytes = result.into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attestation_response_no_tpm() {
        let resp = AttestationResponse {
            tpm_available: false,
            measured: None,
            pcr_values: None,
            event_log: None,
            release_metadata: None,
            signature: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"tpm_available\":false"));
        // Optional fields should be absent
        assert!(!json.contains("measured"));
        assert!(!json.contains("pcr_values"));
    }

    #[test]
    fn test_attestation_response_with_tpm() {
        let resp = AttestationResponse {
            tpm_available: true,
            measured: Some(true),
            pcr_values: Some(vec![PcrValue {
                index: 8,
                bank: "sha256".to_string(),
                value: "abcd".repeat(16),
            }]),
            event_log: Some(serde_json::json!({"events": []})),
            release_metadata: Some(serde_json::json!({"strength": 88})),
            signature: Some("deadbeef".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"tpm_available\":true"));
        assert!(json.contains("\"measured\":true"));
        assert!(json.contains("\"index\":8"));
    }

    #[test]
    fn test_parse_pcr_output_typical() {
        let output = "  sha256:\n    8 : 0xABCDEF01\n    9 : 0x12345678\n   10 : 0xDEADBEEF\n";
        let values = parse_pcr_output(output);
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].index, 8);
        assert_eq!(values[0].value, "abcdef01");
        assert_eq!(values[1].index, 9);
        assert_eq!(values[2].index, 10);
        assert_eq!(values[2].value, "deadbeef");
    }

    #[test]
    fn test_parse_pcr_output_empty() {
        let values = parse_pcr_output("");
        assert!(values.is_empty());
    }

    #[test]
    fn test_parse_pcr_output_partial() {
        let output = "  sha256:\n    8 : 0xAAAA\n";
        let values = parse_pcr_output(output);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].index, 8);
    }

    #[test]
    fn test_pcr_value_serde_roundtrip() {
        let val = PcrValue {
            index: 10,
            bank: "sha256".to_string(),
            value: "ff".repeat(32),
        };
        let json = serde_json::to_string(&val).unwrap();
        let back: PcrValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back.index, 10);
        assert_eq!(back.bank, "sha256");
        assert_eq!(back.value, val.value);
    }

    #[test]
    fn test_hmac_sha256_deterministic() {
        let sig1 = hmac_sha256_hex(b"key", b"message");
        let sig2 = hmac_sha256_hex(b"key", b"message");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_hmac_sha256_different_keys() {
        let sig1 = hmac_sha256_hex(b"key1", b"message");
        let sig2 = hmac_sha256_hex(b"key2", b"message");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_hmac_sha256_different_messages() {
        let sig1 = hmac_sha256_hex(b"key", b"message1");
        let sig2 = hmac_sha256_hex(b"key", b"message2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_compute_signature_empty_pcr_values() {
        let sig = compute_attestation_signature(&[]);
        assert!(sig.is_none());
    }

    #[test]
    fn test_event_log_parse() {
        let json = r#"{"tpm_available":true,"measured":true,"events":[]}"#;
        let val: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(val["tpm_available"].as_bool(), Some(true));
        assert_eq!(val["measured"].as_bool(), Some(true));
    }

    #[test]
    fn test_release_metadata_parse() {
        let json = r#"{"version":"2026.3.18","tpm_measured":true,"strength":88}"#;
        let val: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(val["strength"].as_u64(), Some(88));
        assert_eq!(val["tpm_measured"].as_bool(), Some(true));
    }
}
