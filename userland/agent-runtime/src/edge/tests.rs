//! Tests for the edge fleet module.

use super::fleet::EdgeFleetManager;
use super::types::{
    EdgeCapabilities, EdgeFleetConfig, EdgeFleetError, EdgeNodeStatus, HardwareTarget,
};

fn test_config() -> EdgeFleetConfig {
    EdgeFleetConfig {
        suspect_threshold_secs: 30,
        offline_threshold_secs: 60,
        max_nodes: 100,
        require_tpm: false,
    }
}

fn test_capabilities() -> EdgeCapabilities {
    EdgeCapabilities {
        arch: "aarch64".into(),
        cpu_cores: 4,
        memory_mb: 2048,
        disk_mb: 16384,
        has_gpu: false,
        gpu_memory_mb: None,
        gpu_compute_capability: None,
        network_quality: 0.9,
        location: Some("office".into()),
        tags: vec!["camera".into(), "bluetooth".into()],
    }
}

fn register_test_node(mgr: &mut EdgeFleetManager, name: &str) -> String {
    mgr.register_node(
        name.into(),
        test_capabilities(),
        "secureyeoman-edge".into(),
        "2026.3.11".into(),
        "2026.3.11".into(),
        "http://parent:8090".into(),
    )
    .unwrap()
}

// --- Registration ---

#[test]
fn register_node_success() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "rpi-kitchen");
    assert!(!id.is_empty());
    assert_eq!(mgr.nodes.len(), 1);
    let node = mgr.get_node(&id).unwrap();
    assert_eq!(node.name, "rpi-kitchen");
    assert_eq!(node.status, EdgeNodeStatus::Online);
    assert_eq!(node.agent_binary, "secureyeoman-edge");
}

#[test]
fn register_node_empty_name_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let err = mgr
        .register_node(
            "".into(),
            test_capabilities(),
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::InvalidName(_)));
}

#[test]
fn register_node_duplicate_name_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    register_test_node(&mut mgr, "node-a");
    let err = mgr
        .register_node(
            "node-a".into(),
            test_capabilities(),
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::DuplicateName(_)));
}

#[test]
fn register_after_decommission_allows_same_name() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.decommission(&id).unwrap();
    // Should succeed since old node is decommissioned.
    let id2 = register_test_node(&mut mgr, "node-a");
    assert_ne!(id, id2);
}

#[test]
fn register_fleet_full() {
    let config = EdgeFleetConfig {
        max_nodes: 2,
        ..test_config()
    };
    let mut mgr = EdgeFleetManager::new(config);
    register_test_node(&mut mgr, "a");
    register_test_node(&mut mgr, "b");
    let err = mgr
        .register_node(
            "c".into(),
            test_capabilities(),
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::FleetFull { max: 2 }));
}

// --- Heartbeat ---

#[test]
fn heartbeat_updates_state() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.heartbeat(&id, 3, 100, None, None, None, None).unwrap();
    let node = mgr.get_node(&id).unwrap();
    assert_eq!(node.active_tasks, 3);
    assert_eq!(node.tasks_completed, 100);
}

#[test]
fn heartbeat_unknown_node() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let err = mgr
        .heartbeat("nonexistent", 0, 0, None, None, None, None)
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

#[test]
fn heartbeat_decommissioned_node_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.decommission(&id).unwrap();
    let err = mgr
        .heartbeat(&id, 0, 0, None, None, None, None)
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
}

// --- Health checks ---

#[test]
fn check_health_marks_suspect_and_offline() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");

    // Simulate stale heartbeat.
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(35);
    mgr.check_health();
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Suspect);

    // Simulate very stale heartbeat.
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(90);
    mgr.check_health();
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Offline);
}

#[test]
fn heartbeat_restores_from_suspect() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Suspect;
    mgr.heartbeat(&id, 0, 0, None, None, None, None).unwrap();
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Online);
}

#[test]
fn heartbeat_restores_from_offline() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Offline;
    mgr.heartbeat(&id, 0, 0, None, None, None, None).unwrap();
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Online);
}

#[test]
fn check_health_skips_decommissioned() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.decommission(&id).unwrap();
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(999);
    mgr.check_health();
    assert_eq!(
        mgr.get_node(&id).unwrap().status,
        EdgeNodeStatus::Decommissioned
    );
}

#[test]
fn check_health_skips_updating() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Updating;
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(999);
    mgr.check_health();
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Updating);
}

// --- Decommission ---

#[test]
fn decommission_success() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    let node = mgr.decommission(&id).unwrap();
    assert_eq!(node.status, EdgeNodeStatus::Decommissioned);
}

#[test]
fn decommission_already_decommissioned() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.decommission(&id).unwrap();
    let err = mgr.decommission(&id).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
}

#[test]
fn decommission_unknown_node() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let err = mgr.decommission("fake").unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

// --- List and filter ---

#[test]
fn list_nodes_all() {
    let mut mgr = EdgeFleetManager::new(test_config());
    register_test_node(&mut mgr, "a");
    register_test_node(&mut mgr, "b");
    register_test_node(&mut mgr, "c");
    assert_eq!(mgr.list_nodes(None).len(), 3);
}

#[test]
fn list_nodes_filter_online() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "a");
    register_test_node(&mut mgr, "b");
    mgr.decommission(&id_a).unwrap();
    let online = mgr.list_nodes(Some(EdgeNodeStatus::Online));
    assert_eq!(online.len(), 1);
    assert_eq!(online[0].name, "b");
}

// --- Task routing ---

#[test]
fn route_task_basic() {
    let mut mgr = EdgeFleetManager::new(test_config());
    register_test_node(&mut mgr, "a");
    register_test_node(&mut mgr, "b");
    let candidates = mgr.route_task(&[], false, None, None, None);
    assert_eq!(candidates.len(), 2);
}

#[test]
fn route_task_excludes_offline() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "a");
    register_test_node(&mut mgr, "b");
    mgr.nodes.get_mut(&id_a).unwrap().status = EdgeNodeStatus::Offline;
    let candidates = mgr.route_task(&[], false, None, None, None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "b");
}

#[test]
fn route_task_requires_gpu() {
    let mut mgr = EdgeFleetManager::new(test_config());
    register_test_node(&mut mgr, "no-gpu");
    let id_gpu = mgr
        .register_node(
            "has-gpu".into(),
            EdgeCapabilities {
                has_gpu: true,
                ..test_capabilities()
            },
            "edge".into(),
            "1.0".into(),
            "1.0".into(),
            "http://parent:8090".into(),
        )
        .unwrap();
    let candidates = mgr.route_task(&[], true, None, None, None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, id_gpu);
}

#[test]
fn route_task_requires_tags() {
    let mut mgr = EdgeFleetManager::new(test_config());
    register_test_node(&mut mgr, "has-camera-bt"); // has camera + bluetooth
    mgr.register_node(
        "no-tags".into(),
        EdgeCapabilities {
            tags: vec![],
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&["camera".into()], false, None, None, None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "has-camera-bt");
}

#[test]
fn route_task_prefers_location() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "far".into(),
        EdgeCapabilities {
            location: Some("us-west".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();
    mgr.register_node(
        "near".into(),
        EdgeCapabilities {
            location: Some("office".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&[], false, Some("office"), None, None);
    assert_eq!(candidates[0].name, "near");
}

#[test]
fn route_task_prefers_least_loaded() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "busy");
    register_test_node(&mut mgr, "idle");
    mgr.nodes.get_mut(&id_a).unwrap().active_tasks = 5;

    let candidates = mgr.route_task(&[], false, None, None, None);
    assert_eq!(candidates[0].name, "idle");
}

// --- Updates ---

#[test]
fn start_update_success() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "a");
    mgr.start_update(&id).unwrap();
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Updating);
}

#[test]
fn start_update_busy_node_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "a");
    mgr.nodes.get_mut(&id).unwrap().active_tasks = 2;
    let err = mgr.start_update(&id).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeBusy { .. }));
}

#[test]
fn start_update_decommissioned_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "a");
    mgr.decommission(&id).unwrap();
    let err = mgr.start_update(&id).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
}

#[test]
fn complete_update_success() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "a");
    mgr.start_update(&id).unwrap();
    mgr.complete_update(&id, "2026.4.0".into()).unwrap();
    let node = mgr.get_node(&id).unwrap();
    assert_eq!(node.status, EdgeNodeStatus::Online);
    assert_eq!(node.agent_version, "2026.4.0");
}

#[test]
fn complete_update_not_updating_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "a");
    let err = mgr.complete_update(&id, "2.0".into()).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NotUpdating(_)));
}

// --- Stats ---

#[test]
fn stats_counts_correctly() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "a");
    let id_b = register_test_node(&mut mgr, "b");
    register_test_node(&mut mgr, "c");

    mgr.nodes.get_mut(&id_a).unwrap().active_tasks = 3;
    mgr.nodes.get_mut(&id_a).unwrap().tasks_completed = 50;
    mgr.decommission(&id_b).unwrap();

    let stats = mgr.stats();
    assert_eq!(stats.total_nodes, 3);
    assert_eq!(stats.online, 2);
    assert_eq!(stats.decommissioned, 1);
    assert_eq!(stats.active_tasks, 3);
    assert_eq!(stats.tasks_completed, 50);
}

#[test]
fn stats_empty_fleet() {
    let mgr = EdgeFleetManager::new(test_config());
    let stats = mgr.stats();
    assert_eq!(stats.total_nodes, 0);
    assert_eq!(stats.online, 0);
}

// --- Display ---

#[test]
fn status_display() {
    assert_eq!(EdgeNodeStatus::Online.to_string(), "online");
    assert_eq!(EdgeNodeStatus::Suspect.to_string(), "suspect");
    assert_eq!(EdgeNodeStatus::Offline.to_string(), "offline");
    assert_eq!(EdgeNodeStatus::Updating.to_string(), "updating");
    assert_eq!(EdgeNodeStatus::Decommissioned.to_string(), "decommissioned");
}

#[test]
fn error_display() {
    assert!(EdgeFleetError::FleetFull { max: 10 }
        .to_string()
        .contains("full"));
    assert!(EdgeFleetError::NodeNotFound("x".into())
        .to_string()
        .contains("not found"));
    assert!(EdgeFleetError::DuplicateName("x".into())
        .to_string()
        .contains("duplicate"));
}

#[test]
fn default_capabilities() {
    let caps = EdgeCapabilities::default();
    assert_eq!(caps.arch, "x86_64");
    assert_eq!(caps.cpu_cores, 4);
    assert!(!caps.has_gpu);
    assert!(caps.tags.is_empty());
}

#[test]
fn default_config() {
    let config = EdgeFleetConfig::default();
    assert_eq!(config.suspect_threshold_secs, 30);
    assert_eq!(config.offline_threshold_secs, 60);
    assert_eq!(config.max_nodes, 1000);
    assert!(!config.require_tpm);
}

// --- Bandwidth-aware acceptance (14B) ---

#[test]
fn check_task_acceptance_ok() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    // test_capabilities has network_quality=0.9, memory_mb=2048
    assert!(mgr.check_task_acceptance(&id, 0.5, 512).is_ok());
}

#[test]
fn check_task_acceptance_insufficient_bandwidth() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    let err = mgr.check_task_acceptance(&id, 0.95, 512).unwrap_err();
    assert!(matches!(err, EdgeFleetError::InsufficientBandwidth { .. }));
}

#[test]
fn check_task_acceptance_insufficient_memory() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    let err = mgr.check_task_acceptance(&id, 0.5, 8192).unwrap_err();
    assert!(matches!(err, EdgeFleetError::InsufficientResources { .. }));
}

#[test]
fn check_task_acceptance_unknown_node() {
    let mgr = EdgeFleetManager::new(test_config());
    let err = mgr.check_task_acceptance("fake", 0.5, 512).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

#[test]
fn route_task_with_constraints_filters_bandwidth() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "fast".into(),
        EdgeCapabilities {
            network_quality: 0.95,
            memory_mb: 4096,
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();
    mgr.register_node(
        "slow".into(),
        EdgeCapabilities {
            network_quality: 0.3,
            memory_mb: 4096,
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task_with_constraints(&[], false, None, 0.5, 1024);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "fast");
}

#[test]
fn route_task_with_constraints_filters_memory() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "big".into(),
        EdgeCapabilities {
            memory_mb: 8192,
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();
    mgr.register_node(
        "small".into(),
        EdgeCapabilities {
            memory_mb: 256,
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task_with_constraints(&[], false, None, 0.0, 4096);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "big");
}

#[test]
fn update_capabilities() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    let new_caps = EdgeCapabilities {
        network_quality: 0.1,
        memory_mb: 512,
        ..test_capabilities()
    };
    mgr.update_capabilities(&id, new_caps).unwrap();
    let node = mgr.get_node(&id).unwrap();
    assert!((node.capabilities.network_quality - 0.1).abs() < f64::EPSILON);
    assert_eq!(node.capabilities.memory_mb, 512);
}

#[test]
fn update_capabilities_decommissioned() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-a");
    mgr.decommission(&id).unwrap();
    let err = mgr
        .update_capabilities(&id, test_capabilities())
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
}

#[test]
fn error_display_bandwidth() {
    let err = EdgeFleetError::InsufficientBandwidth {
        node_id: "x".into(),
        required: 0.8,
        available: 0.3,
    };
    assert!(err.to_string().contains("bandwidth"));
}

#[test]
fn error_display_resources() {
    let err = EdgeFleetError::InsufficientResources {
        node_id: "x".into(),
        reason: "low memory".into(),
    };
    assert!(err.to_string().contains("insufficient resources"));
}

// --- HardwareTarget (Phase 14C) ---

#[test]
fn hardware_target_default_ram() {
    assert_eq!(HardwareTarget::RaspberryPi4.default_ram_mb(), 4096);
    assert_eq!(HardwareTarget::RaspberryPi5.default_ram_mb(), 8192);
    assert_eq!(HardwareTarget::IntelNuc.default_ram_mb(), 16384);
    assert_eq!(HardwareTarget::GenericX86_64.default_ram_mb(), 8192);
    assert_eq!(HardwareTarget::GenericArm64.default_ram_mb(), 2048);
    assert_eq!(HardwareTarget::OciContainer.default_ram_mb(), 512);
}

#[test]
fn hardware_target_default_disk() {
    assert_eq!(HardwareTarget::RaspberryPi4.default_disk_mb(), 32768);
    assert_eq!(HardwareTarget::IntelNuc.default_disk_mb(), 262144);
    assert_eq!(HardwareTarget::OciContainer.default_disk_mb(), 256);
}

#[test]
fn hardware_target_arch() {
    assert_eq!(HardwareTarget::RaspberryPi4.arch(), "aarch64");
    assert_eq!(HardwareTarget::RaspberryPi5.arch(), "aarch64");
    assert_eq!(HardwareTarget::GenericArm64.arch(), "aarch64");
    assert_eq!(HardwareTarget::IntelNuc.arch(), "x86_64");
    assert_eq!(HardwareTarget::GenericX86_64.arch(), "x86_64");
    assert_eq!(HardwareTarget::OciContainer.arch(), "x86_64");
}

#[test]
fn hardware_target_gpu_support() {
    assert!(HardwareTarget::RaspberryPi4.supports_gpu());
    assert!(HardwareTarget::RaspberryPi5.supports_gpu());
    assert!(HardwareTarget::IntelNuc.supports_gpu());
    assert!(!HardwareTarget::GenericX86_64.supports_gpu());
    assert!(!HardwareTarget::GenericArm64.supports_gpu());
    assert!(!HardwareTarget::OciContainer.supports_gpu());
}

#[test]
fn hardware_target_kernel_config_fragment() {
    assert_eq!(
        HardwareTarget::RaspberryPi4.kernel_config_fragment(),
        Some("kernel/configs/edge-rpi4.config")
    );
    assert_eq!(
        HardwareTarget::RaspberryPi5.kernel_config_fragment(),
        Some("kernel/configs/edge-rpi5.config")
    );
    assert_eq!(
        HardwareTarget::IntelNuc.kernel_config_fragment(),
        Some("kernel/configs/edge-nuc.config")
    );
    assert_eq!(HardwareTarget::GenericX86_64.kernel_config_fragment(), None);
    assert_eq!(HardwareTarget::GenericArm64.kernel_config_fragment(), None);
    assert_eq!(HardwareTarget::OciContainer.kernel_config_fragment(), None);
}

#[test]
fn hardware_target_display() {
    assert_eq!(HardwareTarget::RaspberryPi4.to_string(), "rpi4");
    assert_eq!(HardwareTarget::RaspberryPi5.to_string(), "rpi5");
    assert_eq!(HardwareTarget::IntelNuc.to_string(), "nuc");
    assert_eq!(HardwareTarget::GenericX86_64.to_string(), "x86_64");
    assert_eq!(HardwareTarget::GenericArm64.to_string(), "arm64");
    assert_eq!(HardwareTarget::OciContainer.to_string(), "oci");
}

#[test]
fn hardware_target_serde_roundtrip() {
    let target = HardwareTarget::RaspberryPi5;
    let json = serde_json::to_string(&target).unwrap();
    let deserialized: HardwareTarget = serde_json::from_str(&json).unwrap();
    assert_eq!(target, deserialized);
}

#[test]
fn hardware_target_clone_eq() {
    let a = HardwareTarget::IntelNuc;
    let b = a.clone();
    assert_eq!(a, b);
    assert_ne!(HardwareTarget::RaspberryPi4, HardwareTarget::RaspberryPi5);
}

// -----------------------------------------------------------------------
// Phase 14D: Edge Security tests
// -----------------------------------------------------------------------

#[test]
fn attest_node_success() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "rpi-secure");
    assert!(!mgr.get_node(&id).unwrap().tpm_attested);
    mgr.attest_node(&id).unwrap();
    assert!(mgr.get_node(&id).unwrap().tpm_attested);
}

#[test]
fn attest_node_not_found() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let err = mgr.attest_node("nonexistent").unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

#[test]
fn attest_node_decommissioned_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "rpi-old");
    mgr.decommission(&id).unwrap();
    let err = mgr.attest_node(&id).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
}

#[test]
fn require_attestation_returns_status() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-att");
    assert!(!mgr.require_attestation(&id).unwrap());
    mgr.attest_node(&id).unwrap();
    assert!(mgr.require_attestation(&id).unwrap());
}

#[test]
fn require_attestation_not_found() {
    let mgr = EdgeFleetManager::new(test_config());
    let err = mgr.require_attestation("ghost").unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

#[test]
fn verify_update_signature_valid() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-ota");
    let result = mgr.verify_update_signature(&id, "abc123def456").unwrap();
    assert!(result);
}

#[test]
fn verify_update_signature_empty_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-ota2");
    let result = mgr.verify_update_signature(&id, "").unwrap();
    assert!(!result);
}

#[test]
fn verify_update_signature_node_not_found() {
    let mgr = EdgeFleetManager::new(test_config());
    let err = mgr.verify_update_signature("fake", "sig").unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

#[test]
fn set_update_signature_stores_value() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "node-sig");
    assert!(mgr.get_node(&id).unwrap().update_signature.is_none());
    mgr.set_update_signature(&id, "ed25519:abcdef".into())
        .unwrap();
    assert_eq!(
        mgr.get_node(&id).unwrap().update_signature.as_deref(),
        Some("ed25519:abcdef")
    );
}

#[test]
fn set_parent_cert_pin_and_verify() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let valid_hash = "a".repeat(64);
    let wrong_hash = "b".repeat(64);
    assert!(!mgr.verify_parent_cert(&valid_hash));
    mgr.set_parent_cert_pin(valid_hash.clone()).unwrap();
    assert!(mgr.verify_parent_cert(&valid_hash));
    assert!(!mgr.verify_parent_cert(&wrong_hash));
}

#[test]
fn set_parent_cert_pin_rejects_invalid() {
    let mut mgr = EdgeFleetManager::new(test_config());
    // Too short
    assert!(mgr.set_parent_cert_pin("abc".into()).is_err());
    // Non-hex
    let non_hex = "g".repeat(64);
    assert!(mgr.set_parent_cert_pin(non_hex).is_err());
}

#[test]
fn verify_parent_cert_no_pin_returns_false() {
    let mgr = EdgeFleetManager::new(test_config());
    let hash = "a".repeat(64);
    assert!(!mgr.verify_parent_cert(&hash));
}

#[test]
fn registered_node_has_no_attestation_or_signature() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "fresh-node");
    let node = mgr.get_node(&id).unwrap();
    assert!(!node.tpm_attested);
    assert!(node.update_signature.is_none());
}

// === Phase 14B: A2A & Sub-Agent Networking ===

// --- mDNS Discovery ---

#[test]
fn discover_peers_empty_by_default() {
    let mut mgr = EdgeFleetManager::new(test_config());
    std::env::remove_var("AGNOS_MDNS_PEERS");
    let peers = mgr.discover_peers();
    assert!(peers.is_empty());
}

#[test]
fn add_discovery_peer_programmatic() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.add_discovery_peer("192.168.1.10:8090".into());
    mgr.add_discovery_peer("192.168.1.11:8090".into());
    assert_eq!(mgr.discovered_peers.len(), 2);
    assert!(mgr
        .discovered_peers
        .contains(&"192.168.1.10:8090".to_string()));
    assert!(mgr
        .discovered_peers
        .contains(&"192.168.1.11:8090".to_string()));
}

#[test]
fn add_discovery_peer_deduplicates() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.add_discovery_peer("192.168.1.10:8090".into());
    mgr.add_discovery_peer("192.168.1.10:8090".into());
    assert_eq!(mgr.discovered_peers.len(), 1);
}

#[test]
fn add_discovery_peer_ignores_empty() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.add_discovery_peer(String::new());
    assert!(mgr.discovered_peers.is_empty());
}

#[test]
fn discover_peers_returns_programmatic() {
    let mut mgr = EdgeFleetManager::new(test_config());
    std::env::remove_var("AGNOS_MDNS_PEERS");
    mgr.add_discovery_peer("manual:8090".into());
    let peers = mgr.discover_peers();
    assert_eq!(peers.len(), 1);
    assert!(peers.contains(&"manual:8090".to_string()));
}

// --- Auto-registration on boot ---

#[test]
fn auto_register_node_success() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = mgr
        .auto_register_node("edge-rpi-01", test_capabilities())
        .unwrap();
    assert!(!id.is_empty());
    let node = mgr.get_node(&id).unwrap();
    assert_eq!(node.name, "edge-rpi-01");
    assert_eq!(node.status, EdgeNodeStatus::Online);
    assert_eq!(node.agent_binary, "agnos-edge");
}

#[test]
fn auto_register_node_empty_hostname_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let err = mgr.auto_register_node("", test_capabilities()).unwrap_err();
    assert!(matches!(err, EdgeFleetError::InvalidName(_)));
}

#[test]
fn auto_register_node_duplicate_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.auto_register_node("edge-01", test_capabilities())
        .unwrap();
    let err = mgr
        .auto_register_node("edge-01", test_capabilities())
        .unwrap_err();
    assert!(matches!(err, EdgeFleetError::DuplicateName(_)));
}

// --- WireGuard mesh config ---

#[test]
fn wireguard_config_single_node() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "solo");
    let wg = mgr.generate_wireguard_config(&id).unwrap();
    assert_eq!(wg.listen_port, 51820);
    assert!(wg.private_key_path.contains(&id));
    assert!(wg.peers.is_empty());
}

#[test]
fn wireguard_config_multi_node() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "node-alpha");
    register_test_node(&mut mgr, "node-beta");
    register_test_node(&mut mgr, "node-gamma");
    let wg = mgr.generate_wireguard_config(&id_a).unwrap();
    assert_eq!(wg.peers.len(), 2);
    for peer in &wg.peers {
        assert!(!peer.endpoint.contains("node-alpha"));
        assert!(!peer.allowed_ips.is_empty());
        assert!(!peer.public_key.is_empty());
    }
}

#[test]
fn wireguard_config_excludes_decommissioned() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "alive-wg1");
    let id_b = register_test_node(&mut mgr, "gone-wg1");
    mgr.decommission(&id_b).unwrap();
    let wg = mgr.generate_wireguard_config(&id_a).unwrap();
    assert!(wg.peers.is_empty());
}

#[test]
fn wireguard_config_excludes_offline() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "alive-wg2");
    let id_b = register_test_node(&mut mgr, "down-wg2");
    mgr.nodes.get_mut(&id_b).unwrap().status = EdgeNodeStatus::Offline;
    let wg = mgr.generate_wireguard_config(&id_a).unwrap();
    assert!(wg.peers.is_empty());
}

#[test]
fn wireguard_config_unknown_node() {
    let mgr = EdgeFleetManager::new(test_config());
    let err = mgr.generate_wireguard_config("nonexistent").unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeNotFound(_)));
}

#[test]
fn wireguard_config_decommissioned_node_rejected() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "dead-wg");
    mgr.decommission(&id).unwrap();
    let err = mgr.generate_wireguard_config(&id).unwrap_err();
    assert!(matches!(err, EdgeFleetError::NodeDecommissioned(_)));
}

// --- Heartbeat watchdog ---

#[test]
fn check_stale_nodes_none_stale() {
    let mut mgr = EdgeFleetManager::new(test_config());
    register_test_node(&mut mgr, "fresh-a14b");
    register_test_node(&mut mgr, "fresh-b14b");
    let stale = mgr.check_stale_nodes(60);
    assert!(stale.is_empty());
}

#[test]
fn check_stale_nodes_marks_offline() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "old-node-14b");
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(120);
    let stale = mgr.check_stale_nodes(60);
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0], id);
    assert_eq!(mgr.get_node(&id).unwrap().status, EdgeNodeStatus::Offline);
}

#[test]
fn check_stale_nodes_skips_already_offline() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "already-off-14b");
    mgr.nodes.get_mut(&id).unwrap().status = EdgeNodeStatus::Offline;
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(999);
    let stale = mgr.check_stale_nodes(60);
    assert!(stale.is_empty());
}

#[test]
fn check_stale_nodes_skips_decommissioned() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "decom-14b");
    mgr.decommission(&id).unwrap();
    mgr.nodes.get_mut(&id).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(999);
    let stale = mgr.check_stale_nodes(60);
    assert!(stale.is_empty());
}

#[test]
fn check_stale_nodes_mixed_fleet() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_fresh = register_test_node(&mut mgr, "fresh-14b");
    let id_stale = register_test_node(&mut mgr, "stale-14b");
    let id_decom = register_test_node(&mut mgr, "decom2-14b");

    mgr.nodes.get_mut(&id_stale).unwrap().last_heartbeat =
        chrono::Utc::now() - chrono::Duration::seconds(300);
    mgr.decommission(&id_decom).unwrap();

    let stale = mgr.check_stale_nodes(60);
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0], id_stale);
    assert_eq!(
        mgr.get_node(&id_fresh).unwrap().status,
        EdgeNodeStatus::Online
    );
}

// === G3.1: GPU capability routing ===

#[test]
fn route_task_gpu_vram_filter_matches() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "gpu-big".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(16384),
            gpu_compute_capability: Some("8.9".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();
    mgr.register_node(
        "gpu-small".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(8192),
            gpu_compute_capability: Some("8.6".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&[], true, None, Some(12288), None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "gpu-big");
}

#[test]
fn route_task_gpu_vram_filter_excludes_no_vram_field() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "gpu-unknown-vram".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: None,
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&[], true, None, Some(4096), None);
    assert!(candidates.is_empty());
}

#[test]
fn route_task_compute_capability_filter() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "ampere".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(8192),
            gpu_compute_capability: Some("8.6".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();
    mgr.register_node(
        "turing".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(8192),
            gpu_compute_capability: Some("7.5".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&[], true, None, None, Some("8.6"));
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "ampere");
}

#[test]
fn route_task_compute_capability_no_match() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "old-gpu".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(4096),
            gpu_compute_capability: Some("6.1".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&[], true, None, None, Some("9.0"));
    assert!(candidates.is_empty());
}

#[test]
fn route_task_gpu_vram_and_cc_combined() {
    let mut mgr = EdgeFleetManager::new(test_config());
    mgr.register_node(
        "perfect".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(24576),
            gpu_compute_capability: Some("8.9".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();
    mgr.register_node(
        "wrong-cc".into(),
        EdgeCapabilities {
            has_gpu: true,
            gpu_memory_mb: Some(24576),
            gpu_compute_capability: Some("7.5".into()),
            ..test_capabilities()
        },
        "edge".into(),
        "1.0".into(),
        "1.0".into(),
        "http://parent:8090".into(),
    )
    .unwrap();

    let candidates = mgr.route_task(&[], true, None, Some(16384), Some("8.9"));
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "perfect");
}

#[test]
fn default_capabilities_have_no_gpu_fields() {
    let caps = EdgeCapabilities::default();
    assert!(caps.gpu_memory_mb.is_none());
    assert!(caps.gpu_compute_capability.is_none());
}

// === G3.2: Local model registry sync ===

#[test]
fn heartbeat_updates_loaded_models() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "model-node");
    assert!(mgr.get_node(&id).unwrap().loaded_models.is_empty());

    let models = vec!["llama3.2:3b".to_string(), "mistral:7b".to_string()];
    mgr.heartbeat(&id, 0, 0, None, None, None, Some(models.clone()))
        .unwrap();
    assert_eq!(mgr.get_node(&id).unwrap().loaded_models, models);
}

#[test]
fn heartbeat_clears_loaded_models_on_empty_list() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "model-node2");
    mgr.heartbeat(
        &id,
        0,
        0,
        None,
        None,
        None,
        Some(vec!["phi3:mini".to_string()]),
    )
    .unwrap();
    assert!(!mgr.get_node(&id).unwrap().loaded_models.is_empty());

    mgr.heartbeat(&id, 0, 0, None, None, None, Some(vec![]))
        .unwrap();
    assert!(mgr.get_node(&id).unwrap().loaded_models.is_empty());
}

#[test]
fn heartbeat_none_loaded_models_preserves_existing() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "model-node3");
    let models = vec!["gemma2:9b".to_string()];
    mgr.heartbeat(&id, 0, 0, None, None, None, Some(models.clone()))
        .unwrap();
    mgr.heartbeat(&id, 1, 5, None, None, None, None).unwrap();
    assert_eq!(mgr.get_node(&id).unwrap().loaded_models, models);
}

#[test]
fn fleet_loaded_models_deduplicates() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "model-fleet-a");
    let id_b = register_test_node(&mut mgr, "model-fleet-b");

    mgr.heartbeat(
        &id_a,
        0,
        0,
        None,
        None,
        None,
        Some(vec!["llama3.2:3b".to_string(), "mistral:7b".to_string()]),
    )
    .unwrap();
    mgr.heartbeat(
        &id_b,
        0,
        0,
        None,
        None,
        None,
        Some(vec!["mistral:7b".to_string(), "phi3:mini".to_string()]),
    )
    .unwrap();

    let all_models = mgr.fleet_loaded_models();
    assert_eq!(all_models, vec!["llama3.2:3b", "mistral:7b", "phi3:mini"]);
}

#[test]
fn fleet_loaded_models_excludes_offline_nodes() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_online = register_test_node(&mut mgr, "online-model");
    let id_offline = register_test_node(&mut mgr, "offline-model");

    mgr.heartbeat(
        &id_online,
        0,
        0,
        None,
        None,
        None,
        Some(vec!["llama3.2:3b".to_string()]),
    )
    .unwrap();
    mgr.heartbeat(
        &id_offline,
        0,
        0,
        None,
        None,
        None,
        Some(vec!["gemma2:9b".to_string()]),
    )
    .unwrap();
    mgr.nodes.get_mut(&id_offline).unwrap().status = EdgeNodeStatus::Offline;

    let models = mgr.fleet_loaded_models();
    assert_eq!(models, vec!["llama3.2:3b"]);
}

#[test]
fn nodes_by_model_returns_correct_mapping() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id_a = register_test_node(&mut mgr, "model-map-a");
    let id_b = register_test_node(&mut mgr, "model-map-b");

    mgr.heartbeat(
        &id_a,
        0,
        0,
        None,
        None,
        None,
        Some(vec!["llama3.2:3b".to_string()]),
    )
    .unwrap();
    mgr.heartbeat(&id_b, 0, 0, None, None, None, None).unwrap();

    let map = mgr.nodes_by_model();
    assert_eq!(map.len(), 1);
    assert!(map.contains_key(&id_a));
    assert!(!map.contains_key(&id_b));
    assert_eq!(map[&id_a], vec!["llama3.2:3b"]);
}

#[test]
fn registered_node_has_empty_loaded_models() {
    let mut mgr = EdgeFleetManager::new(test_config());
    let id = register_test_node(&mut mgr, "no-models");
    assert!(mgr.get_node(&id).unwrap().loaded_models.is_empty());
}
