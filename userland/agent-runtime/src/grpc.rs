//! gRPC API for AGNOS Agent Runtime
//!
//! Defines proto-compatible Rust types and service definitions for a gRPC
//! interface alongside the existing REST API on port 8090. The gRPC server
//! runs on a separate port (default 8091) and mirrors the core REST endpoints.
//!
//! No external protobuf dependency — types are defined in Rust with serde for
//! JSON↔proto compatibility. When tonic is added (post-alpha), these types
//! will be generated from `.proto` files but remain API-compatible.

use std::collections::HashMap;
use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// gRPC Service Definitions
// ---------------------------------------------------------------------------

/// gRPC service descriptor — mirrors the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcServiceDefinition {
    /// Service name (e.g., "AgentService", "HealthService").
    pub name: String,
    /// Service package (e.g., "agnos.runtime.v1").
    pub package: String,
    /// RPC methods exposed by this service.
    pub methods: Vec<GrpcMethod>,
}

/// A single gRPC method (unary, server-streaming, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcMethod {
    /// Method name (e.g., "RegisterAgent").
    pub name: String,
    /// Input message type name.
    pub input_type: String,
    /// Output message type name.
    pub output_type: String,
    /// Streaming mode.
    pub streaming: StreamingMode,
    /// REST equivalent (for documentation).
    pub rest_equivalent: String,
}

/// gRPC streaming modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamingMode {
    /// Standard request-response.
    Unary,
    /// Server pushes a stream of responses.
    ServerStreaming,
    /// Client sends a stream of requests.
    ClientStreaming,
    /// Both sides stream.
    Bidirectional,
}

// ---------------------------------------------------------------------------
// Proto-compatible Message Types
// ---------------------------------------------------------------------------

/// Agent registration request (proto-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Agent registration response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub name: String,
    pub status: String,
    pub registered_at: String,
}

/// Agent details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub registered_at: String,
    pub last_heartbeat: Option<String>,
    pub current_task: Option<String>,
}

/// Heartbeat request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub agent_id: String,
    pub status: Option<String>,
    pub current_task: Option<String>,
    pub cpu_percent: Option<f32>,
    pub memory_mb: Option<u64>,
}

/// Health response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub agents_registered: u32,
    pub uptime_seconds: u64,
    pub grpc_port: u16,
    pub rest_port: u16,
}

/// Vector search request (proto-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchRequest {
    pub collection: String,
    pub embedding: Vec<f64>,
    pub top_k: u32,
    pub min_score: Option<f64>,
}

/// Vector search result entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f64,
    pub content: String,
    pub metadata: serde_json::Value,
}

/// Event for server-streaming EventSubscribe RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMessage {
    pub topic: String,
    pub sender: String,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Server Configuration
// ---------------------------------------------------------------------------

/// gRPC server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// Whether the gRPC server is enabled.
    pub enabled: bool,
    /// Address to bind the gRPC server to.
    pub bind_addr: SocketAddr,
    /// Maximum message size in bytes (default: 4 MB).
    pub max_message_size: usize,
    /// Enable gRPC reflection for service discovery.
    pub reflection: bool,
    /// Enable gRPC health check service (grpc.health.v1).
    pub health_service: bool,
    /// TLS configuration (uses mTLS from mtls.rs when enabled).
    pub tls: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: "0.0.0.0:8091".parse().unwrap(),
            max_message_size: 4 * 1024 * 1024,
            reflection: true,
            health_service: true,
            tls: false,
        }
    }
}

impl GrpcConfig {
    /// Parse from TOML configuration.
    pub fn from_toml(toml_str: &str) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Wrapper {
            grpc: GrpcConfigToml,
        }
        #[derive(Deserialize)]
        struct GrpcConfigToml {
            enabled: Option<bool>,
            bind_addr: Option<String>,
            max_message_size: Option<usize>,
            reflection: Option<bool>,
            health_service: Option<bool>,
            tls: Option<bool>,
        }

        let wrapper: Wrapper = toml::from_str(toml_str)?;
        let c = wrapper.grpc;

        Ok(Self {
            enabled: c.enabled.unwrap_or(false),
            bind_addr: c
                .bind_addr
                .unwrap_or_else(|| "0.0.0.0:8091".to_string())
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid bind_addr: {}", e))?,
            max_message_size: c.max_message_size.unwrap_or(4 * 1024 * 1024),
            reflection: c.reflection.unwrap_or(true),
            health_service: c.health_service.unwrap_or(true),
            tls: c.tls.unwrap_or(false),
        })
    }
}

// ---------------------------------------------------------------------------
// Service Registry
// ---------------------------------------------------------------------------

/// Build the full gRPC service manifest (mirrors REST API).
pub fn build_service_manifest() -> Vec<GrpcServiceDefinition> {
    vec![
        GrpcServiceDefinition {
            name: "AgentService".to_string(),
            package: "agnos.runtime.v1".to_string(),
            methods: vec![
                GrpcMethod {
                    name: "RegisterAgent".to_string(),
                    input_type: "RegisterAgentRequest".to_string(),
                    output_type: "RegisterAgentResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "POST /v1/agents/register".to_string(),
                },
                GrpcMethod {
                    name: "ListAgents".to_string(),
                    input_type: "Empty".to_string(),
                    output_type: "ListAgentsResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "GET /v1/agents".to_string(),
                },
                GrpcMethod {
                    name: "GetAgent".to_string(),
                    input_type: "GetAgentRequest".to_string(),
                    output_type: "AgentInfo".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "GET /v1/agents/:id".to_string(),
                },
                GrpcMethod {
                    name: "Heartbeat".to_string(),
                    input_type: "HeartbeatRequest".to_string(),
                    output_type: "HeartbeatResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "POST /v1/agents/:id/heartbeat".to_string(),
                },
                GrpcMethod {
                    name: "DeregisterAgent".to_string(),
                    input_type: "DeregisterRequest".to_string(),
                    output_type: "DeregisterResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "DELETE /v1/agents/:id".to_string(),
                },
            ],
        },
        GrpcServiceDefinition {
            name: "HealthService".to_string(),
            package: "agnos.runtime.v1".to_string(),
            methods: vec![
                GrpcMethod {
                    name: "Check".to_string(),
                    input_type: "HealthCheckRequest".to_string(),
                    output_type: "HealthResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "GET /v1/health".to_string(),
                },
                GrpcMethod {
                    name: "Watch".to_string(),
                    input_type: "HealthCheckRequest".to_string(),
                    output_type: "HealthResponse".to_string(),
                    streaming: StreamingMode::ServerStreaming,
                    rest_equivalent: "GET /v1/health (polling)".to_string(),
                },
            ],
        },
        GrpcServiceDefinition {
            name: "VectorService".to_string(),
            package: "agnos.runtime.v1".to_string(),
            methods: vec![
                GrpcMethod {
                    name: "Search".to_string(),
                    input_type: "VectorSearchRequest".to_string(),
                    output_type: "VectorSearchResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "POST /v1/vectors/search".to_string(),
                },
                GrpcMethod {
                    name: "Insert".to_string(),
                    input_type: "VectorInsertRequest".to_string(),
                    output_type: "VectorInsertResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "POST /v1/vectors/insert".to_string(),
                },
                GrpcMethod {
                    name: "StreamSearch".to_string(),
                    input_type: "VectorSearchRequest".to_string(),
                    output_type: "VectorSearchResult".to_string(),
                    streaming: StreamingMode::ServerStreaming,
                    rest_equivalent: "POST /v1/vectors/search (streaming)".to_string(),
                },
            ],
        },
        GrpcServiceDefinition {
            name: "EventService".to_string(),
            package: "agnos.runtime.v1".to_string(),
            methods: vec![
                GrpcMethod {
                    name: "Subscribe".to_string(),
                    input_type: "SubscribeRequest".to_string(),
                    output_type: "EventMessage".to_string(),
                    streaming: StreamingMode::ServerStreaming,
                    rest_equivalent: "GET /v1/events/subscribe (SSE)".to_string(),
                },
                GrpcMethod {
                    name: "Publish".to_string(),
                    input_type: "PublishRequest".to_string(),
                    output_type: "PublishResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "POST /v1/events/publish".to_string(),
                },
            ],
        },
        GrpcServiceDefinition {
            name: "McpService".to_string(),
            package: "agnos.runtime.v1".to_string(),
            methods: vec![
                GrpcMethod {
                    name: "ListTools".to_string(),
                    input_type: "Empty".to_string(),
                    output_type: "ToolManifest".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "GET /v1/mcp/tools".to_string(),
                },
                GrpcMethod {
                    name: "CallTool".to_string(),
                    input_type: "ToolCallRequest".to_string(),
                    output_type: "ToolCallResponse".to_string(),
                    streaming: StreamingMode::Unary,
                    rest_equivalent: "POST /v1/mcp/tools/call".to_string(),
                },
            ],
        },
    ]
}

/// Get total method count across all services.
pub fn total_grpc_methods() -> usize {
    build_service_manifest()
        .iter()
        .map(|s| s.methods.len())
        .sum()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_config_default() {
        let config = GrpcConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.bind_addr, "0.0.0.0:8091".parse::<SocketAddr>().unwrap());
        assert_eq!(config.max_message_size, 4 * 1024 * 1024);
        assert!(config.reflection);
        assert!(config.health_service);
        assert!(!config.tls);
    }

    #[test]
    fn test_grpc_config_from_toml() {
        let toml = r#"
[grpc]
enabled = true
bind_addr = "0.0.0.0:9091"
max_message_size = 8388608
reflection = false
tls = true
"#;
        let config = GrpcConfig::from_toml(toml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.bind_addr, "0.0.0.0:9091".parse::<SocketAddr>().unwrap());
        assert_eq!(config.max_message_size, 8_388_608);
        assert!(!config.reflection);
        assert!(config.tls);
    }

    #[test]
    fn test_grpc_config_from_toml_defaults() {
        let toml = r#"
[grpc]
enabled = true
"#;
        let config = GrpcConfig::from_toml(toml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.bind_addr, "0.0.0.0:8091".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_grpc_config_invalid_addr() {
        let toml = r#"
[grpc]
bind_addr = "not-an-address"
"#;
        assert!(GrpcConfig::from_toml(toml).is_err());
    }

    #[test]
    fn test_service_manifest_structure() {
        let manifest = build_service_manifest();
        assert_eq!(manifest.len(), 5);

        let names: Vec<&str> = manifest.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"AgentService"));
        assert!(names.contains(&"HealthService"));
        assert!(names.contains(&"VectorService"));
        assert!(names.contains(&"EventService"));
        assert!(names.contains(&"McpService"));
    }

    #[test]
    fn test_all_services_have_package() {
        let manifest = build_service_manifest();
        for svc in &manifest {
            assert_eq!(svc.package, "agnos.runtime.v1");
        }
    }

    #[test]
    fn test_total_grpc_methods() {
        // Agent(5) + Health(2) + Vector(3) + Event(2) + Mcp(2) = 14
        assert_eq!(total_grpc_methods(), 14);
    }

    #[test]
    fn test_agent_service_methods() {
        let manifest = build_service_manifest();
        let agent_svc = manifest.iter().find(|s| s.name == "AgentService").unwrap();
        assert_eq!(agent_svc.methods.len(), 5);

        let method_names: Vec<&str> = agent_svc.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(method_names.contains(&"RegisterAgent"));
        assert!(method_names.contains(&"ListAgents"));
        assert!(method_names.contains(&"GetAgent"));
        assert!(method_names.contains(&"Heartbeat"));
        assert!(method_names.contains(&"DeregisterAgent"));
    }

    #[test]
    fn test_streaming_methods() {
        let manifest = build_service_manifest();
        let streaming: Vec<&GrpcMethod> = manifest
            .iter()
            .flat_map(|s| &s.methods)
            .filter(|m| m.streaming == StreamingMode::ServerStreaming)
            .collect();

        assert_eq!(streaming.len(), 3);
        let names: Vec<&str> = streaming.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Watch"));
        assert!(names.contains(&"StreamSearch"));
        assert!(names.contains(&"Subscribe"));
    }

    #[test]
    fn test_rest_equivalents_populated() {
        let manifest = build_service_manifest();
        for svc in &manifest {
            for method in &svc.methods {
                assert!(!method.rest_equivalent.is_empty(),
                    "Missing REST equivalent for {}.{}", svc.name, method.name);
            }
        }
    }

    #[test]
    fn test_message_types_serializable() {
        let req = RegisterAgentRequest {
            name: "test".to_string(),
            capabilities: vec!["read".to_string()],
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: RegisterAgentRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
    }

    #[test]
    fn test_health_response_serializable() {
        let resp = HealthResponse {
            status: "serving".to_string(),
            agents_registered: 5,
            uptime_seconds: 3600,
            grpc_port: 8091,
            rest_port: 8090,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("8091"));
    }

    #[test]
    fn test_event_message_serializable() {
        let msg = EventMessage {
            topic: "agent.registered".to_string(),
            sender: "daimon".to_string(),
            payload: serde_json::json!({"agent_id": "abc"}),
            correlation_id: Some("corr-1".to_string()),
            timestamp: "2026-03-10T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: EventMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.topic, "agent.registered");
    }

    #[test]
    fn test_streaming_mode_variants() {
        assert_eq!(
            serde_json::to_string(&StreamingMode::Unary).unwrap(),
            "\"Unary\""
        );
        assert_eq!(
            serde_json::to_string(&StreamingMode::ServerStreaming).unwrap(),
            "\"ServerStreaming\""
        );
        assert_eq!(
            serde_json::to_string(&StreamingMode::Bidirectional).unwrap(),
            "\"Bidirectional\""
        );
    }

    #[test]
    fn test_vector_search_request_serializable() {
        let req = VectorSearchRequest {
            collection: "embeddings".to_string(),
            embedding: vec![1.0, 2.0, 3.0],
            top_k: 10,
            min_score: Some(0.5),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: VectorSearchRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.collection, "embeddings");
        assert_eq!(deserialized.top_k, 10);
    }
}
