use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_agnos(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::AuditView {
            agent_id,
            time_window,
            count,
        } => {
            let mut args = vec!["show".to_string()];
            if let Some(id) = agent_id {
                args.push("--agent".to_string());
                args.push(id.clone());
            }
            if let Some(window) = time_window {
                args.push("--since".to_string());
                args.push(window.clone());
            }
            if let Some(n) = count {
                args.push("--count".to_string());
                args.push(n.to_string());
            }
            let desc = match (agent_id, time_window) {
                (Some(id), Some(w)) => format!("Show audit log for agent {} in last {}", id, w),
                (Some(id), None) => format!("Show audit log for agent {}", id),
                (None, Some(w)) => format!("Show audit log for last {}", w),
                (None, None) => "Show recent audit log entries".to_string(),
            };
            Ok(Translation {
                command: "agnos-audit".to_string(),
                args,
                description: desc.clone(),
                permission: PermissionLevel::Safe,
                explanation: desc,
            })
        }

        Intent::AgentInfo { agent_id } => {
            let args = if let Some(id) = agent_id {
                vec!["status".to_string(), id.clone()]
            } else {
                vec!["list".to_string()]
            };
            let desc = agent_id
                .as_ref()
                .map(|id| format!("Show status for agent {}", id))
                .unwrap_or_else(|| "List all running agents".to_string());
            Ok(Translation {
                command: "agent-runtime".to_string(),
                args,
                description: desc.clone(),
                permission: PermissionLevel::Safe,
                explanation: desc,
            })
        }

        Intent::ServiceControl {
            action,
            service_name,
        } => {
            let mut args = vec!["service".to_string(), action.clone()];
            if let Some(name) = service_name {
                args.push(name.clone());
            }
            let desc = match service_name {
                Some(name) => format!("{} service '{}'", action, name),
                None => format!("{} services", action),
            };
            let permission = match action.as_str() {
                "list" | "status" => PermissionLevel::Safe,
                _ => PermissionLevel::Admin,
            };
            Ok(Translation {
                command: "agent-runtime".to_string(),
                args,
                description: desc.clone(),
                permission,
                explanation: desc,
            })
        }

        _ => unreachable!("translate_agnos called with non-agnos intent"),
    }
}
