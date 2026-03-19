//! Remote execution handler for sutra orchestration.
//!
//! `POST /v1/agents/:id/exec` — execute a shell command in the context of a
//! registered agent.  Used by sutra's `ExecutorKind::Daimon` transport to
//! orchestrate fleet nodes via daimon.

use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::http_api::handlers::audit::AuditEvent;
use crate::http_api::state::ApiState;
use crate::http_api::types::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default command timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum allowed timeout in seconds.
const MAX_TIMEOUT_SECS: u64 = 300;

/// Shell metacharacters that are rejected to prevent injection.
const SHELL_METACHARACTERS: &[char] = &[
    ';', '&', '|', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r', '\\', '!', '#',
];

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for `POST /v1/agents/:id/exec`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecRequest {
    /// The command to execute.  Must not contain shell metacharacters.
    pub command: String,
    /// Optional timeout in seconds (default 30, max 300).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Response body for a successful execution.
#[derive(Debug, Clone, Serialize)]
pub struct ExecResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Returns `Err(reason)` if the command contains disallowed characters.
fn validate_command(command: &str) -> Result<(), String> {
    if command.is_empty() {
        return Err("Command must not be empty".to_string());
    }
    if command.len() > 4096 {
        return Err("Command too long (max 4096 bytes)".to_string());
    }
    for ch in SHELL_METACHARACTERS {
        if command.contains(*ch) {
            return Err(format!(
                "Command contains disallowed shell metacharacter: '{}'",
                ch.escape_default()
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// POST /v1/agents/:id/exec — execute a command on behalf of a registered agent.
pub async fn exec_handler(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecRequest>,
) -> impl IntoResponse {
    // 1. Verify agent exists
    {
        let agents = state.agents_read().await;
        if !agents.contains_key(&id) {
            return not_found(format!("Agent {} not found", id)).into_response();
        }
    }

    // 2. Validate command
    if let Err(reason) = validate_command(&req.command) {
        warn!(
            agent_id = %id,
            command = %req.command,
            "exec rejected: {}",
            reason
        );
        return bad_request(reason).into_response();
    }

    // 3. Resolve timeout
    let timeout_secs = req.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    if timeout_secs == 0 || timeout_secs > MAX_TIMEOUT_SECS {
        return bad_request(format!(
            "timeout_secs must be between 1 and {} (got {})",
            MAX_TIMEOUT_SECS, timeout_secs
        ))
        .into_response();
    }

    info!(
        agent_id = %id,
        command = %req.command,
        timeout_secs = timeout_secs,
        "exec: executing command"
    );

    // 4. Execute via tokio::process::Command with timeout
    let start = Instant::now();
    let child_result = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&req.command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let child = match child_result {
        Ok(c) => c,
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            // Audit the failure
            state
                .push_audit_event(AuditEvent {
                    timestamp: Utc::now().to_rfc3339(),
                    action: "agent.exec".to_string(),
                    agent: Some(id.to_string()),
                    details: serde_json::json!({
                        "command": req.command,
                        "error": format!("spawn failed: {}", e),
                        "duration_ms": duration_ms,
                    }),
                    outcome: "error".to_string(),
                })
                .await;
            return internal_error(format!("Failed to spawn command: {}", e)).into_response();
        }
    };

    let timeout_duration = std::time::Duration::from_secs(timeout_secs);
    let output_result = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match output_result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let outcome = if exit_code == 0 { "success" } else { "failure" };

            // Audit
            state
                .push_audit_event(AuditEvent {
                    timestamp: Utc::now().to_rfc3339(),
                    action: "agent.exec".to_string(),
                    agent: Some(id.to_string()),
                    details: serde_json::json!({
                        "command": req.command,
                        "exit_code": exit_code,
                        "duration_ms": duration_ms,
                        "stdout_len": stdout.len(),
                        "stderr_len": stderr.len(),
                    }),
                    outcome: outcome.to_string(),
                })
                .await;

            info!(
                agent_id = %id,
                exit_code = exit_code,
                duration_ms = duration_ms,
                "exec: command completed"
            );

            (
                StatusCode::OK,
                Json(
                    serde_json::to_value(ExecResponse {
                        exit_code,
                        stdout,
                        stderr,
                        duration_ms,
                    })
                    .unwrap_or_default(),
                ),
            )
                .into_response()
        }
        Ok(Err(e)) => {
            // IO error waiting for child
            state
                .push_audit_event(AuditEvent {
                    timestamp: Utc::now().to_rfc3339(),
                    action: "agent.exec".to_string(),
                    agent: Some(id.to_string()),
                    details: serde_json::json!({
                        "command": req.command,
                        "error": format!("wait failed: {}", e),
                        "duration_ms": duration_ms,
                    }),
                    outcome: "error".to_string(),
                })
                .await;
            internal_error(format!("Command execution failed: {}", e)).into_response()
        }
        Err(_) => {
            // Timeout
            warn!(
                agent_id = %id,
                command = %req.command,
                timeout_secs = timeout_secs,
                "exec: command timed out"
            );

            state
                .push_audit_event(AuditEvent {
                    timestamp: Utc::now().to_rfc3339(),
                    action: "agent.exec".to_string(),
                    agent: Some(id.to_string()),
                    details: serde_json::json!({
                        "command": req.command,
                        "error": "timeout",
                        "timeout_secs": timeout_secs,
                        "duration_ms": duration_ms,
                    }),
                    outcome: "timeout".to_string(),
                })
                .await;

            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({
                    "error": format!("Command timed out after {}s", timeout_secs),
                    "code": 504,
                    "duration_ms": duration_ms,
                })),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::http_api::build_router;
    use crate::http_api::state::ApiState;

    /// Helper: create a test app and register an agent, returning (app_state, agent_id).
    async fn setup() -> (ApiState, Uuid) {
        let state = ApiState::new();
        let id = Uuid::new_v4();
        {
            let mut agents = state.agents_write().await;
            agents.insert(
                id,
                crate::http_api::state::RegisteredAgentEntry {
                    detail: crate::http_api::types::AgentDetail {
                        id,
                        name: "exec-test-agent".to_string(),
                        status: "registered".to_string(),
                        domain: None,
                        capabilities: vec![],
                        resource_needs: Default::default(),
                        metadata: Default::default(),
                        registered_at: chrono::Utc::now(),
                        last_heartbeat: None,
                        current_task: None,
                        cpu_percent: None,
                        memory_mb: None,
                    },
                },
            );
        }
        (state, id)
    }

    #[tokio::test]
    async fn test_exec_missing_agent() {
        let state = ApiState::new();
        let app = build_router(state);
        let fake_id = Uuid::new_v4();

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", fake_id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "echo hello"
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_exec_empty_command() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": ""
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_exec_shell_metacharacter_rejected() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "echo hello; rm -rf /"
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("metacharacter"));
    }

    #[tokio::test]
    async fn test_exec_pipe_rejected() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "cat /etc/passwd | grep root"
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_exec_success() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "echo hello world"
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["exit_code"], 0);
        assert_eq!(json["stdout"].as_str().unwrap().trim(), "hello world");
        assert!(json["duration_ms"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_exec_nonzero_exit() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "false"
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_ne!(json["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "sleep 10",
                    "timeout_secs": 1
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn test_exec_timeout_too_large() {
        let (state, id) = setup().await;
        let app = build_router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "echo hi",
                    "timeout_secs": 999
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_exec_audited() {
        let (state, id) = setup().await;
        let app = build_router(state.clone());

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/exec", id))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "command": "echo audit-test"
                }))
                .unwrap(),
            ))
            .unwrap();

        let _resp = app.oneshot(req).await.unwrap();

        // Verify an audit event was created
        let audit_len = state.audit_buffer_len().await;
        assert!(audit_len > 0, "Expected at least one audit event");

        let buffer = state.audit_buffer.read().await;
        let exec_events: Vec<_> = buffer.iter().filter(|e| e.action == "agent.exec").collect();
        assert!(
            !exec_events.is_empty(),
            "Expected an agent.exec audit event"
        );
    }

    #[test]
    fn test_validate_command_rejects_metacharacters() {
        assert!(validate_command("echo hello").is_ok());
        assert!(validate_command("ls -la /tmp").is_ok());
        assert!(validate_command("").is_err());
        assert!(validate_command("echo; rm").is_err());
        assert!(validate_command("echo && rm").is_err());
        assert!(validate_command("echo | cat").is_err());
        assert!(validate_command("echo `whoami`").is_err());
        assert!(validate_command("echo $HOME").is_err());
        assert!(validate_command("echo $(whoami)").is_err());
        assert!(validate_command("cat < /etc/passwd").is_err());
        assert!(validate_command("echo > /tmp/x").is_err());
    }
}
