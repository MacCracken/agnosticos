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
            // Validate each pipeline stage for shell injection characters.
            // Pipeline stages should be simple commands — reject metacharacters
            // that could break out of the pipe chain (;, &&, ||, $(), ``, etc.)
            let shell_injection_chars = [';', '&', '`', '$', '(', ')', '{', '}'];
            for cmd in commands {
                if cmd.chars().any(|c| shell_injection_chars.contains(&c)) {
                    anyhow::bail!(
                        "Pipeline stage contains disallowed shell metacharacter: {}",
                        cmd
                    );
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_rejects_semicolon_injection() {
        let intent = Intent::Pipeline {
            commands: vec!["ls".to_string(), "grep foo; rm -rf /".to_string()],
        };
        assert!(translate_misc(&intent).is_err());
    }

    #[test]
    fn test_pipeline_rejects_backtick_injection() {
        let intent = Intent::Pipeline {
            commands: vec!["echo `whoami`".to_string()],
        };
        assert!(translate_misc(&intent).is_err());
    }

    #[test]
    fn test_pipeline_rejects_dollar_injection() {
        let intent = Intent::Pipeline {
            commands: vec!["echo $(cat /etc/passwd)".to_string()],
        };
        assert!(translate_misc(&intent).is_err());
    }

    #[test]
    fn test_pipeline_allows_safe_commands() {
        let intent = Intent::Pipeline {
            commands: vec![
                "cat /var/log/syslog".to_string(),
                "grep error".to_string(),
                "wc -l".to_string(),
            ],
        };
        let result = translate_misc(&intent).unwrap();
        assert_eq!(result.command, "sh");
        assert_eq!(
            result.args,
            vec!["-c", "cat /var/log/syslog | grep error | wc -l"]
        );
    }
}
