#![no_main]

use agnos_common::secrets::SecretValue;
use libfuzzer_sys::fuzz_target;

/// Fuzz secrets configuration parsing.
/// Ensures arbitrary JSON never causes panics in the secrets subsystem.
fuzz_target!(|data: &[u8]| {
    if let Ok(json_str) = std::str::from_utf8(data) {
        // Try to parse as SecretValue
        if let Ok(secret) = serde_json::from_str::<SecretValue>(json_str) {
            // Verify round-trip
            let serialized = serde_json::to_string(&secret).unwrap();
            let _roundtrip: SecretValue = serde_json::from_str(&serialized).unwrap();
            // Debug must not panic
            let _ = format!("{:?}", secret);
        }
    }
});
