//! UEFI Secure Boot integration for AGNOS
//!
//! Manages secure boot state, key enrollment, and kernel/module signing.
//! Shells out to `mokutil`, `kmodsign`, and `modinfo` where needed.
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Secure Boot state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecureBootState {
    /// Secure Boot is enabled and enforcing.
    Enabled,
    /// Secure Boot is disabled.
    Disabled,
    /// Firmware is in Setup Mode (keys can be enrolled without authentication).
    SetupMode,
    /// System does not support UEFI Secure Boot (legacy BIOS, non-EFI).
    NotSupported,
}

impl SecureBootState {
    pub fn as_str(&self) -> &str {
        match self {
            SecureBootState::Enabled => "enabled",
            SecureBootState::Disabled => "disabled",
            SecureBootState::SetupMode => "setup_mode",
            SecureBootState::NotSupported => "not_supported",
        }
    }

    /// Whether enforcement is active.
    pub fn is_enforcing(&self) -> bool {
        matches!(self, SecureBootState::Enabled)
    }
}

impl std::fmt::Display for SecureBootState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An enrolled MOK (Machine Owner Key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrolledKey {
    /// Subject / Common Name.
    pub subject: String,
    /// Issuer.
    pub issuer: String,
    /// SHA-1 fingerprint (hex).
    pub fingerprint: String,
    /// Not-before date (text).
    pub not_before: String,
    /// Not-after date (text).
    pub not_after: String,
}

/// Information about a kernel module's signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSignatureInfo {
    /// Module file path.
    pub module_path: PathBuf,
    /// Whether the module has a signature attached.
    pub has_signature: bool,
    /// Signer name (if available).
    pub signer: Option<String>,
    /// Signature algorithm (if available).
    pub sig_algorithm: Option<String>,
}

/// An EFI variable read from sysfs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EfiVariable {
    /// Variable name (e.g. `SecureBoot-8be4df61-...`).
    pub name: String,
    /// Raw hex-encoded value.
    pub value_hex: String,
    /// Size in bytes.
    pub size: usize,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Get the current Secure Boot state.
///
/// Reads `/sys/firmware/efi/efivars/SecureBoot-*` or falls back to `mokutil`.
pub fn get_secureboot_status() -> Result<SecureBootState> {
    #[cfg(target_os = "linux")]
    {
        // First check if EFI is even available
        let efi_dir = Path::new("/sys/firmware/efi");
        if !efi_dir.exists() {
            return Ok(SecureBootState::NotSupported);
        }

        // Try reading from efivars
        let efivars_dir = Path::new("/sys/firmware/efi/efivars");
        if efivars_dir.exists() {
            if let Some(state) = read_secureboot_efivar(efivars_dir) {
                return Ok(state);
            }
        }

        // Fallback: mokutil --sb-state
        match run_command("mokutil", &["--sb-state"]) {
            Ok(output) => {
                let lower = output.to_lowercase();
                if lower.contains("secureboot enabled") {
                    Ok(SecureBootState::Enabled)
                } else if lower.contains("secureboot disabled") {
                    Ok(SecureBootState::Disabled)
                } else if lower.contains("setup mode") {
                    Ok(SecureBootState::SetupMode)
                } else {
                    Ok(SecureBootState::Disabled)
                }
            }
            Err(_) => {
                // mokutil not available; can't determine
                Ok(SecureBootState::NotSupported)
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Read the SecureBoot EFI variable from sysfs.
#[cfg(target_os = "linux")]
fn read_secureboot_efivar(efivars_dir: &Path) -> Option<SecureBootState> {
    // The variable is named SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c
    let sb_guid = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
    let sb_path = efivars_dir.join(format!("SecureBoot-{}", sb_guid));

    if !sb_path.exists() {
        return None;
    }

    let data = std::fs::read(&sb_path).ok()?;
    // EFI variable: 4-byte attributes + payload
    if data.len() < 5 {
        return None;
    }

    // The actual SecureBoot value is byte 4 (after the 4-byte attributes header)
    let value = data[4];
    if value == 1 {
        // Check SetupMode
        let setup_path = efivars_dir.join(format!("SetupMode-{}", sb_guid));
        if let Ok(setup_data) = std::fs::read(&setup_path) {
            if setup_data.len() >= 5 && setup_data[4] == 1 {
                return Some(SecureBootState::SetupMode);
            }
        }
        Some(SecureBootState::Enabled)
    } else {
        Some(SecureBootState::Disabled)
    }
}

/// List enrolled MOK (Machine Owner Key) certificates.
///
/// Uses `mokutil --list-enrolled`.
pub fn list_enrolled_keys() -> Result<Vec<EnrolledKey>> {
    #[cfg(target_os = "linux")]
    {
        let output = run_command("mokutil", &["--list-enrolled"])?;
        parse_mokutil_list(&output)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Parse mokutil --list-enrolled output into EnrolledKey structs.
///
/// Each key block looks like:
/// ```text
/// [key 1]
/// SHA1 Fingerprint: aa:bb:cc:...
///   Subject: CN=My Key
///   Issuer: CN=My CA
///   Valid from: ...
///   Valid until: ...
/// ```
pub fn parse_mokutil_list(output: &str) -> Result<Vec<EnrolledKey>> {
    let mut keys = Vec::new();
    let mut current_subject = String::new();
    let mut current_issuer = String::new();
    let mut current_fingerprint = String::new();
    let mut current_not_before = String::new();
    let mut current_not_after = String::new();
    let mut in_key = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("[key ") {
            // Save previous key if any
            if in_key && !current_fingerprint.is_empty() {
                keys.push(EnrolledKey {
                    subject: current_subject.clone(),
                    issuer: current_issuer.clone(),
                    fingerprint: current_fingerprint.clone(),
                    not_before: current_not_before.clone(),
                    not_after: current_not_after.clone(),
                });
            }
            current_subject.clear();
            current_issuer.clear();
            current_fingerprint.clear();
            current_not_before.clear();
            current_not_after.clear();
            in_key = true;
            continue;
        }

        if !in_key {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("SHA1 Fingerprint:") {
            current_fingerprint = rest.trim().replace(':', "").to_lowercase();
        } else if let Some(rest) = trimmed.strip_prefix("Subject:") {
            current_subject = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("Issuer:") {
            current_issuer = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("Valid from:") {
            current_not_before = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("Valid until:") {
            current_not_after = rest.trim().to_string();
        }
    }

    // Don't forget the last key
    if in_key && !current_fingerprint.is_empty() {
        keys.push(EnrolledKey {
            subject: current_subject,
            issuer: current_issuer,
            fingerprint: current_fingerprint,
            not_before: current_not_before,
            not_after: current_not_after,
        });
    }

    Ok(keys)
}

/// Enroll a DER-encoded certificate into the MOK list.
///
/// Uses `mokutil --import <path>`. A reboot is required to complete enrollment.
pub fn enroll_key(der_path: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if !der_path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Certificate file not found: {}",
                der_path.display()
            )));
        }

        let ext = der_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !["der", "cer", "crt", "pem"].contains(&ext) {
            return Err(SysError::InvalidArgument(format!(
                "Unexpected certificate extension '{}', expected .der/.cer/.crt/.pem",
                ext
            )));
        }

        run_command_checked("mokutil", &["--import", &der_path.to_string_lossy()])?;

        tracing::info!(
            "Enrolled key {} into MOK — reboot required to complete",
            der_path.display()
        );
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = der_path;
        Err(SysError::NotSupported)
    }
}

/// Sign a kernel module with a private key and certificate.
///
/// Uses `kmodsign` (or `/usr/src/linux-headers-*/scripts/sign-file`).
pub fn sign_kernel_module(
    module_path: &Path,
    private_key: &Path,
    certificate: &Path,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if !module_path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Module not found: {}",
                module_path.display()
            )));
        }
        if !private_key.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Private key not found: {}",
                private_key.display()
            )));
        }
        if !certificate.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Certificate not found: {}",
                certificate.display()
            )));
        }

        // Verify it looks like a .ko file
        let ext = module_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext != "ko" {
            return Err(SysError::InvalidArgument(format!(
                "Expected .ko kernel module, got '.{}'",
                ext
            )));
        }

        // Try kmodsign first, fall back to sign-file
        let sign_result = run_command(
            "kmodsign",
            &[
                "sha256",
                &private_key.to_string_lossy(),
                &certificate.to_string_lossy(),
                &module_path.to_string_lossy(),
            ],
        );

        if sign_result.is_err() {
            // Fallback: sign-file from kernel headers
            run_command_checked(
                "/usr/lib/modules-load.d/../linux/scripts/sign-file",
                &[
                    "sha256",
                    &private_key.to_string_lossy(),
                    &certificate.to_string_lossy(),
                    &module_path.to_string_lossy(),
                ],
            )?;
        }

        tracing::info!("Signed kernel module: {}", module_path.display());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (module_path, private_key, certificate);
        Err(SysError::NotSupported)
    }
}

/// Verify a kernel module's signature using `modinfo`.
pub fn verify_module_signature(module_path: &Path) -> Result<ModuleSignatureInfo> {
    #[cfg(target_os = "linux")]
    {
        if !module_path.exists() {
            return Err(SysError::InvalidArgument(format!(
                "Module not found: {}",
                module_path.display()
            )));
        }

        let output = run_command("modinfo", &[&module_path.to_string_lossy()])?;

        let has_signature = output.contains("sig_id:") || output.contains("signature:");
        let signer = output
            .lines()
            .find(|l| l.starts_with("signer:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string());
        let sig_algorithm = output
            .lines()
            .find(|l| l.starts_with("sig_hashalgo:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string());

        Ok(ModuleSignatureInfo {
            module_path: module_path.to_path_buf(),
            has_signature,
            signer,
            sig_algorithm,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = module_path;
        Err(SysError::NotSupported)
    }
}

/// List relevant EFI variables from sysfs.
pub fn get_efi_variables() -> Result<Vec<EfiVariable>> {
    #[cfg(target_os = "linux")]
    {
        let efivars_dir = Path::new("/sys/firmware/efi/efivars");
        if !efivars_dir.exists() {
            return Err(SysError::Unknown(
                "EFI variables directory not found; is this a UEFI system?".into(),
            ));
        }

        let relevant_prefixes = [
            "SecureBoot-",
            "SetupMode-",
            "PK-",
            "KEK-",
            "db-",
            "dbx-",
            "MokList",
        ];

        let mut variables = Vec::new();

        let entries = std::fs::read_dir(efivars_dir)
            .map_err(|e| SysError::Unknown(format!("Failed to read efivars: {}", e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| SysError::Unknown(format!("Failed to read efivars entry: {}", e)))?;
            let name = entry.file_name().to_string_lossy().to_string();

            if !relevant_prefixes.iter().any(|p| name.starts_with(p)) {
                continue;
            }

            let data = std::fs::read(entry.path()).unwrap_or_default();
            let value_hex = hex::encode(&data);
            let size = data.len();

            variables.push(EfiVariable {
                name,
                value_hex,
                size,
            });
        }

        variables.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(variables)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run a command and return its stdout.
#[cfg(target_os = "linux")]
fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
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

/// Run a command and check for success.
#[cfg(target_os = "linux")]
fn run_command_checked(cmd: &str, args: &[&str]) -> Result<()> {
    let _ = run_command(cmd, args)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- SecureBootState ---

    #[test]
    fn test_secureboot_state_as_str() {
        assert_eq!(SecureBootState::Enabled.as_str(), "enabled");
        assert_eq!(SecureBootState::Disabled.as_str(), "disabled");
        assert_eq!(SecureBootState::SetupMode.as_str(), "setup_mode");
        assert_eq!(SecureBootState::NotSupported.as_str(), "not_supported");
    }

    #[test]
    fn test_secureboot_state_display() {
        assert_eq!(format!("{}", SecureBootState::Enabled), "enabled");
        assert_eq!(format!("{}", SecureBootState::SetupMode), "setup_mode");
    }

    #[test]
    fn test_secureboot_state_is_enforcing() {
        assert!(SecureBootState::Enabled.is_enforcing());
        assert!(!SecureBootState::Disabled.is_enforcing());
        assert!(!SecureBootState::SetupMode.is_enforcing());
        assert!(!SecureBootState::NotSupported.is_enforcing());
    }

    #[test]
    fn test_secureboot_state_serde_roundtrip() {
        for state in &[
            SecureBootState::Enabled,
            SecureBootState::Disabled,
            SecureBootState::SetupMode,
            SecureBootState::NotSupported,
        ] {
            let json = serde_json::to_string(state).unwrap();
            let back: SecureBootState = serde_json::from_str(&json).unwrap();
            assert_eq!(*state, back);
        }
    }

    #[test]
    fn test_secureboot_state_clone_copy_eq() {
        let a = SecureBootState::Enabled;
        let b = a;
        let c = a;
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_ne!(SecureBootState::Enabled, SecureBootState::Disabled);
    }

    #[test]
    fn test_secureboot_state_debug() {
        assert_eq!(format!("{:?}", SecureBootState::Enabled), "Enabled");
        assert_eq!(format!("{:?}", SecureBootState::SetupMode), "SetupMode");
    }

    // --- parse_mokutil_list ---

    #[test]
    fn test_parse_mokutil_list_single_key() {
        let output = r#"[key 1]
SHA1 Fingerprint: aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd
        Subject: CN=AGNOS Signing Key
        Issuer: CN=AGNOS CA
        Valid from: Jan  1 00:00:00 2026 GMT
        Valid until: Dec 31 23:59:59 2030 GMT
"#;
        let keys = parse_mokutil_list(output).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].subject, "CN=AGNOS Signing Key");
        assert_eq!(keys[0].issuer, "CN=AGNOS CA");
        assert_eq!(
            keys[0].fingerprint,
            "aabbccddeeff00112233445566778899aabbccdd"
        );
        assert!(keys[0].not_before.contains("2026"));
        assert!(keys[0].not_after.contains("2030"));
    }

    #[test]
    fn test_parse_mokutil_list_multiple_keys() {
        let output = r#"[key 1]
SHA1 Fingerprint: aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd
        Subject: CN=Key One
        Issuer: CN=CA One
        Valid from: Jan 1 2026
        Valid until: Dec 31 2030
[key 2]
SHA1 Fingerprint: 11:22:33:44:55:66:77:88:99:00:aa:bb:cc:dd:ee:ff:11:22:33:44
        Subject: CN=Key Two
        Issuer: CN=CA Two
        Valid from: Feb 1 2026
        Valid until: Feb 1 2031
"#;
        let keys = parse_mokutil_list(output).unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].subject, "CN=Key One");
        assert_eq!(keys[1].subject, "CN=Key Two");
        assert_eq!(
            keys[1].fingerprint,
            "11223344556677889900aabbccddeeff11223344"
        );
    }

    #[test]
    fn test_parse_mokutil_list_empty() {
        let keys = parse_mokutil_list("").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_parse_mokutil_list_no_keys() {
        let output = "MokListRT is empty\n";
        let keys = parse_mokutil_list(output).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_parse_mokutil_list_partial_key() {
        // Key header without fingerprint → not included
        let output = "[key 1]\n        Subject: CN=Orphan\n";
        let keys = parse_mokutil_list(output).unwrap();
        assert!(keys.is_empty());
    }

    // --- EnrolledKey ---

    #[test]
    fn test_enrolled_key_serde_roundtrip() {
        let key = EnrolledKey {
            subject: "CN=Test".to_string(),
            issuer: "CN=CA".to_string(),
            fingerprint: "aabb".to_string(),
            not_before: "2026".to_string(),
            not_after: "2030".to_string(),
        };
        let json = serde_json::to_string(&key).unwrap();
        let back: EnrolledKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, back);
    }

    #[test]
    fn test_enrolled_key_clone_eq() {
        let key = EnrolledKey {
            subject: "CN=A".to_string(),
            issuer: "CN=B".to_string(),
            fingerprint: "ff".to_string(),
            not_before: "now".to_string(),
            not_after: "later".to_string(),
        };
        let cloned = key.clone();
        assert_eq!(key, cloned);
    }

    // --- ModuleSignatureInfo ---

    #[test]
    fn test_module_signature_info_serde() {
        let info = ModuleSignatureInfo {
            module_path: PathBuf::from("/lib/modules/test.ko"),
            has_signature: true,
            signer: Some("AGNOS".to_string()),
            sig_algorithm: Some("sha256".to_string()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ModuleSignatureInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }

    #[test]
    fn test_module_signature_info_no_sig() {
        let info = ModuleSignatureInfo {
            module_path: PathBuf::from("/tmp/unsigned.ko"),
            has_signature: false,
            signer: None,
            sig_algorithm: None,
        };
        assert!(!info.has_signature);
        assert!(info.signer.is_none());
    }

    // --- EfiVariable ---

    #[test]
    fn test_efi_variable_serde() {
        let var = EfiVariable {
            name: "SecureBoot-8be4df61".to_string(),
            value_hex: "0600000001".to_string(),
            size: 5,
        };
        let json = serde_json::to_string(&var).unwrap();
        let back: EfiVariable = serde_json::from_str(&json).unwrap();
        assert_eq!(var, back);
    }

    #[test]
    fn test_efi_variable_debug() {
        let var = EfiVariable {
            name: "PK-1234".to_string(),
            value_hex: "ff".to_string(),
            size: 1,
        };
        let dbg = format!("{:?}", var);
        assert!(dbg.contains("EfiVariable"));
        assert!(dbg.contains("PK-1234"));
    }

    // --- get_secureboot_status (safe to call) ---

    #[test]
    fn test_get_secureboot_status_no_crash() {
        // This may return Enabled, Disabled, or NotSupported depending on the
        // environment — just verify it does not panic.
        let _ = get_secureboot_status();
    }
}
