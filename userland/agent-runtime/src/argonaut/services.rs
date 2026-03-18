//! Service management — registration, state transitions, default service
//! definitions, dependency resolution, and crash handling.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Result};
use chrono::Utc;
use tracing::{debug, info, warn};

use super::types::{
    BootMode, CrashAction, ExitStatus, HealthCheck, HealthCheckType, ManagedService, ProcessSpec,
    ReadyCheck, RestartPolicy, ServiceDefinition, ServiceEvent, ServiceEventType, ServiceState,
};

/// Exponential backoff delay for service restarts.
/// 1s, 2s, 4s, 8s, 16s (capped at 30s).
pub(crate) fn backoff_delay(restart_count: u32) -> u64 {
    let base: u64 = 1000;
    let delay = base * 2u64.saturating_pow(restart_count);
    delay.min(30_000)
}

impl super::ArgonautInit {
    /// Return the PostgreSQL and Redis database service definitions.
    pub fn database_services() -> Vec<ServiceDefinition> {
        vec![
            ServiceDefinition {
                name: "postgres".into(),
                description: "PostgreSQL 17 database server".into(),
                binary_path: PathBuf::from("/usr/lib/postgresql/17/bin/postgres"),
                args: vec![
                    "-D".into(),
                    "/var/lib/postgresql/data".into(),
                    "-c".into(),
                    "config_file=/etc/postgresql/postgresql.conf.agnos".into(),
                ],
                environment: {
                    let mut env = HashMap::new();
                    env.insert("PGDATA".into(), "/var/lib/postgresql/data".into());
                    env
                },
                depends_on: vec![],
                required_for_modes: vec![BootMode::Server, BootMode::Desktop],
                restart_policy: RestartPolicy::OnFailure,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 5432),
                    interval_ms: 15_000,
                    timeout_ms: 2000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 5432),
                    timeout_ms: 10_000,
                    retries: 15,
                    retry_delay_ms: 500,
                }),
            },
            ServiceDefinition {
                name: "redis".into(),
                description: "Redis 7 in-memory cache".into(),
                binary_path: PathBuf::from("/usr/bin/redis-server"),
                args: vec!["/etc/redis/redis.conf".into()],
                environment: HashMap::new(),
                depends_on: vec![],
                required_for_modes: vec![BootMode::Server, BootMode::Desktop],
                restart_policy: RestartPolicy::Always,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 6379),
                    interval_ms: 10_000,
                    timeout_ms: 1000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 6379),
                    timeout_ms: 5000,
                    retries: 10,
                    retry_delay_ms: 200,
                }),
            },
        ]
    }

    /// Return the Synapse LLM management and training service definition.
    pub fn synapse_service() -> ServiceDefinition {
        ServiceDefinition {
            name: "synapse".into(),
            description: "Synapse LLM management and training service".into(),
            binary_path: PathBuf::from("/usr/lib/synapse/bin/synapse"),
            args: vec![
                "serve".into(),
                "--config".into(),
                "/etc/synapse/synapse.toml".into(),
            ],
            environment: {
                let mut env = HashMap::new();
                env.insert("SYNAPSE_DATA_DIR".into(), "/var/lib/synapse".into());
                env.insert("SYNAPSE_MODEL_DIR".into(), "/var/lib/synapse/models".into());
                env
            },
            depends_on: vec!["agent-runtime".into(), "llm-gateway".into()],
            required_for_modes: vec![BootMode::Server, BootMode::Desktop],
            restart_policy: RestartPolicy::OnFailure,
            health_check: Some(HealthCheck {
                check_type: HealthCheckType::HttpGet("http://127.0.0.1:8080/health".into()),
                interval_ms: 15_000,
                timeout_ms: 3000,
                retries: 3,
            }),
            ready_check: Some(ReadyCheck {
                check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8080),
                timeout_ms: 10_000,
                retries: 15,
                retry_delay_ms: 500,
            }),
        }
    }

    /// Return a `ServiceDefinition` for the Shruti DAW.
    ///
    /// Shruti is **not** auto-started — users opt-in by adding the
    /// definition to `ArgonautConfig.services` or enabling it at
    /// runtime via `enable_optional_service("shruti")`.
    pub fn shruti_service() -> ServiceDefinition {
        ServiceDefinition {
            name: "shruti".into(),
            description: "Shruti digital audio workstation".into(),
            binary_path: PathBuf::from("/usr/local/bin/shruti"),
            args: vec![],
            environment: {
                let mut env = HashMap::new();
                env.insert(
                    "SHRUTI_DATA_DIR".into(),
                    "/home/${USER}/.local/share/shruti".into(),
                );
                env.insert("PIPEWIRE_RUNTIME_DIR".into(), "/run/user/1000".into());
                env
            },
            depends_on: vec!["agent-runtime".into(), "aethersafha".into()],
            required_for_modes: vec![], // never auto-started
            restart_policy: RestartPolicy::OnFailure,
            health_check: Some(HealthCheck {
                check_type: HealthCheckType::ProcessAlive,
                interval_ms: 10_000,
                timeout_ms: 1000,
                retries: 3,
            }),
            ready_check: None,
        }
    }

    /// Return the optional service catalogue. These services are not
    /// started by default but can be enabled by the user.
    pub fn optional_services() -> Vec<ServiceDefinition> {
        vec![Self::shruti_service()]
    }

    /// Look up an optional service by name.
    pub fn optional_service(name: &str) -> Option<ServiceDefinition> {
        Self::optional_services()
            .into_iter()
            .find(|s| s.name == name)
    }

    /// Enable an optional service at runtime by inserting its
    /// definition into the managed service set. Returns `true` if the
    /// service was newly inserted, `false` if it was already present.
    pub fn enable_optional_service(&mut self, name: &str) -> bool {
        if self.services.contains_key(name) {
            return false;
        }
        if let Some(def) = Self::optional_service(name) {
            let managed = ManagedService {
                definition: def.clone(),
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            };
            self.services.insert(def.name.clone(), managed);
            info!(service = name, "enabled optional service");
            true
        } else {
            warn!(service = name, "unknown optional service");
            false
        }
    }

    /// Return the default AGNOS services for a boot mode.
    pub fn default_services(mode: BootMode) -> Vec<ServiceDefinition> {
        let mut services = Vec::new();

        // Edge mode: agent-runtime only, no database dependencies.
        if mode == BootMode::Edge {
            services.push(ServiceDefinition {
                name: "agent-runtime".into(),
                description: "Daimon agent orchestrator (edge mode)".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/agent_runtime"),
                args: vec![
                    "--port".into(),
                    "8090".into(),
                    "--mode".into(),
                    "edge".into(),
                ],
                environment: {
                    let mut env = HashMap::new();
                    env.insert("AGNOS_EDGE_MODE".into(), "1".into());
                    env.insert("AGNOS_READONLY_ROOTFS".into(), "1".into());
                    env.insert("AGNOS_EDGE_LUKS".into(), "1".into());
                    env
                },
                depends_on: vec![],
                required_for_modes: vec![BootMode::Edge],
                restart_policy: RestartPolicy::Always,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::HttpGet("http://127.0.0.1:8090/v1/health".into()),
                    interval_ms: 10_000,
                    timeout_ms: 2000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8090),
                    timeout_ms: 3000,
                    retries: 5,
                    retry_delay_ms: 200,
                }),
            });
            return services;
        }

        // Database services for Server and Desktop modes.
        if mode == BootMode::Server || mode == BootMode::Desktop {
            services.extend(Self::database_services());
        }

        // agent-runtime is always present.
        // In Server/Desktop modes it depends on database services.
        let db_deps = if mode == BootMode::Server || mode == BootMode::Desktop {
            vec!["postgres".into(), "redis".into()]
        } else {
            vec![]
        };
        services.push(ServiceDefinition {
            name: "agent-runtime".into(),
            description: "Daimon agent orchestrator".into(),
            binary_path: PathBuf::from("/usr/lib/agnos/agent_runtime"),
            args: vec!["--port".into(), "8090".into()],
            environment: HashMap::new(),
            depends_on: db_deps,
            required_for_modes: vec![BootMode::Minimal, BootMode::Server, BootMode::Desktop],
            restart_policy: RestartPolicy::Always,
            health_check: Some(HealthCheck {
                check_type: HealthCheckType::HttpGet("http://127.0.0.1:8090/v1/health".into()),
                interval_ms: 10_000,
                timeout_ms: 2000,
                retries: 3,
            }),
            ready_check: Some(ReadyCheck {
                check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8090),
                timeout_ms: 5000,
                retries: 10,
                retry_delay_ms: 200,
            }),
        });

        if mode == BootMode::Server || mode == BootMode::Desktop {
            services.push(ServiceDefinition {
                name: "llm-gateway".into(),
                description: "Hoosh LLM inference gateway".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/llm_gateway"),
                args: vec!["--port".into(), "8088".into()],
                environment: HashMap::new(),
                depends_on: vec!["agent-runtime".into()],
                required_for_modes: vec![BootMode::Server, BootMode::Desktop],
                restart_policy: RestartPolicy::OnFailure,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::HttpGet("http://127.0.0.1:8088/health".into()),
                    interval_ms: 15_000,
                    timeout_ms: 2000,
                    retries: 3,
                }),
                ready_check: Some(ReadyCheck {
                    check_type: HealthCheckType::TcpConnect("127.0.0.1".into(), 8088),
                    timeout_ms: 5000,
                    retries: 10,
                    retry_delay_ms: 200,
                }),
            });
            services.push(Self::synapse_service());
        }

        if mode == BootMode::Desktop {
            services.push(ServiceDefinition {
                name: "aethersafha".into(),
                description: "Wayland compositor".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/aethersafha"),
                args: vec![],
                environment: {
                    let mut env = HashMap::new();
                    env.insert("XDG_SESSION_TYPE".into(), "wayland".into());
                    env
                },
                depends_on: vec!["agent-runtime".into()],
                required_for_modes: vec![BootMode::Desktop],
                restart_policy: RestartPolicy::Always,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::ProcessAlive,
                    interval_ms: 5000,
                    timeout_ms: 1000,
                    retries: 2,
                }),
                ready_check: None,
            });

            services.push(ServiceDefinition {
                name: "agnoshi".into(),
                description: "AI terminal shell".into(),
                binary_path: PathBuf::from("/usr/lib/agnos/agnoshi"),
                args: vec![],
                environment: HashMap::new(),
                depends_on: vec!["agent-runtime".into(), "aethersafha".into()],
                required_for_modes: vec![BootMode::Desktop],
                restart_policy: RestartPolicy::OnFailure,
                health_check: Some(HealthCheck {
                    check_type: HealthCheckType::ProcessAlive,
                    interval_ms: 10_000,
                    timeout_ms: 1000,
                    retries: 3,
                }),
                ready_check: None,
            });
        }

        services
    }

    /// Topological sort of service names by `depends_on`. Returns an
    /// ordered list of service names such that every service appears
    /// after its dependencies. Detects cycles and returns an error.
    pub fn resolve_service_order(services: &[ServiceDefinition]) -> Result<Vec<String>> {
        let name_set: HashMap<&str, &ServiceDefinition> =
            services.iter().map(|s| (s.name.as_str(), s)).collect();

        // Kahn's algorithm.
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

        for svc in services {
            in_degree.entry(svc.name.as_str()).or_insert(0);
            for dep in &svc.depends_on {
                if !name_set.contains_key(dep.as_str()) {
                    bail!(
                        "service '{}' depends on '{}' which is not defined",
                        svc.name,
                        dep
                    );
                }
                *in_degree.entry(svc.name.as_str()).or_insert(0) += 1;
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(svc.name.as_str());
            }
        }

        let mut queue: std::collections::BinaryHeap<std::cmp::Reverse<&str>> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| std::cmp::Reverse(name))
            .collect();

        let mut ordered: Vec<String> = Vec::new();

        while let Some(std::cmp::Reverse(current)) = queue.pop() {
            ordered.push(current.to_string());
            if let Some(deps) = dependents.get(current) {
                for &dep in deps {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push(std::cmp::Reverse(dep));
                        }
                    }
                }
            }
        }

        if ordered.len() != services.len() {
            bail!(
                "cycle detected in service dependencies — resolved {} of {} services",
                ordered.len(),
                services.len()
            );
        }

        debug!(order = ?ordered, "resolved service start order");
        Ok(ordered)
    }

    /// Register a new service definition. If a service with the same
    /// name already exists, the definition is updated but runtime state
    /// (state, pid, restart_count, etc.) is preserved.
    pub fn register_service(&mut self, definition: ServiceDefinition) {
        let name = definition.name.clone();
        if let Some(existing) = self.services.get_mut(&name) {
            warn!(service = %name, "service already registered — updating definition, preserving state");
            existing.definition = definition;
        } else {
            let managed = ManagedService {
                definition,
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            };
            info!(service = %name, "registered service");
            self.services.insert(name, managed);
        }
    }

    /// Look up a managed service by name.
    pub fn get_service(&self, name: &str) -> Option<&ManagedService> {
        self.services.get(name)
    }

    /// Get the current state of a service.
    pub fn get_service_state(&self, name: &str) -> Option<&ServiceState> {
        self.services.get(name).map(|s| &s.state)
    }

    /// Transition a service to a new state. Returns `true` if the
    /// service exists and the transition is valid.
    ///
    /// Invalid transitions are rejected with a warning log and `false`
    /// return value. When transitioning to `Starting`, all services
    /// listed in `depends_on` must already be `Running`.
    pub fn set_service_state(&mut self, name: &str, state: ServiceState) -> bool {
        // First, validate the transition and dependency constraints
        // without holding a mutable borrow.
        let validation = {
            if let Some(svc) = self.services.get(name) {
                if !svc.state.valid_transition(&state) {
                    warn!(
                        service = %name,
                        from = %svc.state,
                        to = %state,
                        "invalid state transition"
                    );
                    return false;
                }
                // When transitioning to Starting, check that all
                // dependencies are Running.
                if matches!(state, ServiceState::Starting) {
                    let mut unmet = Vec::new();
                    for dep in &svc.definition.depends_on {
                        let dep_running = self
                            .services
                            .get(dep.as_str())
                            .map(|d| d.state == ServiceState::Running)
                            .unwrap_or(false);
                        if !dep_running {
                            unmet.push(dep.clone());
                        }
                    }
                    if !unmet.is_empty() {
                        warn!(
                            service = %name,
                            unmet_deps = ?unmet,
                            "cannot start: dependencies not running"
                        );
                        return false;
                    }
                }
                true
            } else {
                warn!(service = %name, "set_service_state: unknown service");
                return false;
            }
        };

        if validation {
            if let Some(svc) = self.services.get_mut(name) {
                debug!(service = %name, from = %svc.state, to = %state, "state transition");
                svc.state = state;
            }
        }
        validation
    }

    /// Return service definitions that are required for the given mode.
    pub fn services_for_mode(&self, mode: &BootMode) -> Vec<&ServiceDefinition> {
        self.services
            .values()
            .filter(|s| s.definition.required_for_modes.contains(mode))
            .map(|s| &s.definition)
            .collect()
    }

    /// Return service names in shutdown order (reverse of startup order).
    /// Returns an error if dependency resolution fails (e.g. cycles).
    pub fn shutdown_order(&self) -> Result<Vec<String>> {
        let definitions: Vec<ServiceDefinition> = self
            .services
            .values()
            .map(|s| s.definition.clone())
            .collect();
        let mut order = Self::resolve_service_order(&definitions)?;
        order.reverse();
        Ok(order)
    }

    /// Record a service event in the audit log.
    pub fn record_event(&self, service: &str, event_type: ServiceEventType) -> ServiceEvent {
        let event = ServiceEvent {
            timestamp: Utc::now(),
            service: service.to_string(),
            event_type: event_type.clone(),
            details: None,
        };
        info!(
            service = service,
            event = %event_type,
            "service event"
        );
        event
    }

    /// Build a complete boot execution plan: resolve service order,
    /// create ProcessSpecs, and return the ordered list.
    pub fn boot_execution_plan(&self) -> Result<Vec<(String, ProcessSpec)>> {
        let definitions: Vec<ServiceDefinition> = self
            .services
            .values()
            .map(|s| s.definition.clone())
            .collect();
        let order = Self::resolve_service_order(&definitions)?;

        let plan: Vec<(String, ProcessSpec)> = order
            .into_iter()
            .filter_map(|name| {
                self.services.get(&name).map(|svc| {
                    let spec = ProcessSpec::from_service(&svc.definition);
                    (name, spec)
                })
            })
            .collect();

        Ok(plan)
    }

    /// Determine what action to take when a service crashes.
    pub fn on_service_crash(&self, service_name: &str, exit_status: &ExitStatus) -> CrashAction {
        let svc = match self.services.get(service_name) {
            Some(s) => s,
            None => return CrashAction::Ignore,
        };

        match svc.definition.restart_policy {
            RestartPolicy::Always => {
                if svc.restart_count >= 5 {
                    warn!(
                        service = service_name,
                        restarts = svc.restart_count,
                        "service exceeded restart limit"
                    );
                    CrashAction::GiveUp {
                        reason: format!("exceeded restart limit ({} restarts)", svc.restart_count),
                    }
                } else {
                    CrashAction::Restart {
                        delay_ms: backoff_delay(svc.restart_count),
                    }
                }
            }
            RestartPolicy::OnFailure => {
                if *exit_status == ExitStatus::Code(0) {
                    CrashAction::Ignore
                } else if svc.restart_count >= 5 {
                    CrashAction::GiveUp {
                        reason: format!(
                            "exceeded restart limit after failures ({} restarts)",
                            svc.restart_count
                        ),
                    }
                } else {
                    CrashAction::Restart {
                        delay_ms: backoff_delay(svc.restart_count),
                    }
                }
            }
            RestartPolicy::Never => CrashAction::Ignore,
        }
    }
}
