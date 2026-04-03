#![no_main]

use agnostik::SecretMetadata;
use libfuzzer_sys::fuzz_target;

/// Fuzz secrets metadata parsing.
/// Ensures arbitrary JSON never causes panics in the secrets subsystem.
/// Note: agnostik::Secret is intentionally not Serialize/Deserialize,
/// so we fuzz SecretMetadata instead.
fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        // Try to parse as SecretMetadata
        if let Ok(meta) = serde_json::from_str::<SecretMetadata>(json_str) {
            // Verify round-trip
            let serialized = serde_json::to_string(&meta).unwrap();
            let _roundtrip: SecretMetadata = serde_json::from_str(&serialized).unwrap();
            // Debug must not panic
            let _ = format!("{:?}", meta);
        }
    }
});
