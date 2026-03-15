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
            id: None,
            domain: None,
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

        // Limit — total reflects all matching events, returned page is bounded
        let req = Request::builder()
            .uri("/v1/audit?limit=1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 3); // total is unfiltered count
        assert_eq!(json["events"].as_array().unwrap().len(), 1); // only 1 returned
        assert_eq!(json["limit"], 1);
    }

    #[tokio::test]
    async fn test_list_audit_pagination() {
        let state = test_state();
        let app = build_router(state.clone());

        // Submit 5 events
        let req_body = serde_json::json!({
            "source": "test",
            "events": [
                {"timestamp": "t1", "action": "read", "agent": "a1", "details": {}, "outcome": "ok"},
                {"timestamp": "t2", "action": "read", "agent": "a2", "details": {}, "outcome": "ok"},
                {"timestamp": "t3", "action": "read", "agent": "a3", "details": {}, "outcome": "ok"},
                {"timestamp": "t4", "action": "read", "agent": "a4", "details": {}, "outcome": "ok"},
                {"timestamp": "t5", "action": "read", "agent": "a5", "details": {}, "outcome": "ok"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/audit/forward")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Page: offset=2, limit=2 — should return events 3 and 4
        let req = Request::builder()
            .uri("/v1/audit?offset=2&limit=2")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 5);
        assert_eq!(json["events"].as_array().unwrap().len(), 2);
        assert_eq!(json["offset"], 2);
        assert_eq!(json["limit"], 2);
    }

    #[tokio::test]
    async fn test_dashboard_sync_rejects_too_many_metadata() {
        let state = test_state();
        let app = build_router(state.clone());

        // Build a payload with 51 metadata entries
        let mut metadata = serde_json::Map::new();
        for i in 0..51 {
            metadata.insert(format!("key{}", i), serde_json::json!(format!("val{}", i)));
        }
        let req_body = serde_json::json!({
            "source": "test",
            "agents": [{"name": "a1", "status": "running"}],
            "metadata": metadata
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_system_update_rejects_ssrf_url() {
        let state = test_state();
        let app = build_router(state.clone());

        // Try SSRF via subdomain trick: updates.agnos.org.evil.com
        let req_body = serde_json::json!({
            "update_url": "https://updates.agnos.org.evil.com/payload"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/system/update/check")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Try SSRF via userinfo: user@evil.com
        let req_body = serde_json::json!({
            "update_url": "https://attacker@updates.agnos.org/"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/system/update/check")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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

    /// Register a test agent via the API and return its UUID.
    async fn register_test_agent(app: &Router) -> Uuid {
        let req_body = serde_json::json!({
            "name": format!("test-agent-{}", Uuid::new_v4()),
            "capabilities": ["memory:read", "memory:write"],
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
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        json["id"].as_str().unwrap().parse().unwrap()
    }

    #[tokio::test]
    async fn test_memory_set_and_get() {
        let state = test_state();
        let app = build_router(state.clone());
        let id = register_test_agent(&app).await;

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
        let id = register_test_agent(&app).await;

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
        let id = register_test_agent(&app).await;

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
        let id = register_test_agent(&app).await;

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
        let id1 = register_test_agent(&app).await;
        let id2 = register_test_agent(&app).await;

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

    // ===== Reasoning trace tests =====

    #[tokio::test]
    async fn test_submit_reasoning_trace() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "task": "Analyze code quality of authentication module",
            "steps": [
                {
                    "step": 1,
                    "kind": "observation",
                    "content": "Reading auth module source code",
                    "confidence": 0.9,
                    "duration_ms": 200,
                    "tool": "file_read",
                    "tool_output": "Found 3 files"
                },
                {
                    "step": 2,
                    "kind": "thought",
                    "content": "The module uses constant-time comparison, which is good",
                    "confidence": 0.95,
                    "duration_ms": 150
                },
                {
                    "step": 3,
                    "kind": "action",
                    "content": "Running static analysis",
                    "duration_ms": 500,
                    "tool": "clippy"
                }
            ],
            "conclusion": "Auth module is well-structured with no critical issues",
            "confidence": 0.92,
            "duration_ms": 850,
            "model": "llama2",
            "tokens_used": 1500,
            "metadata": {
                "session_id": "sess-123",
                "crew": "qa-crew"
            }
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/qa-agent/reasoning")
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
        assert_eq!(json["agent_id"], "qa-agent");
        assert_eq!(json["steps_recorded"], 3);
        assert_eq!(json["total_traces"], 1);
        assert!(json["trace_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_submit_reasoning_empty_steps() {
        let app = test_app();
        let req_body = serde_json::json!({
            "task": "Some task",
            "steps": [],
            "duration_ms": 0
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/test-agent/reasoning")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_submit_reasoning_empty_task() {
        let app = test_app();
        let req_body = serde_json::json!({
            "task": "",
            "steps": [{"step": 1, "kind": "thought", "content": "test", "duration_ms": 10, "success": true}],
            "duration_ms": 10
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/test-agent/reasoning")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_submit_reasoning_invalid_confidence() {
        let app = test_app();
        let req_body = serde_json::json!({
            "task": "Test task",
            "steps": [{"step": 1, "kind": "thought", "content": "test", "duration_ms": 10}],
            "confidence": 1.5,
            "duration_ms": 10
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/test-agent/reasoning")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_submit_reasoning_invalid_step_confidence() {
        let app = test_app();
        let req_body = serde_json::json!({
            "task": "Test task",
            "steps": [{"step": 1, "kind": "thought", "content": "test", "confidence": -0.1, "duration_ms": 10}],
            "duration_ms": 10
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/test-agent/reasoning")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_reasoning_traces() {
        let state = test_state();
        let app = build_router(state.clone());

        // Submit two reasoning traces for the same agent
        for task in ["task-1", "task-2"] {
            let req_body = serde_json::json!({
                "task": task,
                "steps": [{"step": 1, "kind": "thought", "content": "thinking", "duration_ms": 10}],
                "confidence": 0.8,
                "duration_ms": 100
            });
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/qa-agent/reasoning")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // List all traces for agent
        let req = Request::builder()
            .uri("/v1/agents/qa-agent/reasoning")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["agent_id"], "qa-agent");
        assert_eq!(json["total"], 2);
        let traces = json["traces"].as_array().unwrap();
        assert_eq!(traces.len(), 2);
    }

    #[tokio::test]
    async fn test_list_reasoning_traces_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/agents/nonexistent/reasoning")
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
    async fn test_list_reasoning_traces_with_confidence_filter() {
        let state = test_state();
        let app = build_router(state.clone());

        // Submit traces with different confidences
        for (task, conf) in [("low", 0.3), ("high", 0.9)] {
            let req_body = serde_json::json!({
                "task": task,
                "steps": [{"step": 1, "kind": "thought", "content": "test", "duration_ms": 10}],
                "confidence": conf,
                "duration_ms": 100
            });
            let req = Request::builder()
                .method("POST")
                .uri("/v1/agents/qa-agent/reasoning")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        // Filter by min_confidence=0.5
        let req = Request::builder()
            .uri("/v1/agents/qa-agent/reasoning?min_confidence=0.5")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
        let traces = json["traces"].as_array().unwrap();
        assert_eq!(traces[0]["task"], "high");
    }

    #[test]
    fn test_reasoning_step_serialization() {
        use crate::http_api::handlers::reasoning::ReasoningStep;
        let step = ReasoningStep {
            step: 1,
            kind: "observation".to_string(),
            content: "Reading source code".to_string(),
            confidence: Some(0.9),
            duration_ms: Some(200),
            tool: Some("file_read".to_string()),
            tool_output: Some("3 files found".to_string()),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deser: ReasoningStep = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.step, 1);
        assert_eq!(deser.kind, "observation");
        assert_eq!(deser.confidence, Some(0.9));
    }

    #[test]
    fn test_reasoning_trace_serialization() {
        use crate::http_api::handlers::reasoning::{ReasoningStep, ReasoningTrace};
        let trace = ReasoningTrace {
            task: "Analyze code".to_string(),
            steps: vec![ReasoningStep {
                step: 1,
                kind: "thought".to_string(),
                content: "Initial analysis".to_string(),
                confidence: None,
                duration_ms: Some(100),
                tool: None,
                tool_output: None,
            }],
            conclusion: Some("Code looks good".to_string()),
            confidence: Some(0.85),
            duration_ms: 100,
            model: Some("llama2".to_string()),
            tokens_used: Some(500),
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&trace).unwrap();
        let deser: ReasoningTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.task, "Analyze code");
        assert_eq!(deser.steps.len(), 1);
        assert_eq!(deser.confidence, Some(0.85));
    }

    // ===== Dashboard sync tests =====

    #[tokio::test]
    async fn test_dashboard_sync() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "source": "agnostic",
            "agents": [
                {
                    "name": "qa-manager",
                    "status": "active",
                    "current_task": "Analyzing auth module",
                    "cpu_percent": 45.2,
                    "memory_mb": 256,
                    "tasks_completed": 12,
                    "error_count": 0
                },
                {
                    "name": "senior-qa",
                    "status": "idle",
                    "tasks_completed": 8,
                    "error_count": 1
                }
            ],
            "session": {
                "session_id": "sess-abc",
                "duration_seconds": 3600,
                "description": "QA run for sprint 42"
            },
            "metrics": {
                "total_tokens": 15000,
                "tasks_completed": 20,
                "tasks_failed": 1,
                "avg_response_ms": 250.5
            },
            "metadata": {
                "crew": "qa-crew",
                "run_id": "run-123"
            }
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
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
        assert_eq!(json["agents_synced"], 2);
        assert!(json["snapshot_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_dashboard_sync_empty_source() {
        let app = test_app();
        let req_body = serde_json::json!({
            "source": "",
            "agents": [{"name": "test", "status": "active"}]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_dashboard_sync_empty_agents() {
        let app = test_app();
        let req_body = serde_json::json!({
            "source": "agnostic",
            "agents": []
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_dashboard_latest_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/dashboard/latest")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_dashboard_sync_then_latest() {
        let state = test_state();
        let app = build_router(state.clone());

        // Sync
        let req_body = serde_json::json!({
            "source": "agnostic",
            "agents": [{"name": "qa-agent", "status": "active"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Latest
        let req = Request::builder()
            .uri("/v1/dashboard/latest")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["source"], "agnostic");
        assert!(json["snapshot_id"].as_str().is_some());
        assert!(json["received_at"].as_str().is_some());
    }

    // ===== Environment profiles tests =====

    #[tokio::test]
    async fn test_get_profile_dev() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/profiles/dev")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "dev");
        assert!(json["env_vars"]["AGNOS_LOG_LEVEL"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_get_profile_prod() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/profiles/prod")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "prod");
        assert_eq!(json["env_vars"]["AGNOS_SANDBOX_MODE"], "strict");
    }

    #[tokio::test]
    async fn test_get_profile_staging() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/profiles/staging")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_profile_not_found() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/profiles/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["available_profiles"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/profiles")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["total"].as_u64().unwrap() >= 3);
        let profiles = json["profiles"].as_array().unwrap();
        let names: Vec<&str> = profiles
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"dev"));
        assert!(names.contains(&"staging"));
        assert!(names.contains(&"prod"));
    }

    #[tokio::test]
    async fn test_upsert_profile_create() {
        let state = test_state();
        let app = build_router(state.clone());

        let body = serde_json::json!({
            "env_vars": {"CUSTOM_VAR": "value1", "ANOTHER": "value2"},
            "description": "Custom test profile"
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/v1/profiles/custom")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Verify it can be retrieved
        let req = Request::builder()
            .uri("/v1/profiles/custom")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["env_vars"]["CUSTOM_VAR"], "value1");
    }

    #[tokio::test]
    async fn test_upsert_profile_update() {
        let state = test_state();
        let app = build_router(state.clone());

        // Update the existing dev profile
        let body = serde_json::json!({
            "env_vars": {"AGNOS_LOG_LEVEL": "trace"},
            "description": "Overridden dev profile"
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/v1/profiles/dev")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ===== OTLP configuration tests =====

    #[tokio::test]
    async fn test_otlp_config() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/traces/otlp-config")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["endpoint"].as_str().is_some());
        assert!(json["protocol"].as_str().is_some());
        assert!(json["sampling_rate"].as_f64().is_some());
        assert!(json["resource_attributes"]["service.name"]
            .as_str()
            .is_some());
        assert_eq!(
            json["resource_attributes"]["service.name"],
            "agnos-agent-runtime"
        );
    }

    #[test]
    fn test_otlp_config_serialization() {
        use crate::http_api::handlers::traces::OtlpConfig;
        let config = OtlpConfig {
            endpoint: "http://localhost:4317".to_string(),
            protocol: "grpc".to_string(),
            export_interval_seconds: 5,
            sampling_rate: 1.0,
            resource_attributes: std::collections::HashMap::from([(
                "service.name".to_string(),
                "test".to_string(),
            )]),
            enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deser: OtlpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.endpoint, "http://localhost:4317");
        assert!(deser.enabled);
    }

    // ===== Vector search tests =====

    #[tokio::test]
    async fn test_vector_collections_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/vectors/collections")
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
    async fn test_create_collection() {
        let state = test_state();
        let app = build_router(state.clone());

        let body = serde_json::json!({"name": "test-collection", "dimension": 128});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/collections")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "created");
        assert_eq!(json["collection"], "test-collection");
    }

    #[tokio::test]
    async fn test_create_collection_duplicate() {
        let state = test_state();
        let app = build_router(state.clone());

        let body = serde_json::json!({"name": "dup-collection"});
        for _ in 0..2 {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/vectors/collections")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            // First should be CREATED, second CONFLICT
            if resp.status() == StatusCode::CONFLICT {
                return; // Expected
            }
        }
        panic!("Expected CONFLICT on duplicate collection creation");
    }

    #[tokio::test]
    async fn test_create_collection_empty_name() {
        let app = test_app();
        let body = serde_json::json!({"name": ""});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/collections")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_vector_insert_and_search() {
        let state = test_state();
        let app = build_router(state.clone());

        // Insert vectors
        let body = serde_json::json!({
            "collection": "test-col",
            "vectors": [
                {"embedding": [1.0, 0.0, 0.0], "content": "first document", "metadata": {"tag": "a"}},
                {"embedding": [0.0, 1.0, 0.0], "content": "second document", "metadata": {"tag": "b"}},
                {"embedding": [0.0, 0.0, 1.0], "content": "third document", "metadata": {"tag": "c"}}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/insert")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 3);

        // Search — query closest to first document
        let body = serde_json::json!({
            "embedding": [0.9, 0.1, 0.0],
            "top_k": 2,
            "collection": "test-col"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let results = json["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        // First result should be the most similar
        assert_eq!(results[0]["content"], "first document");
        assert!(results[0]["score"].as_f64().unwrap() > 0.9);
    }

    #[tokio::test]
    async fn test_vector_search_collection_not_found() {
        let app = test_app();
        let body = serde_json::json!({"embedding": [1.0, 0.0], "collection": "nonexistent"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_vector_search_empty_embedding() {
        let app = test_app();
        let body = serde_json::json!({"embedding": [], "collection": "test"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_vector_insert_empty_vectors() {
        let app = test_app();
        let body = serde_json::json!({"collection": "test", "vectors": []});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/insert")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_delete_collection() {
        let state = test_state();
        let app = build_router(state.clone());

        // Create collection
        let body = serde_json::json!({"name": "to-delete"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/collections")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Delete it
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/vectors/collections/to-delete")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify it's gone
        let req = Request::builder()
            .uri("/v1/vectors/collections")
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
    async fn test_delete_collection_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/vectors/collections/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_vector_search_with_min_score() {
        let state = test_state();
        let app = build_router(state.clone());

        // Insert diverse vectors
        let body = serde_json::json!({
            "collection": "score-test",
            "vectors": [
                {"embedding": [1.0, 0.0, 0.0], "content": "close", "metadata": {}},
                {"embedding": [0.0, 1.0, 0.0], "content": "far", "metadata": {}}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/insert")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Search with high min_score
        let body = serde_json::json!({
            "embedding": [1.0, 0.0, 0.0],
            "top_k": 10,
            "collection": "score-test",
            "min_score": 0.9
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vectors/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Only the close vector should pass the threshold
        assert_eq!(json["total"], 1);
        assert_eq!(json["results"][0]["content"], "close");
    }

    #[test]
    fn test_environment_profile_serialization() {
        use crate::http_api::handlers::profiles::EnvironmentProfile;
        let profile = EnvironmentProfile {
            name: "dev".to_string(),
            env_vars: HashMap::from([("KEY".to_string(), "VAL".to_string())]),
            description: Some("test".to_string()),
            active: false,
        };
        let json = serde_json::to_string(&profile).unwrap();
        let deser: EnvironmentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "dev");
        assert_eq!(deser.env_vars["KEY"], "VAL");
    }

    #[test]
    fn test_dashboard_sync_request_serialization() {
        use crate::http_api::handlers::dashboard::{AgentStatus, DashboardSyncRequest};
        let req = DashboardSyncRequest {
            source: "agnostic".to_string(),
            agents: vec![AgentStatus {
                name: "qa-manager".to_string(),
                status: "active".to_string(),
                current_task: Some("testing".to_string()),
                cpu_percent: Some(25.0),
                memory_mb: Some(128),
                tasks_completed: Some(5),
                error_count: None,
            }],
            session: None,
            metrics: None,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deser: DashboardSyncRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.source, "agnostic");
        assert_eq!(deser.agents.len(), 1);
    }

    #[tokio::test]
    async fn test_submit_reasoning_minimal() {
        let app = test_app();
        let req_body = serde_json::json!({
            "task": "Quick check",
            "steps": [{"step": 1, "kind": "thought", "content": "ok", "duration_ms": 5}],
            "duration_ms": 5
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/minimal-agent/reasoning")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // -----------------------------------------------------------------------
    // Screen capture API tests
    // -----------------------------------------------------------------------

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_full_screen() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "full_screen"},
            "format": "png"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["id"].as_str().is_some());
        assert!(json["width"].as_u64().is_some());
        assert!(json["height"].as_u64().is_some());
        assert_eq!(json["format"], "png");
        assert!(json["data_base64"].as_str().is_some());
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_bmp_format() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "full_screen"},
            "format": "bmp"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["format"], "bmp");
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_raw_format() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "full_screen"},
            "format": "raw_argb"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_region() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "region", "x": 0, "y": 0, "width": 100, "height": 100},
            "format": "png"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_invalid_format() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "full_screen"},
            "format": "jpg"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
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
            .to_lowercase()
            .contains("format"));
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_invalid_surface_id() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "window", "surface_id": "not-a-uuid"},
            "format": "png"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_window_not_found() {
        let app = test_app();
        let fake_uuid = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({
            "target": {"type": "window", "surface_id": fake_uuid},
            "format": "png"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_agent_permission_denied() {
        let app = test_app();
        let req_body = serde_json::json!({
            "target": {"type": "full_screen"},
            "format": "png",
            "agent_id": "rogue-agent"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_grant_permission() {
        let app = test_app();
        let req_body = serde_json::json!({
            "agent_id": "agent-1",
            "allowed_targets": ["full_screen", "window", "region"],
            "expires_in_secs": 3600,
            "max_captures_per_minute": 30
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/permissions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "granted");
        assert_eq!(json["agent_id"], "agent-1");
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_grant_permission_empty_agent() {
        let app = test_app();
        let req_body = serde_json::json!({
            "agent_id": "",
            "allowed_targets": ["full_screen"]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/permissions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_grant_permission_invalid_target() {
        let app = test_app();
        let req_body = serde_json::json!({
            "agent_id": "agent-1",
            "allowed_targets": ["screenshot"]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/permissions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("screenshot"));
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_list_permissions_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/screen/permissions")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["permissions"].as_array().unwrap().is_empty());
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_revoke_permission_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/screen/permissions/nonexistent-agent")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_history_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/screen/history")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_permission_workflow() {
        let state = test_state();
        let app = build_router(state.clone());

        // Step 1: Grant permission to "workflow-agent"
        let grant_body = serde_json::json!({
            "agent_id": "workflow-agent",
            "allowed_targets": ["full_screen"],
            "expires_in_secs": 3600,
            "max_captures_per_minute": 10
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/permissions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&grant_body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Step 2: Capture with that agent_id should succeed
        let capture_body = serde_json::json!({
            "target": {"type": "full_screen"},
            "format": "png",
            "agent_id": "workflow-agent"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&capture_body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Step 3: Revoke permission
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/screen/permissions/workflow-agent")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Step 4: Capture should now fail with 403
        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/capture")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&capture_body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ===== Consumer health tests =====

    #[tokio::test]
    async fn test_consumer_health_no_data() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/health/consumers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "healthy");
        assert_eq!(json["total_consumers"], 0);
        assert!(json["consumers"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_consumer_health_with_recent_sync() {
        let state = test_state();
        let app = build_router(state.clone());

        // Sync a dashboard snapshot
        let req_body = serde_json::json!({
            "source": "agnostic",
            "agents": [
                {"name": "qa-manager", "status": "active"},
                {"name": "senior-qa", "status": "idle"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Check consumer health
        let req = Request::builder()
            .uri("/v1/health/consumers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_consumers"], 1);
        let consumer = &json["consumers"][0];
        assert_eq!(consumer["source"], "agnostic");
        assert_eq!(consumer["status"], "healthy");
        assert_eq!(consumer["agents_total"], 2);
        assert_eq!(consumer["agents_error"], 0);
    }

    #[tokio::test]
    async fn test_consumer_health_degraded_with_errors() {
        let state = test_state();
        let app = build_router(state.clone());

        // Sync a snapshot with an erroring agent
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "yeoman-agent", "status": "error"},
                {"name": "yeoman-worker", "status": "active"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/dashboard/sync")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Check consumer health
        let req = Request::builder()
            .uri("/v1/health/consumers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let consumer = &json["consumers"][0];
        assert_eq!(consumer["status"], "degraded");
        assert_eq!(consumer["agents_error"], 1);
    }

    #[tokio::test]
    async fn test_consumer_health_multiple_sources() {
        let state = test_state();
        let app = build_router(state.clone());

        // Sync from two sources
        for source in &["agnostic", "secureyeoman"] {
            let req_body = serde_json::json!({
                "source": source,
                "agents": [{"name": "agent-1", "status": "active"}]
            });
            let req = Request::builder()
                .method("POST")
                .uri("/v1/dashboard/sync")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }

        let req = Request::builder()
            .uri("/v1/health/consumers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_consumers"], 2);
    }

    // -----------------------------------------------------------------------
    // Service discovery (GET /v1/discover)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_service_discovery() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/discover")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["service"], "agnos-agent-runtime");
        assert_eq!(json["codename"], "daimon");
        assert_eq!(json["protocol_version"], "1.0");
        assert!(json["capabilities"].as_array().unwrap().len() > 10);
        assert!(json["endpoints"]["agents_register"].is_string());
        assert!(json["endpoints"]["agents_register_batch"].is_string());
        assert!(json["endpoints"]["events_subscribe"].is_string());
        assert!(json["companion_services"]["llm_gateway"]["codename"] == "hoosh");
        assert!(json["companion_services"]["llm_gateway"]["status"] == "core");
        assert!(json["companion_services"]["agent_runtime"]["status"] == "core");
        assert!(json["companion_services"]["synapse"]["name"] == "synapse");
        assert!(json["companion_services"]["synapse"]["default_url"] == "http://127.0.0.1:8080");
        assert!(json["companion_services"]["synapse"]["status"] == "optional");

        let caps: Vec<String> = json["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(caps.contains(&"model-management".to_string()));
        assert!(caps.contains(&"inference-backend".to_string()));
        assert!(caps.contains(&"training".to_string()));
    }

    // -----------------------------------------------------------------------
    // Batch agent registration (POST /v1/agents/register/batch)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_batch_register_agents() {
        let app = test_app();
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "sy-researcher", "capabilities": ["web:search", "llm:inference"]},
                {"name": "sy-coder", "capabilities": ["code:execute", "git:commit"]},
                {"name": "sy-analyst", "capabilities": ["data:analyze"]}
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["source"], "secureyeoman");
        assert_eq!(json["registered"], 3);
        assert_eq!(json["errors"], 0);
        assert_eq!(json["results"].as_array().unwrap().len(), 3);
        // All should have UUIDs
        for result in json["results"].as_array().unwrap() {
            assert!(result["id"].is_string());
            assert_eq!(result["status"], "registered");
        }
    }

    #[tokio::test]
    async fn test_batch_register_idempotent() {
        let state = test_state();
        let app = build_router(state.clone());

        // First batch registration
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "sy-worker-1", "capabilities": ["llm:inference"]}
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json1: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first_id = json1["results"][0]["id"].as_str().unwrap().to_string();

        // Second batch registration with same agent
        let app2 = build_router(state);
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Should be marked as already_registered with same ID
        assert_eq!(json2["already_registered"], 1);
        assert_eq!(json2["registered"], 0);
        assert_eq!(json2["results"][0]["id"].as_str().unwrap(), first_id);
        assert_eq!(json2["results"][0]["status"], "already_registered");
    }

    #[tokio::test]
    async fn test_batch_register_empty() {
        let app = test_app();
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": []
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_batch_register_mixed_results() {
        let app = test_app();
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "valid-agent", "capabilities": ["llm"]},
                {"name": "", "capabilities": []},
                {"name": "another-valid", "capabilities": ["search"]}
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["registered"], 2);
        assert_eq!(json["errors"], 1);
        // Empty name should have error
        assert_eq!(json["results"][1]["status"], "error");
        assert!(json["results"][1]["error"].is_string());
    }

    // -----------------------------------------------------------------------
    // Event pub/sub (POST /v1/events/publish, GET /v1/events/topics)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_events_publish() {
        let app = test_app();
        let req_body = serde_json::json!({
            "topic": "agent.task.completed",
            "sender": "secureyeoman",
            "payload": {"task_id": "abc123", "duration_ms": 1500},
            "correlation_id": "corr-001"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/events/publish")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["topic"], "agent.task.completed");
        // No subscribers yet, so delivered_to = 0
        assert_eq!(json["delivered_to"], 0);
    }

    #[tokio::test]
    async fn test_events_publish_empty_topic() {
        let app = test_app();
        let req_body = serde_json::json!({
            "topic": "",
            "sender": "secureyeoman",
            "payload": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/events/publish")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_events_topics_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/events/topics")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["total_topics"], 0);
        assert!(json["topics"].as_array().unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Sandbox profile listing (GET /v1/sandbox/profiles/list)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sandbox_profiles_list() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/sandbox/profiles/list")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let profiles = json["profiles"].as_array().unwrap();
        // 6 presets + 1 app-specific (photis-nadi) = 7
        assert_eq!(profiles.len(), 7);
        assert_eq!(json["total"], 7);

        // Verify photis-nadi is included
        let photis = profiles.iter().find(|p| p["preset"] == "photis-nadi");
        assert!(photis.is_some());
        assert_eq!(photis.unwrap()["app_specific"], true);

        // Verify browser preset has high memory
        let browser = profiles.iter().find(|p| p["preset"] == "browser");
        assert!(browser.is_some());
        assert_eq!(browser.unwrap()["max_memory_mb"], 2048);
    }

    // -----------------------------------------------------------------------
    // Batch register + agents list integration
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_batch_register_agents_visible_in_list() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register a batch
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "sy-brain", "capabilities": ["llm:inference", "memory:store"]},
                {"name": "sy-reviewer", "capabilities": ["code:review"]}
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.oneshot(req).await.unwrap();

        // List agents
        let app2 = build_router(state);
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["total"], 2);
        let agents = json["agents"].as_array().unwrap();
        let names: Vec<&str> = agents.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"sy-brain"));
        assert!(names.contains(&"sy-reviewer"));

        // Check source metadata was set
        for agent in agents {
            assert_eq!(agent["metadata"]["source"], "secureyeoman");
        }
    }

    // -----------------------------------------------------------------------
    // Batch heartbeat tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_batch_heartbeat() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register agents first
        let reg_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "hb-agent-1", "capabilities": ["llm"]},
                {"name": "hb-agent-2", "capabilities": ["search"]}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&reg_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id1 = reg_json["results"][0]["id"].as_str().unwrap();
        let id2 = reg_json["results"][1]["id"].as_str().unwrap();

        // Send batch heartbeat
        let app2 = build_router(state);
        let hb_body = serde_json::json!({
            "source": "secureyeoman",
            "heartbeats": [
                {"id": id1, "status": "busy", "cpu_percent": 45.2, "memory_mb": 512},
                {"id": id2, "current_task": "analyzing data"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/heartbeat/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hb_body).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["source"], "secureyeoman");
        assert_eq!(json["updated"], 2);
        assert_eq!(json["not_found"], 0);
        for result in json["results"].as_array().unwrap() {
            assert_eq!(result["status"], "ok");
        }
    }

    #[tokio::test]
    async fn test_batch_heartbeat_empty() {
        let app = test_app();
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "heartbeats": []
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/heartbeat/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_batch_heartbeat_not_found() {
        let app = test_app();
        let fake_id = uuid::Uuid::new_v4();
        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "heartbeats": [
                {"id": fake_id.to_string(), "status": "idle"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/heartbeat/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["updated"], 0);
        assert_eq!(json["not_found"], 1);
        assert_eq!(json["results"][0]["status"], "not_found");
    }

    #[tokio::test]
    async fn test_batch_heartbeat_mixed() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register one agent
        let reg_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [{"name": "hb-mix-agent", "capabilities": ["llm"]}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&reg_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let real_id = reg_json["results"][0]["id"].as_str().unwrap();
        let fake_id = uuid::Uuid::new_v4();

        // Batch heartbeat with one real, one fake
        let app2 = build_router(state);
        let hb_body = serde_json::json!({
            "source": "secureyeoman",
            "heartbeats": [
                {"id": real_id, "status": "active"},
                {"id": fake_id.to_string(), "status": "idle"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/heartbeat/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&hb_body).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["updated"], 1);
        assert_eq!(json["not_found"], 1);
    }

    // -----------------------------------------------------------------------
    // Feature: Client-specified agent IDs (single register)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_register_agent_with_client_id() {
        let app = test_app();
        let client_id = Uuid::new_v4();
        let req_body = serde_json::json!({
            "name": "client-id-agent",
            "id": client_id.to_string(),
            "capabilities": ["llm:inference"]
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
        assert_eq!(json["id"], client_id.to_string());
        assert_eq!(json["name"], "client-id-agent");
    }

    #[tokio::test]
    async fn test_register_agent_client_id_conflict() {
        let state = test_state();
        let app = build_router(state.clone());
        let client_id = Uuid::new_v4();

        // Register first agent with a specific ID
        let req_body = serde_json::json!({
            "name": "first-agent",
            "id": client_id.to_string(),
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Try to register second agent with same ID
        let app2 = build_router(state);
        let req_body2 = serde_json::json!({
            "name": "second-agent",
            "id": client_id.to_string(),
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body2).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // -----------------------------------------------------------------------
    // Feature: Client-specified IDs in batch register
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_batch_register_with_client_ids() {
        let state = test_state();
        let app = build_router(state.clone());
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let req_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "batch-id-1", "id": id1.to_string()},
                {"name": "batch-id-2", "id": id2.to_string()},
                {"name": "batch-id-3"}
            ]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["registered"], 3);

        let results = json["results"].as_array().unwrap();
        assert_eq!(results[0]["id"], id1.to_string());
        assert_eq!(results[1]["id"], id2.to_string());
        // Third agent gets a server-generated ID
        assert!(results[2]["id"].as_str().is_some());
        assert_ne!(results[2]["id"], id1.to_string());
        assert_ne!(results[2]["id"], id2.to_string());
    }

    // -----------------------------------------------------------------------
    // Feature: Batch deregister
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_batch_deregister_by_source() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register agents from a source
        let reg_body = serde_json::json!({
            "source": "secureyeoman",
            "agents": [
                {"name": "sy-to-remove-1"},
                {"name": "sy-to-remove-2"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&reg_body).unwrap()))
            .unwrap();
        app.oneshot(req).await.unwrap();

        // Batch deregister by source
        let app2 = build_router(state.clone());
        let dereg_body = serde_json::json!({
            "source": "secureyeoman"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/deregister/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&dereg_body).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["deregistered"], 2);
        assert_eq!(json["not_found"], 0);

        // Verify agents are gone
        let app3 = build_router(state);
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app3.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_batch_deregister_by_ids() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register agents
        let reg_body = serde_json::json!({
            "source": "test",
            "agents": [
                {"name": "id-deregist-1"},
                {"name": "id-deregist-2"}
            ]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&reg_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id1: Uuid = reg_json["results"][0]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        // Deregister only the first by ID
        let app2 = build_router(state.clone());
        let dereg_body = serde_json::json!({
            "ids": [id1.to_string()]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/deregister/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&dereg_body).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["deregistered"], 1);

        // One agent should remain
        let app3 = build_router(state);
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app3.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_batch_deregister_no_criteria() {
        let app = test_app();
        let dereg_body = serde_json::json!({});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/deregister/batch")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&dereg_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // -----------------------------------------------------------------------
    // Feature: External MCP tool registration
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mcp_register_external_tool() {
        let state = test_state();
        let app = build_router(state.clone());

        let req_body = serde_json::json!({
            "name": "custom_tool",
            "description": "A custom external tool",
            "inputSchema": {"type": "object", "properties": {}},
            "callback_url": "https://mcp-tools.example.com/tools/custom",
            "source": "secureyeoman"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/mcp/tools")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "custom_tool");
        assert_eq!(json["status"], "registered");

        // Verify it appears in the tool manifest
        let app2 = build_router(state);
        let req = Request::builder()
            .uri("/v1/mcp/tools")
            .body(Body::empty())
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "custom_tool"));
    }

    #[tokio::test]
    async fn test_mcp_register_builtin_conflict() {
        let app = test_app();
        let req_body = serde_json::json!({
            "name": "agnos_health",
            "description": "Conflict",
            "inputSchema": {"type": "object"},
            "callback_url": "https://mcp-tools.example.com/nope"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/mcp/tools")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_mcp_deregister_external_tool() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register
        let req_body = serde_json::json!({
            "name": "to_remove_tool",
            "description": "Will be removed",
            "inputSchema": {"type": "object"},
            "callback_url": "https://mcp-tools.example.com/remove"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/mcp/tools")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.oneshot(req).await.unwrap();

        // Deregister
        let app2 = build_router(state.clone());
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/mcp/tools/to_remove_tool")
            .body(Body::empty())
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify it's gone
        let app3 = build_router(state);
        let req = Request::builder()
            .uri("/v1/mcp/tools")
            .body(Body::empty())
            .unwrap();
        let resp = app3.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["tools"].as_array().unwrap();
        assert!(!tools.iter().any(|t| t["name"] == "to_remove_tool"));
    }

    #[tokio::test]
    async fn test_mcp_deregister_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/mcp/tools/nonexistent_tool")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mcp_register_replaces_existing() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register tool
        let req_body = serde_json::json!({
            "name": "replaceable_tool",
            "description": "version 1",
            "inputSchema": {"type": "object"},
            "callback_url": "https://mcp-tools.example.com/v1"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/mcp/tools")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.oneshot(req).await.unwrap();

        // Register again with new description
        let app2 = build_router(state.clone());
        let req_body2 = serde_json::json!({
            "name": "replaceable_tool",
            "description": "version 2",
            "inputSchema": {"type": "object"},
            "callback_url": "https://mcp-tools.example.com/v2"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/mcp/tools")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body2).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Verify only one instance and it's the updated one
        let app3 = build_router(state);
        let req = Request::builder()
            .uri("/v1/mcp/tools")
            .body(Body::empty())
            .unwrap();
        let resp = app3.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["tools"].as_array().unwrap();
        let matching: Vec<_> = tools
            .iter()
            .filter(|t| t["name"] == "replaceable_tool")
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0]["description"], "version 2");
    }

    // -----------------------------------------------------------------------
    // Feature: Sandbox profile CRUD
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sandbox_custom_profile_crud() {
        let state = test_state();

        // Create a custom profile
        let app = build_router(state.clone());
        let profile_body = serde_json::json!({
            "config": {
                "filesystem_rules": [
                    {"path": "/tmp", "access": "ReadWrite"}
                ],
                "network_access": "LocalhostOnly",
                "seccomp_rules": [],
                "isolate_network": true
            },
            "description": "Test custom profile",
            "created_by": "secureyeoman"
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/v1/sandbox/profiles/custom/my-profile")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&profile_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Get the profile
        let app2 = build_router(state.clone());
        let req = Request::builder()
            .uri("/v1/sandbox/profiles/custom/my-profile")
            .body(Body::empty())
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "my-profile");
        assert_eq!(json["description"], "Test custom profile");

        // List custom profiles
        let app3 = build_router(state.clone());
        let req = Request::builder()
            .uri("/v1/sandbox/profiles/custom")
            .body(Body::empty())
            .unwrap();
        let resp = app3.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 1);

        // Update the profile
        let app4 = build_router(state.clone());
        let updated_body = serde_json::json!({
            "config": {
                "filesystem_rules": [],
                "network_access": "None",
                "seccomp_rules": [],
                "isolate_network": true
            },
            "description": "Updated profile"
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/v1/sandbox/profiles/custom/my-profile")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&updated_body).unwrap()))
            .unwrap();
        let resp = app4.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK); // 200 for update
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "updated");

        // Delete the profile
        let app5 = build_router(state.clone());
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/sandbox/profiles/custom/my-profile")
            .body(Body::empty())
            .unwrap();
        let resp = app5.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify it's gone
        let app6 = build_router(state);
        let req = Request::builder()
            .uri("/v1/sandbox/profiles/custom/my-profile")
            .body(Body::empty())
            .unwrap();
        let resp = app6.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_sandbox_custom_profile_delete_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/sandbox/profiles/custom/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // Feature: Event publish uses sender field
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_event_publish_echoes_sender() {
        let app = test_app();
        let req_body = serde_json::json!({
            "topic": "test.event",
            "sender": "my-service",
            "payload": {"key": "value"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/events/publish")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["topic"], "test.event");
        assert_eq!(json["sender"], "my-service");
        assert!(json["sender_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_event_publish_sender_resolves_agent() {
        let state = test_state();
        let app = build_router(state.clone());

        // Register an agent
        let reg_body = serde_json::json!({
            "name": "event-sender-agent",
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&reg_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let reg_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let agent_id = reg_json["id"].as_str().unwrap();

        // Publish event using agent name as sender
        let app2 = build_router(state);
        let req_body = serde_json::json!({
            "topic": "test.resolve",
            "sender": "event-sender-agent",
            "payload": {}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/events/publish")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["sender"], "event-sender-agent");
        assert_eq!(json["sender_id"], agent_id);
    }

    // -----------------------------------------------------------------------
    // RPC handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rpc_list_methods_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/rpc/methods")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["methods"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_rpc_register_and_list() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({
            "agent_id": agent_id,
            "methods": ["greet", "compute"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "registered");
        assert_eq!(json["methods"].as_array().unwrap().len(), 2);

        // List all methods
        let req = Request::builder()
            .uri("/v1/rpc/methods")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["methods"].as_array().unwrap().len(), 2);

        // List methods for specific agent
        let req = Request::builder()
            .uri(format!("/v1/rpc/methods/{}", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["agent_id"], agent_id);
        assert_eq!(json["methods"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_rpc_register_invalid_uuid() {
        let app = test_app();
        let req_body = serde_json::json!({
            "agent_id": "not-a-uuid",
            "methods": ["test"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rpc_call_found() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        // Register a method
        let req_body = serde_json::json!({
            "agent_id": agent_id,
            "methods": ["hello"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Call it
        let req_body = serde_json::json!({
            "method": "hello",
            "params": {"name": "test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/call")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "routed");
        assert_eq!(json["method"], "hello");
    }

    #[tokio::test]
    async fn test_rpc_call_not_found() {
        let app = test_app();
        let req_body = serde_json::json!({
            "method": "nonexistent",
            "params": {}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/call")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_rpc_agent_methods_invalid_uuid() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/rpc/methods/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // -----------------------------------------------------------------------
    // Anomaly detection handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_anomaly_submit_sample() {
        let app = test_app();
        let agent_id = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({
            "agent_id": agent_id,
            "syscall_count": 100,
            "network_bytes": 5000,
            "file_ops": 50,
            "cpu_percent": 25.0,
            "memory_bytes": 1048576
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/anomaly/sample")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "recorded");
        assert_eq!(json["agent_id"], agent_id);
        assert!(json["alerts"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_anomaly_submit_invalid_uuid() {
        let app = test_app();
        let req_body = serde_json::json!({
            "agent_id": "bad-uuid",
            "syscall_count": 10,
            "network_bytes": 0,
            "file_ops": 0,
            "cpu_percent": 0.0,
            "memory_bytes": 0
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/anomaly/sample")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_anomaly_alerts_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/anomaly/alerts")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 0);
        assert!(json["alerts"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_anomaly_baseline_not_found() {
        let app = test_app();
        let agent_id = Uuid::new_v4().to_string();
        let req = Request::builder()
            .uri(format!("/v1/anomaly/baseline/{}", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_anomaly_baseline_after_samples() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        // Submit a sample first to create baseline
        let req_body = serde_json::json!({
            "agent_id": agent_id,
            "syscall_count": 100,
            "network_bytes": 5000,
            "file_ops": 50,
            "cpu_percent": 25.0,
            "memory_bytes": 1048576
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/anomaly/sample")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Now check baseline
        let req = Request::builder()
            .uri(format!("/v1/anomaly/baseline/{}", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["agent_id"], agent_id);
        assert!(json["sample_count"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn test_anomaly_baseline_invalid_uuid() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/anomaly/baseline/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_anomaly_clear_alerts() {
        let state = test_state();
        let app = build_router(state.clone());
        let agent_id = Uuid::new_v4().to_string();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/anomaly/alerts/{}", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "cleared");
    }

    #[tokio::test]
    async fn test_anomaly_clear_invalid_uuid() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/anomaly/alerts/not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // -----------------------------------------------------------------------
    // RAG & Knowledge Base handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rag_ingest_and_query() {
        let state = test_state();
        let app = build_router(state.clone());

        // Ingest
        let req_body = serde_json::json!({
            "text": "AGNOS is an AI-native operating system built from source.",
            "metadata": {"source": "docs"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/ingest")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ingested");
        assert!(json["chunks"].as_u64().unwrap() > 0);

        // Query
        let req_body = serde_json::json!({
            "query": "what is AGNOS?",
            "top_k": 3
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/query")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["query"], "what is AGNOS?");
        assert!(json["chunks"].as_array().is_some());
        assert!(json["formatted_context"].is_string());
        assert!(json["token_estimate"].is_number());
    }

    #[tokio::test]
    async fn test_rag_ingest_too_large() {
        let app = test_app();
        // 1 MB + 1 byte exceeds limit
        let big_text = "x".repeat(1_048_577);
        let req_body = serde_json::json!({
            "text": big_text,
            "metadata": {}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/ingest")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_rag_query_too_large() {
        let app = test_app();
        let big_query = "x".repeat(10_241);
        let req_body = serde_json::json!({
            "query": big_query
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/query")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rag_stats() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/rag/stats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["index_size"].is_number());
        assert!(json["config"].is_object());
    }

    #[tokio::test]
    async fn test_knowledge_search_empty() {
        let app = test_app();
        let req_body = serde_json::json!({
            "query": "test search",
            "limit": 5
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["query"], "test search");
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_knowledge_search_by_source() {
        let app = test_app();
        let req_body = serde_json::json!({
            "query": "anything",
            "source": "manpage",
            "limit": 5
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
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
    async fn test_knowledge_stats() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/knowledge/stats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["total_entries"].is_number());
        assert!(json["total_bytes"].is_number());
    }

    // -----------------------------------------------------------------------
    // Marketplace handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_marketplace_installed_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/marketplace/installed")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["packages"].as_array().unwrap().is_empty());
        assert_eq!(json["total"], 0);
    }

    #[tokio::test]
    async fn test_marketplace_search_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/marketplace/search?q=nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["results"].as_array().is_some());
        assert_eq!(json["query"], "nonexistent");
    }

    #[tokio::test]
    async fn test_marketplace_search_default_query() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/marketplace/search")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["query"], "");
    }

    #[tokio::test]
    async fn test_marketplace_info_not_found() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/marketplace/nonexistent-pkg")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_marketplace_uninstall_not_found() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/marketplace/nonexistent-pkg")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // Database handler tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_database_provision() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({
            "postgres": true,
            "redis": false,
            "schema": "public",
            "storage_quota": 104857600,
            "extensions": ["uuid-ossp"]
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/database", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "provisioned");
        assert!(json["database"].is_object());
        assert!(json["provision_sql"].is_array());
    }

    #[tokio::test]
    async fn test_database_provision_duplicate() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({"postgres": true});

        // First provision
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/database", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Duplicate provision
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/database", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_database_provision_invalid_uuid() {
        let app = test_app();
        let req_body = serde_json::json!({"postgres": true});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/not-a-uuid/database")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_database_get_after_provision() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({"postgres": true, "redis": true});
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/database", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // GET
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/database", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["database"].is_object());
    }

    #[tokio::test]
    async fn test_database_get_not_found() {
        let app = test_app();
        let agent_id = Uuid::new_v4().to_string();
        let req = Request::builder()
            .uri(format!("/v1/agents/{}/database", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_database_deprovision() {
        let state = test_state();
        let app = build_router(state.clone());

        let agent_id = Uuid::new_v4().to_string();
        let req_body = serde_json::json!({"postgres": true});
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/agents/{}/database", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();

        // Deprovision
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/agents/{}/database", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "deprovisioned");
        assert!(json["cleanup_sql"].is_array());
    }

    #[tokio::test]
    async fn test_database_deprovision_invalid_uuid() {
        let app = test_app();
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/agents/not-a-uuid/database")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_database_stats() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/database/stats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["stats"].is_object());
    }

    // ================================================================
    // Auth middleware tests
    // ================================================================

    fn test_state_with_auth() -> ApiState {
        ApiState::with_api_key(Some("test-secret-key".to_string()))
    }

    fn test_app_with_auth() -> Router {
        build_router(test_state_with_auth())
    }

    #[tokio::test]
    async fn test_auth_health_no_token_allowed() {
        let app = test_app_with_auth();
        let req = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Health endpoint should be accessible without auth
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_missing_token_rejected() {
        let app = test_app_with_auth();
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], 401);
    }

    #[tokio::test]
    async fn test_auth_invalid_token_rejected() {
        let app = test_app_with_auth();
        let req = Request::builder()
            .uri("/v1/agents")
            .header("authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("Invalid"));
    }

    #[tokio::test]
    async fn test_auth_valid_token_accepted() {
        let app = test_app_with_auth();
        let req = Request::builder()
            .uri("/v1/agents")
            .header("authorization", "Bearer test-secret-key")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_malformed_header_rejected() {
        let app = test_app_with_auth();
        let req = Request::builder()
            .uri("/v1/agents")
            .header("authorization", "Basic dXNlcjpwYXNz")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_no_key_dev_mode_passes() {
        // Default test_app has no api_key — dev mode
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ================================================================
    // Additional handler coverage: marketplace with installed packages
    // ================================================================

    #[tokio::test]
    async fn test_marketplace_search_with_query() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/marketplace/search?q=test-agent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["results"].is_array());
        assert_eq!(json["query"], "test-agent");
    }

    #[tokio::test]
    async fn test_marketplace_install_invalid_path() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/marketplace/install")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "name": "test-pkg",
                    "version": "1.0",
                    "path": "/nonexistent/path/to/package.tar"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should fail: path doesn't exist, so canonicalize fails
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_marketplace_install_path_traversal() {
        // Create a temp file to pass canonicalize, but outside allowed dirs
        let tmp = std::env::temp_dir().join("agnos-test-install.tar");
        std::fs::write(&tmp, b"fake").ok();
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/marketplace/install")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "name": "test-pkg",
                    "version": "1.0",
                    "path": tmp.to_str().unwrap()
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should be FORBIDDEN: /tmp/ is not in allowed prefixes (/var/agnos/, /tmp/agnos/)
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let _ = std::fs::remove_file(&tmp);
    }

    // ================================================================
    // Batch register tests (handshake)
    // ================================================================

    #[tokio::test]
    async fn test_batch_register_empty_list() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "agents": [],
                    "source": "test"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_batch_register_too_many() {
        let agents: Vec<serde_json::Value> = (0..101)
            .map(|i| serde_json::json!({"name": format!("agent-{}", i)}))
            .collect();
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "agents": agents,
                    "source": "test"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_batch_register_with_client_id_conflict() {
        let app = test_app();
        let dup_id = Uuid::new_v4();

        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents/register/batch")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "agents": [
                        {"name": "first", "id": dup_id.to_string()},
                        {"name": "second", "id": dup_id.to_string()}
                    ],
                    "source": "test"
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
        let results = json["results"].as_array().unwrap();
        // First should succeed, second should get "error" (ID already in use)
        assert_eq!(results[0]["status"], "registered");
        assert_eq!(results[1]["status"], "error");
    }

    // ================================================================
    // Events topics (handshake)
    // ================================================================

    #[tokio::test]
    async fn test_events_publish_with_correlation() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/events/publish")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "topic": "test.correlated",
                    "sender": "test-service",
                    "payload": {"data": 42},
                    "correlation_id": "req-123",
                    "reply_to": "test.reply"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_events_topics_list() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/events/topics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["topics"].is_array());
    }

    // ================================================================
    // Knowledge index path traversal
    // ================================================================

    #[tokio::test]
    async fn test_knowledge_index_invalid_path() {
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/index")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "path": "/nonexistent/dir",
                    "query": ""
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_knowledge_index_path_traversal() {
        let tmp = std::env::temp_dir().join("agnos-test-kb-index");
        std::fs::create_dir_all(&tmp).ok();
        let app = test_app();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/index")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "path": tmp.to_str().unwrap(),
                    "query": ""
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should be FORBIDDEN: /tmp/ not in allowed prefixes
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let _ = std::fs::remove_dir(&tmp);
    }

    // ================================================================
    // Screen capture permission tests
    // ================================================================

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_permissions_list_empty() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/screen/permissions")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_grant_permission() {
        let app = test_app();
        let agent_id = Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/screen/permissions")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "agent_id": agent_id.to_string(),
                    "capture_type": "full_screen",
                    "max_fps": 5
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Handler may require registered agent — just verify it doesn't 500
        assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_capture_revoke_nonexistent() {
        let app = test_app();
        let agent_id = Uuid::new_v4();
        let req = Request::builder()
            .method("DELETE")
            .uri(&format!("/v1/screen/permissions/{}", agent_id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should either succeed (idempotent) or 404
        assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_screen_history_list() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/screen/history")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // H6: RAG ingest per-agent rate limiting
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rag_ingest_rate_limit() {
        let state = test_state();

        // Directly fill the rate limit counter to just under the limit
        {
            let mut limits = state.rag_ingest_rate_limits.lock().await;
            limits.insert(
                "rate-test-agent".to_string(),
                (99, std::time::Instant::now()),
            );
        }

        let app = build_router(state);

        // The 100th request should succeed (at the limit)
        let body = serde_json::json!({
            "text": "request at limit",
            "agent_id": "rate-test-agent"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/ingest")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // The 101st request should be rate-limited
        let body = serde_json::json!({
            "text": "one too many",
            "agent_id": "rate-test-agent"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/ingest")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["code"], 429);
        assert!(json["error"].as_str().unwrap().contains("Rate limit"));

        // A different agent should still be allowed
        let body = serde_json::json!({
            "text": "from another agent",
            "agent_id": "other-agent"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rag/ingest")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // H9: Reasoning trace size limit
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_reasoning_trace_too_large() {
        let app = test_app();
        let agent_id = Uuid::new_v4();

        // Create a trace with content exceeding 1 MB
        let large_content = "x".repeat(1_100_000);
        let body = serde_json::json!({
            "task": "test task",
            "steps": [{
                "step": 1,
                "kind": "thought",
                "content": large_content
            }],
            "duration_ms": 100
        });

        let req = Request::builder()
            .method("POST")
            .uri(&format!("/v1/agents/{}/reasoning", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["code"], 413);
        assert!(json["error"].as_str().unwrap().contains("too large"));
    }

    #[tokio::test]
    async fn test_reasoning_trace_within_limit() {
        let app = test_app();
        let agent_id = Uuid::new_v4();

        let body = serde_json::json!({
            "task": "small task",
            "steps": [{
                "step": 1,
                "kind": "thought",
                "content": "a short thought"
            }],
            "duration_ms": 50
        });

        let req = Request::builder()
            .method("POST")
            .uri(&format!("/v1/agents/{}/reasoning", agent_id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // -----------------------------------------------------------------------
    // H10: Knowledge source name injection prevention
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_knowledge_search_rejects_invalid_source_name() {
        let app = test_app();

        // Path separator in source name
        let body = serde_json::json!({
            "query": "test",
            "source": "../etc/passwd"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Name too long
        let long_name = "a".repeat(200);
        let body = serde_json::json!({
            "query": "test",
            "source": long_name
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_knowledge_search_accepts_valid_custom_source() {
        let app = test_app();

        let body = serde_json::json!({
            "query": "test",
            "source": "my_custom-source123"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/knowledge/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // H11: RPC method name validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rpc_register_rejects_invalid_method_name() {
        let app = test_app();
        let agent_id = Uuid::new_v4();

        // Method name with special characters
        let body = serde_json::json!({
            "agent_id": agent_id.to_string(),
            "methods": ["valid.method", "bad/method!"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("alphanumeric"));
    }

    #[tokio::test]
    async fn test_rpc_register_rejects_too_long_method_name() {
        let app = test_app();
        let agent_id = Uuid::new_v4();

        let long_name = "a".repeat(300);
        let body = serde_json::json!({
            "agent_id": agent_id.to_string(),
            "methods": [long_name]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rpc_call_rejects_invalid_method_name() {
        let app = test_app();

        let body = serde_json::json!({
            "method": "bad method!",
            "params": {}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/call")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rpc_register_accepts_valid_method_names() {
        let app = test_app();
        let agent_id = Uuid::new_v4();

        let body = serde_json::json!({
            "agent_id": agent_id.to_string(),
            "methods": ["agent.do_work", "my-method", "simple_call"]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/rpc/register")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // H17: Bounded audit/trace buffers with FIFO eviction
    #[tokio::test]
    async fn test_audit_buffer_fifo_eviction() {
        let state = test_state();
        let max = crate::http_api::MAX_AUDIT_BUFFER;
        {
            let mut buf = state.audit_buffer.write().await;
            for i in 0..max {
                buf.push_back(AuditEvent {
                    timestamp: Utc::now().to_rfc3339(),
                    action: format!("action-{}", i),
                    agent: None,
                    details: serde_json::Value::Null,
                    outcome: "success".to_string(),
                });
            }
            assert_eq!(buf.len(), max);
        }
        state
            .push_audit_event(AuditEvent {
                timestamp: Utc::now().to_rfc3339(),
                action: "overflow-action".to_string(),
                agent: None,
                details: serde_json::Value::Null,
                outcome: "success".to_string(),
            })
            .await;
        let buf = state.audit_buffer.read().await;
        assert_eq!(buf.len(), max);
        assert_ne!(buf.front().unwrap().action, "action-0");
        assert_eq!(buf.back().unwrap().action, "overflow-action");
    }

    #[tokio::test]
    async fn test_trace_buffer_fifo_eviction() {
        let state = test_state();
        let max = crate::http_api::MAX_TRACES;
        {
            let mut traces = state.traces.write().await;
            for i in 0..max {
                traces.push_back(serde_json::json!({"id": i}));
            }
            assert_eq!(traces.len(), max);
        }
        state
            .push_trace(serde_json::json!({"id": "overflow"}))
            .await;
        let traces = state.traces.read().await;
        assert_eq!(traces.len(), max);
        assert_ne!(traces.front().unwrap()["id"], 0);
        assert_eq!(traces.back().unwrap()["id"], "overflow");
    }

    // H22: Marketplace install transaction isolation
    #[tokio::test]
    async fn test_marketplace_staged_install_nonexistent_tarball() {
        let tmp = tempfile::TempDir::new().unwrap();
        let registry = crate::marketplace::local_registry::LocalRegistry::new(tmp.path()).unwrap();
        let registry_arc = std::sync::Arc::new(tokio::sync::RwLock::new(registry));
        let fake_tarball = tmp.path().join("nonexistent.tar.gz");
        let result = crate::http_api::handlers::marketplace::staged_install(
            &registry_arc,
            &fake_tarball,
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Failed") || err.contains("stage"),
            "error should indicate staging failure: {}",
            err
        );
    }
}
