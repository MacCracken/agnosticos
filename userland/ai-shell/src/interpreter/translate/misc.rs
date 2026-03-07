use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::{analyze_command_permission, PermissionLevel};

pub(crate) fn translate_misc(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::ShellCommand { command, args } => {
            let perm = analyze_command_permission(command, args);
            Ok(Translation {
                command: command.clone(),
                args: args.clone(),
                description: format!("Execute {} {}", command, args.join(" ")),
                permission: perm,
                explanation: "Direct shell command execution".to_string(),
            })
        }

        Intent::Pipeline { commands } => {
            let pipeline = commands.join(" | ");
            Ok(Translation {
                command: "sh".to_string(),
                args: vec!["-c".to_string(), pipeline.clone()],
                description: format!("Execute pipeline: {}", pipeline),
                permission: PermissionLevel::SystemWrite,
                explanation: format!("Piped command chain with {} stages", commands.len()),
            })
        }

        _ => unreachable!("translate_misc called with non-misc intent"),
    }
}
