use super::intent::{Intent, ListOptions};
use super::Interpreter;

impl Interpreter {
    /// Parse natural language input into intent
    pub fn parse(&self, input: &str) -> Intent {
        let trimmed = input.trim();
        let lowered = trimmed.to_lowercase();
        let input_lower = lowered.as_str();

        // Pipeline detection: "X | Y" or "X then Y"
        // Must be checked first to avoid greedy pattern matches consuming pipe chars
        if trimmed.contains(" | ") || input_lower.contains(" then ") {
            let parts: Vec<String> = if trimmed.contains(" | ") {
                trimmed
                    .split(" | ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                trimmed
                    .split(" then ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            if parts.len() >= 2 {
                return Intent::Pipeline { commands: parts };
            }
        }

        // Check each pattern
        // AGNOS-specific intents matched first (more specific than generic list/show)
        if let Some(caps) = self.try_captures("audit", input_lower) {
            let agent_id = caps.get(7).map(|m| m.as_str().trim().to_string());
            let time_window = caps.get(12).map(|m| m.as_str().trim().to_string());
            return Intent::AuditView {
                agent_id,
                time_window,
                count: None,
            };
        }

        if let Some(caps) = self.try_captures("agent_info", input_lower) {
            let agent_id = caps
                .get(8)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::AgentInfo { agent_id };
        }

        // --- Agnostic QA platform intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("agnostic_run", input_lower) {
            let suite = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let target_url = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !suite.is_empty() {
                return Intent::AgnosticRunSuite { suite, target_url };
            }
        }

        if let Some(caps) = self.try_captures("agnostic_status", input_lower) {
            let run_id = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            if !run_id.is_empty() {
                return Intent::AgnosticTestStatus { run_id };
            }
        }

        if let Some(caps) = self.try_captures("agnostic_report", input_lower) {
            let run_id = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let format = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !run_id.is_empty() {
                return Intent::AgnosticTestReport { run_id, format };
            }
        }

        if let Some(caps) = self.try_captures("agnostic_list_suites", input_lower) {
            let category = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::AgnosticListSuites { category };
        }

        if let Some(caps) = self.try_captures("agnostic_agents", input_lower) {
            let agent_type = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::AgnosticAgentStatus { agent_type };
        }

        // --- Edge fleet management intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("edge_list", input_lower) {
            let status = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::EdgeListNodes { status };
        }

        if let Some(caps) = self.try_captures("edge_deploy", input_lower) {
            let task = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let node = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !task.is_empty() {
                return Intent::EdgeDeploy { task, node };
            }
        }

        if let Some(caps) = self.try_captures("edge_update", input_lower) {
            // Group 1: "edge update <node>", Group 2: "update node <node>"
            let node = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map_or("", |m| m.as_str())
                .trim()
                .to_string();
            let version = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !node.is_empty() {
                return Intent::EdgeUpdate { node, version };
            }
        }

        if let Some(caps) = self.try_captures("edge_health", input_lower) {
            let node = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty() && s != "fleet" && s != "all" && s != "nodes");
            return Intent::EdgeHealth { node };
        }

        if let Some(caps) = self.try_captures("edge_decommission", input_lower) {
            let node = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            if !node.is_empty() {
                return Intent::EdgeDecommission { node };
            }
        }

        // --- Shruti DAW intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("shruti_session", input_lower) {
            let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
            let name = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !action.is_empty() {
                return Intent::ShrutiSession { action, name };
            }
        }

        if let Some(caps) = self.try_captures("shruti_track", input_lower) {
            let action = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let name = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            let kind = caps
                .get(6)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !action.is_empty() {
                return Intent::ShrutiTrack { action, name, kind };
            }
        }

        if let Some(caps) = self.try_captures("shruti_mixer", input_lower) {
            let track = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let gain = caps
                .get(3)
                .and_then(|m| m.as_str().trim().parse::<f64>().ok());
            let mute = if caps.get(4).is_some() {
                Some(true)
            } else {
                None
            };
            let solo = if caps.get(5).is_some() {
                Some(true)
            } else {
                None
            };
            if !track.is_empty() {
                return Intent::ShrutiMixer {
                    track,
                    gain,
                    mute,
                    solo,
                };
            }
        }

        if let Some(caps) = self.try_captures("shruti_transport", input_lower) {
            let action = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let value = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            if !action.is_empty() {
                return Intent::ShrutiTransport { action, value };
            }
        }

        if let Some(caps) = self.try_captures("shruti_export", input_lower) {
            let path = caps
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            let format = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::ShrutiExport { path, format };
        }

        // --- Delta code hosting intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("delta_create_repo", input_lower) {
            let name = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
            let description = caps.get(4).map(|m| m.as_str().trim().to_string());
            if !name.is_empty() {
                return Intent::DeltaCreateRepo { name, description };
            }
        }

        if self.try_captures("delta_list_repos", input_lower).is_some() {
            return Intent::DeltaListRepos;
        }

        if let Some(caps) = self.try_captures("delta_pr", input_lower) {
            let action = caps
                .get(2)
                .map_or("list", |m| m.as_str())
                .trim()
                .to_string();
            let repo = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            let title = caps
                .get(6)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::DeltaPr {
                action,
                repo,
                title,
            };
        }

        if let Some(caps) = self.try_captures("delta_push", input_lower) {
            let repo = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            let branch = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::DeltaPush { repo, branch };
        }

        if let Some(caps) = self.try_captures("delta_ci", input_lower) {
            let repo = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::DeltaCiStatus { repo };
        }

        // --- Aequi accounting intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("aequi_tax", input_lower) {
            let quarter = caps.get(6).map(|m| m.as_str().trim().to_string());
            return Intent::AequiTaxEstimate { quarter };
        }

        if let Some(caps) = self.try_captures("aequi_schedule_c", input_lower) {
            let year = caps.get(4).map(|m| m.as_str().trim().to_string());
            return Intent::AequiScheduleC { year };
        }

        if let Some(caps) = self.try_captures("aequi_import", input_lower) {
            let file_path = caps.get(4).map_or("", |m| m.as_str()).trim().to_string();
            if !file_path.is_empty() {
                return Intent::AequiImportBank { file_path };
            }
        }

        if self.try_captures("aequi_balance", input_lower).is_some() {
            return Intent::AequiBalance;
        }

        if let Some(caps) = self.try_captures("aequi_receipts", input_lower) {
            let status = caps.get(3).map(|m| {
                let s = m.as_str().trim();
                match s {
                    "pending" => "pending_review".to_string(),
                    "unreviewed" => "pending_review".to_string(),
                    other => other.to_string(),
                }
            });
            return Intent::AequiReceipts { status };
        }

        // --- Photis Nadi task management intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("task_list", input_lower) {
            let status = caps.get(4).map(|m| m.as_str().trim().to_string());
            return Intent::TaskList { status };
        }

        if let Some(caps) = self.try_captures("task_create", input) {
            let title = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
            if !title.is_empty() {
                let priority = caps.get(4).map(|m| m.as_str().trim().to_string());
                return Intent::TaskCreate { title, priority };
            }
        }

        if let Some(caps) = self.try_captures("task_update", input_lower) {
            let task_id = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
            let status = caps.get(3).map(|m| m.as_str().trim().to_string());
            if !task_id.is_empty() {
                return Intent::TaskUpdate { task_id, status };
            }
        }

        if let Some(caps) = self.try_captures("ritual_check", input_lower) {
            let date = caps.get(2).map(|m| m.as_str().trim().to_string());
            return Intent::RitualCheck { date };
        }

        if let Some(caps) = self.try_captures("productivity_stats", input_lower) {
            let period = caps.get(2).map(|m| match m.as_str().trim() {
                "daily" => "day".to_string(),
                "weekly" | "this week" => "week".to_string(),
                "monthly" | "this month" => "month".to_string(),
                other => other.to_string(),
            });
            return Intent::ProductivityStats { period };
        }

        if let Some(caps) = self.try_captures("service", input_lower) {
            let action = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let service_name = caps
                .get(4)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::ServiceControl {
                action,
                service_name,
            };
        }

        if let Some(caps) = self.try_captures("network_scan", input_lower) {
            // Determine which alternative matched based on capture groups:
            // group 2 = port scan target, 3 = ping sweep, 4 = dns lookup,
            // 5 = traceroute, 6 = packet capture, 7 = web scan
            if let Some(target) = caps.get(2) {
                return Intent::NetworkScan {
                    action: "port_scan".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(3) {
                return Intent::NetworkScan {
                    action: "ping_sweep".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(4) {
                return Intent::NetworkScan {
                    action: "dns_lookup".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(5) {
                return Intent::NetworkScan {
                    action: "trace_route".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(6) {
                return Intent::NetworkScan {
                    action: "packet_capture".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(7) {
                return Intent::NetworkScan {
                    action: "web_scan".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
        }

        // Extended network tool patterns
        if let Some(caps) = self.try_captures("network_extended", input_lower) {
            let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            if let Some(target) = caps.get(2) {
                return Intent::NetworkScan {
                    action: "mass_scan".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if caps.get(3).is_some() || full.contains("arp scan") {
                let target = caps
                    .get(3)
                    .map(|m| m.as_str().trim().to_string())
                    .filter(|s| !s.is_empty());
                return Intent::NetworkScan {
                    action: "arp_scan".into(),
                    target,
                };
            }
            if let Some(target) = caps.get(4) {
                return Intent::NetworkScan {
                    action: "network_diag".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(5) {
                return Intent::NetworkScan {
                    action: "service_scan".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(6) {
                return Intent::NetworkScan {
                    action: "dir_fuzz".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(7) {
                return Intent::NetworkScan {
                    action: "vuln_scan".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if full.contains("socket") || full.contains("connection") {
                return Intent::NetworkScan {
                    action: "socket_stats".into(),
                    target: None,
                };
            }
            if let Some(target) = caps.get(8) {
                return Intent::NetworkScan {
                    action: "dns_enum".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if let Some(target) = caps.get(9) {
                return Intent::NetworkScan {
                    action: "deep_inspect".into(),
                    target: Some(target.as_str().trim().to_string()),
                };
            }
            if full.contains("bandwidth") {
                return Intent::NetworkScan {
                    action: "bandwidth_monitor".into(),
                    target: None,
                };
            }
        }

        // Journal view
        if let Some(caps) = self.try_captures("journal", input_lower) {
            let unit = caps
                .get(6)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            let since = caps
                .get(8)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::JournalView {
                unit,
                priority: None,
                lines: None,
                since,
            };
        }

        // Journal alt — "show error logs", "show last 50 log entries"
        if let Some(caps) = self.try_captures("journal_alt", input_lower) {
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
            return Intent::JournalView {
                unit,
                priority,
                lines,
                since,
            };
        }

        // Device info
        if let Some(caps) = self.try_captures("device_info", input_lower) {
            let subsystem = caps.get(4).map(|m| m.as_str().trim().to_string());
            let device_path = caps
                .get(9)
                .map(|m| m.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            return Intent::DeviceInfo {
                subsystem,
                device_path,
            };
        }

        // Device info — specific path: "device info for /dev/sda"
        if let Some(caps) = self.try_captures("device_path", input_lower) {
            let device_path = caps.get(4).map(|m| m.as_str().trim().to_string());
            return Intent::DeviceInfo {
                subsystem: None,
                device_path,
            };
        }

        // Mount control — unmount
        if let Some(caps) = self.try_captures("unmount", input_lower) {
            let mountpoint = caps.get(2).map(|m| m.as_str().trim().to_string());
            return Intent::MountControl {
                action: "unmount".to_string(),
                mountpoint,
                filesystem: None,
            };
        }

        // Mount control — mount <device> on <mountpoint>
        if let Some(caps) = self.try_captures("mount_action", input_lower) {
            let filesystem = caps.get(1).map(|m| m.as_str().trim().to_string());
            let mountpoint = caps.get(3).map(|m| m.as_str().trim().to_string());
            return Intent::MountControl {
                action: "mount".to_string(),
                mountpoint,
                filesystem,
            };
        }

        // Mount control — list mounts
        if let Some(caps) = self.try_captures("mount", input_lower) {
            let filesystem = if caps.get(4).is_some() {
                Some("fuse".to_string())
            } else {
                None
            };
            return Intent::MountControl {
                action: "list".to_string(),
                mountpoint: None,
                filesystem,
            };
        }

        // Boot config — set
        if let Some(caps) = self.try_captures("boot_set", input_lower) {
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
            return Intent::BootConfig {
                action,
                entry,
                value,
            };
        }

        // Boot config — list/show
        if self.try_captures("boot", input_lower).is_some() {
            return Intent::BootConfig {
                action: "list".to_string(),
                entry: None,
                value: None,
            };
        }

        // System update
        if let Some(_caps) = self.try_captures("update", input_lower) {
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
            return Intent::SystemUpdate {
                action: action.to_string(),
            };
        }

        // --- Ark unified package manager intents (before greedy list/show) ---
        if let Some(caps) = self.try_captures("ark_install", input_lower) {
            let packages_str = caps.get(1).map_or("", |m| m.as_str()).trim();
            if !packages_str.is_empty() {
                let packages: Vec<String> = packages_str
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                return Intent::ArkInstall {
                    packages,
                    source: None,
                };
            }
        }

        if let Some(caps) = self.try_captures("ark_remove", input_lower) {
            let packages_str = caps.get(2).map_or("", |m| m.as_str()).trim();
            if !packages_str.is_empty() {
                let packages: Vec<String> = packages_str
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                return Intent::ArkRemove { packages };
            }
        }

        if let Some(caps) = self.try_captures("ark_search", input_lower) {
            let query = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            if !query.is_empty() {
                return Intent::ArkSearch { query };
            }
        }

        if let Some(caps) = self.try_captures("ark_info", input_lower) {
            let package = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
            if !package.is_empty() {
                return Intent::ArkInfo { package };
            }
        }

        if self.try_captures("ark_update", input_lower).is_some() {
            return Intent::ArkUpdate;
        }

        if let Some(caps) = self.try_captures("ark_upgrade", input_lower) {
            let packages = caps
                .get(2)
                .map(|m| m.as_str().trim())
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.split_whitespace()
                        .map(|p| p.to_string())
                        .collect::<Vec<String>>()
                });
            return Intent::ArkUpgrade { packages };
        }

        if self.try_captures("ark_status", input_lower).is_some() {
            return Intent::ArkStatus;
        }

        if let Some(caps) = self.try_captures("list", input_lower) {
            let path = caps.get(6).map(|m| m.as_str().trim().to_string());
            let all = input_lower.contains("all");

            return Intent::ListFiles {
                path,
                options: ListOptions {
                    all,
                    ..Default::default()
                },
            };
        }

        if let Some(caps) = self.try_captures("show_file", input_lower) {
            if let Some(path) = caps.get(6) {
                return Intent::ShowFile {
                    path: path.as_str().trim().to_string(),
                    lines: None,
                };
            }
        }

        if let Some(caps) = self.try_captures("cd", input_lower) {
            if let Some(path) = caps.get(4) {
                return Intent::ChangeDirectory {
                    path: path.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.try_captures("mkdir", input_lower) {
            if let Some(path) = caps.get(6) {
                return Intent::CreateDirectory {
                    path: path.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.try_captures("copy", input_lower) {
            if let (Some(source), Some(dest)) = (caps.get(2), caps.get(4)) {
                return Intent::Copy {
                    source: source.as_str().trim().to_string(),
                    destination: dest.as_str().trim().to_string(),
                };
            }
        }

        if let Some(caps) = self.try_captures("move", input_lower) {
            if let (Some(source), Some(dest)) = (caps.get(2), caps.get(4)) {
                return Intent::Move {
                    source: source.as_str().trim().to_string(),
                    destination: dest.as_str().trim().to_string(),
                };
            }
        }

        if self.try_captures("ps", input_lower).is_some() {
            return Intent::ShowProcesses;
        }

        if self.try_captures("sysinfo", input_lower).is_some() {
            return Intent::SystemInfo;
        }

        if let Some(caps) = self.try_captures("marketplace_install", input_lower) {
            let package = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();
            if !package.is_empty() {
                return Intent::MarketplaceInstall { package };
            }
        }

        if let Some(caps) = self.try_captures("marketplace_uninstall", input_lower) {
            let package = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();
            if !package.is_empty() {
                return Intent::MarketplaceUninstall { package };
            }
        }

        if let Some(caps) = self.try_captures("marketplace_search", input_lower) {
            let query = caps.get(4).map_or("", |m| m.as_str()).trim().to_string();
            if !query.is_empty() {
                return Intent::MarketplaceSearch { query };
            }
        }

        if self.try_captures("marketplace_list", input_lower).is_some() {
            return Intent::MarketplaceList;
        }

        if self
            .try_captures("marketplace_update", input_lower)
            .is_some()
        {
            return Intent::MarketplaceUpdate;
        }

        if let Some(caps) = self.try_captures("knowledge", input_lower) {
            let query = caps.get(5).map_or("", |m| m.as_str()).trim().to_string();
            if !query.is_empty() {
                return Intent::KnowledgeSearch {
                    query,
                    source: None,
                };
            }
        }

        if let Some(caps) = self.try_captures("rag_query", input_lower) {
            let query = caps.get(3).map_or("", |m| m.as_str()).trim().to_string();
            if !query.is_empty() {
                return Intent::RagQuery { query };
            }
        }

        if self
            .patterns
            .get("question")
            .is_some_and(|p| p.is_match(input_lower))
        {
            return Intent::Question {
                query: input.to_string(),
            };
        }

        // If it looks like a command, treat it as such
        if !input.contains(' ') || input.starts_with("/") {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if !parts.is_empty() {
                return Intent::ShellCommand {
                    command: parts[0].to_string(),
                    args: parts[1..].iter().map(|s| s.to_string()).collect(),
                };
            }
        }

        Intent::Unknown
    }
}
