//! Edge — Types, enums, and data structures.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Edge node status
// ---------------------------------------------------------------------------

/// Health status of an edge node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeNodeStatus {
    /// Receiving heartbeats, ready for tasks.
    Online,
    /// Missed heartbeats (>30s), may be failing.
    Suspect,
    /// No heartbeat for >60s, considered unreachable.
    Offline,
    /// Actively being updated (OTA in progress).
    Updating,
    /// Marked for removal from fleet.
    Decommissioned,
}

impl fmt::Display for EdgeNodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Suspect => write!(f, "suspect"),
            Self::Offline => write!(f, "offline"),
            Self::Updating => write!(f, "updating"),
            Self::Decommissioned => write!(f, "decommissioned"),
        }
    }
}

// ---------------------------------------------------------------------------
// Edge node capabilities
// ---------------------------------------------------------------------------

/// Hardware and software capabilities advertised by an edge node.
/// Used for capability-based task routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeCapabilities {
    /// CPU architecture (e.g. "x86_64", "aarch64", "riscv64").
    pub arch: String,
    /// Number of CPU cores.
    pub cpu_cores: u32,
    /// Available memory in MB.
    pub memory_mb: u64,
    /// Available disk in MB.
    pub disk_mb: u64,
    /// Whether a GPU is available.
    pub has_gpu: bool,
    /// Total GPU VRAM in MB (e.g. 8192 for an 8 GB GPU). `None` if no GPU.
    #[serde(default)]
    pub gpu_memory_mb: Option<u64>,
    /// CUDA compute capability version string (e.g. "8.6", "9.0"). `None` if no CUDA GPU.
    #[serde(default)]
    pub gpu_compute_capability: Option<String>,
    /// Network bandwidth quality (0.0 = poor, 1.0 = excellent).
    pub network_quality: f64,
    /// Geographic location label (optional, e.g. "us-east", "office-floor-2").
    pub location: Option<String>,
    /// Custom capability tags (e.g. ["camera", "bluetooth", "tpm"]).
    pub tags: Vec<String>,
}

impl Default for EdgeCapabilities {
    fn default() -> Self {
        Self {
            arch: "x86_64".into(),
            cpu_cores: 4,
            memory_mb: 1024,
            disk_mb: 4096,
            has_gpu: false,
            gpu_memory_mb: None,
            gpu_compute_capability: None,
            network_quality: 0.8,
            location: None,
            tags: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Edge node
// ---------------------------------------------------------------------------

/// An edge node in the fleet registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeNode {
    /// Unique node identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Current health status.
    pub status: EdgeNodeStatus,
    /// Hardware/software capabilities.
    pub capabilities: EdgeCapabilities,
    /// The agent binary running on this node (e.g. "secureyeoman-edge").
    pub agent_binary: String,
    /// Agent binary version.
    pub agent_version: String,
    /// AGNOS version running on the node.
    pub os_version: String,
    /// URL of the parent instance this node reports to.
    pub parent_url: String,
    /// Timestamp of last heartbeat received.
    pub last_heartbeat: DateTime<Utc>,
    /// Timestamp when the node registered.
    pub registered_at: DateTime<Utc>,
    /// Number of tasks currently running on this node.
    pub active_tasks: u32,
    /// Total tasks completed since registration.
    pub tasks_completed: u64,
    /// Whether TPM attestation passed.
    pub tpm_attested: bool,
    /// Signature of the last OTA update (for signed OTA verification).
    pub update_signature: Option<String>,
    /// Latest GPU utilization percentage (0.0–100.0), from heartbeat.
    #[serde(default)]
    pub gpu_utilization_pct: Option<f32>,
    /// Latest GPU memory used in MB, from heartbeat.
    #[serde(default)]
    pub gpu_memory_used_mb: Option<u64>,
    /// Latest GPU temperature in Celsius, from heartbeat.
    #[serde(default)]
    pub gpu_temperature_c: Option<f32>,
    /// Models currently loaded on this node, advertised to hoosh for local inference routing.
    /// Updated on each heartbeat. Example: ["llama3.2:3b", "mistral:7b"].
    #[serde(default)]
    pub loaded_models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Edge fleet config
// ---------------------------------------------------------------------------

/// Configuration for the edge fleet manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeFleetConfig {
    /// Seconds before a node is marked suspect.
    pub suspect_threshold_secs: u64,
    /// Seconds before a node is marked offline.
    pub offline_threshold_secs: u64,
    /// Maximum number of edge nodes.
    pub max_nodes: usize,
    /// Whether to require TPM attestation for new nodes.
    pub require_tpm: bool,
}

impl Default for EdgeFleetConfig {
    fn default() -> Self {
        Self {
            suspect_threshold_secs: 30,
            offline_threshold_secs: 60,
            max_nodes: 1000,
            require_tpm: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Fleet statistics
// ---------------------------------------------------------------------------

/// Fleet-wide statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeFleetStats {
    pub total_nodes: u32,
    pub online: u32,
    pub suspect: u32,
    pub offline: u32,
    pub updating: u32,
    pub decommissioned: u32,
    pub active_tasks: u32,
    pub tasks_completed: u64,
}

// ---------------------------------------------------------------------------
// WireGuard mesh networking (Phase 14B)
// ---------------------------------------------------------------------------

/// A peer in the WireGuard mesh network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireguardPeer {
    /// WireGuard public key for this peer.
    pub public_key: String,
    /// Network endpoint (host:port).
    pub endpoint: String,
    /// Allowed IP ranges for this peer.
    pub allowed_ips: Vec<String>,
}

/// WireGuard configuration for a node in the mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireguardConfig {
    /// Path to the private key file on disk.
    pub private_key_path: String,
    /// UDP port WireGuard listens on.
    pub listen_port: u16,
    /// Peer entries for all other nodes in the mesh.
    pub peers: Vec<WireguardPeer>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from edge fleet operations.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeFleetError {
    FleetFull {
        max: usize,
    },
    InvalidName(String),
    DuplicateName(String),
    NodeNotFound(String),
    NodeDecommissioned(String),
    NodeNotOnline(String),
    NodeBusy {
        node_id: String,
        active_tasks: u32,
    },
    NotUpdating(String),
    InsufficientBandwidth {
        node_id: String,
        required: f64,
        available: f64,
    },
    InsufficientResources {
        node_id: String,
        reason: String,
    },
}

impl fmt::Display for EdgeFleetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FleetFull { max } => write!(f, "fleet is full (max {} nodes)", max),
            Self::InvalidName(msg) => write!(f, "invalid node name: {}", msg),
            Self::DuplicateName(name) => write!(f, "duplicate node name: {}", name),
            Self::NodeNotFound(id) => write!(f, "edge node not found: {}", id),
            Self::NodeDecommissioned(id) => write!(f, "edge node decommissioned: {}", id),
            Self::NodeNotOnline(id) => write!(f, "edge node {} is not online", id),
            Self::NodeBusy {
                node_id,
                active_tasks,
            } => write!(f, "edge node {} has {} active tasks", node_id, active_tasks),
            Self::NotUpdating(id) => write!(f, "edge node {} is not in updating state", id),
            Self::InsufficientBandwidth {
                node_id,
                required,
                available,
            } => write!(
                f,
                "node {} bandwidth too low (need {:.1}, have {:.1})",
                node_id, required, available
            ),
            Self::InsufficientResources { node_id, reason } => {
                write!(f, "node {} insufficient resources: {}", node_id, reason)
            }
        }
    }
}

impl std::error::Error for EdgeFleetError {}

// ---------------------------------------------------------------------------
// Hardware targets (Phase 14C)
// ---------------------------------------------------------------------------

/// Known hardware targets for AGNOS Edge deployments.
///
/// Each variant carries default resource assumptions and maps to a kernel
/// config fragment (where applicable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTarget {
    RaspberryPi4,
    RaspberryPi5,
    IntelNuc,
    GenericX86_64,
    GenericArm64,
    OciContainer,
}

impl HardwareTarget {
    /// Typical RAM in MiB for the target.
    pub fn default_ram_mb(&self) -> u64 {
        match self {
            Self::RaspberryPi4 => 4096,
            Self::RaspberryPi5 => 8192,
            Self::IntelNuc => 16384,
            Self::GenericX86_64 => 8192,
            Self::GenericArm64 => 2048,
            Self::OciContainer => 512,
        }
    }

    /// Typical disk in MiB for the target.
    pub fn default_disk_mb(&self) -> u64 {
        match self {
            Self::RaspberryPi4 => 32768,
            Self::RaspberryPi5 => 65536,
            Self::IntelNuc => 262144,
            Self::GenericX86_64 => 131072,
            Self::GenericArm64 => 16384,
            Self::OciContainer => 256,
        }
    }

    /// CPU architecture string.
    pub fn arch(&self) -> &str {
        match self {
            Self::RaspberryPi4 | Self::RaspberryPi5 | Self::GenericArm64 => "aarch64",
            Self::IntelNuc | Self::GenericX86_64 => "x86_64",
            Self::OciContainer => "x86_64",
        }
    }

    /// Whether the target has a GPU that AGNOS can leverage.
    pub fn supports_gpu(&self) -> bool {
        match self {
            Self::RaspberryPi4 | Self::RaspberryPi5 => true, // VideoCore
            Self::IntelNuc => true,                          // Intel UHD
            Self::GenericX86_64 | Self::GenericArm64 | Self::OciContainer => false,
        }
    }

    /// Path (relative to repo root) to the kernel config fragment, if any.
    pub fn kernel_config_fragment(&self) -> Option<&str> {
        match self {
            Self::RaspberryPi4 => Some("kernel/configs/edge-rpi4.config"),
            Self::RaspberryPi5 => Some("kernel/configs/edge-rpi5.config"),
            Self::IntelNuc => Some("kernel/configs/edge-nuc.config"),
            Self::GenericX86_64 | Self::GenericArm64 => None,
            Self::OciContainer => None,
        }
    }
}

impl fmt::Display for HardwareTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RaspberryPi4 => write!(f, "rpi4"),
            Self::RaspberryPi5 => write!(f, "rpi5"),
            Self::IntelNuc => write!(f, "nuc"),
            Self::GenericX86_64 => write!(f, "x86_64"),
            Self::GenericArm64 => write!(f, "arm64"),
            Self::OciContainer => write!(f, "oci"),
        }
    }
}
