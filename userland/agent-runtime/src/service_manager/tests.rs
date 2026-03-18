//! Tests for the service manager.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use chrono::{Datelike, Timelike};

    use super::super::health::{CronSchedule, ScheduledTask, TaskScheduler};
    use super::super::types::{
        default_max_restarts, default_readiness_timeout, default_restart_delay, FleetConfig,
        ReconciliationPlan, RestartPolicy, ServiceDefinition, ServiceResources, ServiceState,
        ServiceStatus, ServiceType,
    };
    use super::super::*;

    fn make_def(name: &str, after: &[&str]) -> ServiceDefinition {
        ServiceDefinition {
            name: name.to_string(),
            exec_start: format!("/usr/bin/{}", name),
            args: vec![],
            environment: vec![],
            after: after.iter().map(|s| s.to_string()).collect(),
            wants: vec![],
            restart: RestartPolicy::Always,
            max_restarts: 5,
            restart_delay_secs: 1,
            user: String::new(),
            group: String::new(),
            working_directory: String::new(),
            service_type: ServiceType::Simple,
            readiness_timeout_secs: 30,
            resources: ServiceResources::default(),
            enabled: true,
            description: String::new(),
        }
    }

    #[test]
    fn test_topological_sort_basic() {
        let mut services = HashMap::new();
        services.insert("c".into(), make_def("c", &["b"]));
        services.insert("b".into(), make_def("b", &["a"]));
        services.insert("a".into(), make_def("a", &[]));

        let order = topological_sort(&services).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_parallel_roots() {
        let mut services = HashMap::new();
        services.insert("audit".into(), make_def("audit", &[]));
        services.insert("network".into(), make_def("network", &[]));
        services.insert("runtime".into(), make_def("runtime", &["audit", "network"]));

        let order = topological_sort(&services).unwrap();
        // audit and network should come before runtime
        let runtime_pos = order.iter().position(|s| s == "runtime").unwrap();
        let audit_pos = order.iter().position(|s| s == "audit").unwrap();
        let network_pos = order.iter().position(|s| s == "network").unwrap();
        assert!(audit_pos < runtime_pos);
        assert!(network_pos < runtime_pos);
    }

    #[test]
    fn test_topological_sort_cycle_detection() {
        let mut services = HashMap::new();
        services.insert("a".into(), make_def("a", &["c"]));
        services.insert("b".into(), make_def("b", &["a"]));
        services.insert("c".into(), make_def("c", &["b"]));

        let result = topological_sort(&services);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_topological_sort_ignores_unknown_deps() {
        let mut services = HashMap::new();
        services.insert("a".into(), make_def("a", &["nonexistent"]));

        let order = topological_sort(&services).unwrap();
        assert_eq!(order, vec!["a"]);
    }

    #[test]
    fn test_dependency_levels_basic() {
        let mut services = HashMap::new();
        services.insert("a".into(), make_def("a", &[]));
        services.insert("b".into(), make_def("b", &["a"]));
        services.insert("c".into(), make_def("c", &["b"]));

        let order = topological_sort(&services).unwrap();
        let levels = lifecycle::dependency_levels(&services, &order);

        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["a"]);
        assert_eq!(levels[1], vec!["b"]);
        assert_eq!(levels[2], vec!["c"]);
    }

    #[test]
    fn test_dependency_levels_parallel() {
        let mut services = HashMap::new();
        services.insert("audit".into(), make_def("audit", &[]));
        services.insert("network".into(), make_def("network", &[]));
        services.insert("runtime".into(), make_def("runtime", &["audit", "network"]));

        let order = topological_sort(&services).unwrap();
        let levels = lifecycle::dependency_levels(&services, &order);

        assert_eq!(levels.len(), 2);
        // Level 0 has audit and network (parallel)
        assert_eq!(levels[0].len(), 2);
        assert!(levels[0].contains(&"audit".to_string()));
        assert!(levels[0].contains(&"network".to_string()));
        // Level 1 has runtime
        assert_eq!(levels[1], vec!["runtime"]);
    }

    #[test]
    fn test_dependency_levels_diamond() {
        let mut services = HashMap::new();
        services.insert("base".into(), make_def("base", &[]));
        services.insert("left".into(), make_def("left", &["base"]));
        services.insert("right".into(), make_def("right", &["base"]));
        services.insert("top".into(), make_def("top", &["left", "right"]));

        let order = topological_sort(&services).unwrap();
        let levels = lifecycle::dependency_levels(&services, &order);

        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["base"]);
        assert_eq!(levels[1].len(), 2); // left and right in parallel
        assert_eq!(levels[2], vec!["top"]);
    }

    #[test]
    fn test_service_status_uptime_display() {
        let status = ServiceStatus {
            name: "test".to_string(),
            state: ServiceState::Running,
            pid: Some(1234),
            restart_count: 0,
            uptime: Some(Duration::from_secs(3661)),
            exit_code: None,
            enabled: true,
            description: "test service".to_string(),
        };
        assert_eq!(status.uptime_display(), "1h 1m");
    }

    #[test]
    fn test_service_status_uptime_none() {
        let status = ServiceStatus {
            name: "test".to_string(),
            state: ServiceState::Stopped,
            pid: None,
            restart_count: 0,
            uptime: None,
            exit_code: None,
            enabled: true,
            description: String::new(),
        };
        assert_eq!(status.uptime_display(), "-");
    }

    #[test]
    fn test_restart_policy_default() {
        let policy = RestartPolicy::default();
        assert_eq!(policy, RestartPolicy::Always);
    }

    #[test]
    fn test_service_type_default() {
        let stype = ServiceType::default();
        assert_eq!(stype, ServiceType::Simple);
    }

    #[test]
    fn test_service_state_display() {
        assert_eq!(ServiceState::Running.to_string(), "running");
        assert_eq!(ServiceState::Failed.to_string(), "failed");
        assert_eq!(ServiceState::Starting.to_string(), "starting");
        assert_eq!(ServiceState::Stopped.to_string(), "stopped");
        assert_eq!(ServiceState::Stopping.to_string(), "stopping");
        assert_eq!(ServiceState::Exited.to_string(), "exited");
    }

    #[test]
    fn test_service_resources_default() {
        let res = ServiceResources::default();
        assert_eq!(res.memory_max, 0);
        assert_eq!(res.cpu_quota_percent, 0);
        assert_eq!(res.tasks_max, 0);
    }

    #[tokio::test]
    async fn test_service_manager_new() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        let list = mgr.list_services().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_service_manager_register() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        mgr.register(make_def("test-svc", &[])).await;

        let list = mgr.list_services().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test-svc");
        assert_eq!(list[0].state, ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_service_manager_get_status() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        mgr.register(make_def("foo", &[])).await;

        let status = mgr.get_status("foo").await;
        assert!(status.is_some());
        let status = status.unwrap();
        assert_eq!(status.name, "foo");
        assert!(status.enabled);

        let missing = mgr.get_status("bar").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_service_manager_stop_already_stopped() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        mgr.register(make_def("stopped-svc", &[])).await;

        // Stopping an already-stopped service should succeed
        let result = mgr.stop_service("stopped-svc").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_service_manager_stop_unknown() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        let result = mgr.stop_service("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_service_manager_start_unknown() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services");
        let result = mgr.start_service("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_service_manager_load_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ServiceManager::new(dir.path());
        let count = mgr.load_definitions().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_service_manager_load_toml() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = r#"
name = "test-service"
exec_start = "/bin/true"
description = "A test service"
"#;
        tokio::fs::write(dir.path().join("test-service.toml"), toml_content)
            .await
            .unwrap();

        let mgr = ServiceManager::new(dir.path());
        let count = mgr.load_definitions().await.unwrap();
        assert_eq!(count, 1);

        let status = mgr.get_status("test-service").await.unwrap();
        assert_eq!(status.description, "A test service");
    }

    #[tokio::test]
    async fn test_service_manager_start_real_process() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services-start");
        mgr.register(ServiceDefinition {
            name: "sleeper".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("sleeper", &[])
        })
        .await;

        let result = mgr.start_service("sleeper").await;
        assert!(result.is_ok());

        let status = mgr.get_status("sleeper").await.unwrap();
        assert_eq!(status.state, ServiceState::Running);
        assert!(status.pid.is_some());

        // Clean up
        mgr.stop_service("sleeper").await.unwrap();
        let status = mgr.get_status("sleeper").await.unwrap();
        assert_eq!(status.state, ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_service_manager_oneshot() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services-oneshot");
        mgr.register(ServiceDefinition {
            name: "init-dirs".to_string(),
            exec_start: "/bin/true".to_string(),
            service_type: ServiceType::Oneshot,
            restart: RestartPolicy::No,
            ..make_def("init-dirs", &[])
        })
        .await;

        let result = mgr.start_service("init-dirs").await;
        assert!(result.is_ok());

        let status = mgr.get_status("init-dirs").await.unwrap();
        assert_eq!(status.state, ServiceState::Exited);
    }

    #[tokio::test]
    async fn test_service_manager_oneshot_failure() {
        let mgr = ServiceManager::new("/tmp/agnos-test-services-oneshot-fail");
        mgr.register(ServiceDefinition {
            name: "bad-init".to_string(),
            exec_start: "/bin/false".to_string(),
            service_type: ServiceType::Oneshot,
            restart: RestartPolicy::No,
            ..make_def("bad-init", &[])
        })
        .await;

        let result = mgr.start_service("bad-init").await;
        assert!(result.is_err());

        let status = mgr.get_status("bad-init").await.unwrap();
        assert_eq!(status.state, ServiceState::Failed);
    }

    #[tokio::test]
    async fn test_service_manager_boot_order() {
        let mgr = ServiceManager::new("/tmp/agnos-test-boot");
        mgr.register(ServiceDefinition {
            name: "base".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("base", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "dep".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["base".to_string()],
            ..make_def("dep", &[])
        })
        .await;

        let result = mgr.boot().await;
        assert!(result.is_ok());

        let base_status = mgr.get_status("base").await.unwrap();
        let dep_status = mgr.get_status("dep").await.unwrap();
        assert_eq!(base_status.state, ServiceState::Running);
        assert_eq!(dep_status.state, ServiceState::Running);

        // Shutdown in reverse order
        mgr.shutdown_all().await.unwrap();

        let base_status = mgr.get_status("base").await.unwrap();
        let dep_status = mgr.get_status("dep").await.unwrap();
        assert_eq!(base_status.state, ServiceState::Stopped);
        assert_eq!(dep_status.state, ServiceState::Stopped);
    }

    #[tokio::test]
    async fn test_service_manager_restart() {
        let mgr = ServiceManager::new("/tmp/agnos-test-restart");
        mgr.register(ServiceDefinition {
            name: "restartable".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("restartable", &[])
        })
        .await;

        mgr.start_service("restartable").await.unwrap();
        let pid1 = mgr.get_status("restartable").await.unwrap().pid;

        mgr.restart_service("restartable").await.unwrap();
        let pid2 = mgr.get_status("restartable").await.unwrap().pid;

        // Should have a new PID
        assert_ne!(pid1, pid2);

        mgr.stop_service("restartable").await.unwrap();
    }

    #[tokio::test]
    async fn test_service_manager_dependency_auto_start() {
        let mgr = ServiceManager::new("/tmp/agnos-test-dep-auto");
        mgr.register(ServiceDefinition {
            name: "dep-base".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("dep-base", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "dep-child".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["dep-base".to_string()],
            ..make_def("dep-child", &[])
        })
        .await;

        // Starting dep-child should auto-start dep-base
        mgr.start_service("dep-child").await.unwrap();

        let base = mgr.get_status("dep-base").await.unwrap();
        let child = mgr.get_status("dep-child").await.unwrap();
        assert_eq!(base.state, ServiceState::Running);
        assert_eq!(child.state, ServiceState::Running);

        mgr.shutdown_all().await.unwrap();
    }

    #[test]
    fn test_service_definition_toml_roundtrip() {
        let def = make_def("test", &["dep1", "dep2"]);
        let toml_str = toml::to_string_pretty(&def).unwrap();
        let parsed: ServiceDefinition = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.after, vec!["dep1", "dep2"]);
        assert_eq!(parsed.restart, RestartPolicy::Always);
    }

    #[test]
    fn test_service_definition_minimal_toml() {
        let toml_str = r#"
name = "minimal"
exec_start = "/bin/true"
"#;
        let def: ServiceDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.name, "minimal");
        assert!(def.after.is_empty());
        assert_eq!(def.restart, RestartPolicy::Always);
        assert_eq!(def.service_type, ServiceType::Simple);
        assert!(def.enabled);
    }

    // -----------------------------------------------------------------------
    // FleetConfig and ReconciliationPlan tests
    // -----------------------------------------------------------------------

    fn fleet_service(name: &str, enabled: bool) -> ServiceDefinition {
        ServiceDefinition {
            enabled,
            ..make_def(name, &[])
        }
    }

    #[test]
    fn test_reconcile_empty_desired_empty_running() {
        let fleet = FleetConfig { services: vec![] };
        let plan = fleet.reconcile(&[]);
        assert!(plan.to_start.is_empty());
        assert!(plan.to_stop.is_empty());
        assert!(plan.unchanged.is_empty());
        assert!(!plan.has_changes());
    }

    #[test]
    fn test_reconcile_start_new() {
        let fleet = FleetConfig {
            services: vec![fleet_service("svc-a", true), fleet_service("svc-b", true)],
        };
        let plan = fleet.reconcile(&[]);
        assert_eq!(plan.to_start.len(), 2);
        assert!(plan.to_stop.is_empty());
        assert!(plan.unchanged.is_empty());
        assert!(plan.has_changes());
    }

    #[test]
    fn test_reconcile_stop_extra() {
        let fleet = FleetConfig { services: vec![] };
        let plan = fleet.reconcile(&["old-svc".to_string()]);
        assert!(plan.to_start.is_empty());
        assert_eq!(plan.to_stop, vec!["old-svc".to_string()]);
        assert!(plan.unchanged.is_empty());
        assert!(plan.has_changes());
    }

    #[test]
    fn test_reconcile_mixed() {
        let fleet = FleetConfig {
            services: vec![fleet_service("keep", true), fleet_service("new-svc", true)],
        };
        let running = vec!["keep".to_string(), "remove-me".to_string()];
        let plan = fleet.reconcile(&running);
        assert!(plan.to_start.contains(&"new-svc".to_string()));
        assert!(plan.to_stop.contains(&"remove-me".to_string()));
        assert!(plan.unchanged.contains(&"keep".to_string()));
    }

    #[test]
    fn test_reconcile_no_changes() {
        let fleet = FleetConfig {
            services: vec![fleet_service("svc-a", true)],
        };
        let plan = fleet.reconcile(&["svc-a".to_string()]);
        assert!(plan.to_start.is_empty());
        assert!(plan.to_stop.is_empty());
        assert_eq!(plan.unchanged, vec!["svc-a".to_string()]);
        assert!(!plan.has_changes());
    }

    #[test]
    fn test_reconcile_disabled_services_excluded() {
        let fleet = FleetConfig {
            services: vec![
                fleet_service("enabled", true),
                fleet_service("disabled", false),
            ],
        };
        let plan = fleet.reconcile(&[]);
        assert_eq!(plan.to_start.len(), 1);
        assert!(plan.to_start.contains(&"enabled".to_string()));
    }

    #[test]
    fn test_has_changes_true() {
        let plan = ReconciliationPlan {
            to_start: vec!["a".to_string()],
            to_stop: vec![],
            unchanged: vec![],
        };
        assert!(plan.has_changes());
    }

    #[test]
    fn test_has_changes_false() {
        let plan = ReconciliationPlan {
            to_start: vec![],
            to_stop: vec![],
            unchanged: vec!["a".to_string()],
        };
        assert!(!plan.has_changes());
    }

    #[test]
    fn test_summary_formats_start() {
        let plan = ReconciliationPlan {
            to_start: vec!["svc-a".to_string()],
            to_stop: vec![],
            unchanged: vec![],
        };
        let s = plan.summary();
        assert!(s.contains("start: svc-a"));
    }

    #[test]
    fn test_summary_formats_stop() {
        let plan = ReconciliationPlan {
            to_start: vec![],
            to_stop: vec!["old".to_string()],
            unchanged: vec![],
        };
        let s = plan.summary();
        assert!(s.contains("stop: old"));
    }

    #[test]
    fn test_summary_formats_mixed() {
        let plan = ReconciliationPlan {
            to_start: vec!["new".to_string()],
            to_stop: vec!["old".to_string()],
            unchanged: vec!["keep".to_string()],
        };
        let s = plan.summary();
        assert!(s.contains("start: new"));
        assert!(s.contains("stop: old"));
        assert!(s.contains("unchanged: keep"));
        assert!(s.contains(" | "));
    }

    #[test]
    fn test_summary_no_changes() {
        let plan = ReconciliationPlan {
            to_start: vec![],
            to_stop: vec![],
            unchanged: vec![],
        };
        assert_eq!(plan.summary(), "No changes needed");
    }

    #[test]
    fn test_fleet_config_serialization() {
        let fleet = FleetConfig {
            services: vec![fleet_service("svc-a", true)],
        };
        let toml_str = toml::to_string_pretty(&fleet).unwrap();
        let parsed: FleetConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.services.len(), 1);
        assert_eq!(parsed.services[0].name, "svc-a");
    }

    #[test]
    fn test_reconciliation_plan_serialization() {
        let plan = ReconciliationPlan {
            to_start: vec!["a".to_string()],
            to_stop: vec!["b".to_string()],
            unchanged: vec!["c".to_string()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let deser: ReconciliationPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.to_start, vec!["a"]);
        assert_eq!(deser.to_stop, vec!["b"]);
        assert_eq!(deser.unchanged, vec!["c"]);
    }

    #[tokio::test]
    async fn test_fleet_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let fleet_path = dir.path().join("fleet.toml");
        let content = r#"
[[services]]
name = "gateway"
exec_start = "/usr/bin/gateway"
enabled = true

[[services]]
name = "monitor"
exec_start = "/usr/bin/monitor"
enabled = false
"#;
        tokio::fs::write(&fleet_path, content).await.unwrap();

        let fleet = FleetConfig::from_file(&fleet_path).await.unwrap();
        assert_eq!(fleet.services.len(), 2);
        assert_eq!(fleet.services[0].name, "gateway");
        assert!(fleet.services[0].enabled);
        assert_eq!(fleet.services[1].name, "monitor");
        assert!(!fleet.services[1].enabled);

        // Reconcile: only gateway is enabled
        let plan = fleet.reconcile(&[]);
        assert_eq!(plan.to_start.len(), 1);
        assert!(plan.to_start.contains(&"gateway".to_string()));
    }

    // ==================================================================
    // Cron Schedule & Task Scheduler tests
    // ==================================================================

    #[test]
    fn test_cron_parse_every_5_min() {
        let schedule = CronSchedule::new("*/5 * * * *").unwrap();
        assert_eq!(schedule.expression, "*/5 * * * *");
    }

    #[test]
    fn test_cron_parse_sunday_2am() {
        let schedule = CronSchedule::new("0 2 * * 0").unwrap();
        assert_eq!(schedule.expression, "0 2 * * 0");
    }

    #[test]
    fn test_cron_parse_every_6_hours() {
        let schedule = CronSchedule::new("0 */6 * * *").unwrap();
        assert_eq!(schedule.expression, "0 */6 * * *");
    }

    #[test]
    fn test_cron_parse_invalid_field_count() {
        assert!(CronSchedule::new("* * *").is_err());
        assert!(CronSchedule::new("* * * * * *").is_err());
        assert!(CronSchedule::new("").is_err());
    }

    #[test]
    fn test_cron_parse_invalid_field_value() {
        assert!(CronSchedule::new("abc * * * *").is_err());
        assert!(CronSchedule::new("*/0 * * * *").is_err());
    }

    #[test]
    fn test_cron_matches_every_minute() {
        let schedule = CronSchedule::new("* * * * *").unwrap();
        let dt = chrono::Utc::now();
        assert!(schedule.matches(&dt));
    }

    #[test]
    fn test_cron_matches_specific_minute() {
        let schedule = CronSchedule::new("30 * * * *").unwrap();
        // 2026-03-06 12:30:00 UTC
        let dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 6)
            .unwrap()
            .and_hms_opt(12, 30, 0)
            .unwrap()
            .and_utc();
        assert!(schedule.matches(&dt));

        let dt_wrong = chrono::NaiveDate::from_ymd_opt(2026, 3, 6)
            .unwrap()
            .and_hms_opt(12, 15, 0)
            .unwrap()
            .and_utc();
        assert!(!schedule.matches(&dt_wrong));
    }

    #[test]
    fn test_cron_matches_step() {
        let schedule = CronSchedule::new("*/15 * * * *").unwrap();
        let dt0 = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let dt15 = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 15, 0)
            .unwrap()
            .and_utc();
        let dt7 = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 7, 0)
            .unwrap()
            .and_utc();

        assert!(schedule.matches(&dt0));
        assert!(schedule.matches(&dt15));
        assert!(!schedule.matches(&dt7));
    }

    #[test]
    fn test_cron_matches_specific_hour_and_minute() {
        let schedule = CronSchedule::new("0 2 * * *").unwrap();
        let dt = chrono::NaiveDate::from_ymd_opt(2026, 6, 15)
            .unwrap()
            .and_hms_opt(2, 0, 0)
            .unwrap()
            .and_utc();
        assert!(schedule.matches(&dt));

        let wrong_hour = chrono::NaiveDate::from_ymd_opt(2026, 6, 15)
            .unwrap()
            .and_hms_opt(3, 0, 0)
            .unwrap()
            .and_utc();
        assert!(!schedule.matches(&wrong_hour));
    }

    #[test]
    fn test_cron_matches_day_of_week() {
        // 0 = Sunday
        let schedule = CronSchedule::new("0 0 * * 0").unwrap();
        // 2026-03-01 is a Sunday
        let sunday = chrono::NaiveDate::from_ymd_opt(2026, 3, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert!(sunday.weekday().num_days_from_sunday() == 0);
        assert!(schedule.matches(&sunday));

        // Monday
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 3, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert!(!schedule.matches(&monday));
    }

    #[test]
    fn test_cron_next_run_after() {
        let schedule = CronSchedule::new("0 * * * *").unwrap(); // every hour at :00
        let now = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(12, 30, 0)
            .unwrap()
            .and_utc();

        let next = schedule.next_run_after(now).unwrap();
        assert_eq!(next.hour(), 13);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_cron_next_run_every_5_min() {
        let schedule = CronSchedule::new("*/5 * * * *").unwrap();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(10, 3, 0)
            .unwrap()
            .and_utc();

        let next = schedule.next_run_after(now).unwrap();
        assert_eq!(next.minute(), 5);
        assert_eq!(next.hour(), 10);
    }

    #[test]
    fn test_cron_with_description() {
        let schedule = CronSchedule::new("*/5 * * * *")
            .unwrap()
            .with_description("Every 5 minutes");
        assert_eq!(schedule.description, "Every 5 minutes");
    }

    #[test]
    fn test_scheduled_task_new() {
        let schedule = CronSchedule::new("* * * * *").unwrap();
        let task = ScheduledTask::new("test-task", "my-service", schedule);
        assert_eq!(task.name, "test-task");
        assert_eq!(task.service_name, "my-service");
        assert!(task.enabled);
        assert!(task.last_run.is_none());
        assert!(task.next_run.is_some());
    }

    #[test]
    fn test_task_scheduler_new() {
        let scheduler = TaskScheduler::new();
        assert!(scheduler.list_tasks().is_empty());
    }

    #[test]
    fn test_task_scheduler_add_task() {
        let mut scheduler = TaskScheduler::new();
        let schedule = CronSchedule::new("*/5 * * * *").unwrap();
        let task = ScheduledTask::new("scanner", "port-scanner-svc", schedule);

        scheduler.add_task(task).unwrap();
        assert_eq!(scheduler.list_tasks().len(), 1);
    }

    #[test]
    fn test_task_scheduler_add_task_empty_name() {
        let mut scheduler = TaskScheduler::new();
        let schedule = CronSchedule::new("* * * * *").unwrap();
        let task = ScheduledTask::new("", "svc", schedule);
        assert!(scheduler.add_task(task).is_err());
    }

    #[test]
    fn test_task_scheduler_remove_task() {
        let mut scheduler = TaskScheduler::new();
        let schedule = CronSchedule::new("* * * * *").unwrap();
        let task = ScheduledTask::new("removable", "svc", schedule);
        let id = task.id;

        scheduler.add_task(task).unwrap();
        let removed = scheduler.remove_task(&id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "removable");
        assert!(scheduler.list_tasks().is_empty());
    }

    #[test]
    fn test_task_scheduler_remove_nonexistent() {
        let mut scheduler = TaskScheduler::new();
        let id = uuid::Uuid::new_v4();
        assert!(scheduler.remove_task(&id).is_none());
    }

    #[test]
    fn test_task_scheduler_due_tasks() {
        let mut scheduler = TaskScheduler::new();

        // Task whose next_run is in the past
        let schedule = CronSchedule::new("* * * * *").unwrap();
        let mut task = ScheduledTask::new("due-task", "svc", schedule);
        task.next_run = Some(chrono::Utc::now() - chrono::Duration::minutes(1));
        scheduler.add_task(task).unwrap();

        // Task whose next_run is in the future
        let schedule2 = CronSchedule::new("* * * * *").unwrap();
        let mut task2 = ScheduledTask::new("future-task", "svc2", schedule2);
        task2.next_run = Some(chrono::Utc::now() + chrono::Duration::hours(1));
        scheduler.add_task(task2).unwrap();

        let due = scheduler.due_tasks(&chrono::Utc::now());
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "due-task");
    }

    #[test]
    fn test_task_scheduler_due_tasks_disabled() {
        let mut scheduler = TaskScheduler::new();
        let schedule = CronSchedule::new("* * * * *").unwrap();
        let mut task = ScheduledTask::new("disabled-task", "svc", schedule);
        task.next_run = Some(chrono::Utc::now() - chrono::Duration::minutes(5));
        task.enabled = false;
        scheduler.add_task(task).unwrap();

        let due = scheduler.due_tasks(&chrono::Utc::now());
        assert!(due.is_empty());
    }

    #[test]
    fn test_task_scheduler_mark_completed() {
        let mut scheduler = TaskScheduler::new();
        let schedule = CronSchedule::new("0 * * * *").unwrap(); // every hour at :00
        let task = ScheduledTask::new("hourly", "svc", schedule);
        let id = task.id;
        scheduler.add_task(task).unwrap();

        let completed_at = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        scheduler.mark_completed(&id, completed_at);

        let tasks = scheduler.list_tasks();
        let t = tasks.iter().find(|t| t.id == id).unwrap();
        assert_eq!(t.last_run, Some(completed_at));
        // next_run should be 13:00
        assert!(t.next_run.is_some());
        assert_eq!(t.next_run.unwrap().hour(), 13);
    }

    #[test]
    fn test_task_scheduler_default() {
        let scheduler = TaskScheduler::default();
        assert!(scheduler.list_tasks().is_empty());
    }

    #[test]
    fn test_cron_schedule_serialization() {
        let schedule = CronSchedule::new("*/10 * * * *")
            .unwrap()
            .with_description("Every 10 min");
        let json = serde_json::to_string(&schedule).unwrap();
        assert!(json.contains("*/10 * * * *"));
        assert!(json.contains("Every 10 min"));
    }

    #[test]
    fn test_scheduled_task_serialization() {
        let schedule = CronSchedule::new("0 0 * * *").unwrap();
        let task = ScheduledTask::new("midnight", "cleanup-svc", schedule);
        let json = serde_json::to_string(&task).unwrap();
        let deser: ScheduledTask = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "midnight");
        assert_eq!(deser.service_name, "cleanup-svc");
    }

    // --- Default helper functions ---

    #[test]
    fn test_default_max_restarts_value() {
        assert_eq!(default_max_restarts(), 5);
    }

    #[test]
    fn test_default_restart_delay_value() {
        assert_eq!(default_restart_delay(), 1);
    }

    #[test]
    fn test_default_readiness_timeout_value() {
        assert_eq!(default_readiness_timeout(), 30);
    }

    // --- Enum serialization roundtrips ---

    #[test]
    fn test_restart_policy_serialization_roundtrip() {
        let variants = [
            RestartPolicy::No,
            RestartPolicy::Always,
            RestartPolicy::OnFailure,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let deser: RestartPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deser);
        }
    }

    #[test]
    fn test_service_type_serialization_roundtrip() {
        let variants = [
            ServiceType::Simple,
            ServiceType::Notify,
            ServiceType::Oneshot,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let deser: ServiceType = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deser);
        }
    }

    #[test]
    fn test_service_state_serialization_roundtrip() {
        let variants = [
            ServiceState::Stopped,
            ServiceState::Starting,
            ServiceState::Running,
            ServiceState::Stopping,
            ServiceState::Failed,
            ServiceState::Exited,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let deser: ServiceState = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deser);
        }
    }

    // --- ServiceDefinition TOML parsing ---

    #[test]
    fn test_service_definition_from_toml_full() {
        let toml_str = r#"
            name = "test-svc"
            exec_start = "/usr/bin/test"
            args = ["--flag"]
            after = ["network.target"]
            restart = "onfailure"
            max_restarts = 3
            description = "A test service"
        "#;
        let def: ServiceDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.name, "test-svc");
        assert_eq!(def.exec_start, "/usr/bin/test");
        assert_eq!(def.args, vec!["--flag"]);
        assert_eq!(def.after, vec!["network.target"]);
        assert_eq!(def.restart, RestartPolicy::OnFailure);
        assert_eq!(def.max_restarts, 3);
        assert!(def.enabled); // default = true
    }

    // --- dependency_levels ---

    #[test]
    fn test_dependency_levels_independent() {
        let mut services = HashMap::new();
        services.insert(
            "a".to_string(),
            ServiceDefinition {
                name: "a".to_string(),
                exec_start: "/bin/a".to_string(),
                ..service_def_defaults()
            },
        );
        services.insert(
            "b".to_string(),
            ServiceDefinition {
                name: "b".to_string(),
                exec_start: "/bin/b".to_string(),
                ..service_def_defaults()
            },
        );
        let order = topological_sort(&services).unwrap();
        let levels = lifecycle::dependency_levels(&services, &order);
        // Both should be level 0 (no deps)
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 2);
    }

    #[test]
    fn test_dependency_levels_chain() {
        let mut services = HashMap::new();
        services.insert(
            "base".to_string(),
            ServiceDefinition {
                name: "base".to_string(),
                exec_start: "/bin/base".to_string(),
                ..service_def_defaults()
            },
        );
        services.insert(
            "mid".to_string(),
            ServiceDefinition {
                name: "mid".to_string(),
                exec_start: "/bin/mid".to_string(),
                after: vec!["base".to_string()],
                ..service_def_defaults()
            },
        );
        services.insert(
            "top".to_string(),
            ServiceDefinition {
                name: "top".to_string(),
                exec_start: "/bin/top".to_string(),
                after: vec!["mid".to_string()],
                ..service_def_defaults()
            },
        );
        let order = topological_sort(&services).unwrap();
        let levels = lifecycle::dependency_levels(&services, &order);
        assert_eq!(levels.len(), 3);
    }

    // --- FleetConfig reconcile ---

    #[test]
    fn test_fleet_reconcile_start_new() {
        let config = FleetConfig {
            services: vec![
                ServiceDefinition {
                    name: "svc-a".to_string(),
                    exec_start: "/bin/a".to_string(),
                    ..service_def_defaults()
                },
                ServiceDefinition {
                    name: "svc-b".to_string(),
                    exec_start: "/bin/b".to_string(),
                    ..service_def_defaults()
                },
            ],
        };
        let plan = config.reconcile(&[]);
        assert_eq!(plan.to_start.len(), 2);
        assert!(plan.to_stop.is_empty());
        assert!(plan.unchanged.is_empty());
        assert!(plan.has_changes());
    }

    #[test]
    fn test_fleet_reconcile_stop_removed() {
        let config = FleetConfig { services: vec![] };
        let plan = config.reconcile(&["old-svc".to_string()]);
        assert!(plan.to_start.is_empty());
        assert_eq!(plan.to_stop, vec!["old-svc"]);
        assert!(plan.has_changes());
    }

    #[test]
    fn test_fleet_reconcile_no_changes() {
        let config = FleetConfig {
            services: vec![ServiceDefinition {
                name: "running".to_string(),
                exec_start: "/bin/running".to_string(),
                ..service_def_defaults()
            }],
        };
        let plan = config.reconcile(&["running".to_string()]);
        assert!(!plan.has_changes());
        assert_eq!(plan.unchanged, vec!["running"]);
    }

    #[test]
    fn test_fleet_reconcile_disabled_service_not_started() {
        let config = FleetConfig {
            services: vec![ServiceDefinition {
                name: "disabled-svc".to_string(),
                exec_start: "/bin/x".to_string(),
                enabled: false,
                ..service_def_defaults()
            }],
        };
        let plan = config.reconcile(&[]);
        assert!(plan.to_start.is_empty());
    }

    // --- ReconciliationPlan ---

    #[test]
    fn test_reconciliation_plan_summary_all_sections() {
        let plan = ReconciliationPlan {
            to_start: vec!["new-svc".to_string()],
            to_stop: vec!["old-svc".to_string()],
            unchanged: vec!["stable-svc".to_string()],
        };
        let summary = plan.summary();
        assert!(summary.contains("start: new-svc"));
        assert!(summary.contains("stop: old-svc"));
        assert!(summary.contains("unchanged: stable-svc"));
    }

    #[test]
    fn test_reconciliation_plan_summary_no_changes() {
        let plan = ReconciliationPlan {
            to_start: vec![],
            to_stop: vec![],
            unchanged: vec![],
        };
        assert_eq!(plan.summary(), "No changes needed");
        assert!(!plan.has_changes());
    }

    #[test]
    fn test_reconciliation_plan_serialization_roundtrip() {
        let plan = ReconciliationPlan {
            to_start: vec!["a".to_string()],
            to_stop: vec!["b".to_string()],
            unchanged: vec!["c".to_string()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let deser: ReconciliationPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.to_start, vec!["a"]);
        assert_eq!(deser.to_stop, vec!["b"]);
        assert_eq!(deser.unchanged, vec!["c"]);
    }

    // ==================================================================
    // H20: Shutdown ordering tests
    // ==================================================================

    #[tokio::test]
    async fn test_shutdown_uses_reverse_start_order() {
        let mgr = ServiceManager::new("/tmp/agnos-test-h20-shutdown");

        // Register: a -> b -> c (c depends on b, b depends on a)
        mgr.register(ServiceDefinition {
            name: "a".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("a", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "b".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["a".to_string()],
            ..make_def("b", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "c".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["b".to_string()],
            ..make_def("c", &[])
        })
        .await;

        // Boot records start order
        mgr.boot().await.unwrap();

        let start_order = mgr.get_start_order().await;
        assert_eq!(start_order, vec!["a", "b", "c"]);

        // Shutdown in reverse order
        mgr.shutdown_all().await.unwrap();

        for name in &["a", "b", "c"] {
            let status = mgr.get_status(name).await.unwrap();
            assert_eq!(status.state, ServiceState::Stopped);
        }
    }

    #[tokio::test]
    async fn test_start_order_tracking() {
        let mgr = ServiceManager::new("/tmp/agnos-test-h20-start-order");

        mgr.register(ServiceDefinition {
            name: "first".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            ..make_def("first", &[])
        })
        .await;

        mgr.register(ServiceDefinition {
            name: "second".to_string(),
            exec_start: "/bin/sleep".to_string(),
            args: vec!["60".to_string()],
            service_type: ServiceType::Simple,
            restart: RestartPolicy::No,
            after: vec!["first".to_string()],
            ..make_def("second", &[])
        })
        .await;

        // Start individually (not via boot)
        mgr.start_service("second").await.unwrap();

        let order = mgr.get_start_order().await;
        // "first" should have been auto-started before "second"
        assert_eq!(order.len(), 2);
        let first_idx = order.iter().position(|x| x == "first").unwrap();
        let second_idx = order.iter().position(|x| x == "second").unwrap();
        assert!(first_idx < second_idx);

        mgr.shutdown_all().await.unwrap();
    }

    // helper for tests
    fn service_def_defaults() -> ServiceDefinition {
        ServiceDefinition {
            name: String::new(),
            exec_start: String::new(),
            args: vec![],
            environment: vec![],
            after: vec![],
            wants: vec![],
            restart: RestartPolicy::default(),
            max_restarts: default_max_restarts(),
            restart_delay_secs: default_restart_delay(),
            user: String::new(),
            group: String::new(),
            working_directory: String::new(),
            service_type: ServiceType::default(),
            readiness_timeout_secs: default_readiness_timeout(),
            resources: ServiceResources::default(),
            enabled: true,
            description: String::new(),
        }
    }
}
