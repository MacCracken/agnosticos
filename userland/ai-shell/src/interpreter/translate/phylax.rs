use anyhow::{anyhow, Result};

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_phylax(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::PhylaxScan { target, mode } => {
            let mode_str = mode.as_deref().unwrap_or("on_demand");
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/scan/file".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    format!(r#"{{"path":"{}","mode":"{}"}}"#, target, mode_str),
                ],
                description: format!("Scan {} for threats", target),
                permission: PermissionLevel::Admin,
                explanation: "Runs phylax threat detection engine on target file".to_string(),
            })
        }
        Intent::PhylaxFindings { severity } => {
            let query = severity
                .as_ref()
                .map(|s| format!("?severity={}", s))
                .unwrap_or_default();
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    format!("http://127.0.0.1:8090/v1/scan/history{}", query),
                ],
                description: "View threat scan findings".to_string(),
                permission: PermissionLevel::ReadOnly,
                explanation: "Retrieves phylax threat detection findings".to_string(),
            })
        }
        Intent::PhylaxHistory { .. } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "http://127.0.0.1:8090/v1/scan/history".to_string(),
            ],
            description: "View phylax scan history".to_string(),
            permission: PermissionLevel::ReadOnly,
            explanation: "Lists recent phylax scan results".to_string(),
        }),
        Intent::PhylaxStatus => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "http://127.0.0.1:8090/v1/scan/status".to_string(),
            ],
            description: "Get phylax scanner status".to_string(),
            permission: PermissionLevel::ReadOnly,
            explanation: "Shows phylax threat detection engine statistics".to_string(),
        }),
        Intent::PhylaxRules => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "http://127.0.0.1:8090/v1/scan/rules".to_string(),
            ],
            description: "List phylax detection rules".to_string(),
            permission: PermissionLevel::ReadOnly,
            explanation: "Lists all loaded YARA-compatible detection rules".to_string(),
        }),
        _ => Err(anyhow!("translate_phylax called with non-phylax intent")),
    }
}
