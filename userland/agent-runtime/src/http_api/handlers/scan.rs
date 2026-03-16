use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::audit::AuditEvent;
use crate::http_api::state::ApiState;
use crate::phylax::{ScanMode, ScanTarget};
// MAX_AUDIT_BUFFER eviction handled by ApiState::push_audit_event (H17)

// ---------------------------------------------------------------------------
// Scan request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFileRequest {
    pub path: String,
    #[serde(default = "default_scan_mode")]
    pub mode: String,
}

fn default_scan_mode() -> String {
    "on_demand".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanBytesRequest {
    pub data: String,
    #[serde(default)]
    pub target_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a scan mode string into a `ScanMode`.
fn parse_scan_mode(mode: &str) -> Result<ScanMode, String> {
    match mode {
        "on_demand" => Ok(ScanMode::OnDemand),
        "real_time" => Ok(ScanMode::RealTime),
        "scheduled" => Ok(ScanMode::Scheduled),
        "pre_install" => Ok(ScanMode::PreInstall),
        "pre_exec" => Ok(ScanMode::PreExec),
        other => Err(format!("unknown scan mode: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Scan handlers
// ---------------------------------------------------------------------------

/// POST /v1/scan/file — scan a file on disk via phylax.
pub async fn scan_file_handler(
    State(state): State<ApiState>,
    Json(req): Json<ScanFileRequest>,
) -> impl IntoResponse {
    let mode = match parse_scan_mode(&req.mode) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": e,
                    "mode": req.mode,
                })),
            )
                .into_response();
        }
    };

    let path = PathBuf::from(&req.path);
    let mut scanner = state.phylax_scanner.write().await;
    let result = scanner.scan_file(&path, mode);

    // Log audit event for scan operation -- FIFO eviction via ApiState (H17)
    state
        .push_audit_event(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            action: "phylax_scan_file".to_string(),
            agent: None,
            details: serde_json::json!({
                "path": req.path,
                "mode": req.mode,
                "scan_id": result.id,
                "findings_count": result.findings.len(),
                "clean": result.clean,
            }),
            outcome: if result.clean { "clean" } else { "findings_detected" }.to_string(),
        })
        .await;

    Json(serde_json::json!({
        "scan_id": result.id,
        "target": format!("{}", result.target),
        "mode": format!("{}", result.mode),
        "started_at": result.started_at.to_rfc3339(),
        "completed_at": result.completed_at.to_rfc3339(),
        "clean": result.clean,
        "entropy": result.entropy,
        "file_size": result.file_size,
        "findings": result.findings.iter().map(|f| serde_json::json!({
            "id": f.id,
            "severity": format!("{}", f.severity),
            "category": format!("{}", f.category),
            "description": f.description,
            "rule_id": f.rule_id,
            "offset": f.offset,
            "recommendation": f.recommendation,
        })).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// POST /v1/scan/bytes — scan raw base64-encoded bytes via phylax.
pub async fn scan_bytes_handler(
    State(state): State<ApiState>,
    Json(req): Json<ScanBytesRequest>,
) -> impl IntoResponse {
    let data = match base64::engine::general_purpose::STANDARD.decode(&req.data) {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("invalid base64 data: {e}"),
                })),
            )
                .into_response();
        }
    };

    let target_name = req.target_name.as_deref().unwrap_or("anonymous");
    let target = ScanTarget::Memory;

    let mut scanner = state.phylax_scanner.write().await;
    let result = scanner.scan_bytes(&data, target, ScanMode::OnDemand);

    // Log audit event for byte scan -- FIFO eviction via ApiState (H17)
    state
        .push_audit_event(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            action: "phylax_scan_bytes".to_string(),
            agent: None,
            details: serde_json::json!({
                "target_name": target_name,
                "data_size": data.len(),
                "scan_id": result.id,
                "findings_count": result.findings.len(),
                "clean": result.clean,
            }),
            outcome: if result.clean { "clean" } else { "findings_detected" }.to_string(),
        })
        .await;

    Json(serde_json::json!({
        "scan_id": result.id,
        "target": format!("{}", result.target),
        "target_name": target_name,
        "mode": format!("{}", result.mode),
        "started_at": result.started_at.to_rfc3339(),
        "completed_at": result.completed_at.to_rfc3339(),
        "clean": result.clean,
        "entropy": result.entropy,
        "file_size": result.file_size,
        "findings": result.findings.iter().map(|f| serde_json::json!({
            "id": f.id,
            "severity": format!("{}", f.severity),
            "category": format!("{}", f.category),
            "description": f.description,
            "rule_id": f.rule_id,
            "offset": f.offset,
            "recommendation": f.recommendation,
        })).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// GET /v1/scan/status — return phylax scanner statistics.
pub async fn scan_status_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let scanner = state.phylax_scanner.read().await;
    let stats = scanner.stats();

    Json(serde_json::json!({
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

/// GET /v1/scan/history — return recent scan results.
pub async fn scan_history_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let scanner = state.phylax_scanner.read().await;
    let history = scanner.scan_history();

    Json(serde_json::json!({
        "scans": history.iter().map(|r| serde_json::json!({
            "scan_id": r.id,
            "target": format!("{}", r.target),
            "mode": format!("{}", r.mode),
            "started_at": r.started_at.to_rfc3339(),
            "completed_at": r.completed_at.to_rfc3339(),
            "clean": r.clean,
            "entropy": r.entropy,
            "file_size": r.file_size,
            "findings_count": r.findings.len(),
            "max_severity": format!("{}", r.max_severity()),
        })).collect::<Vec<_>>(),
        "total": history.len(),
    }))
}

/// GET /v1/scan/rules — return list of loaded YARA rules.
pub async fn scan_rules_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let scanner = state.phylax_scanner.read().await;
    let rules = scanner.rules();

    Json(serde_json::json!({
        "rules": rules.iter().map(|r| serde_json::json!({
            "id": r.id,
            "description": r.description,
            "category": format!("{}", r.category),
            "severity": format!("{}", r.severity),
            "tags": r.tags,
            "enabled": r.enabled,
            "pattern_count": r.patterns.len(),
        })).collect::<Vec<_>>(),
        "total": rules.len(),
        "enabled": rules.iter().filter(|r| r.enabled).count(),
    }))
}
