//! Edge — Fleet Management for AGNOS Edge Nodes
//!
//! Manages a registry of edge nodes running AGNOS in Edge boot mode.
//! Edge nodes are constrained-hardware devices (Raspberry Pi, NUCs, IoT
//! gateways) that run a single agent binary (e.g. SecureYeoman edge) and
//! connect upstream to a parent AGNOS instance via A2A protocol.
//!
//! This module provides:
//! - Edge node registration and decommissioning
//! - Health monitoring with heartbeat tracking
//! - Capability-based task routing
//! - Fleet-wide deployment and update operations
//! - mDNS-based peer discovery (Phase 14B)
//! - Auto-registration on boot (Phase 14B)
//! - WireGuard mesh networking config (Phase 14B)
//! - Heartbeat watchdog with stale node detection (Phase 14B)
//! - TPM 2.0 attestation wiring (Phase 14D)
//! - Signed OTA update verification (Phase 14D)
//! - Certificate pinning for parent-only trust (Phase 14D)

pub mod fleet;
pub mod mqtt_bridge;
pub mod ota;
pub mod routing;
pub mod telemetry;
pub mod types;

// Re-export the full public API so existing consumers are unaffected.
pub use fleet::EdgeFleetManager;
pub use types::{
    EdgeCapabilities, EdgeFleetConfig, EdgeFleetError, EdgeFleetStats, EdgeNode, EdgeNodeStatus,
    HardwareTarget, WireguardConfig, WireguardPeer,
};

#[cfg(test)]
mod tests;
