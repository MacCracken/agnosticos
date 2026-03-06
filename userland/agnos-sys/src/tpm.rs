//! TPM 2.0 Integration for AGNOS
//!
//! Provides measured boot verification, sealed secrets, and PCR management
//! using the TPM 2.0 device at /dev/tpmrm0.
//!
//! Shells out to `tpm2-tools` commands (`tpm2_pcrread`, `tpm2_pcrextend`,
//! `tpm2_create`, `tpm2_load`, `tpm2_unseal`, `tpm2_getrandom`).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// TPM 2.0 PCR hash bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TpmPcrBank {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl TpmPcrBank {
    pub fn as_str(&self) -> &str {
        match self {
            TpmPcrBank::Sha1 => "sha1",
            TpmPcrBank::Sha256 => "sha256",
            TpmPcrBank::Sha384 => "sha384",
            TpmPcrBank::Sha512 => "sha512",
        }
    }

    /// Expected hex-encoded hash length for this bank.
    pub fn hash_hex_len(&self) -> usize {
        match self {
            TpmPcrBank::Sha1 => 40,
            TpmPcrBank::Sha256 => 64,
            TpmPcrBank::Sha384 => 96,
            TpmPcrBank::Sha512 => 128,
        }
    }
}

impl std::fmt::Display for TpmPcrBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single PCR value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TpmPcrValue {
    /// PCR index (0–23).
    pub index: u32,
    /// Hash bank.
    pub bank: TpmPcrBank,
    /// Hex-encoded PCR value.
    pub value: String,
}

/// Policy that describes which PCR values must match for seal/unseal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TpmPcrPolicy {
    /// PCR indices that must match.
    pub pcr_indices: Vec<u32>,
    /// Hash bank to use.
    pub bank: TpmPcrBank,
}

impl TpmPcrPolicy {
    pub fn new(bank: TpmPcrBank, pcr_indices: Vec<u32>) -> Result<Self> {
        for &idx in &pcr_indices {
            if idx > 23 {
                return Err(SysError::InvalidArgument(format!(
                    "PCR index {} out of range (0-23)",
                    idx
                )));
            }
        }
        if pcr_indices.is_empty() {
            return Err(SysError::InvalidArgument(
                "PCR policy must include at least one PCR index".into(),
            ));
        }
        Ok(Self { pcr_indices, bank })
    }

    /// Render the PCR selection string for tpm2-tools (e.g. `sha256:0,1,7`).
    pub fn pcr_selection(&self) -> String {
        let indices: Vec<String> = self.pcr_indices.iter().map(|i| i.to_string()).collect();
        format!("{}:{}", self.bank, indices.join(","))
    }
}

/// Sealed secret handle (context file path + policy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedSecret {
    /// Path to the sealed object context file.
    pub context_path: PathBuf,
    /// The PCR policy used during sealing.
    pub policy: TpmPcrPolicy,
}

/// Known-good PCR values for measured boot verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasuredBootBaseline {
    /// Expected PCR values (index → hex value).
    pub expected: Vec<TpmPcrValue>,
}

/// Handle to the TPM device.
#[derive(Debug)]
pub struct TpmDevice {
    /// Path to the TPM resource-manager device.
    pub device_path: PathBuf,
}

// ---------------------------------------------------------------------------
// TpmDevice implementation
// ---------------------------------------------------------------------------

impl TpmDevice {
    /// Default device path for the TPM 2.0 resource manager.
    pub const DEFAULT_DEVICE: &'static str = "/dev/tpmrm0";

    /// Open the TPM device, verifying it exists.
    pub fn open() -> Result<Self> {
        Self::open_path(Path::new(Self::DEFAULT_DEVICE))
    }

    /// Open a TPM device at a specific path.
    pub fn open_path(device_path: &Path) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            if !device_path.exists() {
                return Err(SysError::Unknown(format!(
                    "TPM device not found: {}",
                    device_path.display()
                )));
            }
            Ok(Self {
                device_path: device_path.to_path_buf(),
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = device_path;
            Err(SysError::NotSupported)
        }
    }
}

/// Check whether a TPM 2.0 device is available on this system.
pub fn tpm_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(TpmDevice::DEFAULT_DEVICE).exists()
            || Path::new("/dev/tpm0").exists()
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Read one or more PCR values.
///
/// Uses `tpm2_pcrread` to read the specified bank+indices.
pub fn read_pcr(bank: TpmPcrBank, indices: &[u32]) -> Result<Vec<TpmPcrValue>> {
    #[cfg(target_os = "linux")]
    {
        for &idx in indices {
            if idx > 23 {
                return Err(SysError::InvalidArgument(format!(
                    "PCR index {} out of range (0-23)",
                    idx
                )));
            }
        }
        if indices.is_empty() {
            return Err(SysError::InvalidArgument(
                "Must specify at least one PCR index".into(),
            ));
        }

        let idx_str: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
        let selection = format!("{}:{}", bank, idx_str.join(","));

        let output = run_tpm2_tool("tpm2_pcrread", &[&selection])?;

        parse_pcr_read_output(&output, bank, indices)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (bank, indices);
        Err(SysError::NotSupported)
    }
}

/// Parse tpm2_pcrread output into TpmPcrValue structs.
///
/// Expected format (YAML-like):
/// ```text
///   sha256:
///     0 : 0x0000...
///     7 : 0xABCD...
/// ```
pub fn parse_pcr_read_output(
    output: &str,
    bank: TpmPcrBank,
    indices: &[u32],
) -> Result<Vec<TpmPcrValue>> {
    let mut values = Vec::new();

    for &idx in indices {
        // Look for lines like "  0 : 0xABCDEF..." or "  0: 0xABCDEF..."
        let pattern_a = format!("{} :", idx);
        let pattern_b = format!("{}:", idx);

        let hex_value = output
            .lines()
            .find(|line| {
                let trimmed = line.trim();
                trimmed.starts_with(&pattern_a) || trimmed.starts_with(&pattern_b)
            })
            .and_then(|line| line.split(':').nth(1))
            .map(|v| v.trim().trim_start_matches("0x").to_lowercase())
            .unwrap_or_else(|| "0".repeat(bank.hash_hex_len()));

        values.push(TpmPcrValue {
            index: idx,
            bank,
            value: hex_value,
        });
    }

    Ok(values)
}

/// Extend a PCR with a measurement hash.
///
/// Uses `tpm2_pcrextend`.
pub fn extend_pcr(bank: TpmPcrBank, index: u32, hash: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if index > 23 {
            return Err(SysError::InvalidArgument(format!(
                "PCR index {} out of range (0-23)",
                index
            )));
        }

        if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SysError::InvalidArgument(
                "Hash must be a non-empty hex string".into(),
            ));
        }

        // tpm2_pcrextend <index>:<bank>=<hash>
        let arg = format!("{}:{}={}", index, bank, hash);
        run_tpm2_tool_checked("tpm2_pcrextend", &[&arg])?;

        tracing::info!("Extended PCR {} ({}) with hash", index, bank);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (bank, index, hash);
        Err(SysError::NotSupported)
    }
}

/// Seal data to specific PCR state.
///
/// Returns a `SealedSecret` handle that can be used with `unseal_secret`.
pub fn seal_secret(
    policy: &TpmPcrPolicy,
    data: &[u8],
    output_dir: &Path,
) -> Result<SealedSecret> {
    #[cfg(target_os = "linux")]
    {
        if data.is_empty() {
            return Err(SysError::InvalidArgument(
                "Cannot seal empty data".into(),
            ));
        }
        if data.len() > 2048 {
            return Err(SysError::InvalidArgument(
                "Sealed data too large (max 2048 bytes)".into(),
            ));
        }
        if !output_dir.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Output directory not found: {}",
                output_dir.display()
            )));
        }

        let data_file = output_dir.join("seal_input.bin");
        let ctx_file = output_dir.join("sealed.ctx");
        let pub_file = output_dir.join("sealed.pub");
        let priv_file = output_dir.join("sealed.priv");

        std::fs::write(&data_file, data).map_err(|e| {
            SysError::Unknown(format!("Failed to write seal input: {}", e))
        })?;

        let pcr_sel = policy.pcr_selection();

        // Create sealed object
        run_tpm2_tool_checked(
            "tpm2_create",
            &[
                "-C", "owner",
                "-i", &data_file.to_string_lossy(),
                "-u", &pub_file.to_string_lossy(),
                "-r", &priv_file.to_string_lossy(),
                "-L", &pcr_sel,
            ],
        )?;

        // Load into TPM
        run_tpm2_tool_checked(
            "tpm2_load",
            &[
                "-C", "owner",
                "-u", &pub_file.to_string_lossy(),
                "-r", &priv_file.to_string_lossy(),
                "-c", &ctx_file.to_string_lossy(),
            ],
        )?;

        // Clean up input
        let _ = std::fs::remove_file(&data_file);

        tracing::info!("Sealed {} bytes to PCR policy {}", data.len(), pcr_sel);

        Ok(SealedSecret {
            context_path: ctx_file,
            policy: policy.clone(),
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (policy, data, output_dir);
        Err(SysError::NotSupported)
    }
}

/// Unseal a secret; only succeeds if current PCR values match the policy.
pub fn unseal_secret(sealed: &SealedSecret) -> Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        if !sealed.context_path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Sealed context file not found: {}",
                sealed.context_path.display()
            )));
        }

        let pcr_sel = sealed.policy.pcr_selection();
        let output = run_tpm2_tool(
            "tpm2_unseal",
            &[
                "-c", &sealed.context_path.to_string_lossy(),
                "-p", &format!("pcr:{}", pcr_sel),
            ],
        )?;

        Ok(output.into_bytes())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = sealed;
        Err(SysError::NotSupported)
    }
}

/// Get hardware random bytes from the TPM.
pub fn get_random_bytes(count: usize) -> Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        if count == 0 {
            return Err(SysError::InvalidArgument(
                "Cannot request 0 random bytes".into(),
            ));
        }
        if count > 4096 {
            return Err(SysError::InvalidArgument(
                "Cannot request more than 4096 bytes at once".into(),
            ));
        }

        let output = run_tpm2_tool(
            "tpm2_getrandom",
            &["--hex", &count.to_string()],
        )?;

        let hex_str = output.trim();
        hex::decode(hex_str).map_err(|e| {
            SysError::Unknown(format!("Failed to decode tpm2_getrandom output: {}", e))
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = count;
        Err(SysError::NotSupported)
    }
}

/// Verify measured boot by comparing current PCR 0-7 against known-good values.
pub fn verify_measured_boot(baseline: &MeasuredBootBaseline) -> Result<bool> {
    #[cfg(target_os = "linux")]
    {
        if baseline.expected.is_empty() {
            return Err(SysError::InvalidArgument(
                "Baseline has no expected PCR values".into(),
            ));
        }

        // Group by bank
        let bank = baseline.expected[0].bank;
        let indices: Vec<u32> = baseline.expected.iter().map(|v| v.index).collect();

        let current = read_pcr(bank, &indices)?;

        for expected in &baseline.expected {
            let actual = current
                .iter()
                .find(|v| v.index == expected.index && v.bank == expected.bank);

            match actual {
                Some(val) if val.value == expected.value => continue,
                Some(val) => {
                    tracing::warn!(
                        "PCR {} ({}) mismatch: expected={}, actual={}",
                        expected.index,
                        expected.bank,
                        &expected.value[..16.min(expected.value.len())],
                        &val.value[..16.min(val.value.len())]
                    );
                    return Ok(false);
                }
                None => {
                    tracing::warn!(
                        "PCR {} ({}) not found in current readings",
                        expected.index,
                        expected.bank
                    );
                    return Ok(false);
                }
            }
        }

        tracing::info!("Measured boot verification PASSED");
        Ok(true)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = baseline;
        Err(SysError::NotSupported)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run a tpm2-tools command and return stdout.
#[cfg(target_os = "linux")]
fn run_tpm2_tool(cmd: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run {}: {}", cmd, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "{} {} failed: {}",
            cmd,
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a tpm2-tools command and just check for success.
#[cfg(target_os = "linux")]
fn run_tpm2_tool_checked(cmd: &str, args: &[&str]) -> Result<()> {
    let _ = run_tpm2_tool(cmd, args)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- TpmPcrBank ---

    #[test]
    fn test_pcr_bank_as_str() {
        assert_eq!(TpmPcrBank::Sha1.as_str(), "sha1");
        assert_eq!(TpmPcrBank::Sha256.as_str(), "sha256");
        assert_eq!(TpmPcrBank::Sha384.as_str(), "sha384");
        assert_eq!(TpmPcrBank::Sha512.as_str(), "sha512");
    }

    #[test]
    fn test_pcr_bank_hash_hex_len() {
        assert_eq!(TpmPcrBank::Sha1.hash_hex_len(), 40);
        assert_eq!(TpmPcrBank::Sha256.hash_hex_len(), 64);
        assert_eq!(TpmPcrBank::Sha384.hash_hex_len(), 96);
        assert_eq!(TpmPcrBank::Sha512.hash_hex_len(), 128);
    }

    #[test]
    fn test_pcr_bank_display() {
        assert_eq!(format!("{}", TpmPcrBank::Sha256), "sha256");
        assert_eq!(format!("{}", TpmPcrBank::Sha1), "sha1");
    }

    #[test]
    fn test_pcr_bank_serde_roundtrip() {
        for bank in &[
            TpmPcrBank::Sha1,
            TpmPcrBank::Sha256,
            TpmPcrBank::Sha384,
            TpmPcrBank::Sha512,
        ] {
            let json = serde_json::to_string(bank).unwrap();
            let back: TpmPcrBank = serde_json::from_str(&json).unwrap();
            assert_eq!(*bank, back);
        }
    }

    #[test]
    fn test_pcr_bank_clone_copy_eq() {
        let a = TpmPcrBank::Sha256;
        let b = a;
        let c = a.clone();
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_ne!(TpmPcrBank::Sha1, TpmPcrBank::Sha256);
    }

    // --- TpmPcrPolicy ---

    #[test]
    fn test_pcr_policy_new_valid() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0, 1, 7]).unwrap();
        assert_eq!(policy.pcr_indices, vec![0, 1, 7]);
        assert_eq!(policy.bank, TpmPcrBank::Sha256);
    }

    #[test]
    fn test_pcr_policy_new_empty_indices() {
        let err = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![]).unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn test_pcr_policy_new_index_out_of_range() {
        let err = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0, 24]).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn test_pcr_policy_selection_string() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0, 1, 7]).unwrap();
        assert_eq!(policy.pcr_selection(), "sha256:0,1,7");
    }

    #[test]
    fn test_pcr_policy_selection_single() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha1, vec![10]).unwrap();
        assert_eq!(policy.pcr_selection(), "sha1:10");
    }

    #[test]
    fn test_pcr_policy_serde_roundtrip() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0, 7]).unwrap();
        let json = serde_json::to_string(&policy).unwrap();
        let back: TpmPcrPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    // --- TpmPcrValue ---

    #[test]
    fn test_pcr_value_serde_roundtrip() {
        let val = TpmPcrValue {
            index: 7,
            bank: TpmPcrBank::Sha256,
            value: "abcdef01".to_string(),
        };
        let json = serde_json::to_string(&val).unwrap();
        let back: TpmPcrValue = serde_json::from_str(&json).unwrap();
        assert_eq!(val, back);
    }

    #[test]
    fn test_pcr_value_clone_eq() {
        let val = TpmPcrValue {
            index: 0,
            bank: TpmPcrBank::Sha1,
            value: "ff".repeat(20),
        };
        let cloned = val.clone();
        assert_eq!(val, cloned);
    }

    // --- parse_pcr_read_output ---

    #[test]
    fn test_parse_pcr_read_output_typical() {
        let output = r#"
  sha256:
    0 : 0xA1B2C3D4E5F60000000000000000000000000000000000000000000000000000
    7 : 0x0000000000000000000000000000000000000000000000000000000000000000
"#;
        let values =
            parse_pcr_read_output(output, TpmPcrBank::Sha256, &[0, 7]).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].index, 0);
        assert!(values[0].value.starts_with("a1b2c3d4"));
        assert_eq!(values[1].index, 7);
    }

    #[test]
    fn test_parse_pcr_read_output_missing_index() {
        let output = "  sha256:\n    0 : 0xABCD\n";
        let values =
            parse_pcr_read_output(output, TpmPcrBank::Sha256, &[0, 5]).unwrap();
        assert_eq!(values.len(), 2);
        // Index 5 not found → gets zero-filled default
        assert_eq!(values[1].value.len(), TpmPcrBank::Sha256.hash_hex_len());
        assert!(values[1].value.chars().all(|c| c == '0'));
    }

    #[test]
    fn test_parse_pcr_read_output_empty() {
        let values =
            parse_pcr_read_output("", TpmPcrBank::Sha1, &[0]).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].value.len(), TpmPcrBank::Sha1.hash_hex_len());
    }

    // --- MeasuredBootBaseline ---

    #[test]
    fn test_measured_boot_baseline_serde() {
        let baseline = MeasuredBootBaseline {
            expected: vec![
                TpmPcrValue {
                    index: 0,
                    bank: TpmPcrBank::Sha256,
                    value: "a".repeat(64),
                },
                TpmPcrValue {
                    index: 7,
                    bank: TpmPcrBank::Sha256,
                    value: "b".repeat(64),
                },
            ],
        };
        let json = serde_json::to_string(&baseline).unwrap();
        let back: MeasuredBootBaseline = serde_json::from_str(&json).unwrap();
        assert_eq!(baseline, back);
    }

    // --- SealedSecret ---

    #[test]
    fn test_sealed_secret_serde() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0, 7]).unwrap();
        let sealed = SealedSecret {
            context_path: PathBuf::from("/tmp/sealed.ctx"),
            policy,
        };
        let json = serde_json::to_string(&sealed).unwrap();
        let back: SealedSecret = serde_json::from_str(&json).unwrap();
        assert_eq!(back.context_path, PathBuf::from("/tmp/sealed.ctx"));
        assert_eq!(back.policy.pcr_indices, vec![0, 7]);
    }

    // --- Validation in functions ---

    #[test]
    fn test_pcr_policy_boundary_index_23() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![23]);
        assert!(policy.is_ok());
    }

    #[test]
    fn test_pcr_policy_boundary_index_0() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0]);
        assert!(policy.is_ok());
    }

    #[test]
    fn test_tpm_available_no_crash() {
        // Just verify it doesn't panic
        let _ = tpm_available();
    }

    // --- Debug output ---

    #[test]
    fn test_pcr_bank_debug() {
        assert_eq!(format!("{:?}", TpmPcrBank::Sha256), "Sha256");
        assert_eq!(format!("{:?}", TpmPcrBank::Sha384), "Sha384");
    }

    #[test]
    fn test_pcr_policy_debug() {
        let policy = TpmPcrPolicy::new(TpmPcrBank::Sha256, vec![0, 7]).unwrap();
        let dbg = format!("{:?}", policy);
        assert!(dbg.contains("TpmPcrPolicy"));
        assert!(dbg.contains("Sha256"));
    }
}
