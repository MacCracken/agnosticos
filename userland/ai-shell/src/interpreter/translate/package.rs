use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_package(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::InstallPackage { packages } => {
            let mut args = vec!["install".to_string(), "-y".to_string()];
            args.extend(packages.iter().cloned());
            Ok(Translation {
                command: "apt-get".to_string(),
                args,
                description: format!("Install package(s): {}", packages.join(", ")),
                permission: PermissionLevel::SystemWrite,
                explanation: "Installs system packages (requires root)".to_string(),
            })
        }

        Intent::ArkInstall {
            packages,
            source: _,
        } => {
            let body = serde_json::json!({"packages": packages});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/ark/install".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!("Install packages via ark: {}", packages.join(", ")),
                permission: PermissionLevel::SystemWrite,
                explanation: "Installs packages using the AGNOS unified package manager"
                    .to_string(),
            })
        }

        Intent::ArkRemove { packages } => {
            let body = serde_json::json!({"packages": packages});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/ark/remove".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!("Remove packages via ark: {}", packages.join(", ")),
                permission: PermissionLevel::SystemWrite,
                explanation: "Removes packages using the AGNOS unified package manager".to_string(),
            })
        }

        Intent::ArkSearch { query } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                format!("http://127.0.0.1:8090/v1/ark/search?q={}", query),
            ],
            description: format!("Search packages via ark: {}", query),
            permission: PermissionLevel::Safe,
            explanation: "Searches for packages across all configured sources".to_string(),
        }),

        Intent::ArkInfo { package } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                format!("http://127.0.0.1:8090/v1/ark/info/{}", package),
            ],
            description: format!("Show ark package info: {}", package),
            permission: PermissionLevel::Safe,
            explanation: "Retrieves detailed information about a package".to_string(),
        }),

        Intent::ArkUpdate => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "-X".to_string(),
                "POST".to_string(),
                "http://127.0.0.1:8090/v1/ark/update".to_string(),
            ],
            description: "Check for package updates via ark".to_string(),
            permission: PermissionLevel::Safe,
            explanation: "Refreshes package index from all configured sources".to_string(),
        }),

        Intent::ArkUpgrade { packages } => {
            let body = serde_json::json!({"packages": packages});
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/ark/upgrade".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    serde_json::to_string(&body).unwrap(),
                ],
                description: format!(
                    "Upgrade packages via ark{}",
                    packages
                        .as_ref()
                        .map_or(String::new(), |p| format!(": {}", p.join(", ")))
                ),
                permission: PermissionLevel::SystemWrite,
                explanation: "Upgrades packages to latest versions".to_string(),
            })
        }

        Intent::ArkStatus => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "http://127.0.0.1:8090/v1/ark/status".to_string(),
            ],
            description: "Show ark package manager status".to_string(),
            permission: PermissionLevel::Safe,
            explanation: "Displays the status of the AGNOS unified package manager".to_string(),
        }),

        _ => unreachable!("translate_package called with non-package intent"),
    }
}
