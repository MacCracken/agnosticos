#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use chrono::Utc;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::http_api::build_router;
    use crate::http_api::handlers::agents::gather_system_health;
    use crate::http_api::handlers::audit::AuditEvent;
    use crate::http_api::handlers::traces::TraceStep;
    use crate::http_api::handlers::webhooks::WebhookRegistration;
    use crate::http_api::state::ApiState;
    use crate::http_api::types::*;

    fn test_state() -> ApiState {
        ApiState::new()
    }

    fn test_app() -> Router {
        build_router(test_state())
    }

    #[tokio::test]
    async fn test_health() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: HealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.service, "agnos-agent-runtime");
        // Components should exist
        assert!(json.components.contains_key("agent_registry"));
        assert!(json.components.contains_key("llm_gateway"));
        // System health should be populated
        assert!(json.system.is_some());
    }

    #[tokio::test]
    async fn test_register_agent() {
        let app = test_app();
        let req_body = serde_json::json!({
            "name": "test-agent",
            "capabilities": ["file:read", "llm:inference"],
            "resource_needs": {"min_memory_mb": 512, "min_cpu_shares": 100}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "test-agent");
        assert_eq!(json["status"], "registered");
        assert!(json["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_register_empty_name() {
        let app = test_app();
        let req_body = serde_json::json!({"name": ""});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_register_duplicate_name() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register first
        let req_body = serde_json::json!({"name": "dup-agent"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Duplicate
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_list_agents() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register two agents
        for name in ["agent-a", "agent-b"] {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({"name": name})).unwrap(),
                ))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: AgentListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total, 2);
    }

    #[tokio::test]
    async fn test_get_agent() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "get-me"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Get
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        let app = test_app();
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "hb-agent"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Heartbeat
        let hb_body = serde_json::json!({
            "status": "running",
            "current_task": "processing",
            "cpu_percent": 25.5,
            "memory_mb": 512
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hb_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify heartbeat updated the agent
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let detail: AgentDetail = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail.status, "running");
        assert_eq!(detail.current_task, Some("processing".to_string()));
        assert!(detail.last_heartbeat.is_some());
    }

    #[tokio::test]
    async fn test_heartbeat_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", Uuid::new_v4()))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({})).unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_deregister_agent() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "delete-me"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Delete
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify gone
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_deregister_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_api_state_default() {
        let state = ApiState::default();
        assert!(state.started_at() <= Utc::now());
    }

    #[test]
    fn test_resource_needs_default() {
        let rn = ResourceNeeds::default();
        assert_eq!(rn.min_memory_mb, 0);
        assert_eq!(rn.min_cpu_shares, 0);
    }

    #[tokio::test]
    async fn test_register_name_too_long() {
        let app = test_app();
        let long_name = "x".repeat(257);
        let req_body = serde_json::json!({"name": long_name});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_heartbeat_partial_update() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "partial-hb"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Heartbeat with only status (no task, cpu, mem)
        let hb_body = serde_json::json!({"status": "idle"});
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hb_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: AgentListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total, 0);
        assert!(json.agents.is_empty());
    }

    #[tokio::test]
    async fn test_metrics_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: AgentMetricsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total_agents, 0);
        assert!(json.agents_by_status.is_empty());
        assert!(json.uptime_seconds < 5);
        assert!(json.avg_cpu_percent.is_none());
        assert_eq!(json.total_memory_mb, 0);
    }

    #[tokio::test]
    async fn test_metrics_with_agents() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register two agents
        for name in ["metric-a", "metric-b"] {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({"name": name})).unwrap(),
                ))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::CREATED);

            // Get agent ID for heartbeat
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let id = reg["id"].as_str().unwrap();

            // Send heartbeat with CPU and memory
            let hb = serde_json::json!({
                "status": "running",
                "cpu_percent": 50.0,
                "memory_mb": 256
            });
            let req = Request::builder()
                .method("POST")
                .uri(format!("/v1/agents/{}/heartbeat", id))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&hb).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // Check metrics
        let req = Request::builder()
            .uri("/v1/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: AgentMetricsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total_agents, 2);
        assert_eq!(json.agents_by_status.get("running"), Some(&2));
        assert_eq!(json.avg_cpu_percent, Some(50.0));
        assert_eq!(json.total_memory_mb, 512);
    }

    // ==================================================================
    // New coverage: request/response types, validation, serialization,
    // heartbeat empty body, register with metadata, name boundary
    // ==================================================================

    #[test]
    fn test_register_request_serialization() {
        let req = RegisterAgentRequest {
            name: "test".to_string(),
            capabilities: vec!["file:read".to_string()],
            resource_needs: ResourceNeeds {
                min_memory_mb: 256,
                min_cpu_shares: 50,
            },
            metadata: {
                let mut m = HashMap::new();
                m.insert("version".to_string(), "1.0".to_string());
                m
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let deser: RegisterAgentRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
        assert_eq!(deser.capabilities.len(), 1);
        assert_eq!(deser.resource_needs.min_memory_mb, 256);
        assert_eq!(deser.metadata.get("version").unwrap(), "1.0");
    }

    #[test]
    fn test_heartbeat_request_defaults() {
        let json = "{}";
        let req: HeartbeatRequest = serde_json::from_str(json).unwrap();
        assert!(req.status.is_none());
        assert!(req.current_task.is_none());
        assert!(req.cpu_percent.is_none());
        assert!(req.memory_mb.is_none());
    }

    #[test]
    fn test_error_response_serialization() {
        let err = ErrorResponse {
            error: "Not found".to_string(),
            code: 404,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Not found"));
        assert!(json.contains("404"));
    }

    #[test]
    fn test_health_response_serialization() {
        let mut components = HashMap::new();
        components.insert(
            "agent_registry".to_string(),
            ComponentHealth {
                status: "ok".to_string(),
                message: Some("5 agents registered".to_string()),
            },
        );
        let resp = HealthResponse {
            status: "ok".to_string(),
            service: "test".to_string(),
            version: "0.1.0".to_string(),
            agents_registered: 5,
            uptime_seconds: 3600,
            components,
            system: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.agents_registered, 5);
        assert_eq!(deser.uptime_seconds, 3600);
        assert!(deser.components.contains_key("agent_registry"));
    }

    #[test]
    fn test_component_health_serialization() {
        let ch = ComponentHealth {
            status: "ok".to_string(),
            message: Some("all good".to_string()),
        };
        let json = serde_json::to_string(&ch).unwrap();
        let deser: ComponentHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.status, "ok");
        assert_eq!(deser.message.unwrap(), "all good");

        // With None message
        let ch2 = ComponentHealth {
            status: "degraded".to_string(),
            message: None,
        };
        let json2 = serde_json::to_string(&ch2).unwrap();
        let deser2: ComponentHealth = serde_json::from_str(&json2).unwrap();
        assert_eq!(deser2.status, "degraded");
        assert!(deser2.message.is_none());
    }

    #[test]
    fn test_system_health_serialization() {
        let sh = SystemHealth {
            hostname: "test-host".to_string(),
            load_average: [1.5, 2.0, 0.5],
            memory_total_mb: 16384,
            memory_available_mb: 8192,
            disk_free_mb: 50000,
        };
        let json = serde_json::to_string(&sh).unwrap();
        let deser: SystemHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.hostname, "test-host");
        assert_eq!(deser.load_average[0], 1.5);
        assert_eq!(deser.memory_total_mb, 16384);
        assert_eq!(deser.memory_available_mb, 8192);
        assert_eq!(deser.disk_free_mb, 50000);
    }

    #[test]
    fn test_gather_system_health() {
        let health = gather_system_health();
        // Should have a non-empty hostname on any system
        assert!(!health.hostname.is_empty());
        // On Linux these should be populated
        if cfg!(target_os = "linux") {
            assert!(health.memory_total_mb > 0);
        }
    }

    #[test]
    fn test_agent_metrics_response_serialization() {
        let resp = AgentMetricsResponse {
            total_agents: 3,
            agents_by_status: {
                let mut m = HashMap::new();
                m.insert("running".to_string(), 2);
                m.insert("idle".to_string(), 1);
                m
            },
            uptime_seconds: 120,
            avg_cpu_percent: Some(42.5),
            total_memory_mb: 1024,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: AgentMetricsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_agents, 3);
        assert_eq!(deser.avg_cpu_percent, Some(42.5));
    }

    #[test]
    fn test_default_port_constant() {
        assert_eq!(crate::http_api::DEFAULT_PORT, 8090);
    }

    #[tokio::test]
    async fn test_register_name_exactly_256_chars() {
        let app = test_app();
        let name = "x".repeat(256);
        let req_body = serde_json::json!({"name": name});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // 256 chars is exactly the limit, should succeed
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_register_with_metadata() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "name": "meta-agent",
            "capabilities": [],
            "metadata": {"runtime": "python", "version": "3.11"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Fetch and check metadata was stored
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let detail: AgentDetail = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail.metadata.get("runtime").unwrap(), "python");
    }

    #[tokio::test]
    async fn test_heartbeat_empty_body_updates_timestamp() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "hb-empty"})).unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = reg["id"].as_str().unwrap();

        // Empty heartbeat
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/heartbeat", id))
            .header("content-type", "application/json")
            .body(Body::from(b"{}".to_vec()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify last_heartbeat was set
        let req = Request::builder()
            .uri(format!("/v1/agents/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let detail: AgentDetail = serde_json::from_slice(&body).unwrap();
        assert!(detail.last_heartbeat.is_some());
        // Status should remain "registered" since no status was sent
        assert_eq!(detail.status, "registered");
    }

    // ==================================================================
    // Phase 6.8: Prometheus, Webhooks, Audit, Memory, Traces tests
    // ==================================================================

    // --- 3a. Prometheus metrics ---

    #[tokio::test]
    async fn test_prometheus_metrics_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/metrics/prometheus")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("# HELP agnos_agents_total"));
        assert!(text.contains("# TYPE agnos_agents_total gauge"));
        assert!(text.contains("agnos_agents_total 0"));
        assert!(text.contains("agnos_uptime_seconds"));
    }

    #[tokio::test]
    async fn test_prometheus_metrics_with_agents() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register an agent
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"name": "prom-agent"})).unwrap(),
            ))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        let req = Request::builder()
            .uri("/v1/metrics/prometheus")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("agnos_agents_total 1"));
        assert!(text.contains("agnos_agent_status"));
    }

    // --- 3b. Webhook tests ---

    #[tokio::test]
    async fn test_register_webhook() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "url": "https://example.com/hook",
            "events": ["agent.registered", "agent.heartbeat"],
            "secret": "s3cret"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["id"].as_str().is_some());
        assert_eq!(json["status"], "registered");
    }

    #[tokio::test]
    async fn test_register_webhook_empty_url() {
        let app = test_app();
        let req_body = serde_json::json!({"url": "", "events": []});

        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_webhooks() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register a webhook
        let req_body = serde_json::json!({"url": "https://example.com/hook", "events": ["test"]});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // List
        let req = Request::builder()
            .uri("/v1/webhooks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_delete_webhook() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req_body = serde_json::json!({"url": "https://example.com/hook"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/webhooks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = json["id"].as_str().unwrap();

        // Delete
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/webhooks/{}", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify empty
        let req = Request::builder()
            .uri("/v1/webhooks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_delete_webhook_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/webhooks/{}", Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_webhooks_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/webhooks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    // --- 3c. Audit tests ---

    #[tokio::test]
    async fn test_forward_audit_events() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "source": "agnostic-python",
            "correlation_id": "corr-123",
            "events": [
                {
                    "timestamp": "2026-03-06T12:00:00Z",
                    "action": "file.read",
                    "agent": "researcher",
                    "details": {"path": "/tmp/data.csv"},
                    "outcome": "success"
                },
                {
                    "timestamp": "2026-03-06T12:01:00Z",
                    "action": "llm.query",
                    "details": {},
                    "outcome": "success"
                }
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["events_received"], 2);
    }

    #[tokio::test]
    async fn test_list_audit_events() {
        let state = test_state();
        let app = build_router(state.clone());

        // Forward some events
        let req_body = serde_json::json!({
            "source": "test",
            "events": [
                {"timestamp": "t1", "action": "read", "agent": "a1", "details": {}, "outcome": "ok"},
                {"timestamp": "t2", "action": "write", "agent": "a2", "details": {}, "outcome": "ok"},
                {"timestamp": "t3", "action": "read", "agent": "a1", "details": {}, "outcome": "fail"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // List all
        let req = Request::builder()
            .uri("/v1/audit")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 3);

        // Filter by agent
        let req = Request::builder()
            .uri("/v1/audit?agent=a1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);

        // Filter by action
        let req = Request::builder()
            .uri("/v1/audit?action=write")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);

        // Limit
        let req = Request::builder()
            .uri("/v1/audit?limit=1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_list_audit_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/audit")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_forward_audit_empty_events() {
        let app = test_app();
        let req_body = serde_json::json!({"source": "test", "events": []});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["events_received"], 0);
    }

    #[test]
    fn test_audit_event_serialization() {
        let event = AuditEvent {
            timestamp: "2026-03-06T00:00:00Z".to_string(),
            action: "test".to_string(),
            agent: Some("agent-1".to_string()),
            details: serde_json::json!({"key": "value"}),
            outcome: "success".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.action, "test");
        assert_eq!(deser.agent, Some("agent-1".to_string()));
    }

    // --- 3d. Memory bridge tests ---

    #[tokio::test]
    async fn test_memory_set_and_get() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        // Set
        let req_body = serde_json::json!({"value": {"greeting": "hello"}, "tags": ["test"]});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/mykey", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Get
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/mykey", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["key"], "mykey");
        assert_eq!(json["value"]["greeting"], "hello");
    }

    #[tokio::test]
    async fn test_memory_get_not_found() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/nonexistent", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_memory_list_keys() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        // Set two keys
        for key in ["alpha", "beta"] {
            let req_body = serde_json::json!({"value": 1});
            let req = Request::builder()
                .method("PUT")
                .uri(format!("/v1/agents/{}/memory/{}", id, key))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);
    }

    #[tokio::test]
    async fn test_memory_delete_key() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        // Set
        let req_body = serde_json::json!({"value": "data"});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/delme", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Delete
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}/memory/delme", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify gone
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/delme", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_memory_delete_not_found() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}/memory/ghost", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_memory_list_empty() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = Uuid::new_v4();

        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_memory_isolation_between_agents() {
        let state = test_state();
        let app = build_router(state.clone());
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Set same key for different agents
        let req_body = serde_json::json!({"value": "agent1-data"});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/shared", id1))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        let req_body = serde_json::json!({"value": "agent2-data"});
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/agents/{}/memory/shared", id2))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Verify isolation
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/memory/shared", id1))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["value"], "agent1-data");
    }

    // --- 3e. Traces tests ---

    #[tokio::test]
    async fn test_submit_trace() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "agent_id": "research-agent",
            "input": "What is AGNOS?",
            "steps": [
                {
                    "name": "search",
                    "rationale": "Need to find information",
                    "tool": "web_search",
                    "output": "Found docs",
                    "duration_ms": 150,
                    "success": true
                },
                {
                    "name": "summarize",
                    "rationale": "Condense results",
                    "duration_ms": 200,
                    "success": true
                }
            ],
            "result": "AGNOS is an AI-native operating system.",
            "duration_ms": 350
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "accepted");
    }

    #[tokio::test]
    async fn test_list_traces() {
        let state = test_state();
        let app = build_router(state.clone());

        // Submit two traces
        for agent in ["agent-a", "agent-b"] {
            let req_body = serde_json::json!({
                "agent_id": agent,
                "input": "test",
                "steps": [],
                "duration_ms": 100
            });
            let req = Request::builder()
                .method("POST")
                .uri("/v1/traces")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List all
        let req = Request::builder()
            .uri("/v1/traces")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);

        // Filter by agent_id
        let req = Request::builder()
            .uri("/v1/traces?agent_id=agent-a")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_list_traces_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/traces")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[test]
    fn test_trace_step_serialization() {
        let step = TraceStep {
            name: "analyze".to_string(),
            rationale: "need to check".to_string(),
            tool: Some("grep".to_string()),
            output: Some("found 5 matches".to_string()),
            duration_ms: 50,
            success: true,
        };
        let json = serde_json::to_string(&step).unwrap();
        let deser: TraceStep = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "analyze");
        assert!(deser.success);
    }

    #[test]
    fn test_webhook_registration_serialization() {
        let wh = WebhookRegistration {
            id: Uuid::new_v4(),
            url: "https://example.com/hook".to_string(),
            events: vec!["test".to_string()],
            secret: Some("key".to_string()),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&wh).unwrap();
        let deser: WebhookRegistration = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.url, "https://example.com/hook");
    }

    // --- Audit chain HTTP endpoint tests ---

    #[tokio::test]
    async fn test_audit_chain_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/audit/chain")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
        assert_eq!(json["entries"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_audit_chain_verify_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/audit/chain/verify")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
    }

    #[tokio::test]
    async fn test_audit_chain_populated_via_forward() {
        let state = test_state();
        let app = build_router(state.clone());

        // Forward two events
        let req_body = serde_json::json!({
            "source": "test",
            "events": [
                {"timestamp": "2026-03-06T12:00:00Z", "action": "read", "agent": "a1", "details": {}, "outcome": "success"},
                {"timestamp": "2026-03-06T12:01:00Z", "action": "write", "details": {}, "outcome": "success"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Check chain has 2 entries
        let req = Request::builder()
            .uri("/v1/audit/chain")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        // Second entry's previous_hash should match first entry's entry_hash
        assert_eq!(entries[1]["previous_hash"], entries[0]["entry_hash"]);

        // Verify chain
        let req = Request::builder()
            .uri("/v1/audit/chain/verify")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
        assert_eq!(json["entries"], 2);
    }

    #[tokio::test]
    async fn test_audit_chain_pagination() {
        let state = test_state();
        let app = build_router(state.clone());

        // Forward 5 events
        let events: Vec<serde_json::Value> = (0..5)
            .map(|i| {
                serde_json::json!({
                    "timestamp": format!("2026-03-06T12:0{}:00Z", i),
                    "action": format!("action_{}", i),
                    "details": {},
                    "outcome": "success"
                })
            })
            .collect();
        let req_body = serde_json::json!({"source": "test", "events": events});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Page: offset=1, limit=2
        let req = Request::builder()
            .uri("/v1/audit/chain?offset=1&limit=2")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 5);
        assert_eq!(json["offset"], 1);
        assert_eq!(json["limit"], 2);
        assert_eq!(json["entries"].as_array().unwrap().len(), 2);
    }

    // -----------------------------------------------------------------------
    // Sandbox Profile Mapping Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sandbox_translate_basic() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "test-profile",
            "filesystem": [
                {"path": "/tmp", "access": "readwrite"},
                {"path": "/etc", "access": "read"}
            ],
            "network_mode": "localhost",
            "blocked_syscalls": ["ptrace", "mount"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "LocalhostOnly");
        assert_eq!(json["isolate_network"], true);
        let fs_rules = json["filesystem_rules"].as_array().unwrap();
        assert_eq!(fs_rules.len(), 2);
        assert_eq!(fs_rules[0]["access"], "ReadWrite");
        assert_eq!(fs_rules[1]["access"], "ReadOnly");
        let seccomp = json["seccomp_rules"].as_array().unwrap();
        assert_eq!(seccomp.len(), 2);
        assert_eq!(seccomp[0]["syscall"], "ptrace");
        assert_eq!(seccomp[0]["action"], "Deny");
    }

    #[tokio::test]
    async fn test_sandbox_translate_empty_name() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "",
            "network_mode": "none"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sandbox_translate_path_traversal() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "evil",
            "filesystem": [{"path": "/tmp/../etc/shadow", "access": "read"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("traversal"));
    }

    #[tokio::test]
    async fn test_sandbox_translate_invalid_access() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "bad-access",
            "filesystem": [{"path": "/tmp", "access": "execute"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sandbox_translate_invalid_network_mode() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "bad-net",
            "network_mode": "bridged"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sandbox_translate_unknown_syscall() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "bad-syscall",
            "blocked_syscalls": ["read", "totally_fake_syscall"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("totally_fake_syscall"));
    }

    #[tokio::test]
    async fn test_sandbox_translate_restricted_with_policy() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "restricted-profile",
            "network_mode": "restricted",
            "allowed_hosts": ["api.example.com"],
            "allowed_ports": [443, 8080]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "Restricted");
        let policy = &json["network_policy"];
        assert!(policy.is_object());
        assert_eq!(policy["allowed_outbound_hosts"][0], "api.example.com");
        assert_eq!(policy["allowed_outbound_ports"][0], 443);
        assert_eq!(policy["allowed_outbound_ports"][1], 8080);
    }

    #[tokio::test]
    async fn test_sandbox_default_profile() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/sandbox/profiles/default")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "LocalhostOnly");
        assert_eq!(json["isolate_network"], true);
        let fs = json["filesystem_rules"].as_array().unwrap();
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0]["path"], "/tmp");
        assert_eq!(fs[0]["access"], "ReadWrite");
    }

    #[tokio::test]
    async fn test_sandbox_validate_valid_config() {
        let app = test_app();
        let config = serde_json::json!({
            "filesystem_rules": [{"path": "/tmp", "access": "ReadWrite"}],
            "network_access": "LocalhostOnly",
            "seccomp_rules": [{"syscall": "ptrace", "action": "Deny"}],
            "isolate_network": true,
            "network_policy": null,
            "mac_profile": null,
            "encrypted_storage": null
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles/validate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
        assert!(json["errors"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_sandbox_validate_path_traversal_and_unknown_syscall() {
        let app = test_app();
        let config = serde_json::json!({
            "filesystem_rules": [
                {"path": "/tmp/../etc/shadow", "access": "ReadOnly"},
                {"path": "relative/path", "access": "ReadWrite"}
            ],
            "network_access": "Restricted",
            "seccomp_rules": [{"syscall": "bogus_call", "action": "Deny"}],
            "isolate_network": true,
            "network_policy": null,
            "mac_profile": null,
            "encrypted_storage": null
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles/validate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], false);
        let errors = json["errors"].as_array().unwrap();
        assert!(errors
            .iter()
            .any(|e| e.as_str().unwrap().contains("traversal")));
        assert!(errors
            .iter()
            .any(|e| e.as_str().unwrap().contains("bogus_call")));
        let warnings = json["warnings"].as_array().unwrap();
        assert!(warnings
            .iter()
            .any(|e| e.as_str().unwrap().contains("Relative path")));
        assert!(warnings
            .iter()
            .any(|e| e.as_str().unwrap().contains("no network_policy")));
    }

    #[tokio::test]
    async fn test_sandbox_validate_inconsistent_network() {
        let app = test_app();
        let config = serde_json::json!({
            "filesystem_rules": [],
            "network_access": "Full",
            "seccomp_rules": [],
            "isolate_network": true,
            "network_policy": {
                "allowed_outbound_ports": [80],
                "allowed_outbound_hosts": [],
                "allowed_inbound_ports": [],
                "enable_nat": true
            },
            "mac_profile": null,
            "encrypted_storage": null
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles/validate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["valid"], true);
        let warnings = json["warnings"].as_array().unwrap();
        assert!(warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("not Restricted")));
        assert!(warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("Full network access")));
    }

    #[tokio::test]
    async fn test_sandbox_translate_full_network_no_isolation() {
        let app = test_app();
        let body = serde_json::json!({
            "name": "full-net",
            "network_mode": "full",
            "isolate_network": false
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/sandbox/profiles")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["network_access"], "Full");
        assert_eq!(json["isolate_network"], false);
        assert!(json["network_policy"].is_null());
    }

    // ===== Ark unified package manager tests =====

    #[tokio::test]
    async fn test_ark_status_handler() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/ark/status")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["version"].as_str().is_some());
        assert_eq!(json["resolver"], "nous");
        let sources = json["sources"].as_array().unwrap();
        assert!(sources.len() >= 2);
    }

    #[tokio::test]
    async fn test_ark_install_request() {
        let app = test_app();
        let body = serde_json::json!({"packages": ["nginx", "curl"]});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/ark/install")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "planned");
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2);
    }

    #[tokio::test]
    async fn test_ark_remove_request() {
        let app = test_app();
        let body = serde_json::json!({"packages": ["nginx"], "purge": true});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/ark/remove")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "planned");
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps[0]["purge"], true);
    }

    #[tokio::test]
    async fn test_ark_search_query() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/ark/search?q=nginx")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["query"], "nginx");
        assert_eq!(json["total"], 0);
        assert!(json["sources_searched"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_ark_upgrade_no_packages() {
        let app = test_app();
        let body = serde_json::json!({"packages": null});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/ark/upgrade")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "planned");
        assert!(json["message"].as_str().unwrap().contains("all"));
    }

    #[tokio::test]
    async fn test_ark_upgrade_specific_packages() {
        let app = test_app();
        let body = serde_json::json!({"packages": ["nginx"]});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/ark/upgrade")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "planned");
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps[0]["package"], "nginx");
    }

    #[test]
    fn test_ark_install_request_deserialize() {
        use crate::http_api::handlers::ark::ArkInstallRequest;
        let json = r#"{"packages": ["nginx", "curl"], "force": true}"#;
        let req: ArkInstallRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.packages, vec!["nginx", "curl"]);
        assert!(req.force);
    }

    #[test]
    fn test_ark_remove_request_deserialize() {
        use crate::http_api::handlers::ark::ArkRemoveRequest;
        let json = r#"{"packages": ["nginx"], "purge": false}"#;
        let req: ArkRemoveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.packages, vec!["nginx"]);
        assert!(!req.purge);
    }

    // -- System update API tests --

    #[tokio::test]
    async fn test_system_update_status() {
        let app = test_app();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/system/update/status")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("current_slot").is_some());
        assert!(json.get("current_version").is_some());
        assert!(json.get("rollback_available").is_some());
    }

    #[tokio::test]
    async fn test_system_update_check() {
        let app = test_app();
        let body = serde_json::json!({});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/system/update/check")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Will return 500 since no update server is running, or OK with no update
        // Either is acceptable in test — we just verify the endpoint exists and responds
        assert!(
            resp.status() == StatusCode::OK || resp.status() == StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn test_system_update_rollback_no_state() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/system/update/rollback")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // No state file exists, so rollback should fail gracefully
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("error").is_some());
    }

    #[tokio::test]
    async fn test_system_update_confirm_no_state() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/system/update/confirm")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // No state file, so confirm should fail gracefully
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_system_update_apply_invalid_manifest() {
        let app = test_app();
        let body = serde_json::json!({"invalid": true});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/system/update/apply")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
