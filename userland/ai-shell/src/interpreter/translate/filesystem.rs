use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_filesystem(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::ListFiles { path, options } => {
            let mut args = vec!["-la".to_string()];
            if options.human_readable {
                args.push("-h".to_string());
            }
            if let Some(p) = path {
                args.push(p.clone());
            }

            Ok(Translation {
                command: "ls".to_string(),
                args,
                description: format!(
                    "List files{}",
                    path.as_ref()
                        .map(|p| format!(" in {}", p))
                        .unwrap_or_default()
                ),
                permission: PermissionLevel::ReadOnly,
                explanation: "Lists files and directories with details".to_string(),
            })
        }

        Intent::ShowFile { path, lines } => {
            let (cmd, args) = if let Some(n) = lines {
                ("head".to_string(), vec![format!("-{}", n), path.clone()])
            } else {
                ("cat".to_string(), vec![path.clone()])
            };

            Ok(Translation {
                command: cmd,
                args,
                description: format!("Display contents of {}", path),
                permission: PermissionLevel::ReadOnly,
                explanation: "Shows file contents".to_string(),
            })
        }

        Intent::ChangeDirectory { path } => Ok(Translation {
            command: "cd".to_string(),
            args: vec![path.clone()],
            description: format!("Change directory to {}", path),
            permission: PermissionLevel::Safe,
            explanation: "Changes current working directory".to_string(),
        }),

        Intent::CreateDirectory { path } => Ok(Translation {
            command: "mkdir".to_string(),
            args: vec!["-p".to_string(), path.clone()],
            description: format!("Create directory {}", path),
            permission: PermissionLevel::UserWrite,
            explanation: "Creates a new directory".to_string(),
        }),

        Intent::Copy {
            source,
            destination,
        } => Ok(Translation {
            command: "cp".to_string(),
            args: vec!["-r".to_string(), source.clone(), destination.clone()],
            description: format!("Copy {} to {}", source, destination),
            permission: PermissionLevel::UserWrite,
            explanation: "Copies files or directories".to_string(),
        }),

        Intent::Move {
            source,
            destination,
        } => Ok(Translation {
            command: "mv".to_string(),
            args: vec![source.clone(), destination.clone()],
            description: format!("Move {} to {}", source, destination),
            permission: PermissionLevel::UserWrite,
            explanation: "Moves or renames files/directories".to_string(),
        }),

        Intent::FindFiles { pattern, path } => {
            let args = vec![
                path.clone().unwrap_or_else(|| ".".to_string()),
                "-name".to_string(),
                pattern.clone(),
            ];
            Ok(Translation {
                command: "find".to_string(),
                args,
                description: format!(
                    "Find files matching '{}'{}",
                    pattern,
                    path.as_ref()
                        .map(|p| format!(" in {}", p))
                        .unwrap_or_default()
                ),
                permission: PermissionLevel::ReadOnly,
                explanation: "Searches for files by name pattern".to_string(),
            })
        }

        Intent::SearchContent { pattern, path } => {
            let mut args = vec!["-rn".to_string(), pattern.clone()];
            if let Some(p) = path {
                args.push(p.clone());
            }
            Ok(Translation {
                command: "grep".to_string(),
                args,
                description: format!(
                    "Search for '{}' in files{}",
                    pattern,
                    path.as_ref()
                        .map(|p| format!(" under {}", p))
                        .unwrap_or_default()
                ),
                permission: PermissionLevel::ReadOnly,
                explanation: "Searches file contents for a text pattern".to_string(),
            })
        }

        Intent::Remove { path, recursive } => {
            let mut args = Vec::new();
            if *recursive {
                args.push("-r".to_string());
            }
            args.push(path.clone());
            Ok(Translation {
                command: "rm".to_string(),
                args,
                description: format!(
                    "Remove {}{}",
                    path,
                    if *recursive { " (recursive)" } else { "" }
                ),
                permission: PermissionLevel::Admin,
                explanation: "Deletes files or directories permanently".to_string(),
            })
        }

        _ => unreachable!("translate_filesystem called with non-filesystem intent"),
    }
}
