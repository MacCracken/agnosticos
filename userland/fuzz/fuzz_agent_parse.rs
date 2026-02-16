#![no_main]

use agnos_common::AgentConfig;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(config_str) = std::str::from_utf8(data) {
        if let Ok(config) = serde_json::from_str::<AgentConfig>(config_str) {
            // Validate config is well-formed
            let _ = config.name.len();
            let _ = config.agent_type;
        }
    }
});
