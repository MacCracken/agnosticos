//! Argonaut unit tests.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;

use super::edge_boot::{configure_readonly_rootfs, verify_rootfs_integrity};
use super::types::{
    ArgonautConfig, BootMode, BootStage, BootStepStatus, CrashAction,
    EdgeBootConfig, EmergencyShellConfig, ExitStatus, HealthCheckType, ManagedService,
    RestartPolicy, Runlevel, SafeCommand, ServiceDefinition, ServiceEventType, ServiceState,
    ServiceTarget, ShutdownAction, ShutdownStepStatus, ShutdownType,
};
use super::ArgonautInit;

// --- helpers ---

fn minimal_config() -> ArgonautConfig {
    ArgonautConfig {
        boot_mode: BootMode::Minimal,
        ..Default::default()
    }
}

fn server_config() -> ArgonautConfig {
    ArgonautConfig {
        boot_mode: BootMode::Server,
        ..Default::default()
    }
}

fn desktop_config() -> ArgonautConfig {
    ArgonautConfig {
        boot_mode: BootMode::Desktop,
        ..Default::default()
    }
}

fn dummy_service(name: &str, deps: Vec<&str>) -> ServiceDefinition {
    ServiceDefinition {
        name: name.into(),
        description: format!("test service {name}"),
        binary_path: PathBuf::from(format!("/usr/bin/{name}")),
        args: vec![],
        environment: HashMap::new(),
        depends_on: deps.into_iter().map(String::from).collect(),
        required_for_modes: vec![BootMode::Minimal],
        restart_policy: RestartPolicy::Never,
        health_check: None,
        ready_check: None,
    }
}

// --- BootMode ---

#[test]
fn boot_mode_display_server() {
    assert_eq!(BootMode::Server.to_string(), "server");
}

#[test]
fn boot_mode_display_desktop() {
    assert_eq!(BootMode::Desktop.to_string(), "desktop");
}

#[test]
fn boot_mode_display_minimal() {
    assert_eq!(BootMode::Minimal.to_string(), "minimal");
}

#[test]
fn boot_mode_display_edge() {
    assert_eq!(BootMode::Edge.to_string(), "edge");
}

// --- BootStage ordering ---

#[test]
fn boot_stage_ordering() {
    assert!(BootStage::MountFilesystems < BootStage::StartDeviceManager);
    assert!(BootStage::StartDeviceManager < BootStage::VerifyRootfs);
    assert!(BootStage::VerifyRootfs < BootStage::StartSecurity);
    assert!(BootStage::StartSecurity < BootStage::StartDatabaseServices);
    assert!(BootStage::StartDatabaseServices < BootStage::StartAgentRuntime);
    assert!(BootStage::StartAgentRuntime < BootStage::StartLlmGateway);
    assert!(BootStage::StartLlmGateway < BootStage::StartModelServices);
    assert!(BootStage::StartModelServices < BootStage::StartCompositor);
    assert!(BootStage::StartCompositor < BootStage::StartShell);
    assert!(BootStage::StartShell < BootStage::BootComplete);
}

#[test]
fn boot_stage_display() {
    assert_eq!(BootStage::MountFilesystems.to_string(), "mount-filesystems");
    assert_eq!(BootStage::BootComplete.to_string(), "boot-complete");
}

// --- BootStepStatus ---

#[test]
fn boot_step_status_variants() {
    assert_eq!(BootStepStatus::Pending.to_string(), "pending");
    assert_eq!(BootStepStatus::Running.to_string(), "running");
    assert_eq!(BootStepStatus::Complete.to_string(), "complete");
    assert_eq!(BootStepStatus::Failed.to_string(), "failed");
    assert_eq!(BootStepStatus::Skipped.to_string(), "skipped");
}

// --- RestartPolicy ---

#[test]
fn restart_policy_display() {
    assert_eq!(RestartPolicy::Always.to_string(), "always");
    assert_eq!(RestartPolicy::OnFailure.to_string(), "on-failure");
    assert_eq!(RestartPolicy::Never.to_string(), "never");
}

// --- HealthCheckType ---

#[test]
fn health_check_type_variants() {
    let http = HealthCheckType::HttpGet("http://localhost/health".into());
    let tcp = HealthCheckType::TcpConnect("127.0.0.1".into(), 8080);
    let cmd = HealthCheckType::Command("systemctl is-active foo".into());
    let alive = HealthCheckType::ProcessAlive;

    assert!(matches!(http, HealthCheckType::HttpGet(_)));
    assert!(matches!(tcp, HealthCheckType::TcpConnect(_, 8080)));
    assert!(matches!(cmd, HealthCheckType::Command(_)));
    assert!(matches!(alive, HealthCheckType::ProcessAlive));
}

// --- Boot sequence per mode ---

#[test]
fn boot_sequence_minimal_no_compositor() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
    let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
    assert!(!stages.contains(&BootStage::StartCompositor));
    assert!(!stages.contains(&BootStage::StartShell));
    assert!(!stages.contains(&BootStage::StartLlmGateway));
    assert!(stages.contains(&BootStage::StartAgentRuntime));
    assert!(stages.contains(&BootStage::BootComplete));
}

#[test]
fn boot_sequence_server_no_compositor() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
    let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
    assert!(!stages.contains(&BootStage::StartCompositor));
    assert!(!stages.contains(&BootStage::StartShell));
    assert!(stages.contains(&BootStage::StartLlmGateway));
    assert!(stages.contains(&BootStage::StartAgentRuntime));
}

#[test]
fn boot_sequence_desktop_all_stages() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
    let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
    assert!(stages.contains(&BootStage::StartCompositor));
    assert!(stages.contains(&BootStage::StartShell));
    assert!(stages.contains(&BootStage::StartLlmGateway));
    assert!(stages.contains(&BootStage::StartAgentRuntime));
    assert!(stages.contains(&BootStage::BootComplete));
}

#[test]
fn boot_sequence_step_count_minimal() {
    // MountFS, DevMgr, Verify, Security, AgentRuntime, BootComplete = 6
    let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
    assert_eq!(steps.len(), 6);
}

#[test]
fn boot_sequence_step_count_server() {
    // 6 (minimal) + DatabaseServices + LlmGateway + ModelServices = 9
    let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
    assert_eq!(steps.len(), 9);
}

#[test]
fn boot_sequence_step_count_desktop() {
    // 9 (server) + Compositor + Shell = 11
    let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
    assert_eq!(steps.len(), 11);
}

// --- Default services ---

#[test]
fn default_services_minimal() {
    let svcs = ArgonautInit::default_services(BootMode::Minimal);
    assert_eq!(svcs.len(), 1);
    assert_eq!(svcs[0].name, "agent-runtime");
}

#[test]
fn default_services_server() {
    let svcs = ArgonautInit::default_services(BootMode::Server);
    assert_eq!(svcs.len(), 5);
    let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"agent-runtime"));
    assert!(names.contains(&"llm-gateway"));
}

#[test]
fn default_services_desktop() {
    let svcs = ArgonautInit::default_services(BootMode::Desktop);
    assert_eq!(svcs.len(), 7);
    let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"agent-runtime"));
    assert!(names.contains(&"llm-gateway"));
    assert!(names.contains(&"synapse"));
    assert!(names.contains(&"aethersafha"));
    assert!(names.contains(&"agnoshi"));
}

// --- Service order resolution ---

#[test]
fn resolve_service_order_simple_chain() {
    let services = vec![
        dummy_service("c", vec!["b"]),
        dummy_service("b", vec!["a"]),
        dummy_service("a", vec![]),
    ];
    let order = ArgonautInit::resolve_service_order(&services).unwrap();
    assert_eq!(order, vec!["a", "b", "c"]);
}

#[test]
fn resolve_service_order_independent() {
    let services = vec![
        dummy_service("alpha", vec![]),
        dummy_service("beta", vec![]),
        dummy_service("gamma", vec![]),
    ];
    let order = ArgonautInit::resolve_service_order(&services).unwrap();
    assert_eq!(order.len(), 3);
    // All independent — any valid topological order contains all three.
    assert!(order.contains(&"alpha".to_string()));
    assert!(order.contains(&"beta".to_string()));
    assert!(order.contains(&"gamma".to_string()));
}

#[test]
fn resolve_service_order_cycle_detection() {
    let services = vec![dummy_service("a", vec!["b"]), dummy_service("b", vec!["a"])];
    let result = ArgonautInit::resolve_service_order(&services);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cycle detected"));
}

// --- Register and get service ---

#[test]
fn register_and_get_service() {
    let mut init = ArgonautInit::new(minimal_config());
    let svc = dummy_service("my-service", vec![]);
    init.register_service(svc);
    let got = init.get_service("my-service");
    assert!(got.is_some());
    assert_eq!(got.unwrap().definition.name, "my-service");
    assert_eq!(got.unwrap().state, ServiceState::Stopped);
}

#[test]
fn get_service_not_found() {
    let init = ArgonautInit::new(minimal_config());
    assert!(init.get_service("nonexistent").is_none());
}

// --- Service state transitions ---

#[test]
fn set_service_state_valid_transitions() {
    let mut init = ArgonautInit::new(minimal_config());
    // Stopped → Starting (agent-runtime has no deps)
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert_eq!(
        init.get_service_state("agent-runtime"),
        Some(&ServiceState::Starting)
    );
    // Starting → Running
    assert!(init.set_service_state("agent-runtime", ServiceState::Running));
    assert_eq!(
        init.get_service_state("agent-runtime"),
        Some(&ServiceState::Running)
    );
}

#[test]
fn set_service_state_unknown_service() {
    let mut init = ArgonautInit::new(minimal_config());
    assert!(!init.set_service_state("nonexistent", ServiceState::Running));
}

// --- Boot step marking ---

#[test]
fn mark_step_complete() {
    let mut init = ArgonautInit::new(minimal_config());
    assert!(init.mark_step_complete(BootStage::MountFilesystems));
    let step = init
        .boot_sequence
        .iter()
        .find(|s| s.stage == BootStage::MountFilesystems)
        .unwrap();
    assert_eq!(step.status, BootStepStatus::Complete);
    assert!(step.completed_at.is_some());
}

#[test]
fn mark_step_failed() {
    let mut init = ArgonautInit::new(minimal_config());
    assert!(init.mark_step_failed(BootStage::VerifyRootfs, "dm-verity mismatch".into()));
    let step = init
        .boot_sequence
        .iter()
        .find(|s| s.stage == BootStage::VerifyRootfs)
        .unwrap();
    assert_eq!(step.status, BootStepStatus::Failed);
    assert_eq!(step.error.as_deref(), Some("dm-verity mismatch"));
}

#[test]
fn mark_step_nonexistent() {
    let mut init = ArgonautInit::new(minimal_config());
    // Minimal mode has no compositor stage.
    assert!(!init.mark_step_complete(BootStage::StartCompositor));
}

// --- Current stage ---

#[test]
fn current_stage_returns_first_pending() {
    let init = ArgonautInit::new(minimal_config());
    let current = init.current_stage().unwrap();
    assert_eq!(current.stage, BootStage::MountFilesystems);
}

#[test]
fn current_stage_skips_complete() {
    let mut init = ArgonautInit::new(minimal_config());
    init.mark_step_complete(BootStage::MountFilesystems);
    let current = init.current_stage().unwrap();
    assert_eq!(current.stage, BootStage::StartDeviceManager);
}

// --- Boot complete ---

#[test]
fn is_boot_complete_all_complete() {
    let mut init = ArgonautInit::new(minimal_config());
    for step in &mut init.boot_sequence {
        step.status = BootStepStatus::Complete;
    }
    assert!(init.is_boot_complete());
}

#[test]
fn is_boot_complete_required_failed() {
    let mut init = ArgonautInit::new(minimal_config());
    for step in &mut init.boot_sequence {
        if step.required {
            step.status = BootStepStatus::Failed;
        } else {
            step.status = BootStepStatus::Complete;
        }
    }
    // Required steps failed — boot is NOT complete.
    assert!(!init.is_boot_complete());
}

#[test]
fn is_boot_complete_optional_failed_ok() {
    let mut init = ArgonautInit::new(desktop_config());
    for step in &mut init.boot_sequence {
        if step.required {
            step.status = BootStepStatus::Complete;
        } else {
            step.status = BootStepStatus::Failed;
        }
    }
    // Non-required failures are tolerated.
    assert!(init.is_boot_complete());
}

// --- Boot duration ---

#[test]
fn boot_duration_ms_calculation() {
    let mut init = ArgonautInit::new(minimal_config());
    let start = Utc::now();
    init.boot_started = Some(start);
    init.boot_completed = Some(start + chrono::Duration::milliseconds(1234));
    assert_eq!(init.boot_duration_ms(), Some(1234));
}

#[test]
fn boot_duration_ms_not_complete() {
    let init = ArgonautInit::new(minimal_config());
    assert_eq!(init.boot_duration_ms(), None);
}

// --- Failed steps ---

#[test]
fn failed_steps_list() {
    let mut init = ArgonautInit::new(minimal_config());
    init.mark_step_failed(BootStage::VerifyRootfs, "bad hash".into());
    init.mark_step_failed(BootStage::StartSecurity, "seccomp err".into());
    let failed = init.failed_steps();
    assert_eq!(failed.len(), 2);
    let stages: Vec<BootStage> = failed.iter().map(|s| s.stage).collect();
    assert!(stages.contains(&BootStage::VerifyRootfs));
    assert!(stages.contains(&BootStage::StartSecurity));
}

// --- Shutdown order ---

#[test]
fn shutdown_order_is_reverse_of_startup() {
    let init = ArgonautInit::new(desktop_config());
    let definitions: Vec<ServiceDefinition> = init
        .services
        .values()
        .map(|s| s.definition.clone())
        .collect();
    let startup = ArgonautInit::resolve_service_order(&definitions).unwrap();
    let shutdown = init.shutdown_order().unwrap();
    let reversed_startup: Vec<String> = startup.into_iter().rev().collect();
    assert_eq!(shutdown, reversed_startup);
}

// --- ArgonautConfig defaults ---

#[test]
fn config_defaults() {
    let cfg = ArgonautConfig::default();
    assert_eq!(cfg.boot_mode, BootMode::Desktop);
    assert_eq!(cfg.boot_timeout_ms, 30_000);
    assert_eq!(cfg.shutdown_timeout_ms, 10_000);
    assert!(cfg.log_to_console);
    assert!(cfg.verify_on_boot);
    assert!(cfg.services.is_empty());
}

// --- ServiceDefinition with checks ---

#[test]
fn service_definition_with_health_check() {
    let svcs = ArgonautInit::default_services(BootMode::Minimal);
    let agent_rt = &svcs[0];
    assert!(agent_rt.health_check.is_some());
    let hc = agent_rt.health_check.as_ref().unwrap();
    assert!(matches!(hc.check_type, HealthCheckType::HttpGet(_)));
    assert_eq!(hc.retries, 3);
}

#[test]
fn service_definition_with_ready_check() {
    let svcs = ArgonautInit::default_services(BootMode::Minimal);
    let agent_rt = &svcs[0];
    assert!(agent_rt.ready_check.is_some());
    let rc = agent_rt.ready_check.as_ref().unwrap();
    assert!(matches!(
        rc.check_type,
        HealthCheckType::TcpConnect(_, 8090)
    ));
    assert_eq!(rc.retry_delay_ms, 200);
}

// --- ManagedService initial state ---

#[test]
fn managed_service_initial_state() {
    let init = ArgonautInit::new(minimal_config());
    let svc = init.get_service("agent-runtime").unwrap();
    assert_eq!(svc.state, ServiceState::Stopped);
    assert!(svc.pid.is_none());
    assert!(svc.started_at.is_none());
    assert_eq!(svc.restart_count, 0);
    assert!(svc.last_health_check.is_none());
}

// --- services_for_mode ---

#[test]
fn services_for_mode_filtering() {
    let init = ArgonautInit::new(desktop_config());
    let minimal_svcs = init.services_for_mode(&BootMode::Minimal);
    // Only agent-runtime is required for Minimal.
    assert_eq!(minimal_svcs.len(), 1);
    assert_eq!(minimal_svcs[0].name, "agent-runtime");
}

#[test]
fn services_for_mode_desktop() {
    let init = ArgonautInit::new(desktop_config());
    let desktop_svcs = init.services_for_mode(&BootMode::Desktop);
    assert_eq!(desktop_svcs.len(), 7);
}

// --- Stats ---

#[test]
fn stats_accuracy() {
    let mut init = ArgonautInit::new(server_config());
    // Start database services first (agent-runtime depends on them in server mode)
    assert!(init.set_service_state("postgres", ServiceState::Starting));
    assert!(init.set_service_state("postgres", ServiceState::Running));
    assert!(init.set_service_state("redis", ServiceState::Starting));
    assert!(init.set_service_state("redis", ServiceState::Running));
    // Valid transition path: Stopped → Starting → Running
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert!(init.set_service_state("agent-runtime", ServiceState::Running));
    // llm-gateway depends on agent-runtime which is now Running
    assert!(init.set_service_state("llm-gateway", ServiceState::Starting));
    assert!(init.set_service_state("llm-gateway", ServiceState::Failed("crash".into()),));
    if let Some(svc) = init.services.get_mut("agent-runtime") {
        svc.restart_count = 3;
    }
    let s = init.stats();
    assert_eq!(s.boot_mode, BootMode::Server);
    assert_eq!(s.services_running, 3);
    assert_eq!(s.services_failed, 1);
    assert_eq!(s.total_restarts, 3);
    assert!(!s.boot_complete);
}

#[test]
fn stats_empty_init() {
    let init = ArgonautInit::new(minimal_config());
    let s = init.stats();
    assert_eq!(s.boot_mode, BootMode::Minimal);
    assert_eq!(s.services_running, 0);
    assert_eq!(s.services_failed, 0);
    assert_eq!(s.total_restarts, 0);
    assert!(!s.boot_complete);
    assert!(s.boot_duration_ms.is_none());
}

// --- Boot step timeout values ---

#[test]
fn boot_step_timeout_values() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Desktop);
    let fs_step = steps
        .iter()
        .find(|s| s.stage == BootStage::MountFilesystems)
        .unwrap();
    assert_eq!(fs_step.timeout_ms, 2000);
    let verify_step = steps
        .iter()
        .find(|s| s.stage == BootStage::VerifyRootfs)
        .unwrap();
    assert_eq!(verify_step.timeout_ms, 5000);
    let complete_step = steps
        .iter()
        .find(|s| s.stage == BootStage::BootComplete)
        .unwrap();
    assert_eq!(complete_step.timeout_ms, 1000);
}

// --- Depends-on resolution for default desktop ---

#[test]
fn service_depends_on_resolution_desktop() {
    let svcs = ArgonautInit::default_services(BootMode::Desktop);
    let order = ArgonautInit::resolve_service_order(&svcs).unwrap();
    let rt_pos = order.iter().position(|n| n == "agent-runtime").unwrap();
    let gw_pos = order.iter().position(|n| n == "llm-gateway").unwrap();
    let comp_pos = order.iter().position(|n| n == "aethersafha").unwrap();
    let shell_pos = order.iter().position(|n| n == "agnoshi").unwrap();
    // agent-runtime before everything.
    assert!(rt_pos < gw_pos);
    assert!(rt_pos < comp_pos);
    assert!(rt_pos < shell_pos);
    // aethersafha before agnoshi (shell depends on compositor).
    assert!(comp_pos < shell_pos);
}

// --- Audit fix tests ---

#[test]
fn invalid_state_transition_stopped_to_running() {
    let mut init = ArgonautInit::new(minimal_config());
    // Stopped → Running is not valid (must go through Starting)
    assert!(!init.set_service_state("agent-runtime", ServiceState::Running));
    // State should remain Stopped
    assert_eq!(
        init.get_service_state("agent-runtime"),
        Some(&ServiceState::Stopped)
    );
}

#[test]
fn valid_state_transition_full_lifecycle() {
    let mut init = ArgonautInit::new(minimal_config());
    // Stopped → Starting → Running → Stopping → Stopped
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert!(init.set_service_state("agent-runtime", ServiceState::Running));
    assert!(init.set_service_state("agent-runtime", ServiceState::Stopping));
    assert!(init.set_service_state("agent-runtime", ServiceState::Stopped));
    // Failed → Starting (restart), Failed → Stopped
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert!(init.set_service_state("agent-runtime", ServiceState::Failed("err".into())));
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert!(init.set_service_state("agent-runtime", ServiceState::Failed("err2".into())));
    assert!(init.set_service_state("agent-runtime", ServiceState::Stopped));
}

#[test]
fn starting_blocked_when_dependency_not_running() {
    let mut init = ArgonautInit::new(server_config());
    // llm-gateway depends on agent-runtime. agent-runtime is Stopped.
    assert!(!init.set_service_state("llm-gateway", ServiceState::Starting));
    // Start database services first (agent-runtime depends on them in server mode)
    assert!(init.set_service_state("postgres", ServiceState::Starting));
    assert!(init.set_service_state("postgres", ServiceState::Running));
    assert!(init.set_service_state("redis", ServiceState::Starting));
    assert!(init.set_service_state("redis", ServiceState::Running));
    // Start agent-runtime but leave it in Starting (not Running).
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert!(!init.set_service_state("llm-gateway", ServiceState::Starting));
    // Now make agent-runtime Running.
    assert!(init.set_service_state("agent-runtime", ServiceState::Running));
    assert!(init.set_service_state("llm-gateway", ServiceState::Starting));
}

#[test]
fn register_service_overwrites_definition_preserves_state() {
    let mut init = ArgonautInit::new(minimal_config());
    let svc = dummy_service("my-svc", vec![]);
    init.register_service(svc);
    // Transition to Starting
    assert!(init.set_service_state("my-svc", ServiceState::Starting));
    assert!(init.set_service_state("my-svc", ServiceState::Running));
    if let Some(s) = init.services.get_mut("my-svc") {
        s.restart_count = 5;
    }
    // Re-register with updated description
    let mut svc2 = dummy_service("my-svc", vec![]);
    svc2.description = "updated description".into();
    init.register_service(svc2);
    let got = init.get_service("my-svc").unwrap();
    // Definition updated
    assert_eq!(got.definition.description, "updated description");
    // State preserved
    assert_eq!(got.state, ServiceState::Running);
    assert_eq!(got.restart_count, 5);
}

#[test]
fn boot_started_set_after_first_step_completes() {
    let mut init = ArgonautInit::new(minimal_config());
    assert!(init.boot_started.is_none());
    init.mark_step_complete(BootStage::MountFilesystems);
    assert!(init.boot_started.is_some());
}

#[test]
fn boot_started_set_after_first_step_fails() {
    let mut init = ArgonautInit::new(minimal_config());
    assert!(init.boot_started.is_none());
    init.mark_step_failed(BootStage::MountFilesystems, "fail".into());
    assert!(init.boot_started.is_some());
}

#[test]
fn started_at_populated_on_mark_step_complete() {
    let mut init = ArgonautInit::new(minimal_config());
    init.mark_step_complete(BootStage::MountFilesystems);
    let step = init
        .boot_sequence
        .iter()
        .find(|s| s.stage == BootStage::MountFilesystems)
        .unwrap();
    assert!(step.started_at.is_some());
    assert!(step.completed_at.is_some());
    // started_at should be <= completed_at
    assert!(step.started_at.unwrap() <= step.completed_at.unwrap());
}

#[test]
fn started_at_populated_on_mark_step_failed() {
    let mut init = ArgonautInit::new(minimal_config());
    init.mark_step_failed(BootStage::MountFilesystems, "oops".into());
    let step = init
        .boot_sequence
        .iter()
        .find(|s| s.stage == BootStage::MountFilesystems)
        .unwrap();
    assert!(step.started_at.is_some());
}

#[test]
fn missing_dependency_returns_error() {
    let services = vec![dummy_service("a", vec!["nonexistent"])];
    let result = ArgonautInit::resolve_service_order(&services);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("depends on"));
    assert!(err.contains("nonexistent"));
    assert!(err.contains("not defined"));
}

#[test]
fn shutdown_order_returns_error_on_cycle() {
    let mut init = ArgonautInit::new(minimal_config());
    // Create a cycle: x depends on y, y depends on x
    let svc_x = dummy_service("x", vec!["y"]);
    let svc_y = dummy_service("y", vec!["x"]);
    init.services.clear();
    init.services.insert(
        "x".into(),
        ManagedService {
            definition: svc_x,
            state: ServiceState::Stopped,
            pid: None,
            started_at: None,
            restart_count: 0,
            last_health_check: None,
        },
    );
    init.services.insert(
        "y".into(),
        ManagedService {
            definition: svc_y,
            state: ServiceState::Stopped,
            pid: None,
            started_at: None,
            restart_count: 0,
            last_health_check: None,
        },
    );
    let result = init.shutdown_order();
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cycle detected"));
}

// --- Database services ---

#[test]
fn database_services_returns_postgres_and_redis() {
    let svcs = ArgonautInit::database_services();
    assert_eq!(svcs.len(), 2);
    let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"postgres"));
    assert!(names.contains(&"redis"));
}

#[test]
fn database_services_health_checks() {
    let svcs = ArgonautInit::database_services();
    for svc in &svcs {
        assert!(svc.health_check.is_some());
        assert!(svc.ready_check.is_some());
    }
}

#[test]
fn database_services_restart_policies() {
    let svcs = ArgonautInit::database_services();
    let pg = svcs.iter().find(|s| s.name == "postgres").unwrap();
    let redis = svcs.iter().find(|s| s.name == "redis").unwrap();
    assert_eq!(pg.restart_policy, RestartPolicy::OnFailure);
    assert_eq!(redis.restart_policy, RestartPolicy::Always);
}

#[test]
fn database_services_modes() {
    let svcs = ArgonautInit::database_services();
    for svc in &svcs {
        assert!(svc.required_for_modes.contains(&BootMode::Server));
        assert!(svc.required_for_modes.contains(&BootMode::Desktop));
        assert!(!svc.required_for_modes.contains(&BootMode::Minimal));
    }
}

#[test]
fn default_services_server_includes_databases() {
    let svcs = ArgonautInit::default_services(BootMode::Server);
    let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"postgres"));
    assert!(names.contains(&"redis"));
}

#[test]
fn default_services_minimal_excludes_databases() {
    let svcs = ArgonautInit::default_services(BootMode::Minimal);
    let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
    assert!(!names.contains(&"postgres"));
    assert!(!names.contains(&"redis"));
}

#[test]
fn agent_runtime_depends_on_databases_in_server_mode() {
    let svcs = ArgonautInit::default_services(BootMode::Server);
    let rt = svcs.iter().find(|s| s.name == "agent-runtime").unwrap();
    assert!(rt.depends_on.contains(&"postgres".to_string()));
    assert!(rt.depends_on.contains(&"redis".to_string()));
}

#[test]
fn agent_runtime_no_db_deps_in_minimal_mode() {
    let svcs = ArgonautInit::default_services(BootMode::Minimal);
    let rt = svcs.iter().find(|s| s.name == "agent-runtime").unwrap();
    assert!(rt.depends_on.is_empty());
}

#[test]
fn boot_stage_database_ordering() {
    assert!(BootStage::StartSecurity < BootStage::StartDatabaseServices);
    assert!(BootStage::StartDatabaseServices < BootStage::StartAgentRuntime);
}

#[test]
fn boot_stage_database_display() {
    assert_eq!(
        BootStage::StartDatabaseServices.to_string(),
        "start-database-services"
    );
}

#[test]
fn boot_sequence_server_includes_database_stage() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
    let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
    assert!(stages.contains(&BootStage::StartDatabaseServices));
}

#[test]
fn boot_sequence_minimal_excludes_database_stage() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
    let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
    assert!(!stages.contains(&BootStage::StartDatabaseServices));
}

// --- Synapse service ---

#[test]
fn synapse_service_definition() {
    let svc = ArgonautInit::synapse_service();
    assert_eq!(svc.name, "synapse");
    assert!(svc.depends_on.contains(&"agent-runtime".to_string()));
    assert!(svc.depends_on.contains(&"llm-gateway".to_string()));
    assert!(svc.health_check.is_some());
    assert!(svc.ready_check.is_some());
}

#[test]
fn server_mode_includes_synapse() {
    let services = ArgonautInit::default_services(BootMode::Server);
    assert!(services.iter().any(|s| s.name == "synapse"));
}

#[test]
fn desktop_mode_includes_synapse() {
    let services = ArgonautInit::default_services(BootMode::Desktop);
    assert!(services.iter().any(|s| s.name == "synapse"));
}

#[test]
fn minimal_mode_excludes_synapse() {
    let services = ArgonautInit::default_services(BootMode::Minimal);
    assert!(!services.iter().any(|s| s.name == "synapse"));
}

#[test]
fn synapse_starts_after_llm_gateway() {
    let services = ArgonautInit::default_services(BootMode::Server);
    let order = ArgonautInit::resolve_service_order(&services).unwrap();
    let gw_pos = order.iter().position(|s| s == "llm-gateway").unwrap();
    let syn_pos = order.iter().position(|s| s == "synapse").unwrap();
    assert!(syn_pos > gw_pos);
}

#[test]
fn boot_sequence_includes_model_services_for_server() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Server);
    assert!(steps
        .iter()
        .any(|s| s.stage == BootStage::StartModelServices));
}

#[test]
fn boot_sequence_excludes_model_services_for_minimal() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Minimal);
    assert!(!steps
        .iter()
        .any(|s| s.stage == BootStage::StartModelServices));
}

#[test]
fn model_services_stage_after_llm_gateway() {
    assert!(BootStage::StartModelServices > BootStage::StartLlmGateway);
    assert!(BootStage::StartModelServices < BootStage::StartCompositor);
}

#[test]
fn server_service_count_with_synapse() {
    let services = ArgonautInit::default_services(BootMode::Server);
    // postgres, redis, agent-runtime, llm-gateway, synapse = 5
    assert_eq!(services.len(), 5);
}

#[test]
fn desktop_service_count_with_synapse() {
    let services = ArgonautInit::default_services(BootMode::Desktop);
    // postgres, redis, agent-runtime, llm-gateway, synapse, aethersafha, agnoshi = 7
    assert_eq!(services.len(), 7);
}

// -----------------------------------------------------------------------
// Phase 12A: Runlevel tests
// -----------------------------------------------------------------------

#[test]
fn runlevel_to_boot_mode() {
    assert_eq!(Runlevel::Console.to_boot_mode(), Some(BootMode::Server));
    assert_eq!(Runlevel::Graphical.to_boot_mode(), Some(BootMode::Desktop));
    assert_eq!(Runlevel::Container.to_boot_mode(), Some(BootMode::Minimal));
    assert_eq!(Runlevel::Emergency.to_boot_mode(), None);
    assert_eq!(Runlevel::Rescue.to_boot_mode(), None);
}

#[test]
fn runlevel_from_boot_mode() {
    assert_eq!(
        Runlevel::from_boot_mode(BootMode::Server),
        Runlevel::Console
    );
    assert_eq!(
        Runlevel::from_boot_mode(BootMode::Desktop),
        Runlevel::Graphical
    );
    assert_eq!(
        Runlevel::from_boot_mode(BootMode::Minimal),
        Runlevel::Container
    );
}

#[test]
fn runlevel_levels() {
    assert_eq!(Runlevel::Emergency.level(), 0);
    assert_eq!(Runlevel::Rescue.level(), 1);
    assert_eq!(Runlevel::Console.level(), 3);
    assert_eq!(Runlevel::Graphical.level(), 5);
    assert_eq!(Runlevel::Container.level(), 7);
}

#[test]
fn runlevel_display() {
    assert_eq!(format!("{}", Runlevel::Emergency), "emergency");
    assert_eq!(format!("{}", Runlevel::Rescue), "rescue");
    assert_eq!(format!("{}", Runlevel::Console), "console");
    assert_eq!(format!("{}", Runlevel::Graphical), "graphical");
    assert_eq!(format!("{}", Runlevel::Container), "container");
}

// -----------------------------------------------------------------------
// Service target tests
// -----------------------------------------------------------------------

#[test]
fn default_targets_exist() {
    let targets = ServiceTarget::defaults();
    assert_eq!(targets.len(), 5);
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"basic"));
    assert!(names.contains(&"network"));
    assert!(names.contains(&"agnos-core"));
    assert!(names.contains(&"graphical"));
    assert!(names.contains(&"edge"));
}

#[test]
fn target_active_in_runlevel() {
    let targets = ServiceTarget::defaults();
    let basic = targets.iter().find(|t| t.name == "basic").unwrap();
    assert!(basic.is_active_in(Runlevel::Console));
    assert!(basic.is_active_in(Runlevel::Graphical));
    assert!(basic.is_active_in(Runlevel::Rescue));
    assert!(!basic.is_active_in(Runlevel::Emergency));

    let graphical = targets.iter().find(|t| t.name == "graphical").unwrap();
    assert!(graphical.is_active_in(Runlevel::Graphical));
    assert!(!graphical.is_active_in(Runlevel::Console));
}

#[test]
fn target_all_services() {
    let targets = ServiceTarget::defaults();
    let network = targets.iter().find(|t| t.name == "network").unwrap();
    let svcs = network.all_services();
    assert!(svcs.contains(&"networkmanager"));
    assert!(svcs.contains(&"nftables"));
    assert!(svcs.contains(&"openssh"));
}

// -----------------------------------------------------------------------
// Shutdown plan tests
// -----------------------------------------------------------------------

#[test]
fn shutdown_plan_poweroff() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
    assert_eq!(plan.shutdown_type, ShutdownType::Poweroff);
    assert!(!plan.steps.is_empty());
    // Last step should be KernelAction
    let last = plan.steps.last().unwrap();
    assert_eq!(
        last.action,
        ShutdownAction::KernelAction(ShutdownType::Poweroff)
    );
}

#[test]
fn shutdown_plan_reboot() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let plan = init.plan_shutdown(ShutdownType::Reboot).unwrap();
    assert_eq!(plan.shutdown_type, ShutdownType::Reboot);
    let last = plan.steps.last().unwrap();
    assert_eq!(
        last.action,
        ShutdownAction::KernelAction(ShutdownType::Reboot)
    );
}

#[test]
fn shutdown_plan_includes_sync() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action == ShutdownAction::SyncFilesystems));
}

#[test]
fn shutdown_plan_includes_unmount() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let plan = init.plan_shutdown(ShutdownType::Halt).unwrap();
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action == ShutdownAction::UnmountFilesystems));
}

#[test]
fn shutdown_plan_has_wall_message() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let plan = init.plan_shutdown(ShutdownType::Reboot).unwrap();
    assert!(plan.wall_message.is_some());
    assert!(plan.wall_message.unwrap().contains("reboot"));
}

#[test]
fn shutdown_plan_stops_running_services() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Minimal,
        ..ArgonautConfig::default()
    };
    let mut init = ArgonautInit::new(config);
    // Minimal has only agent-runtime with no deps
    init.set_service_state("agent-runtime", ServiceState::Starting);
    init.set_service_state("agent-runtime", ServiceState::Running);

    let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
    let stop_steps: Vec<_> = plan
        .steps
        .iter()
        .filter(|s| matches!(s.action, ShutdownAction::StopService { .. }))
        .collect();
    assert!(!stop_steps.is_empty());
}

#[test]
fn shutdown_plan_includes_luks_close() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let plan = init.plan_shutdown(ShutdownType::Poweroff).unwrap();
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action == ShutdownAction::CloseLuks));
}

#[test]
fn shutdown_type_display() {
    assert_eq!(format!("{}", ShutdownType::Poweroff), "poweroff");
    assert_eq!(format!("{}", ShutdownType::Reboot), "reboot");
    assert_eq!(format!("{}", ShutdownType::Halt), "halt");
    assert_eq!(format!("{}", ShutdownType::Kexec), "kexec");
}

#[test]
fn shutdown_step_status_display() {
    assert_eq!(format!("{}", ShutdownStepStatus::Pending), "pending");
    assert_eq!(
        format!("{}", ShutdownStepStatus::Failed("disk busy".into())),
        "failed: disk busy"
    );
}

// -----------------------------------------------------------------------
// Runlevel switch plan tests
// -----------------------------------------------------------------------

#[test]
fn runlevel_switch_console_to_graphical() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Server,
        ..ArgonautConfig::default()
    };
    let init = ArgonautInit::new(config);
    let targets = ServiceTarget::defaults();
    let plan = init.plan_runlevel_switch(Runlevel::Graphical, &targets);
    assert_eq!(plan.from, Runlevel::Console);
    assert_eq!(plan.to, Runlevel::Graphical);
    // Should want to start graphical services
    assert!(plan.services_to_start.contains(&"aethersafha".to_string()));
    assert!(!plan.drop_to_shell);
}

#[test]
fn runlevel_switch_to_emergency_stops_all() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Minimal,
        ..ArgonautConfig::default()
    };
    let mut init = ArgonautInit::new(config);
    // Minimal has agent-runtime with no deps, so state transition works
    init.set_service_state("agent-runtime", ServiceState::Starting);
    init.set_service_state("agent-runtime", ServiceState::Running);

    let targets = ServiceTarget::defaults();
    let plan = init.plan_runlevel_switch(Runlevel::Emergency, &targets);
    assert!(plan.drop_to_shell);
    assert!(plan.services_to_start.is_empty());
    // Should stop running services
    assert!(plan.services_to_stop.contains(&"agent-runtime".to_string()));
}

#[test]
fn runlevel_switch_rescue_drops_to_shell() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let targets = ServiceTarget::defaults();
    let plan = init.plan_runlevel_switch(Runlevel::Rescue, &targets);
    assert!(plan.drop_to_shell);
}

// -----------------------------------------------------------------------
// Health tracker tests
// -----------------------------------------------------------------------

#[test]
fn health_tracker_records_pass() {
    use super::types::HealthTracker;
    let mut tracker = HealthTracker::new();
    let should_restart = tracker.record("svc1", true, 3);
    assert!(!should_restart);
    assert_eq!(tracker.failure_count("svc1"), 0);
}

#[test]
fn health_tracker_records_failures() {
    use super::types::HealthTracker;
    let mut tracker = HealthTracker::new();
    assert!(!tracker.record("svc1", false, 3));
    assert_eq!(tracker.failure_count("svc1"), 1);
    assert!(!tracker.record("svc1", false, 3));
    assert_eq!(tracker.failure_count("svc1"), 2);
    // Third failure triggers restart
    assert!(tracker.record("svc1", false, 3));
    assert_eq!(tracker.failure_count("svc1"), 3);
}

#[test]
fn health_tracker_resets_on_pass() {
    use super::types::HealthTracker;
    let mut tracker = HealthTracker::new();
    tracker.record("svc1", false, 3);
    tracker.record("svc1", false, 3);
    // Pass resets counter
    tracker.record("svc1", true, 3);
    assert_eq!(tracker.failure_count("svc1"), 0);
}

#[test]
fn health_tracker_reset_manual() {
    use super::types::HealthTracker;
    let mut tracker = HealthTracker::new();
    tracker.record("svc1", false, 3);
    tracker.record("svc1", false, 3);
    tracker.reset("svc1");
    assert_eq!(tracker.failure_count("svc1"), 0);
}

#[test]
fn health_tracker_independent_services() {
    use super::types::HealthTracker;
    let mut tracker = HealthTracker::new();
    tracker.record("svc1", false, 2);
    tracker.record("svc2", false, 2);
    assert_eq!(tracker.failure_count("svc1"), 1);
    assert_eq!(tracker.failure_count("svc2"), 1);
    // Only svc1 reaches threshold
    assert!(tracker.record("svc1", false, 2));
    assert!(!tracker.record("svc2", true, 2));
}

// -----------------------------------------------------------------------
// Exit status and event tests
// -----------------------------------------------------------------------

#[test]
fn exit_status_display() {
    assert_eq!(format!("{}", ExitStatus::Code(0)), "exit(0)");
    assert_eq!(format!("{}", ExitStatus::Code(1)), "exit(1)");
    assert_eq!(format!("{}", ExitStatus::Signal(9)), "signal(9)");
    assert_eq!(format!("{}", ExitStatus::Signal(15)), "signal(15)");
    assert_eq!(format!("{}", ExitStatus::Running), "running");
    assert_eq!(format!("{}", ExitStatus::NotStarted), "not-started");
}

#[test]
fn service_event_type_display() {
    assert_eq!(format!("{}", ServiceEventType::Starting), "starting");
    assert_eq!(
        format!("{}", ServiceEventType::Started { pid: 42 }),
        "started(pid=42)"
    );
    assert_eq!(
        format!("{}", ServiceEventType::HealthCheckFailed { consecutive: 3 }),
        "health-fail(3x)"
    );
    assert_eq!(
        format!("{}", ServiceEventType::TimeoutKilled),
        "timeout-killed"
    );
    assert_eq!(
        format!(
            "{}",
            ServiceEventType::CrashDetected {
                exit_status: ExitStatus::Signal(11)
            }
        ),
        "crash(signal(11))"
    );
}

#[test]
fn record_event_creates_event() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let event = init.record_event("daimon", ServiceEventType::Starting);
    assert_eq!(event.service, "daimon");
    assert_eq!(event.event_type, ServiceEventType::Starting);
}

// -----------------------------------------------------------------------
// Process spec tests
// -----------------------------------------------------------------------

#[test]
fn process_spec_from_service() {
    use super::types::ProcessSpec;
    let def = ServiceDefinition {
        name: "test-svc".into(),
        description: "test".into(),
        binary_path: PathBuf::from("/usr/bin/test"),
        args: vec!["--flag".into()],
        environment: HashMap::new(),
        depends_on: vec![],
        required_for_modes: vec![BootMode::Server],
        restart_policy: RestartPolicy::Always,
        health_check: None,
        ready_check: None,
    };
    let spec = ProcessSpec::from_service(&def);
    assert_eq!(spec.binary, PathBuf::from("/usr/bin/test"));
    assert_eq!(spec.args, vec!["--flag"]);
    assert!(spec
        .stdout_log
        .unwrap()
        .to_str()
        .unwrap()
        .contains("test-svc"));
    assert!(spec
        .stderr_log
        .unwrap()
        .to_str()
        .unwrap()
        .contains("test-svc"));
}

// -----------------------------------------------------------------------
// Emergency shell tests
// -----------------------------------------------------------------------

#[test]
fn emergency_shell_default_config() {
    let config = EmergencyShellConfig::default();
    assert_eq!(config.shell_path, PathBuf::from("/usr/bin/agnoshi"));
    assert!(!config.require_auth);
    assert!(config.banner.contains("Emergency"));
    assert_eq!(config.environment.get("SHELL").unwrap(), "/usr/bin/agnoshi");
}

#[test]
fn should_drop_to_emergency_on_required_failure() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Server,
        ..ArgonautConfig::default()
    };
    let mut init = ArgonautInit::new(config);
    // MountFilesystems is required — fail it
    init.mark_step_failed(BootStage::MountFilesystems, "disk error".into());
    assert!(init.should_drop_to_emergency());
}

#[test]
fn no_emergency_without_required_failure() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    assert!(!init.should_drop_to_emergency());
}

// -----------------------------------------------------------------------
// Boot execution plan tests
// -----------------------------------------------------------------------

#[test]
fn boot_execution_plan_ordered() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Minimal,
        ..ArgonautConfig::default()
    };
    let init = ArgonautInit::new(config);
    let plan = init.boot_execution_plan().unwrap();
    assert!(!plan.is_empty());
    // First service should be agent-runtime (only service in minimal)
    assert_eq!(plan[0].0, "agent-runtime");
}

#[test]
fn boot_execution_plan_server() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Server,
        ..ArgonautConfig::default()
    };
    let init = ArgonautInit::new(config);
    let plan = init.boot_execution_plan().unwrap();
    let names: Vec<&str> = plan.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"agent-runtime"));
    assert!(names.contains(&"llm-gateway"));
    // agent-runtime should come after postgres/redis (dependencies)
    let pg_idx = names.iter().position(|n| *n == "postgres");
    let ar_idx = names.iter().position(|n| *n == "agent-runtime");
    if let (Some(pg), Some(ar)) = (pg_idx, ar_idx) {
        assert!(pg < ar, "postgres should start before agent-runtime");
    }
}

// -----------------------------------------------------------------------
// Crash action tests
// -----------------------------------------------------------------------

#[test]
fn crash_action_always_restarts() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Minimal,
        ..ArgonautConfig::default()
    };
    let init = ArgonautInit::new(config);
    let action = init.on_service_crash("agent-runtime", &ExitStatus::Code(1));
    assert!(matches!(action, CrashAction::Restart { .. }));
}

#[test]
fn crash_action_on_failure_ignores_clean_exit() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Server,
        ..ArgonautConfig::default()
    };
    let init = ArgonautInit::new(config);
    // postgres has OnFailure restart policy
    let action = init.on_service_crash("postgres", &ExitStatus::Code(0));
    assert_eq!(action, CrashAction::Ignore);
}

#[test]
fn crash_action_on_failure_restarts_on_error() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Server,
        ..ArgonautConfig::default()
    };
    let init = ArgonautInit::new(config);
    let action = init.on_service_crash("postgres", &ExitStatus::Code(1));
    assert!(matches!(action, CrashAction::Restart { .. }));
}

#[test]
fn crash_action_unknown_service() {
    let config = ArgonautConfig::default();
    let init = ArgonautInit::new(config);
    let action = init.on_service_crash("nonexistent", &ExitStatus::Code(1));
    assert_eq!(action, CrashAction::Ignore);
}

#[test]
fn crash_action_gives_up_after_limit() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Minimal,
        ..ArgonautConfig::default()
    };
    let mut init = ArgonautInit::new(config);
    // Simulate 5 restarts
    if let Some(svc) = init.services.get_mut("agent-runtime") {
        svc.restart_count = 5;
    }
    let action = init.on_service_crash("agent-runtime", &ExitStatus::Signal(11));
    assert!(matches!(action, CrashAction::GiveUp { .. }));
}

#[test]
fn backoff_delay_exponential() {
    use super::services::backoff_delay;
    assert_eq!(backoff_delay(0), 1000);
    assert_eq!(backoff_delay(1), 2000);
    assert_eq!(backoff_delay(2), 4000);
    assert_eq!(backoff_delay(3), 8000);
    assert_eq!(backoff_delay(4), 16000);
    // Capped at 30s
    assert_eq!(backoff_delay(5), 30000);
    assert_eq!(backoff_delay(10), 30000);
}

// -----------------------------------------------------------------------
// Phase 14A: Edge boot mode tests
// -----------------------------------------------------------------------

fn edge_config() -> ArgonautConfig {
    ArgonautConfig {
        boot_mode: BootMode::Edge,
        ..Default::default()
    }
}

#[test]
fn boot_sequence_edge_minimal_stages() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Edge);
    let stages: Vec<BootStage> = steps.iter().map(|s| s.stage).collect();
    // Edge gets: MountFS, DevMgr, Verify, Security, AgentRuntime, BootComplete = 6
    assert_eq!(steps.len(), 6);
    assert!(stages.contains(&BootStage::MountFilesystems));
    assert!(stages.contains(&BootStage::VerifyRootfs));
    assert!(stages.contains(&BootStage::StartAgentRuntime));
    assert!(stages.contains(&BootStage::BootComplete));
    // Must NOT have these:
    assert!(!stages.contains(&BootStage::StartDatabaseServices));
    assert!(!stages.contains(&BootStage::StartLlmGateway));
    assert!(!stages.contains(&BootStage::StartModelServices));
    assert!(!stages.contains(&BootStage::StartCompositor));
    assert!(!stages.contains(&BootStage::StartShell));
}

#[test]
fn boot_sequence_edge_fast_timeouts() {
    let steps = ArgonautInit::build_boot_sequence(BootMode::Edge);
    let rt_step = steps
        .iter()
        .find(|s| s.stage == BootStage::StartAgentRuntime)
        .unwrap();
    // Edge agent-runtime timeout should be 3s (tight for fast boot)
    assert_eq!(rt_step.timeout_ms, 3000);
    let complete = steps
        .iter()
        .find(|s| s.stage == BootStage::BootComplete)
        .unwrap();
    assert_eq!(complete.timeout_ms, 500);
}

#[test]
fn default_services_edge() {
    let svcs = ArgonautInit::default_services(BootMode::Edge);
    assert_eq!(svcs.len(), 1);
    assert_eq!(svcs[0].name, "agent-runtime");
    assert!(svcs[0].depends_on.is_empty());
    assert!(svcs[0].required_for_modes.contains(&BootMode::Edge));
    // Edge mode env vars
    assert_eq!(svcs[0].environment.get("AGNOS_EDGE_MODE").unwrap(), "1");
    assert_eq!(
        svcs[0].environment.get("AGNOS_READONLY_ROOTFS").unwrap(),
        "1"
    );
}

#[test]
fn edge_services_no_databases() {
    let svcs = ArgonautInit::default_services(BootMode::Edge);
    let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
    assert!(!names.contains(&"postgres"));
    assert!(!names.contains(&"redis"));
    assert!(!names.contains(&"llm-gateway"));
    assert!(!names.contains(&"synapse"));
    assert!(!names.contains(&"aethersafha"));
    assert!(!names.contains(&"agnoshi"));
}

#[test]
fn edge_init_creates_single_service() {
    let init = ArgonautInit::new(edge_config());
    assert_eq!(init.services.len(), 1);
    assert!(init.services.contains_key("agent-runtime"));
}

#[test]
fn edge_boot_can_complete() {
    let mut init = ArgonautInit::new(edge_config());
    for step in &mut init.boot_sequence {
        step.status = BootStepStatus::Complete;
    }
    assert!(init.is_boot_complete());
}

#[test]
fn edge_shutdown_plan() {
    let init = ArgonautInit::new(edge_config());
    let plan = init.plan_shutdown(ShutdownType::Reboot).unwrap();
    assert_eq!(plan.shutdown_type, ShutdownType::Reboot);
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action == ShutdownAction::CloseLuks));
}

#[test]
fn runlevel_edge_mapping() {
    assert_eq!(Runlevel::Edge.to_boot_mode(), Some(BootMode::Edge));
    assert_eq!(Runlevel::from_boot_mode(BootMode::Edge), Runlevel::Edge);
    assert_eq!(Runlevel::Edge.level(), 8);
    assert_eq!(format!("{}", Runlevel::Edge), "edge");
}

#[test]
fn edge_target_active_in_edge_runlevel() {
    let targets = ServiceTarget::defaults();
    let edge = targets.iter().find(|t| t.name == "edge").unwrap();
    assert!(edge.is_active_in(Runlevel::Edge));
    assert!(!edge.is_active_in(Runlevel::Console));
    assert!(!edge.is_active_in(Runlevel::Graphical));
    assert!(edge.requires.contains(&"daimon".to_string()));
    assert!(edge.wants.contains(&"aegis".to_string()));
}

#[test]
fn edge_service_state_lifecycle() {
    let mut init = ArgonautInit::new(edge_config());
    assert!(init.set_service_state("agent-runtime", ServiceState::Starting));
    assert!(init.set_service_state("agent-runtime", ServiceState::Running));
    let stats = init.stats();
    assert_eq!(stats.boot_mode, BootMode::Edge);
    assert_eq!(stats.services_running, 1);
}

// --- Read-only rootfs / dm-verity (Phase 14A-3/4) ---

#[test]
fn readonly_rootfs_returns_five_commands() {
    let cmds = configure_readonly_rootfs();
    assert_eq!(cmds.len(), 5);
}

#[test]
fn readonly_rootfs_remounts_root_ro() {
    let cmds = configure_readonly_rootfs();
    assert_eq!(cmds[0], "mount -o remount,ro /");
}

#[test]
fn readonly_rootfs_tmpfs_noexec() {
    let cmds = configure_readonly_rootfs();
    // /tmp and /var/tmp should have noexec
    assert!(cmds[1].contains("noexec"));
    assert!(cmds[4].contains("noexec"));
    // /var/run and /var/log should NOT have noexec
    assert!(!cmds[2].contains("noexec"));
    assert!(!cmds[3].contains("noexec"));
}

#[test]
fn verify_rootfs_integrity_success() {
    let hash = "a".repeat(64);
    let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &hash);
    assert!(result.is_ok());
    let cmds = result.unwrap();
    assert_eq!(cmds.len(), 3);
    assert_eq!(cmds[0].binary, "veritysetup");
    assert_eq!(cmds[0].args[0], "verify");
    assert_eq!(cmds[1].binary, "veritysetup");
    assert_eq!(cmds[1].args[0], "open");
    assert_eq!(cmds[2].binary, "mount");
    assert!(cmds[2]
        .args
        .contains(&"/dev/mapper/verified-root".to_string()));
}

#[test]
fn verify_rootfs_integrity_empty_params() {
    let hash = "a".repeat(64);
    assert!(verify_rootfs_integrity("", "/dev/sda2", &hash).is_err());
    assert!(verify_rootfs_integrity("/dev/sda1", "", &hash).is_err());
    assert!(verify_rootfs_integrity("/dev/sda1", "/dev/sda2", "").is_err());
}

#[test]
fn verify_rootfs_integrity_bad_hash_length() {
    let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", "abcdef");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("64 hex characters"));

    let long_hash = "a".repeat(128);
    let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &long_hash);
    assert!(result.is_err());
}

#[test]
fn verify_rootfs_integrity_bad_hash_chars() {
    // 64 chars but non-hex
    let hash = "g".repeat(64);
    let result = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &hash);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("hex characters"));
}

#[test]
fn verify_rootfs_integrity_bad_device_path() {
    let hash = "a".repeat(64);
    // Path without /dev/ prefix
    let result = verify_rootfs_integrity("/tmp/sda1", "/dev/sda2", &hash);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("/dev/"));

    // Path with shell metacharacters
    let result = verify_rootfs_integrity("/dev/sda1; rm -rf /", "/dev/sda2", &hash);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid characters"));
}

#[test]
fn verify_rootfs_integrity_commands_contain_devices() {
    let hash = "b".repeat(64);
    let cmds = verify_rootfs_integrity("/dev/vda1", "/dev/vda2", &hash).unwrap();
    assert!(cmds[0].args.contains(&"/dev/vda1".to_string()));
    assert!(cmds[0].args.contains(&"/dev/vda2".to_string()));
    assert!(cmds[0].args.contains(&hash));
    assert!(cmds[1].args.contains(&"/dev/vda1".to_string()));
    assert!(cmds[1].args.contains(&"verified-root".to_string()));
}

#[test]
fn verify_rootfs_integrity_mount_is_readonly() {
    let hash = "c".repeat(64);
    let cmds = verify_rootfs_integrity("/dev/sda1", "/dev/sda2", &hash).unwrap();
    assert!(cmds[2].args.contains(&"ro".to_string()));
}

#[test]
fn safe_command_display() {
    let cmd = SafeCommand {
        binary: "mount".to_string(),
        args: vec!["-o".to_string(), "ro".to_string(), "/dev/sda1".to_string()],
    };
    assert_eq!(cmd.display(), "mount -o ro /dev/sda1");
}

// -----------------------------------------------------------------------
// Phase 14D: Edge Security tests
// -----------------------------------------------------------------------

#[test]
fn edge_boot_config_defaults() {
    let cfg = EdgeBootConfig::default();
    assert!(cfg.readonly_rootfs);
    assert!(cfg.luks_enabled);
    assert!(!cfg.tpm_attestation);
    assert_eq!(cfg.max_boot_time_ms, 3000);
}

#[test]
fn edge_boot_config_custom() {
    let cfg = EdgeBootConfig {
        readonly_rootfs: false,
        luks_enabled: false,
        tpm_attestation: true,
        max_boot_time_ms: 5000,
    };
    assert!(!cfg.readonly_rootfs);
    assert!(!cfg.luks_enabled);
    assert!(cfg.tpm_attestation);
    assert_eq!(cfg.max_boot_time_ms, 5000);
}

#[test]
fn edge_boot_config_serde_roundtrip() {
    let cfg = EdgeBootConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let deserialized: EdgeBootConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.luks_enabled, cfg.luks_enabled);
    assert_eq!(deserialized.readonly_rootfs, cfg.readonly_rootfs);
    assert_eq!(deserialized.tpm_attestation, cfg.tpm_attestation);
    assert_eq!(deserialized.max_boot_time_ms, cfg.max_boot_time_ms);
}

#[test]
fn default_services_edge_has_luks_env() {
    let svcs = ArgonautInit::default_services(BootMode::Edge);
    assert_eq!(svcs.len(), 1);
    assert_eq!(svcs[0].environment.get("AGNOS_EDGE_LUKS").unwrap(), "1");
}

// -----------------------------------------------------------------------
// Shruti optional service
// -----------------------------------------------------------------------

#[test]
fn shruti_service_definition() {
    let svc = ArgonautInit::shruti_service();
    assert_eq!(svc.name, "shruti");
    assert_eq!(svc.binary_path, PathBuf::from("/usr/local/bin/shruti"));
    assert!(svc.depends_on.contains(&"agent-runtime".into()));
    assert!(svc.depends_on.contains(&"aethersafha".into()));
    assert!(
        svc.required_for_modes.is_empty(),
        "shruti must not auto-start"
    );
    assert_eq!(svc.restart_policy, RestartPolicy::OnFailure);
    assert!(svc.health_check.is_some());
    assert!(svc.ready_check.is_none());
}

#[test]
fn shruti_not_in_default_services() {
    for mode in [
        BootMode::Desktop,
        BootMode::Server,
        BootMode::Minimal,
        BootMode::Edge,
    ] {
        let svcs = ArgonautInit::default_services(mode);
        assert!(
            !svcs.iter().any(|s| s.name == "shruti"),
            "shruti should not appear in default services for {:?}",
            mode,
        );
    }
}

#[test]
fn shruti_optional_service_lookup() {
    assert!(ArgonautInit::optional_service("shruti").is_some());
    assert!(ArgonautInit::optional_service("nonexistent").is_none());
}

#[test]
fn enable_optional_shruti_service() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Desktop,
        ..Default::default()
    };
    let mut init = ArgonautInit::new(config);
    assert!(!init.services.contains_key("shruti"));

    let added = init.enable_optional_service("shruti");
    assert!(added);
    assert!(init.services.contains_key("shruti"));
    assert_eq!(init.services["shruti"].state, ServiceState::Stopped);

    // Second call is a no-op
    let added_again = init.enable_optional_service("shruti");
    assert!(!added_again);
}

#[test]
fn shruti_user_config_service() {
    let config = ArgonautConfig {
        boot_mode: BootMode::Desktop,
        services: vec![ArgonautInit::shruti_service()],
        ..Default::default()
    };
    let init = ArgonautInit::new(config);
    assert!(init.services.contains_key("shruti"));
}
