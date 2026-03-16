use crate::interpreter::intent::Intent;
use crate::interpreter::Interpreter;

/// Parse consumer platform intents: Agnostic, Edge, SecureYeoman, Delta, Aequi, Photis Nadi
pub(super) fn parse_platforms(
    interp: &Interpreter,
    input: &str,
    input_lower: &str,
) -> Option<Intent> {
    // --- Agnostic QA platform intents ---
    if let Some(caps) = interp.try_captures("agnostic_run", input_lower) {
        let suite = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let target_url = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !suite.is_empty() {
            return Some(Intent::AgnosticRunSuite { suite, target_url });
        }
    }

    if let Some(caps) = interp.try_captures("agnostic_status", input_lower) {
        let run_id = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        if !run_id.is_empty() {
            return Some(Intent::AgnosticTestStatus { run_id });
        }
    }

    if let Some(caps) = interp.try_captures("agnostic_report", input_lower) {
        let run_id = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let format = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !run_id.is_empty() {
            return Some(Intent::AgnosticTestReport { run_id, format });
        }
    }

    if let Some(caps) = interp.try_captures("agnostic_list_suites", input_lower) {
        let category = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::AgnosticListSuites { category });
    }

    if let Some(caps) = interp.try_captures("agnostic_agents", input_lower) {
        let agent_type = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::AgnosticAgentStatus { agent_type });
    }

    if let Some(caps) = interp.try_captures("agnostic_coverage", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let suite = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::AgnosticCoverage { action, suite });
        }
    }
    if let Some(caps) = interp.try_captures("agnostic_schedule", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let suite = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::AgnosticSchedule { action, suite });
        }
    }

    if let Some(caps) = interp.try_captures("agnostic_run_crew", input_lower) {
        let title = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let preset = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !title.is_empty() {
            return Some(Intent::AgnosticRunCrew { title, preset });
        }
    }

    if let Some(caps) = interp.try_captures("agnostic_crew_status", input_lower) {
        let crew_id = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        if !crew_id.is_empty() {
            return Some(Intent::AgnosticCrewStatus { crew_id });
        }
    }

    if let Some(caps) = interp.try_captures("agnostic_list_presets", input_lower) {
        let domain = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::AgnosticListPresets { domain });
    }

    if let Some(caps) = interp.try_captures("agnostic_list_definitions", input_lower) {
        let domain = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::AgnosticListDefinitions { domain });
    }

    if let Some(caps) = interp.try_captures("agnostic_create_agent", input_lower) {
        let agent_key = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let name = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let role = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default();
        if !agent_key.is_empty() && !name.is_empty() {
            return Some(Intent::AgnosticCreateAgent {
                agent_key,
                name,
                role,
            });
        }
    }

    // --- Edge fleet management intents ---
    if let Some(caps) = interp.try_captures("edge_list", input_lower) {
        let status = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::EdgeListNodes { status });
    }

    if let Some(caps) = interp.try_captures("edge_deploy", input_lower) {
        let task = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let node = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !task.is_empty() {
            return Some(Intent::EdgeDeploy { task, node });
        }
    }

    if let Some(caps) = interp.try_captures("edge_update", input_lower) {
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
            return Some(Intent::EdgeUpdate { node, version });
        }
    }

    if let Some(caps) = interp.try_captures("edge_health", input_lower) {
        let node = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty() && s != "fleet" && s != "all" && s != "nodes");
        return Some(Intent::EdgeHealth { node });
    }

    if let Some(caps) = interp.try_captures("edge_decommission", input_lower) {
        let node = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        if !node.is_empty() {
            return Some(Intent::EdgeDecommission { node });
        }
    }

    if let Some(caps) = interp.try_captures("edge_logs", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let node = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::EdgeLogs { action, node });
        }
    }
    if let Some(caps) = interp.try_captures("edge_config", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let node = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::EdgeConfig {
                action,
                node,
                key: None,
            });
        }
    }

    // --- SecureYeoman AI platform intents ---
    if let Some(caps) = interp.try_captures("yeoman_agents", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let agent_id = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::YeomanAgents {
                action,
                agent_id: agent_id.clone(),
                name: agent_id,
            });
        }
    }

    if let Some(caps) = interp.try_captures("yeoman_tasks", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let description = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::YeomanTasks {
                action,
                description: description.clone(),
                task_id: description,
            });
        }
    }

    if let Some(caps) = interp.try_captures("yeoman_tools", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let query = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::YeomanTools { action, query });
        }
    }

    if let Some(caps) = interp.try_captures("yeoman_integrations", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let name = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::YeomanIntegrations { action, name });
        }
    }

    if interp.try_captures("yeoman_status", input_lower).is_some() {
        return Some(Intent::YeomanStatus);
    }

    if let Some(caps) = interp.try_captures("yeoman_logs", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let agent_id = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::YeomanLogs { action, agent_id });
        }
    }
    if let Some(caps) = interp.try_captures("yeoman_workflows", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let name = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::YeomanWorkflows { action, name });
        }
    }

    // --- Delta code hosting intents ---
    if let Some(caps) = interp.try_captures("delta_create_repo", input_lower) {
        let name = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let description = caps.get(4).map(|m| m.as_str().trim().to_string());
        if !name.is_empty() {
            return Some(Intent::DeltaCreateRepo { name, description });
        }
    }

    if interp
        .try_captures("delta_list_repos", input_lower)
        .is_some()
    {
        return Some(Intent::DeltaListRepos);
    }

    if let Some(caps) = interp.try_captures("delta_pr", input_lower) {
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
        return Some(Intent::DeltaPr {
            action,
            repo,
            title,
        });
    }

    if let Some(caps) = interp.try_captures("delta_push", input_lower) {
        let repo = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        let branch = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::DeltaPush { repo, branch });
    }

    if let Some(caps) = interp.try_captures("delta_ci", input_lower) {
        let repo = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::DeltaCiStatus { repo });
    }

    if let Some(caps) = interp.try_captures("delta_branches", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let name = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::DeltaBranches {
                action,
                repo: None,
                name,
            });
        }
    }
    if let Some(caps) = interp.try_captures("delta_review", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let pr_id = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::DeltaReview { action, pr_id });
        }
    }

    // --- Aequi accounting intents ---
    if let Some(caps) = interp.try_captures("aequi_tax", input_lower) {
        let quarter = caps.get(6).map(|m| m.as_str().trim().to_string());
        return Some(Intent::AequiTaxEstimate { quarter });
    }

    if let Some(caps) = interp.try_captures("aequi_schedule_c", input_lower) {
        let year = caps.get(4).map(|m| m.as_str().trim().to_string());
        return Some(Intent::AequiScheduleC { year });
    }

    if let Some(caps) = interp.try_captures("aequi_import", input_lower) {
        let file_path = caps.get(4).map_or("", |m| m.as_str()).trim().to_string();
        if !file_path.is_empty() {
            return Some(Intent::AequiImportBank { file_path });
        }
    }

    if interp.try_captures("aequi_balance", input_lower).is_some() {
        return Some(Intent::AequiBalance);
    }

    if let Some(caps) = interp.try_captures("aequi_receipts", input_lower) {
        let status = caps.get(3).map(|m| {
            let s = m.as_str().trim();
            match s {
                "pending" => "pending_review".to_string(),
                "unreviewed" => "pending_review".to_string(),
                other => other.to_string(),
            }
        });
        return Some(Intent::AequiReceipts { status });
    }

    if let Some(caps) = interp.try_captures("aequi_invoices", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let client = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::AequiInvoices { action, client });
        }
    }
    if let Some(caps) = interp.try_captures("aequi_reports", input_lower) {
        let action = caps
            .get(2)
            .map_or("", |m| m.as_str())
            .trim()
            .replace(' ', "_")
            .to_string();
        let period = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::AequiReports { action, period });
        }
    }

    // --- Photis Nadi task management intents ---
    if let Some(caps) = interp.try_captures("task_list", input_lower) {
        let status = caps.get(4).map(|m| m.as_str().trim().to_string());
        return Some(Intent::TaskList { status });
    }

    // Note: task_create uses original-case input for title preservation
    if let Some(caps) = interp.try_captures("task_create", input) {
        let title = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        if !title.is_empty() {
            let priority = caps.get(4).map(|m| m.as_str().trim().to_string());
            return Some(Intent::TaskCreate { title, priority });
        }
    }

    if let Some(caps) = interp.try_captures("task_update", input_lower) {
        let task_id = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let status = caps.get(3).map(|m| m.as_str().trim().to_string());
        if !task_id.is_empty() {
            return Some(Intent::TaskUpdate { task_id, status });
        }
    }

    if let Some(caps) = interp.try_captures("ritual_check", input_lower) {
        let date = caps.get(2).map(|m| m.as_str().trim().to_string());
        return Some(Intent::RitualCheck { date });
    }

    if let Some(caps) = interp.try_captures("productivity_stats", input_lower) {
        let period = caps.get(2).map(|m| match m.as_str().trim() {
            "daily" => "day".to_string(),
            "weekly" | "this week" => "week".to_string(),
            "monthly" | "this month" => "month".to_string(),
            other => other.to_string(),
        });
        return Some(Intent::ProductivityStats { period });
    }

    if let Some(caps) = interp.try_captures("photis_boards", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let name = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::PhotoisBoards { action, name });
        }
    }
    if let Some(caps) = interp.try_captures("photis_notes", input_lower) {
        let action = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let content = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !action.is_empty() {
            return Some(Intent::PhotoisNotes { action, content });
        }
    }

    // --- Phylax threat detection intents ---
    if let Some(caps) = interp.try_captures("phylax_scan", input_lower) {
        let target = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let mode = caps
            .get(2)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        if !target.is_empty() {
            return Some(Intent::PhylaxScan { target, mode });
        }
    }

    if interp.try_captures("phylax_findings", input_lower).is_some() {
        let caps = interp.try_captures("phylax_findings", input_lower).unwrap();
        let severity = caps
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        return Some(Intent::PhylaxFindings { severity });
    }

    if let Some(caps) = interp.try_captures("phylax_history", input_lower) {
        let limit = caps
            .get(1)
            .and_then(|m| m.as_str().trim().parse::<usize>().ok());
        return Some(Intent::PhylaxHistory { limit });
    }

    if interp.try_captures("phylax_status", input_lower).is_some() {
        return Some(Intent::PhylaxStatus);
    }

    if interp.try_captures("phylax_rules", input_lower).is_some() {
        return Some(Intent::PhylaxRules);
    }

    None
}
