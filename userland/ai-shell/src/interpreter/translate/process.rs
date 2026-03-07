use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_process(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::ShowProcesses => Ok(Translation {
            command: "ps".to_string(),
            args: vec!["aux".to_string()],
            description: "Show running processes".to_string(),
            permission: PermissionLevel::ReadOnly,
            explanation: "Lists all running processes".to_string(),
        }),

        Intent::SystemInfo => Ok(Translation {
            command: "uname".to_string(),
            args: vec!["-a".to_string()],
            description: "Show system information".to_string(),
            permission: PermissionLevel::ReadOnly,
            explanation: "Displays system kernel information".to_string(),
        }),

        Intent::KillProcess { pid } => Ok(Translation {
            command: "kill".to_string(),
            args: vec![pid.to_string()],
            description: format!("Kill process {}", pid),
            permission: PermissionLevel::Admin,
            explanation: "Sends termination signal to a process".to_string(),
        }),

        Intent::DiskUsage { path } => {
            let mut args = vec!["-h".to_string()];
            if let Some(p) = path {
                args.push(p.clone());
            }
            Ok(Translation {
                command: "df".to_string(),
                args,
                description: format!(
                    "Show disk usage{}",
                    path.as_ref()
                        .map(|p| format!(" for {}", p))
                        .unwrap_or_default()
                ),
                permission: PermissionLevel::ReadOnly,
                explanation: "Shows filesystem disk space usage".to_string(),
            })
        }

        Intent::NetworkInfo => Ok(Translation {
            command: "ip".to_string(),
            args: vec!["addr".to_string(), "show".to_string()],
            description: "Show network interfaces and addresses".to_string(),
            permission: PermissionLevel::ReadOnly,
            explanation: "Displays network interface configuration".to_string(),
        }),

        _ => unreachable!("translate_process called with non-process intent"),
    }
}
