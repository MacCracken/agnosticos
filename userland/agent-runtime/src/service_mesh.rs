//! Service Mesh Readiness for AGNOS Agent Runtime
//!
//! Provides configuration and metadata for running AGNOS behind service mesh
//! proxies (Envoy, Linkerd, Istio). Generates sidecar injection annotations,
//! health/readiness probe definitions, and mesh-compatible service descriptors.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Mesh Provider
// ---------------------------------------------------------------------------

/// Supported service mesh implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MeshProvider {
    /// Envoy-based (Istio, standalone Envoy).
    Envoy,
    /// Linkerd (Rust-based, lightweight).
    Linkerd,
    /// No mesh — direct networking.
    #[default]
    None,
}

impl std::fmt::Display for MeshProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envoy => write!(f, "envoy"),
            Self::Linkerd => write!(f, "linkerd"),
            Self::None => write!(f, "none"),
        }
    }
}

// ---------------------------------------------------------------------------
// Health Probe Definitions
// ---------------------------------------------------------------------------

/// HTTP health probe configuration for mesh sidecar injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthProbe {
    /// Probe path (e.g., "/v1/health").
    pub path: String,
    /// Port to probe.
    pub port: u16,
    /// Initial delay before first probe (seconds).
    pub initial_delay_secs: u32,
    /// Interval between probes (seconds).
    pub period_secs: u32,
    /// Timeout for each probe (seconds).
    pub timeout_secs: u32,
    /// Number of failures before marking unhealthy.
    pub failure_threshold: u32,
    /// Number of successes before marking healthy.
    pub success_threshold: u32,
}

impl HealthProbe {
    /// Liveness probe: is the service alive?
    pub fn liveness() -> Self {
        Self {
            path: "/v1/health".to_string(),
            port: 8090,
            initial_delay_secs: 5,
            period_secs: 10,
            timeout_secs: 3,
            failure_threshold: 3,
            success_threshold: 1,
        }
    }

    /// Readiness probe: is the service ready to accept traffic?
    pub fn readiness() -> Self {
        Self {
            path: "/v1/health".to_string(),
            port: 8090,
            initial_delay_secs: 3,
            period_secs: 5,
            timeout_secs: 2,
            failure_threshold: 2,
            success_threshold: 1,
        }
    }

    /// Startup probe: has the service finished initializing?
    pub fn startup() -> Self {
        Self {
            path: "/v1/health".to_string(),
            port: 8090,
            initial_delay_secs: 0,
            period_secs: 2,
            timeout_secs: 2,
            failure_threshold: 15,
            success_threshold: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Service Mesh Configuration
// ---------------------------------------------------------------------------

/// Service mesh configuration for the AGNOS runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Which mesh provider to target.
    pub provider: MeshProvider,
    /// Enable sidecar injection.
    pub sidecar_injection: bool,
    /// Enable mTLS between services (uses AGNOS mTLS when mesh mTLS is disabled).
    pub mtls: bool,
    /// Service ports to register with the mesh.
    pub service_ports: Vec<ServicePort>,
    /// Health probes.
    pub liveness: HealthProbe,
    pub readiness: HealthProbe,
    pub startup: HealthProbe,
    /// Custom annotations for mesh sidecar.
    pub annotations: HashMap<String, String>,
}

/// A service port exposed to the mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePort {
    /// Port name (e.g., "http-rest", "grpc", "metrics").
    pub name: String,
    /// Port number.
    pub port: u16,
    /// Protocol (HTTP, gRPC, TCP).
    pub protocol: PortProtocol,
}

/// Port protocol for mesh routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortProtocol {
    Http,
    Grpc,
    Tcp,
}

impl std::fmt::Display for PortProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http => write!(f, "http"),
            Self::Grpc => write!(f, "grpc"),
            Self::Tcp => write!(f, "tcp"),
        }
    }
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            provider: MeshProvider::None,
            sidecar_injection: false,
            mtls: false,
            service_ports: vec![
                ServicePort {
                    name: "http-rest".to_string(),
                    port: 8090,
                    protocol: PortProtocol::Http,
                },
                ServicePort {
                    name: "grpc".to_string(),
                    port: 8091,
                    protocol: PortProtocol::Grpc,
                },
                ServicePort {
                    name: "metrics".to_string(),
                    port: 8090,
                    protocol: PortProtocol::Http,
                },
            ],
            liveness: HealthProbe::liveness(),
            readiness: HealthProbe::readiness(),
            startup: HealthProbe::startup(),
            annotations: HashMap::new(),
        }
    }
}

impl MeshConfig {
    /// Generate Kubernetes/mesh annotations for sidecar injection.
    pub fn sidecar_annotations(&self) -> HashMap<String, String> {
        let mut annotations = self.annotations.clone();

        match self.provider {
            MeshProvider::Envoy => {
                annotations.insert(
                    "sidecar.istio.io/inject".to_string(),
                    self.sidecar_injection.to_string(),
                );
                if self.mtls {
                    annotations
                        .insert("security.istio.io/tlsMode".to_string(), "istio".to_string());
                }
                // Skip outbound interception for AGNOS IPC sockets
                annotations.insert(
                    "traffic.sidecar.istio.io/excludeOutboundIPRanges".to_string(),
                    "127.0.0.0/8".to_string(),
                );
                // Exclude agent IPC port from mesh
                annotations.insert(
                    "traffic.sidecar.istio.io/excludeInboundPorts".to_string(),
                    "".to_string(),
                );
            }
            MeshProvider::Linkerd => {
                annotations.insert(
                    "linkerd.io/inject".to_string(),
                    if self.sidecar_injection {
                        "enabled"
                    } else {
                        "disabled"
                    }
                    .to_string(),
                );
                if self.mtls {
                    annotations.insert(
                        "config.linkerd.io/proxy-require-identity-on-inbound".to_string(),
                        "true".to_string(),
                    );
                }
                // Skip localhost (agent IPC)
                annotations.insert(
                    "config.linkerd.io/skip-outbound-ports".to_string(),
                    "".to_string(),
                );
            }
            MeshProvider::None => {}
        }

        annotations
    }

    /// Generate a service descriptor for mesh registration.
    pub fn service_descriptor(&self) -> MeshServiceDescriptor {
        MeshServiceDescriptor {
            name: "agnos-agent-runtime".to_string(),
            namespace: "agnos".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            ports: self.service_ports.clone(),
            annotations: self.sidecar_annotations(),
            labels: {
                let mut labels = HashMap::new();
                labels.insert("app".to_string(), "agnos-runtime".to_string());
                labels.insert("component".to_string(), "daimon".to_string());
                labels.insert("mesh.provider".to_string(), self.provider.to_string());
                labels
            },
            probes: ServiceProbes {
                liveness: self.liveness.clone(),
                readiness: self.readiness.clone(),
                startup: self.startup.clone(),
            },
        }
    }

    /// Build config for Envoy sidecar.
    pub fn for_envoy() -> Self {
        Self {
            provider: MeshProvider::Envoy,
            sidecar_injection: true,
            mtls: true,
            ..Default::default()
        }
    }

    /// Build config for Linkerd sidecar.
    pub fn for_linkerd() -> Self {
        Self {
            provider: MeshProvider::Linkerd,
            sidecar_injection: true,
            mtls: true,
            ..Default::default()
        }
    }
}

/// Full service descriptor for mesh registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshServiceDescriptor {
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub ports: Vec<ServicePort>,
    pub annotations: HashMap<String, String>,
    pub labels: HashMap<String, String>,
    pub probes: ServiceProbes,
}

/// Health probes bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceProbes {
    pub liveness: HealthProbe,
    pub readiness: HealthProbe,
    pub startup: HealthProbe,
}

// ---------------------------------------------------------------------------
// Companion Service Descriptors
// ---------------------------------------------------------------------------

/// Generate mesh descriptors for all AGNOS services.
pub fn all_service_descriptors(config: &MeshConfig) -> Vec<MeshServiceDescriptor> {
    let annotations = config.sidecar_annotations();

    vec![
        config.service_descriptor(),
        MeshServiceDescriptor {
            name: "agnos-llm-gateway".to_string(),
            namespace: "agnos".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            ports: vec![ServicePort {
                name: "http-rest".to_string(),
                port: 8088,
                protocol: PortProtocol::Http,
            }],
            annotations: annotations.clone(),
            labels: {
                let mut labels = HashMap::new();
                labels.insert("app".to_string(), "agnos-llm-gateway".to_string());
                labels.insert("component".to_string(), "hoosh".to_string());
                labels
            },
            probes: ServiceProbes {
                liveness: HealthProbe {
                    port: 8088,
                    ..HealthProbe::liveness()
                },
                readiness: HealthProbe {
                    port: 8088,
                    ..HealthProbe::readiness()
                },
                startup: HealthProbe {
                    port: 8088,
                    ..HealthProbe::startup()
                },
            },
        },
        MeshServiceDescriptor {
            name: "agnos-synapse".to_string(),
            namespace: "agnos".to_string(),
            version: "0.0.0".to_string(),
            ports: vec![ServicePort {
                name: "http-rest".to_string(),
                port: 8080,
                protocol: PortProtocol::Http,
            }],
            annotations: annotations.clone(),
            labels: {
                let mut labels = HashMap::new();
                labels.insert("app".to_string(), "agnos-synapse".to_string());
                labels.insert("component".to_string(), "synapse".to_string());
                labels
            },
            probes: ServiceProbes {
                liveness: HealthProbe {
                    port: 8080,
                    ..HealthProbe::liveness()
                },
                readiness: HealthProbe {
                    port: 8080,
                    ..HealthProbe::readiness()
                },
                startup: HealthProbe {
                    port: 8080,
                    ..HealthProbe::startup()
                },
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert_eq!(config.provider, MeshProvider::None);
        assert!(!config.sidecar_injection);
        assert!(!config.mtls);
        assert_eq!(config.service_ports.len(), 3);
    }

    #[test]
    fn test_mesh_config_for_envoy() {
        let config = MeshConfig::for_envoy();
        assert_eq!(config.provider, MeshProvider::Envoy);
        assert!(config.sidecar_injection);
        assert!(config.mtls);
    }

    #[test]
    fn test_mesh_config_for_linkerd() {
        let config = MeshConfig::for_linkerd();
        assert_eq!(config.provider, MeshProvider::Linkerd);
        assert!(config.sidecar_injection);
        assert!(config.mtls);
    }

    #[test]
    fn test_envoy_annotations() {
        let config = MeshConfig::for_envoy();
        let annotations = config.sidecar_annotations();
        assert_eq!(annotations.get("sidecar.istio.io/inject").unwrap(), "true");
        assert_eq!(
            annotations.get("security.istio.io/tlsMode").unwrap(),
            "istio"
        );
        assert!(annotations.contains_key("traffic.sidecar.istio.io/excludeOutboundIPRanges"));
    }

    #[test]
    fn test_envoy_annotations_no_mtls() {
        let mut config = MeshConfig::for_envoy();
        config.mtls = false;
        let annotations = config.sidecar_annotations();
        assert!(!annotations.contains_key("security.istio.io/tlsMode"));
    }

    #[test]
    fn test_linkerd_annotations() {
        let config = MeshConfig::for_linkerd();
        let annotations = config.sidecar_annotations();
        assert_eq!(annotations.get("linkerd.io/inject").unwrap(), "enabled");
        assert_eq!(
            annotations
                .get("config.linkerd.io/proxy-require-identity-on-inbound")
                .unwrap(),
            "true"
        );
    }

    #[test]
    fn test_linkerd_annotations_disabled() {
        let mut config = MeshConfig::for_linkerd();
        config.sidecar_injection = false;
        let annotations = config.sidecar_annotations();
        assert_eq!(annotations.get("linkerd.io/inject").unwrap(), "disabled");
    }

    #[test]
    fn test_none_provider_no_annotations() {
        let config = MeshConfig::default();
        let annotations = config.sidecar_annotations();
        assert!(annotations.is_empty());
    }

    #[test]
    fn test_service_descriptor() {
        let config = MeshConfig::for_envoy();
        let desc = config.service_descriptor();
        assert_eq!(desc.name, "agnos-agent-runtime");
        assert_eq!(desc.namespace, "agnos");
        assert_eq!(desc.labels.get("component").unwrap(), "daimon");
        assert_eq!(desc.ports.len(), 3);
    }

    #[test]
    fn test_service_ports() {
        let config = MeshConfig::default();
        let port_names: Vec<&str> = config
            .service_ports
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert!(port_names.contains(&"http-rest"));
        assert!(port_names.contains(&"grpc"));
        assert!(port_names.contains(&"metrics"));
    }

    #[test]
    fn test_health_probe_liveness() {
        let probe = HealthProbe::liveness();
        assert_eq!(probe.path, "/v1/health");
        assert_eq!(probe.port, 8090);
        assert_eq!(probe.initial_delay_secs, 5);
        assert_eq!(probe.failure_threshold, 3);
    }

    #[test]
    fn test_health_probe_readiness() {
        let probe = HealthProbe::readiness();
        assert_eq!(probe.period_secs, 5);
        assert_eq!(probe.failure_threshold, 2);
    }

    #[test]
    fn test_health_probe_startup() {
        let probe = HealthProbe::startup();
        assert_eq!(probe.initial_delay_secs, 0);
        assert_eq!(probe.failure_threshold, 15);
    }

    #[test]
    fn test_all_service_descriptors() {
        let config = MeshConfig::for_envoy();
        let descriptors = all_service_descriptors(&config);
        assert_eq!(descriptors.len(), 3);

        let names: Vec<&str> = descriptors.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"agnos-agent-runtime"));
        assert!(names.contains(&"agnos-llm-gateway"));
        assert!(names.contains(&"agnos-synapse"));
    }

    #[test]
    fn test_companion_service_probes() {
        let config = MeshConfig::for_linkerd();
        let descriptors = all_service_descriptors(&config);

        let hoosh = descriptors
            .iter()
            .find(|d| d.name == "agnos-llm-gateway")
            .unwrap();
        assert_eq!(hoosh.probes.liveness.port, 8088);
        assert_eq!(hoosh.probes.readiness.port, 8088);

        let synapse = descriptors
            .iter()
            .find(|d| d.name == "agnos-synapse")
            .unwrap();
        assert_eq!(synapse.probes.liveness.port, 8080);
    }

    #[test]
    fn test_mesh_provider_display() {
        assert_eq!(MeshProvider::Envoy.to_string(), "envoy");
        assert_eq!(MeshProvider::Linkerd.to_string(), "linkerd");
        assert_eq!(MeshProvider::None.to_string(), "none");
    }

    #[test]
    fn test_port_protocol_display() {
        assert_eq!(PortProtocol::Http.to_string(), "http");
        assert_eq!(PortProtocol::Grpc.to_string(), "grpc");
        assert_eq!(PortProtocol::Tcp.to_string(), "tcp");
    }

    #[test]
    fn test_custom_annotations_preserved() {
        let mut config = MeshConfig::for_envoy();
        config
            .annotations
            .insert("custom/key".to_string(), "value".to_string());
        let annotations = config.sidecar_annotations();
        assert_eq!(annotations.get("custom/key").unwrap(), "value");
        // Envoy annotations still present
        assert!(annotations.contains_key("sidecar.istio.io/inject"));
    }

    #[test]
    fn test_descriptor_serializable() {
        let config = MeshConfig::for_envoy();
        let desc = config.service_descriptor();
        let json = serde_json::to_string(&desc).unwrap();
        let deserialized: MeshServiceDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "agnos-agent-runtime");
    }
}
