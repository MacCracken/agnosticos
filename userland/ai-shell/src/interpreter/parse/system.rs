use crate::interpreter::intent::{Intent, ListOptions};
use crate::interpreter::Interpreter;

/// Parse core system intents: AGNOS agent/audit, journal, device, mount, boot, update,
/// filesystem, process, knowledge, marketplace, ark, and generic fallback patterns
pub(super) fn parse_core(interp: &Interpreter, input: &str, input_lower: &str) -> Option<Intent> {
    // AGNOS-specific intents matched first (more specific than generic list/show)
    if let Some(caps) = interp.try_captures("audit", input_lower) {
        let agent_id = caps.get(7).map(|m| m.as_str().trim().to_string());
        let time_window = caps.get(12).map(|m| m.as_str().trim().to_string());
        return Some(Intent::AuditView {
            agent_id,
            time_window,
            count: None,
        });
    }

    if let Some(caps) = interp.try_captures("agent_info", input_lower) {
        let agent_id = caps
            .get(8)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::AgentInfo { agent_id });
    }

    // Service control
    if let Some(caps) = interp.try_captures("service", input_lower) {
        let action = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let service_name = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::ServiceControl {
            action,
            service_name,
        });
    }

    // Journal view
    if let Some(caps) = interp.try_captures("journal", input_lower) {
        let unit = caps
            .get(6)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        let since = caps
            .get(8)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::JournalView {
            unit,
            priority: None,
            lines: None,
            since,
        });
    }

    // Journal alt -- "show error logs", "show last 50 log entries"
    if let Some(caps) = interp.try_captures("journal_alt", input_lower) {
        let lines = caps.get(4).and_then(|m| m.as_str().parse::<usize>().ok());
        let priority = caps.get(5).map(|m| m.as_str().trim().to_string());
        let unit = caps
            .get(8)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        let since = caps
            .get(10)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::JournalView {
            unit,
            priority,
            lines,
            since,
        });
    }

    // Device info
    if let Some(caps) = interp.try_captures("device_info", input_lower) {
        let subsystem = caps.get(4).map(|m| m.as_str().trim().to_string());
        let device_path = caps
            .get(9)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::DeviceInfo {
            subsystem,
            device_path,
        });
    }

    // Device info -- specific path: "device info for /dev/sda"
    if let Some(caps) = interp.try_captures("device_path", input_lower) {
        let device_path = caps.get(4).map(|m| m.as_str().trim().to_string());
        return Some(Intent::DeviceInfo {
            subsystem: None,
            device_path,
        });
    }

    // Mount control -- unmount
    if let Some(caps) = interp.try_captures("unmount", input_lower) {
        let mountpoint = caps.get(2).map(|m| m.as_str().trim().to_string());
        return Some(Intent::MountControl {
            action: "unmount".to_string(),
            mountpoint,
            filesystem: None,
        });
    }

    // Mount control -- mount <device> on <mountpoint>
    if let Some(caps) = interp.try_captures("mount_action", input_lower) {
        let filesystem = caps.get(1).map(|m| m.as_str().trim().to_string());
        let mountpoint = caps.get(3).map(|m| m.as_str().trim().to_string());
        return Some(Intent::MountControl {
            action: "mount".to_string(),
            mountpoint,
            filesystem,
        });
    }

    // Mount control -- list mounts
    if let Some(caps) = interp.try_captures("mount", input_lower) {
        let filesystem = if caps.get(4).is_some() {
            Some("fuse".to_string())
        } else {
            None
        };
        return Some(Intent::MountControl {
            action: "list".to_string(),
            mountpoint: None,
            filesystem,
        });
    }

    // Boot config -- set
    if let Some(caps) = interp.try_captures("boot_set", input_lower) {
        let action_word = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("default");
        let action = match action_word {
            "timeout" => "timeout".to_string(),
            _ => "default".to_string(),
        };
        let value = caps.get(4).map(|m| m.as_str().trim().to_string());
        let entry = if action == "default" {
            value.clone()
        } else {
            None
        };
        return Some(Intent::BootConfig {
            action,
            entry,
            value,
        });
    }

    // Boot config -- list/show
    if interp.try_captures("boot", input_lower).is_some() {
        return Some(Intent::BootConfig {
            action: "list".to_string(),
            entry: None,
            value: None,
        });
    }

    // System update
    if interp.try_captures("update", input_lower).is_some() {
        let action = if input_lower.contains("check") {
            "check"
        } else if input_lower.contains("apply") {
            "apply"
        } else if input_lower.contains("rollback") {
            "rollback"
        } else if input_lower.contains("status") || input_lower.contains("version") {
            "status"
        } else {
            "check"
        };
        return Some(Intent::SystemUpdate {
            action: action.to_string(),
        });
    }

    // --- Ark unified package manager intents ---

    // Group install: "install desktop", "setup ai", "ark install --group edge"
    if let Some(caps) = interp.try_captures("ark_install_group", input_lower) {
        let group = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        if !group.is_empty() {
            let meta = match group.as_str() {
                "desktop" => "agnos-desktop",
                "ai" => "agnos-ai",
                "edge" => "agnos-edge-agent",
                _ => &group,
            };
            return Some(Intent::ArkInstall {
                packages: vec![meta.to_string()],
                source: None,
            });
        }
    }

    if let Some(caps) = interp.try_captures("ark_install", input_lower) {
        let packages_str = caps.get(1).map_or("", |m| m.as_str()).trim();
        if !packages_str.is_empty() {
            let packages: Vec<String> = packages_str
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            return Some(Intent::ArkInstall {
                packages,
                source: None,
            });
        }
    }

    if let Some(caps) = interp.try_captures("ark_remove", input_lower) {
        let packages_str = caps.get(2).map_or("", |m| m.as_str()).trim();
        if !packages_str.is_empty() {
            let packages: Vec<String> = packages_str
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            return Some(Intent::ArkRemove { packages });
        }
    }

    if let Some(caps) = interp.try_captures("ark_search", input_lower) {
        let query = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        if !query.is_empty() {
            return Some(Intent::ArkSearch { query });
        }
    }

    if let Some(caps) = interp.try_captures("ark_info", input_lower) {
        let package = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        if !package.is_empty() {
            return Some(Intent::ArkInfo { package });
        }
    }

    if interp.try_captures("ark_update", input_lower).is_some() {
        return Some(Intent::ArkUpdate);
    }

    if let Some(caps) = interp.try_captures("ark_upgrade", input_lower) {
        let packages = caps
            .get(2)
            .map(|m| m.as_str().trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split_whitespace()
                    .map(|p| p.to_string())
                    .collect::<Vec<String>>()
            });
        return Some(Intent::ArkUpgrade { packages });
    }

    if interp.try_captures("ark_status", input_lower).is_some() {
        return Some(Intent::ArkStatus);
    }

    // Filesystem patterns
    if let Some(caps) = interp.try_captures("list", input_lower) {
        let path = caps.get(6).map(|m| m.as_str().trim().to_string());
        let all = input_lower.contains("all");

        return Some(Intent::ListFiles {
            path,
            options: ListOptions {
                all,
                ..Default::default()
            },
        });
    }

    if let Some(caps) = interp.try_captures("show_file", input_lower) {
        if let Some(path) = caps.get(6) {
            return Some(Intent::ShowFile {
                path: path.as_str().trim().to_string(),
                lines: None,
            });
        }
    }

    if let Some(caps) = interp.try_captures("cd", input_lower) {
        if let Some(path) = caps.get(4) {
            return Some(Intent::ChangeDirectory {
                path: path.as_str().trim().to_string(),
            });
        }
    }

    if let Some(caps) = interp.try_captures("mkdir", input_lower) {
        if let Some(path) = caps.get(6) {
            return Some(Intent::CreateDirectory {
                path: path.as_str().trim().to_string(),
            });
        }
    }

    if let Some(caps) = interp.try_captures("copy", input_lower) {
        if let (Some(source), Some(dest)) = (caps.get(2), caps.get(4)) {
            return Some(Intent::Copy {
                source: source.as_str().trim().to_string(),
                destination: dest.as_str().trim().to_string(),
            });
        }
    }

    if let Some(caps) = interp.try_captures("move", input_lower) {
        if let (Some(source), Some(dest)) = (caps.get(2), caps.get(4)) {
            return Some(Intent::Move {
                source: source.as_str().trim().to_string(),
                destination: dest.as_str().trim().to_string(),
            });
        }
    }

    if interp.try_captures("ps", input_lower).is_some() {
        return Some(Intent::ShowProcesses);
    }

    if interp.try_captures("sysinfo", input_lower).is_some() {
        return Some(Intent::SystemInfo);
    }

    // Marketplace patterns
    if let Some(caps) = interp.try_captures("marketplace_install", input_lower) {
        let package = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();
        if !package.is_empty() {
            return Some(Intent::MarketplaceInstall { package });
        }
    }

    if let Some(caps) = interp.try_captures("marketplace_uninstall", input_lower) {
        let package = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();
        if !package.is_empty() {
            return Some(Intent::MarketplaceUninstall { package });
        }
    }

    if let Some(caps) = interp.try_captures("marketplace_search", input_lower) {
        let query = caps.get(4).map_or("", |m| m.as_str()).trim().to_string();
        if !query.is_empty() {
            return Some(Intent::MarketplaceSearch { query });
        }
    }

    if interp
        .try_captures("marketplace_list", input_lower)
        .is_some()
    {
        return Some(Intent::MarketplaceList);
    }

    if interp
        .try_captures("marketplace_update", input_lower)
        .is_some()
    {
        return Some(Intent::MarketplaceUpdate);
    }

    // Knowledge/RAG patterns
    if let Some(caps) = interp.try_captures("knowledge", input_lower) {
        let query = caps.get(5).map_or("", |m| m.as_str()).trim().to_string();
        if !query.is_empty() {
            return Some(Intent::KnowledgeSearch {
                query,
                source: None,
            });
        }
    }

    if let Some(caps) = interp.try_captures("rag_query", input_lower) {
        let query = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();
        if !query.is_empty() {
            return Some(Intent::RagQuery { query });
        }
    }

    // Question detection
    if interp
        .patterns
        .get("question")
        .is_some_and(|p| p.is_match(input_lower))
    {
        return Some(Intent::Question {
            query: input.to_string(),
        });
    }

    // If it looks like a command, treat it as such
    if !input.contains(' ') || input.starts_with("/") {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if !parts.is_empty() {
            return Some(Intent::ShellCommand {
                command: parts[0].to_string(),
                args: parts[1..].iter().map(|s| s.to_string()).collect(),
            });
        }
    }

    None
}
