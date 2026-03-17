//! Unified Sandbox Module
//!
//! All sandbox-related functionality consolidated under one module:
//!
//! - **core**: Base sandbox configuration and enforcement (Landlock, MAC, audit)
//! - **v2**: Capability-based security, taint tracking, environment profiles
//! - **backends**: gVisor (runsc) and Firecracker (microVM) backends
//! - **monitor**: Runtime integrity checks and offender tracking
//! - **credential_proxy**: Parent-process HTTP proxy for secret injection
//! - **egress_gate**: Outbound data scanning for secrets/PII
//! - **seccomp**: Seccomp BPF syscall filtering profiles

pub mod backends;
pub mod core;
pub mod credential_proxy;
pub mod egress_gate;
pub mod monitor;
pub mod seccomp;
pub mod v2;

// Re-export key types at module level
pub use backends::{BackendConfig, BackendResult, FirecrackerBackend, GVisorBackend, NetworkMode};
pub use self::core::Sandbox;
pub use credential_proxy::{CredentialProxyConfig, CredentialProxyManager, ProxyDecision};
pub use egress_gate::{ExternalizationGate, ExternalizationGateConfig, GateDecision};
pub use monitor::{OffenderTracker, SandboxMonitor, MonitorConfig};
pub use v2::{
    CapabilityToken, EnvironmentProfile, SandboxBackend, SandboxCapability, SandboxEnvironment,
};
