use anyhow::{anyhow, Result};

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_system(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::JournalView {
            unit,
            priority,
            lines,
            since,
        } => {
            let mut args = Vec::new();
            if let Some(u) = unit {
                args.push("-u".to_string());
                args.push(u.clone());
            }
            if let Some(p) = priority {
                args.push("-p".to_string());
                args.push(p.clone());
            }
            if let Some(n) = lines {
                args.push("-n".to_string());
                args.push(n.to_string());
            }
            if let Some(s) = since {
                args.push("--since".to_string());
                args.push(s.clone());
            }
            if args.is_empty() {
                // Default: show recent entries
                args.push("-n".to_string());
                args.push("50".to_string());
            }
            let desc = match (unit, priority) {
                (Some(u), Some(p)) => format!("Show {} priority journal logs for {}", p, u),
                (Some(u), None) => format!("Show journal logs for {}", u),
                (None, Some(p)) => format!("Show {} priority journal logs", p),
                (None, None) => "Show recent journal log entries".to_string(),
            };
            Ok(Translation {
                command: "journalctl".to_string(),
                args,
                description: desc.clone(),
                permission: PermissionLevel::ReadOnly,
                explanation: desc,
            })
        }

        Intent::DeviceInfo {
            subsystem,
            device_path,
        } => {
            let (args, desc) = if let Some(path) = device_path {
                (
                    vec![
                        "info".to_string(),
                        "--query=all".to_string(),
                        "--name".to_string(),
                        path.clone(),
                    ],
                    format!("Show device info for {}", path),
                )
            } else if let Some(sub) = subsystem {
                (
                    vec![
                        "info".to_string(),
                        "--subsystem-match".to_string(),
                        sub.clone(),
                    ],
                    format!("List {} devices", sub),
                )
            } else {
                (
                    vec!["info".to_string(), "--export-db".to_string()],
                    "List all devices".to_string(),
                )
            };
            Ok(Translation {
                command: "udevadm".to_string(),
                args,
                description: desc.clone(),
                permission: PermissionLevel::ReadOnly,
                explanation: desc,
            })
        }

        Intent::MountControl {
            action,
            mountpoint,
            filesystem,
        } => {
            let (command, args, desc, permission) = match action.as_str() {
                "list" => {
                    let mut a = Vec::new();
                    if let Some(fs) = filesystem {
                        a.push("-t".to_string());
                        a.push(fs.clone());
                    }
                    let d = if filesystem.is_some() {
                        format!("List {} mounts", filesystem.as_deref().unwrap_or("all"))
                    } else {
                        "List all mounted filesystems".to_string()
                    };
                    ("findmnt".to_string(), a, d, PermissionLevel::Safe)
                }
                "unmount" => {
                    let mp = mountpoint.as_deref().unwrap_or("/mnt");
                    (
                        "fusermount".to_string(),
                        vec!["-u".to_string(), mp.to_string()],
                        format!("Unmount {}", mp),
                        PermissionLevel::Admin,
                    )
                }
                "mount" => {
                    let fs = filesystem.as_deref().unwrap_or("");
                    let mp = mountpoint.as_deref().unwrap_or("/mnt");
                    (
                        "mount".to_string(),
                        vec![fs.to_string(), mp.to_string()],
                        format!("Mount {} on {}", fs, mp),
                        PermissionLevel::Admin,
                    )
                }
                other => {
                    return Err(anyhow!("Unknown mount action: {}", other));
                }
            };
            Ok(Translation {
                command,
                args,
                description: desc.clone(),
                permission,
                explanation: desc,
            })
        }

        Intent::BootConfig {
            action,
            entry,
            value,
        } => {
            let (args, desc, permission) = match action.as_str() {
                "list" => (
                    vec!["list".to_string()],
                    "List boot entries".to_string(),
                    PermissionLevel::ReadOnly,
                ),
                "default" => {
                    let e = entry.as_deref().unwrap_or("unknown");
                    (
                        vec!["set-default".to_string(), e.to_string()],
                        format!("Set default boot entry to {}", e),
                        PermissionLevel::Admin,
                    )
                }
                "timeout" => {
                    let v = value.as_deref().unwrap_or("5");
                    (
                        vec!["set-timeout".to_string(), v.to_string()],
                        format!("Set boot timeout to {}", v),
                        PermissionLevel::Admin,
                    )
                }
                other => {
                    return Err(anyhow!("Unknown boot config action: {}", other));
                }
            };
            Ok(Translation {
                command: "bootctl".to_string(),
                args,
                description: desc.clone(),
                permission,
                explanation: desc,
            })
        }

        Intent::SystemUpdate { action } => {
            let (args, desc, permission) = match action.as_str() {
                "check" => (
                    vec!["check".to_string()],
                    "Check for available system updates".to_string(),
                    PermissionLevel::Safe,
                ),
                "apply" => (
                    vec!["apply".to_string()],
                    "Apply system updates".to_string(),
                    PermissionLevel::Admin,
                ),
                "rollback" => (
                    vec!["rollback".to_string()],
                    "Rollback last system update".to_string(),
                    PermissionLevel::Admin,
                ),
                "status" => (
                    vec!["status".to_string()],
                    "Show current system version and update status".to_string(),
                    PermissionLevel::Safe,
                ),
                other => {
                    return Err(anyhow!("Unknown update action: {}", other));
                }
            };
            Ok(Translation {
                command: "agnos-update".to_string(),
                args,
                description: desc.clone(),
                permission,
                explanation: desc,
            })
        }

        _ => unreachable!("translate_system called with non-system intent"),
    }
}
