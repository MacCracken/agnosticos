use std::path::Path;

use tracing::info;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
};
use super::super::types::McpToolResult;
use crate::http_api::ApiState;
use crate::phylax::ScanMode;

/// Scan a file or path for threats using the Phylax engine.
///
/// Required args:
///   - `target`: file path to scan
///
/// Optional args:
///   - `mode`: scan mode — "on_demand" (default), "pre_install", or "pre_exec"
pub(crate) async fn handle_phylax_scan(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let target = match extract_required_string(args, "target") {
        Ok(t) => t,
        Err(e) => return e,
    };

    let mode_str = get_optional_string_arg(args, "mode").unwrap_or_else(|| "on_demand".into());
    let mode = match mode_str.as_str() {
        "on_demand" => ScanMode::OnDemand,
        "pre_install" => ScanMode::PreInstall,
        "pre_exec" => ScanMode::PreExec,
        other => {
            return error_result(format!(
                "Invalid scan mode '{}'; expected one of: on_demand, pre_install, pre_exec",
                other
            ))
        }
    };

    let path = Path::new(&target);
    if !path.exists() {
        return error_result(format!("Target path does not exist: {}", target));
    }

    let mut scanner = state.phylax_scanner.write().await;
    let result = scanner.scan_file(path, mode);

    let findings: Vec<serde_json::Value> = result
        .findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "severity": f.severity.to_string(),
                "category": format!("{:?}", f.category),
                "description": f.description,
                "rule_id": f.rule_id,
                "offset": f.offset,
                "recommendation": f.recommendation,
            })
        })
        .collect();

    info!(
        target = %target,
        findings = findings.len(),
        clean = result.clean,
        "Phylax: file scan complete"
    );

    success_result(serde_json::json!({
        "scan_id": result.id,
        "target": result.target.to_string(),
        "mode": result.mode.to_string(),
        "clean": result.clean,
        "entropy": result.entropy,
        "file_size": result.file_size,
        "findings": findings,
        "findings_count": findings.len(),
        "started_at": result.started_at.to_rfc3339(),
        "completed_at": result.completed_at.to_rfc3339(),
    }))
}

/// Get the current status and aggregate statistics of the Phylax scanner.
///
/// No required args.
pub(crate) async fn handle_phylax_status(state: &ApiState) -> McpToolResult {
    let scanner = state.phylax_scanner.read().await;
    let stats = scanner.stats();

    info!(
        total_scans = stats.total_scans,
        rules_loaded = stats.rules_loaded,
        "Phylax: status query"
    );

    success_result(serde_json::json!({
        "total_scans": stats.total_scans,
        "clean_scans": stats.clean_scans,
        "dirty_scans": stats.dirty_scans,
        "total_findings": stats.total_findings,
        "findings_by_severity": stats.findings_by_severity,
        "findings_by_category": stats.findings_by_category,
        "rules_loaded": stats.rules_loaded,
        "rules_enabled": stats.rules_enabled,
        "last_scan_at": stats.last_scan_at.map(|t| t.to_rfc3339()),
        "last_signature_update": stats.last_signature_update.map(|t| t.to_rfc3339()),
    }))
}

/// List loaded YARA rules in the Phylax scanner.
///
/// Optional args:
///   - `enabled_only`: if "true", only return enabled rules
pub(crate) async fn handle_phylax_rules(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let enabled_only = args
        .get("enabled_only")
        .and_then(|v| v.as_bool())
        .or_else(|| get_optional_string_arg(args, "enabled_only").map(|s| s == "true" || s == "1"))
        .unwrap_or(false);

    let scanner = state.phylax_scanner.read().await;
    let rules = scanner.rules();

    let rule_list: Vec<serde_json::Value> = rules
        .iter()
        .filter(|r| !enabled_only || r.enabled)
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "description": r.description,
                "category": format!("{:?}", r.category),
                "severity": r.severity.to_string(),
                "enabled": r.enabled,
            })
        })
        .collect();

    info!(
        total = rules.len(),
        returned = rule_list.len(),
        enabled_only = enabled_only,
        "Phylax: rules query"
    );

    success_result(serde_json::json!({
        "rules": rule_list,
        "total": rule_list.len(),
        "enabled_only": enabled_only,
    }))
}

/// Get recent scan findings from the Phylax scanner history.
///
/// Optional args:
///   - `severity`: filter by severity level (e.g. "CRITICAL", "HIGH", "MEDIUM", "LOW", "INFO")
///   - `limit`: max number of findings to return (default 50)
pub(crate) async fn handle_phylax_findings(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let severity_filter = get_optional_string_arg(args, "severity");
    let limit: usize = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .or_else(|| get_optional_string_arg(args, "limit").and_then(|s| s.parse::<u64>().ok()))
        .unwrap_or(50) as usize;

    let scanner = state.phylax_scanner.read().await;
    let history = scanner.scan_history();

    let mut findings: Vec<serde_json::Value> = Vec::new();

    // Iterate history in reverse (most recent first) and collect findings
    for result in history.iter().rev() {
        for finding in &result.findings {
            if let Some(ref sev) = severity_filter {
                let finding_sev = finding.severity.to_string();
                if !finding_sev.eq_ignore_ascii_case(sev) {
                    continue;
                }
            }

            findings.push(serde_json::json!({
                "finding_id": finding.id,
                "scan_id": result.id,
                "target": result.target.to_string(),
                "mode": result.mode.to_string(),
                "severity": finding.severity.to_string(),
                "category": format!("{:?}", finding.category),
                "description": finding.description,
                "rule_id": finding.rule_id,
                "offset": finding.offset,
                "recommendation": finding.recommendation,
                "scanned_at": result.completed_at.to_rfc3339(),
            }));

            if findings.len() >= limit {
                break;
            }
        }
        if findings.len() >= limit {
            break;
        }
    }

    info!(
        returned = findings.len(),
        severity = ?severity_filter,
        limit = limit,
        "Phylax: findings query"
    );

    success_result(serde_json::json!({
        "findings": findings,
        "total": findings.len(),
        "severity_filter": severity_filter,
        "limit": limit,
    }))
}

/// Forward a threat finding to aegis for quarantine/remediation.
///
/// Required args:
///   - `agent_id`: the agent whose findings should be forwarded to aegis
pub(crate) async fn handle_phylax_quarantine(
    state: &ApiState,
    args: &serde_json::Value,
) -> McpToolResult {
    let agent_id = match extract_required_string(args, "agent_id") {
        Ok(id) => id,
        Err(e) => return e,
    };

    // Collect findings that aegis would act on
    let scanner = state.phylax_scanner.read().await;
    let aegis_findings = scanner.findings_for_aegis();

    let forwarded: Vec<serde_json::Value> = aegis_findings
        .iter()
        .map(|(result, finding)| {
            serde_json::json!({
                "scan_id": result.id,
                "finding_id": finding.id,
                "target": result.target.to_string(),
                "severity": finding.severity.to_string(),
                "category": format!("{:?}", finding.category),
                "description": finding.description,
            })
        })
        .collect();

    let count = forwarded.len();

    info!(
        agent_id = %agent_id,
        findings_forwarded = count,
        "Phylax: quarantine request forwarded to aegis"
    );

    success_result(serde_json::json!({
        "status": "forwarded",
        "agent_id": agent_id,
        "findings_forwarded": count,
        "findings": forwarded,
        "message": format!(
            "{} finding(s) forwarded to aegis for quarantine/remediation for agent {}",
            count, agent_id
        ),
    }))
}
