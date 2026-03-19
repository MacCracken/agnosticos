//! Tests for the ark package manager.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;
    use tempfile::TempDir;

    // -- parse_args tests --

    #[test]
    fn test_parse_install_single() {
        let cmd = parse_args(&["install", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Install {
                packages: vec!["nginx".to_string()],
                force: false,
            }
        );
    }

    #[test]
    fn test_parse_install_multiple() {
        let cmd = parse_args(&["install", "nginx", "curl"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Install {
                packages: vec!["nginx".to_string(), "curl".to_string()],
                force: false,
            }
        );
    }

    #[test]
    fn test_parse_install_force() {
        let cmd = parse_args(&["install", "--force", "nginx", "curl"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Install {
                packages: vec!["nginx".to_string(), "curl".to_string()],
                force: true,
            }
        );
    }

    #[test]
    fn test_parse_remove_basic() {
        let cmd = parse_args(&["remove", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Remove {
                packages: vec!["nginx".to_string()],
                purge: false,
            }
        );
    }

    #[test]
    fn test_parse_remove_purge() {
        let cmd = parse_args(&["remove", "--purge", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Remove {
                packages: vec!["nginx".to_string()],
                purge: true,
            }
        );
    }

    #[test]
    fn test_parse_search_query() {
        let cmd = parse_args(&["search", "web", "server"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Search {
                query: "web server".to_string(),
                source: None,
            }
        );
    }

    #[test]
    fn test_parse_search_with_source() {
        let cmd = parse_args(&["search", "--source", "system", "web", "server"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Search {
                query: "web server".to_string(),
                source: Some(PackageSource::System),
            }
        );
    }

    #[test]
    fn test_parse_list_all() {
        let cmd = parse_args(&["list"]).unwrap();
        assert_eq!(cmd, ArkCommand::List { source: None });
    }

    #[test]
    fn test_parse_list_marketplace() {
        let cmd = parse_args(&["list", "--marketplace"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::List {
                source: Some(PackageSource::Marketplace),
            }
        );
    }

    #[test]
    fn test_parse_info() {
        let cmd = parse_args(&["info", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Info {
                package: "nginx".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_update() {
        let cmd = parse_args(&["update"]).unwrap();
        assert_eq!(cmd, ArkCommand::Update);
    }

    #[test]
    fn test_parse_upgrade_all() {
        let cmd = parse_args(&["upgrade"]).unwrap();
        assert_eq!(cmd, ArkCommand::Upgrade { packages: None });
    }

    #[test]
    fn test_parse_upgrade_specific() {
        let cmd = parse_args(&["upgrade", "nginx"]).unwrap();
        assert_eq!(
            cmd,
            ArkCommand::Upgrade {
                packages: Some(vec!["nginx".to_string()]),
            }
        );
    }

    #[test]
    fn test_parse_status() {
        let cmd = parse_args(&["status"]).unwrap();
        assert_eq!(cmd, ArkCommand::Status);
    }

    #[test]
    fn test_parse_empty_args() {
        assert!(parse_args(&[]).is_err());
    }

    #[test]
    fn test_parse_unknown_command() {
        assert!(parse_args(&["frobnicate"]).is_err());
    }

    // -- Config and construction tests --

    #[test]
    fn test_ark_config_defaults() {
        use crate::nous::ResolutionStrategy;
        let config = ArkConfig::default();
        assert_eq!(config.default_strategy, ResolutionStrategy::SystemFirst);
        assert!(config.confirm_system_installs);
        assert!(config.confirm_removals);
        assert!(!config.auto_update_check);
        assert!(config.color_output);
        assert_eq!(
            config.marketplace_dir,
            std::path::PathBuf::from("/var/lib/agnos/marketplace")
        );
        assert_eq!(
            config.cache_dir,
            std::path::PathBuf::from("/var/cache/agnos/ark")
        );
    }

    #[test]
    fn test_ark_package_manager_new() {
        use crate::nous::ResolutionStrategy;
        let tmp = TempDir::new().unwrap();
        let config = ArkConfig {
            marketplace_dir: tmp.path().to_path_buf(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();
        assert_eq!(mgr.config.default_strategy, ResolutionStrategy::SystemFirst);
    }

    // -- Plan and execution tests --

    #[test]
    fn test_plan_install_marketplace() {
        let tmp = TempDir::new().unwrap();
        let marketplace_dir = tmp.path().to_path_buf();

        // Write an index.json that the LocalRegistry (used by nous) will load.
        // This simulates a marketplace package being installed.
        let index_json = serde_json::json!({
            "test-agent": {
                "manifest": {
                    "name": "test-agent",
                    "description": "A test marketplace agent",
                    "version": "1.0.0",
                    "publisher": {
                        "name": "Test",
                        "key_id": "abc12345",
                        "homepage": ""
                    },
                    "category": "Utility",
                    "runtime": "native",
                    "screenshots": [],
                    "changelog": "",
                    "min_agnos_version": "",
                    "dependencies": {},
                    "tags": []
                },
                "installed_at": "2026-03-06T00:00:00Z",
                "install_dir": "/tmp/test-agent",
                "package_hash": "fakehash",
                "auto_update": false,
                "installed_size": 1024
            }
        });
        std::fs::write(
            marketplace_dir.join("index.json"),
            serde_json::to_string_pretty(&index_json).unwrap(),
        )
        .unwrap();

        let config = ArkConfig {
            marketplace_dir: marketplace_dir.clone(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();

        let plan = mgr.plan_install(&["test-agent".to_string()]);
        assert!(plan.is_ok(), "plan_install failed: {:?}", plan.err());
        let plan = plan.unwrap();
        assert!(!plan.steps.is_empty());
        // Should resolve as marketplace
        assert_eq!(
            plan.steps[0],
            InstallStep::MarketplaceInstall {
                package: "test-agent".to_string(),
                version: Some("1.0.0".to_string()),
            }
        );
        assert!(!plan.requires_root);
    }

    #[test]
    fn test_plan_remove_generates_steps() {
        let tmp = TempDir::new().unwrap();
        let marketplace_dir = tmp.path().to_path_buf();

        // Populate marketplace index so nous can find the package for removal
        let index_json = serde_json::json!({
            "my-agent": {
                "manifest": {
                    "name": "my-agent",
                    "description": "Agent to remove",
                    "version": "1.0.0",
                    "publisher": {
                        "name": "Test",
                        "key_id": "abc12345",
                        "homepage": ""
                    },
                    "category": "Utility",
                    "runtime": "native",
                    "screenshots": [],
                    "changelog": "",
                    "min_agnos_version": "",
                    "dependencies": {},
                    "tags": []
                },
                "installed_at": "2026-03-06T00:00:00Z",
                "install_dir": "/tmp/my-agent",
                "package_hash": "fakehash",
                "auto_update": false,
                "installed_size": 512
            }
        });
        std::fs::write(
            marketplace_dir.join("index.json"),
            serde_json::to_string_pretty(&index_json).unwrap(),
        )
        .unwrap();

        let config = ArkConfig {
            marketplace_dir: marketplace_dir.clone(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();
        let plan = mgr.plan_remove(&["my-agent".to_string()], false);
        assert!(plan.is_ok(), "plan_remove failed: {:?}", plan.err());
        let plan = plan.unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            plan.steps[0],
            InstallStep::MarketplaceRemove {
                package: "my-agent".to_string(),
            }
        );
        assert!(!plan.requires_root);
    }

    #[test]
    fn test_format_plan_output() {
        let plan = InstallPlan {
            steps: vec![
                InstallStep::SystemInstall {
                    package: "nginx".to_string(),
                    version: None,
                },
                InstallStep::MarketplaceInstall {
                    package: "my-agent".to_string(),
                    version: Some("1.0.0".to_string()),
                },
            ],
            requires_root: true,
            estimated_size_bytes: 1024,
        };

        let output = ArkPackageManager::format_plan(&plan);
        let text = output.to_display_string();
        assert!(text.contains("Execution plan"));
        assert!(text.contains("apt-get install nginx"));
        assert!(text.contains("marketplace install my-agent"));
        assert!(text.contains("Requires root"));
        assert!(text.contains("true"));
    }

    #[test]
    fn test_ark_output_formatting() {
        let mut output = ArkOutput::new();
        output
            .lines
            .push(ArkOutputLine::Header("Test Header".to_string()));
        output.lines.push(ArkOutputLine::Package {
            name: "nginx".to_string(),
            version: "1.24.0".to_string(),
            source: PackageSource::System,
            description: "HTTP server".to_string(),
        });
        output.lines.push(ArkOutputLine::Separator);
        output
            .lines
            .push(ArkOutputLine::Success("Done".to_string()));
        output
            .lines
            .push(ArkOutputLine::Error("Something failed".to_string()));
        output
            .lines
            .push(ArkOutputLine::Warning("Caution".to_string()));

        let text = output.to_display_string();
        assert!(text.contains("=== Test Header ==="));
        assert!(text.contains("nginx (1.24.0) [system] -- HTTP server"));
        assert!(text.contains("---"));
        assert!(text.contains("OK: Done"));
        assert!(text.contains("ERROR: Something failed"));
        assert!(text.contains("WARN: Caution"));
    }

    #[test]
    fn test_install_plan_mixed_sources() {
        let plan = InstallPlan {
            steps: vec![
                InstallStep::MarketplaceInstall {
                    package: "my-agent".to_string(),
                    version: None,
                },
                InstallStep::FlutterInstall {
                    package: "my-app".to_string(),
                    version: None,
                },
                InstallStep::SystemInstall {
                    package: "curl".to_string(),
                    version: None,
                },
            ],
            requires_root: true,
            estimated_size_bytes: 0,
        };

        assert_eq!(plan.steps.len(), 3);
        assert_eq!(
            plan.steps[0],
            InstallStep::MarketplaceInstall {
                package: "my-agent".to_string(),
                version: None,
            }
        );
        assert_eq!(
            plan.steps[1],
            InstallStep::FlutterInstall {
                package: "my-app".to_string(),
                version: None,
            }
        );
        assert_eq!(
            plan.steps[2],
            InstallStep::SystemInstall {
                package: "curl".to_string(),
                version: None,
            }
        );
        assert!(plan.requires_root);
    }

    #[test]
    fn test_status_returns_valid_output() {
        let tmp = TempDir::new().unwrap();
        let config = ArkConfig {
            marketplace_dir: tmp.path().to_path_buf(),
            cache_dir: tmp.path().join("cache"),
            ..ArkConfig::default()
        };
        let mgr = ArkPackageManager::new(config).unwrap();
        let output = mgr.status();
        let text = output.to_display_string();

        assert!(text.contains("ark status"));
        assert!(text.contains(ARK_VERSION));
        assert!(text.contains("Sources"));
        assert!(text.contains("ark is operational"));
    }

    // -- PackageSource display (from nous, verify it works through ark) --

    #[test]
    fn test_package_source_display_through_output() {
        let output_line = ArkOutputLine::Package {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: PackageSource::System,
            description: "test pkg".to_string(),
        };
        let output = ArkOutput {
            lines: vec![output_line],
        };
        let text = output.to_display_string();
        assert!(text.contains("[system]"));

        let output_line = ArkOutputLine::Package {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: PackageSource::Marketplace,
            description: "test pkg".to_string(),
        };
        let output = ArkOutput {
            lines: vec![output_line],
        };
        let text = output.to_display_string();
        assert!(text.contains("[marketplace]"));

        let output_line = ArkOutputLine::Package {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: PackageSource::FlutterApp,
            description: "test pkg".to_string(),
        };
        let output = ArkOutput {
            lines: vec![output_line],
        };
        let text = output.to_display_string();
        assert!(text.contains("[flutter-app]"));
    }

    // -----------------------------------------------------------------------
    // Phase 12B: Transaction log tests
    // -----------------------------------------------------------------------

    #[test]
    fn transaction_begin_and_commit() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        assert_eq!(log.len(), 1);
        assert!(log.commit(&id));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.status, TransactionStatus::Committed);
        assert!(txn.completed_at.is_some());
    }

    #[test]
    fn transaction_rollback() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.add_op(
            &id,
            TransactionOp {
                op_type: TransactionOpType::Install,
                package: "nginx".into(),
                version: Some("1.25".into()),
                source: PackageSource::System,
                status: TransactionOpStatus::Pending,
                error: None,
            },
        );
        assert!(log.rollback(&id));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.status, TransactionStatus::RolledBack);
        assert_eq!(txn.operations[0].status, TransactionOpStatus::RolledBack);
    }

    #[test]
    fn transaction_add_op() {
        let mut log = TransactionLog::new();
        let id = log.begin("user1");
        assert!(log.add_op(
            &id,
            TransactionOp {
                op_type: TransactionOpType::Install,
                package: "pkg1".into(),
                version: Some("1.0".into()),
                source: PackageSource::Marketplace,
                status: TransactionOpStatus::Pending,
                error: None,
            },
        ));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.operations.len(), 1);
        assert_eq!(txn.operations[0].package, "pkg1");
    }

    #[test]
    fn transaction_mark_op_complete() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.add_op(
            &id,
            TransactionOp {
                op_type: TransactionOpType::Install,
                package: "test-pkg".into(),
                version: None,
                source: PackageSource::System,
                status: TransactionOpStatus::InProgress,
                error: None,
            },
        );
        assert!(log.mark_op_complete(&id, "test-pkg"));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.operations[0].status, TransactionOpStatus::Complete);
    }

    #[test]
    fn transaction_mark_op_failed() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.add_op(
            &id,
            TransactionOp {
                op_type: TransactionOpType::Remove,
                package: "fail-pkg".into(),
                version: None,
                source: PackageSource::System,
                status: TransactionOpStatus::InProgress,
                error: None,
            },
        );
        assert!(log.mark_op_failed(&id, "fail-pkg", "permission denied"));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.operations[0].status, TransactionOpStatus::Failed);
        assert_eq!(
            txn.operations[0].error.as_deref(),
            Some("permission denied")
        );
    }

    #[test]
    fn transaction_fail() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        assert!(log.fail(&id, "disk full"));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.status, TransactionStatus::Failed("disk full".into()));
    }

    #[test]
    fn transaction_fail_rejects_committed() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.commit(&id);
        // Cannot fail an already-committed transaction
        assert!(!log.fail(&id, "too late"));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.status, TransactionStatus::Committed);
    }

    #[test]
    fn transaction_fail_rejects_rolled_back() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.rollback(&id);
        // Cannot fail an already-rolled-back transaction
        assert!(!log.fail(&id, "too late"));
        let txn = log.get(&id).unwrap();
        assert_eq!(txn.status, TransactionStatus::RolledBack);
    }

    #[test]
    fn transaction_cannot_add_op_after_commit() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.commit(&id);
        assert!(!log.add_op(
            &id,
            TransactionOp {
                op_type: TransactionOpType::Install,
                package: "late-pkg".into(),
                version: None,
                source: PackageSource::System,
                status: TransactionOpStatus::Pending,
                error: None,
            },
        ));
    }

    #[test]
    fn transaction_recent() {
        let mut log = TransactionLog::new();
        let id1 = log.begin("user1");
        log.commit(&id1);
        let id2 = log.begin("user2");
        log.commit(&id2);
        let id3 = log.begin("user3");
        log.commit(&id3);

        let recent = log.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, id3);
        assert_eq!(recent[1].id, id2);
    }

    #[test]
    fn transaction_status_display() {
        assert_eq!(format!("{}", TransactionStatus::InProgress), "in-progress");
        assert_eq!(format!("{}", TransactionStatus::Committed), "committed");
        assert_eq!(format!("{}", TransactionStatus::RolledBack), "rolled-back");
        assert_eq!(
            format!("{}", TransactionStatus::Failed("err".into())),
            "failed: err"
        );
    }

    #[test]
    fn transaction_upgrade_op() {
        let mut log = TransactionLog::new();
        let id = log.begin("root");
        log.add_op(
            &id,
            TransactionOp {
                op_type: TransactionOpType::Upgrade {
                    from_version: "1.0".into(),
                },
                package: "pkg".into(),
                version: Some("2.0".into()),
                source: PackageSource::System,
                status: TransactionOpStatus::Pending,
                error: None,
            },
        );
        let txn = log.get(&id).unwrap();
        assert!(matches!(
            txn.operations[0].op_type,
            TransactionOpType::Upgrade { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Phase 12B: Package database tests
    // -----------------------------------------------------------------------

    fn make_db_entry(name: &str, version: &str, files: Vec<&str>) -> PackageDbEntry {
        PackageDbEntry {
            name: name.into(),
            version: version.into(),
            source: PackageSource::System,
            installed_at: chrono::Utc::now(),
            installed_by: "root".into(),
            size_bytes: 1024,
            checksum: "abc123".into(),
            files: files.into_iter().map(String::from).collect(),
            dependencies: vec![],
            transaction_id: None,
        }
    }

    #[test]
    fn package_db_register_and_get() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("nginx", "1.25", vec!["/usr/bin/nginx"]));
        assert!(db.is_installed("nginx"));
        let entry = db.get("nginx").unwrap();
        assert_eq!(entry.version, "1.25");
    }

    #[test]
    fn package_db_unregister() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("pkg", "1.0", vec![]));
        let removed = db.unregister("pkg");
        assert!(removed.is_some());
        assert!(!db.is_installed("pkg"));
    }

    #[test]
    fn package_db_list() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("a", "1.0", vec![]));
        db.register(make_db_entry("b", "2.0", vec![]));
        assert_eq!(db.count(), 2);
        assert_eq!(db.list().len(), 2);
    }

    #[test]
    fn package_db_search() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("nginx-core", "1.25", vec![]));
        db.register(make_db_entry("redis", "7.0", vec![]));
        let results = db.search("nginx");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "nginx-core");
    }

    #[test]
    fn package_db_by_source() {
        let mut db = PackageDb::new();
        let mut entry = make_db_entry("agent1", "1.0", vec![]);
        entry.source = PackageSource::Marketplace;
        db.register(entry);
        db.register(make_db_entry("nginx", "1.25", vec![]));

        let marketplace = db.by_source(&PackageSource::Marketplace);
        assert_eq!(marketplace.len(), 1);
        assert_eq!(marketplace[0].name, "agent1");
    }

    #[test]
    fn package_db_total_size() {
        let mut db = PackageDb::new();
        let mut e1 = make_db_entry("a", "1.0", vec![]);
        e1.size_bytes = 500;
        let mut e2 = make_db_entry("b", "1.0", vec![]);
        e2.size_bytes = 700;
        db.register(e1);
        db.register(e2);
        assert_eq!(db.total_size(), 1200);
    }

    #[test]
    fn package_db_files_for() {
        let mut db = PackageDb::new();
        db.register(make_db_entry(
            "nginx",
            "1.25",
            vec!["/usr/bin/nginx", "/etc/nginx/nginx.conf"],
        ));
        let files = db.files_for("nginx");
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"/usr/bin/nginx"));
    }

    #[test]
    fn package_db_owner_of() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("bash", "5.3", vec!["/usr/bin/bash"]));
        db.register(make_db_entry("zsh", "5.9", vec!["/usr/bin/zsh"]));
        assert_eq!(db.owner_of("/usr/bin/bash"), Some("bash"));
        assert_eq!(db.owner_of("/usr/bin/zsh"), Some("zsh"));
        assert_eq!(db.owner_of("/usr/bin/fish"), None);
    }

    #[test]
    fn package_db_integrity_no_manifest() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("empty-pkg", "1.0", vec![]));
        let issues = db.check_integrity();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, IntegrityIssueType::NoFileManifest);
    }

    #[test]
    fn package_db_integrity_with_manifest() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("good-pkg", "1.0", vec!["/usr/bin/good"]));
        let issues = db.check_integrity();
        assert!(issues.is_empty());
    }

    #[test]
    fn package_db_resolve_install_order() {
        let mut db = PackageDb::new();
        let mut entry = make_db_entry("app", "1.0", vec![]);
        entry.dependencies = vec!["lib".into()];
        db.register(entry);
        db.register(make_db_entry("lib", "1.0", vec![]));

        let order = db.resolve_install_order(&["app", "lib"]).unwrap();
        let lib_idx = order.iter().position(|n| n == "lib").unwrap();
        let app_idx = order.iter().position(|n| n == "app").unwrap();
        assert!(lib_idx < app_idx);
    }

    #[test]
    fn package_db_resolve_circular_dep() {
        let mut db = PackageDb::new();
        let mut a = make_db_entry("a", "1.0", vec![]);
        a.dependencies = vec!["b".into()];
        let mut b = make_db_entry("b", "1.0", vec![]);
        b.dependencies = vec!["a".into()];
        db.register(a);
        db.register(b);

        let result = db.resolve_install_order(&["a", "b"]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Coverage improvement: ArkOutput, InstallPlan, TransactionLog, PackageDb
    // -----------------------------------------------------------------------

    #[test]
    fn test_ark_output_display_impl() {
        let output = ArkOutput {
            lines: vec![
                ArkOutputLine::Header("Test".to_string()),
                ArkOutputLine::Info {
                    key: "Status".to_string(),
                    value: "OK".to_string(),
                },
                ArkOutputLine::Separator,
                ArkOutputLine::Success("Done".to_string()),
                ArkOutputLine::Error("Failed".to_string()),
                ArkOutputLine::Warning("Caution".to_string()),
                ArkOutputLine::Package {
                    name: "nginx".to_string(),
                    version: "1.25".to_string(),
                    source: PackageSource::System,
                    description: "web server".to_string(),
                },
            ],
        };
        let display = format!("{}", output);
        let method = output.to_display_string();
        assert_eq!(display, method);
        assert!(display.contains("=== Test ==="));
        assert!(display.contains("Status: OK"));
        assert!(display.contains("---"));
        assert!(display.contains("OK: Done"));
        assert!(display.contains("ERROR: Failed"));
        assert!(display.contains("WARN: Caution"));
        assert!(display.contains("nginx"));
    }

    #[test]
    fn test_ark_output_default() {
        let output = ArkOutput::default();
        assert!(output.lines.is_empty());
        assert_eq!(format!("{}", output), "");
    }

    #[test]
    fn test_install_plan_new_and_default() {
        let plan = InstallPlan::new();
        assert!(plan.steps.is_empty());
        assert!(!plan.requires_root);
        assert_eq!(plan.estimated_size_bytes, 0);

        let default_plan = InstallPlan::default();
        assert_eq!(plan.steps.len(), default_plan.steps.len());
    }

    #[test]
    fn test_install_plan_serialization() {
        let plan = InstallPlan {
            steps: vec![
                InstallStep::SystemInstall {
                    package: "curl".to_string(),
                    version: Some("7.85.0".to_string()),
                },
                InstallStep::MarketplaceInstall {
                    package: "agent".to_string(),
                    version: None,
                },
                InstallStep::FlutterInstall {
                    package: "app".to_string(),
                    version: Some("1.0".to_string()),
                },
                InstallStep::SystemRemove {
                    package: "old".to_string(),
                    purge: true,
                },
                InstallStep::MarketplaceRemove {
                    package: "old-agent".to_string(),
                },
                InstallStep::FlutterRemove {
                    package: "old-app".to_string(),
                },
                InstallStep::SystemUpdate,
            ],
            requires_root: true,
            estimated_size_bytes: 2048,
        };
        let json = serde_json::to_string(&plan).unwrap();
        let deser: InstallPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.steps.len(), 7);
        assert!(deser.requires_root);
        assert_eq!(deser.estimated_size_bytes, 2048);
    }

    #[test]
    fn test_ark_command_serialization() {
        let commands = vec![
            ArkCommand::Install {
                packages: vec!["nginx".into()],
                force: true,
            },
            ArkCommand::Remove {
                packages: vec!["old".into()],
                purge: false,
            },
            ArkCommand::Search {
                query: "web".into(),
                source: Some(PackageSource::System),
            },
            ArkCommand::List {
                source: Some(PackageSource::Marketplace),
            },
            ArkCommand::Info {
                package: "nginx".into(),
            },
            ArkCommand::Update,
            ArkCommand::Upgrade {
                packages: Some(vec!["nginx".into()]),
            },
            ArkCommand::Status,
        ];
        for cmd in &commands {
            let json = serde_json::to_string(cmd).unwrap();
            let deser: ArkCommand = serde_json::from_str(&json).unwrap();
            assert_eq!(&deser, cmd);
        }
    }

    #[test]
    fn test_ark_result_serialization() {
        let result = ArkResult {
            success: true,
            message: "Installed".to_string(),
            packages_affected: vec!["nginx".into(), "curl".into()],
            source: PackageSource::System,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: ArkResult = serde_json::from_str(&json).unwrap();
        assert!(deser.success);
        assert_eq!(deser.packages_affected.len(), 2);
    }

    #[test]
    fn test_ark_config_default() {
        let config = ArkConfig::default();
        assert!(config.confirm_system_installs);
        assert!(config.confirm_removals);
        assert!(!config.auto_update_check);
        assert!(config.color_output);
    }

    #[test]
    fn test_transaction_log_lifecycle() {
        let mut log = TransactionLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        let txn_id = log.begin("root");
        assert!(!log.is_empty());
        assert_eq!(log.len(), 1);

        // Add operations
        let op = TransactionOp {
            package: "nginx".to_string(),
            op_type: TransactionOpType::Install,
            source: PackageSource::System,
            version: Some("1.25".to_string()),
            status: TransactionOpStatus::Pending,
            error: None,
        };
        assert!(log.add_op(&txn_id, op));

        // Mark op complete
        assert!(log.mark_op_complete(&txn_id, "nginx"));

        // Commit
        assert!(log.commit(&txn_id));

        // Can't commit again
        assert!(!log.commit(&txn_id));

        // Get and verify
        let txn = log.get(&txn_id).unwrap();
        assert_eq!(txn.status, TransactionStatus::Committed);
        assert!(txn.completed_at.is_some());
    }

    #[test]
    fn test_transaction_log_rollback() {
        let mut log = TransactionLog::new();
        let txn_id = log.begin("user");

        let op = TransactionOp {
            package: "failing-pkg".to_string(),
            op_type: TransactionOpType::Install,
            source: PackageSource::Marketplace,
            version: None,
            status: TransactionOpStatus::Pending,
            error: None,
        };
        log.add_op(&txn_id, op);
        assert!(log.rollback(&txn_id));

        let txn = log.get(&txn_id).unwrap();
        assert_eq!(txn.status, TransactionStatus::RolledBack);
        assert_eq!(txn.operations[0].status, TransactionOpStatus::RolledBack);
    }

    #[test]
    fn test_transaction_log_fail() {
        let mut log = TransactionLog::new();
        let txn_id = log.begin("user");

        assert!(log.fail(&txn_id, "disk full"));
        let txn = log.get(&txn_id).unwrap();
        assert!(matches!(txn.status, TransactionStatus::Failed(_)));

        // Can't fail again
        assert!(!log.fail(&txn_id, "another error"));
    }

    #[test]
    fn test_transaction_log_mark_op_failed() {
        let mut log = TransactionLog::new();
        let txn_id = log.begin("user");

        let op = TransactionOp {
            package: "bad-pkg".to_string(),
            op_type: TransactionOpType::Install,
            source: PackageSource::System,
            version: None,
            status: TransactionOpStatus::InProgress,
            error: None,
        };
        log.add_op(&txn_id, op);
        assert!(log.mark_op_failed(&txn_id, "bad-pkg", "checksum mismatch"));

        let txn = log.get(&txn_id).unwrap();
        assert_eq!(txn.operations[0].status, TransactionOpStatus::Failed);
        assert_eq!(
            txn.operations[0].error.as_deref(),
            Some("checksum mismatch")
        );
    }

    #[test]
    fn test_transaction_log_recent() {
        let mut log = TransactionLog::new();
        log.begin("a");
        log.begin("b");
        log.begin("c");

        let recent = log.recent(2);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_transaction_log_sequential_ids() {
        let mut log = TransactionLog::new();
        let id1 = log.begin("u");
        let id2 = log.begin("u");
        assert_ne!(id1, id2);
        assert!(id1.starts_with("txn-"));
        assert!(id2.starts_with("txn-"));
    }

    #[test]
    fn test_package_db_by_source() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("sys-pkg", "1.0", vec![]));

        let mut mkt_entry = make_db_entry("mkt-pkg", "2.0", vec![]);
        mkt_entry.source = PackageSource::Marketplace;
        db.register(mkt_entry);

        let system = db.by_source(&PackageSource::System);
        assert_eq!(system.len(), 1);
        assert_eq!(system[0].name, "sys-pkg");

        let marketplace = db.by_source(&PackageSource::Marketplace);
        assert_eq!(marketplace.len(), 1);
        assert_eq!(marketplace[0].name, "mkt-pkg");
    }

    #[test]
    fn test_package_db_total_size() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("a", "1.0", vec![]));
        db.register(make_db_entry("b", "1.0", vec![]));
        assert_eq!(db.total_size(), 2048); // 1024 each from make_db_entry
    }

    #[test]
    fn test_package_db_owner_of() {
        let mut db = PackageDb::new();
        db.register(make_db_entry("nginx", "1.25", vec!["/usr/sbin/nginx"]));
        assert_eq!(db.owner_of("/usr/sbin/nginx"), Some("nginx"));
        assert_eq!(db.owner_of("/usr/bin/curl"), None);
    }

    #[test]
    fn test_package_db_files_for() {
        let mut db = PackageDb::new();
        db.register(make_db_entry(
            "curl",
            "7.0",
            vec!["/usr/bin/curl", "/usr/share/man/curl.1"],
        ));
        let files = db.files_for("curl");
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"/usr/bin/curl"));

        let empty = db.files_for("nonexistent");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_step_result_serialization() {
        let sr = StepResult {
            step: InstallStep::SystemInstall {
                package: "curl".to_string(),
                version: None,
            },
            success: true,
            message: "OK".to_string(),
            duration_ms: 500,
        };
        let json = serde_json::to_string(&sr).unwrap();
        let deser: StepResult = serde_json::from_str(&json).unwrap();
        assert!(deser.success);
        assert_eq!(deser.duration_ms, 500);
    }

    #[test]
    fn test_plan_execution_result_serialization() {
        let per = PlanExecutionResult {
            transaction_id: "txn-000001".to_string(),
            success: true,
            steps_completed: 3,
            steps_failed: 0,
            total_duration_ms: 1500,
            step_results: vec![],
        };
        let json = serde_json::to_string(&per).unwrap();
        let deser: PlanExecutionResult = serde_json::from_str(&json).unwrap();
        assert!(deser.success);
        assert_eq!(deser.steps_completed, 3);
    }

    #[test]
    fn test_integrity_issue_type_serialization() {
        let types = vec![
            IntegrityIssueType::NoFileManifest,
            IntegrityIssueType::MissingFile("/usr/bin/test".to_string()),
            IntegrityIssueType::ChecksumMismatch("sha256:abc".to_string()),
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let deser: IntegrityIssueType = serde_json::from_str(&json).unwrap();
            assert_eq!(&deser, t);
        }
    }

    #[test]
    fn test_parse_source_arg_variations() {
        // system
        let cmd = parse_args(&["search", "--source", "system", "test"]).unwrap();
        if let ArkCommand::Search { source, .. } = cmd {
            assert_eq!(source, Some(PackageSource::System));
        } else {
            panic!("Expected Search command");
        }

        // apt alias
        let cmd = parse_args(&["search", "--source", "apt", "test"]).unwrap();
        if let ArkCommand::Search { source, .. } = cmd {
            assert_eq!(source, Some(PackageSource::System));
        } else {
            panic!("Expected Search command");
        }

        // marketplace
        let cmd = parse_args(&["search", "--source", "marketplace", "test"]).unwrap();
        if let ArkCommand::Search { source, .. } = cmd {
            assert_eq!(source, Some(PackageSource::Marketplace));
        } else {
            panic!("Expected Search command");
        }

        // flutter
        let cmd = parse_args(&["search", "--source", "flutter", "test"]).unwrap();
        if let ArkCommand::Search { source, .. } = cmd {
            assert_eq!(source, Some(PackageSource::FlutterApp));
        } else {
            panic!("Expected Search command");
        }
    }

    // H18: Transaction log persistence tests
    #[test]
    fn transaction_log_persist_and_recover() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("transaction.log");
        {
            let mut log = TransactionLog::load(&log_path).unwrap();
            assert!(log.is_empty());
            let id1 = log.begin("user1");
            log.add_op(
                &id1,
                TransactionOp {
                    op_type: TransactionOpType::Install,
                    package: "pkg-a".into(),
                    version: Some("1.0".into()),
                    source: PackageSource::System,
                    status: TransactionOpStatus::Pending,
                    error: None,
                },
            );
            log.commit(&id1);
            let id2 = log.begin("user2");
            log.add_op(
                &id2,
                TransactionOp {
                    op_type: TransactionOpType::Remove,
                    package: "pkg-b".into(),
                    version: None,
                    source: PackageSource::Marketplace,
                    status: TransactionOpStatus::InProgress,
                    error: None,
                },
            );
            log.rollback(&id2);
            let id3 = log.begin("user3");
            log.fail(&id3, "disk full");
            assert_eq!(log.len(), 3);
        }
        assert!(log_path.exists());
        let recovered = TransactionLog::load(&log_path).unwrap();
        assert_eq!(recovered.len(), 3);
        assert_eq!(
            recovered.get("txn-000001").unwrap().status,
            TransactionStatus::Committed
        );
        assert_eq!(
            recovered.get("txn-000002").unwrap().status,
            TransactionStatus::RolledBack
        );
        assert!(matches!(
            recovered.get("txn-000003").unwrap().status,
            TransactionStatus::Failed(_)
        ));
    }

    #[test]
    fn transaction_log_fresh_start_no_file() {
        let tmp = TempDir::new().unwrap();
        let log = TransactionLog::load(&tmp.path().join("nonexistent.log")).unwrap();
        assert!(log.is_empty());
    }

    #[test]
    fn transaction_log_survives_corrupt_lines() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("corrupt.log");
        {
            let mut log = TransactionLog::load(&log_path).unwrap();
            let id = log.begin("user1");
            log.commit(&id);
        }
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_path)
                .unwrap();
            writeln!(f, "{{not valid json}}").unwrap();
        }
        let recovered = TransactionLog::load(&log_path).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(
            recovered.get("txn-000001").unwrap().status,
            TransactionStatus::Committed
        );
    }
}
