//! Argonaut — Init System for AGNOS
//!
//! Minimal init system that boots AGNOS in under 3 seconds. Manages
//! service startup ordering, health checks, and shutdown sequences.
//! Named after the Greek Argonauts who sailed the Argo — one letter
//! off from AGNOS.
//!
//! This module defines the shared types and boot orchestration logic
//! that agent-runtime uses. The actual PID 1 binary will live in a
//! separate crate; this module provides the brain.
//!
//! Submodules:
//! - **types**: All enums, structs, and configuration types
//! - **boot**: Boot sequence construction per [`BootMode`]
//! - **services**: Service management, default definitions, dependency resolution
//! - **runlevels**: Shutdown planning and runlevel switching
//! - **edge_boot**: Read-only rootfs and dm-verity helpers

pub mod types;

mod boot;
mod runlevels;
mod services;

pub mod edge_boot;

#[cfg(test)]
mod tests;

// Re-export all public types so external consumers see the same flat API
// as they did when this was a single argonaut.rs file.
pub use edge_boot::{configure_readonly_rootfs, verify_rootfs_integrity};
pub use types::{
    ArgonautConfig, ArgonautStats, BootMode, BootStage, BootStep, BootStepStatus, CrashAction,
    EdgeBootConfig, EmergencyShellConfig, ExitStatus, HealthCheck, HealthCheckResult,
    HealthCheckType, HealthTracker, ManagedService, ProcessSpec, ReadyCheck, RestartPolicy,
    Runlevel, RunlevelSwitchPlan, SafeCommand, ServiceDefinition, ServiceEvent, ServiceEventType,
    ServiceState, ServiceTarget, ShutdownAction, ShutdownPlan, ShutdownStep, ShutdownStepStatus,
    ShutdownType,
};

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::info;

/// The main init-system orchestrator. Holds the boot sequence and
/// all managed services.
pub struct ArgonautInit {
    pub config: ArgonautConfig,
    pub boot_sequence: Vec<BootStep>,
    pub services: HashMap<String, ManagedService>,
    pub boot_started: Option<DateTime<Utc>>,
    pub boot_completed: Option<DateTime<Utc>>,
}

impl ArgonautInit {
    /// Create a new init system from the given configuration. Builds
    /// the boot sequence and registers default services for the mode.
    pub fn new(config: ArgonautConfig) -> Self {
        let boot_sequence = Self::build_boot_sequence(config.boot_mode);
        let default_svc = Self::default_services(config.boot_mode);
        let mut services = HashMap::new();
        for svc in default_svc {
            let managed = ManagedService {
                definition: svc.clone(),
                state: ServiceState::Stopped,
                pid: None,
                started_at: None,
                restart_count: 0,
                last_health_check: None,
            };
            services.insert(svc.name.clone(), managed);
        }
        // Also register any extra services from config.
        for svc in &config.services {
            if !services.contains_key(&svc.name) {
                let managed = ManagedService {
                    definition: svc.clone(),
                    state: ServiceState::Stopped,
                    pid: None,
                    started_at: None,
                    restart_count: 0,
                    last_health_check: None,
                };
                services.insert(svc.name.clone(), managed);
            }
        }
        info!(mode = %config.boot_mode, steps = boot_sequence.len(), services = services.len(), "argonaut initialized");
        Self {
            config,
            boot_sequence,
            services,
            boot_started: None,
            boot_completed: None,
        }
    }

    /// Mark a boot stage as complete. Returns `true` if the stage was
    /// found and updated.
    pub fn mark_step_complete(&mut self, stage: BootStage) -> bool {
        if let Some(step) = self.boot_sequence.iter_mut().find(|s| s.stage == stage) {
            // Ensure started_at is populated.
            if step.started_at.is_none() {
                step.started_at = Some(Utc::now());
            }
            step.status = BootStepStatus::Complete;
            step.completed_at = Some(Utc::now());
            info!(stage = %stage, "boot step complete");

            // Set boot_started on the first step completion if not yet set.
            if self.boot_started.is_none() {
                self.boot_started = Some(Utc::now());
            }

            // If this was the BootComplete stage, mark the overall boot time.
            if stage == BootStage::BootComplete {
                self.boot_completed = Some(Utc::now());
            }
            true
        } else {
            false
        }
    }

    /// Mark a boot stage as failed. Returns `true` if the stage was
    /// found and updated.
    pub fn mark_step_failed(&mut self, stage: BootStage, error: String) -> bool {
        if let Some(step) = self.boot_sequence.iter_mut().find(|s| s.stage == stage) {
            tracing::warn!(stage = %stage, error = %error, "boot step failed");
            // Ensure started_at is populated.
            if step.started_at.is_none() {
                step.started_at = Some(Utc::now());
            }
            step.status = BootStepStatus::Failed;
            step.completed_at = Some(Utc::now());
            step.error = Some(error);

            // Set boot_started on the first step if not yet set.
            if self.boot_started.is_none() {
                self.boot_started = Some(Utc::now());
            }
            true
        } else {
            false
        }
    }

    /// The first boot step that is not yet complete or failed.
    pub fn current_stage(&self) -> Option<&BootStep> {
        self.boot_sequence.iter().find(|s| {
            s.status != BootStepStatus::Complete
                && s.status != BootStepStatus::Failed
                && s.status != BootStepStatus::Skipped
        })
    }

    /// Whether every boot step has completed (or been skipped).
    pub fn is_boot_complete(&self) -> bool {
        self.boot_sequence.iter().all(|s| {
            s.status == BootStepStatus::Complete
                || s.status == BootStepStatus::Skipped
                || (!s.required && s.status == BootStepStatus::Failed)
        })
    }

    /// Total boot duration in milliseconds, if boot has completed.
    pub fn boot_duration_ms(&self) -> Option<u64> {
        match (self.boot_started, self.boot_completed) {
            (Some(start), Some(end)) => {
                let dur = end.signed_duration_since(start);
                Some(dur.num_milliseconds().max(0) as u64)
            }
            _ => None,
        }
    }

    /// All boot steps that have failed.
    pub fn failed_steps(&self) -> Vec<&BootStep> {
        self.boot_sequence
            .iter()
            .filter(|s| s.status == BootStepStatus::Failed)
            .collect()
    }

    /// Collect current statistics.
    pub fn stats(&self) -> ArgonautStats {
        let services_running = self
            .services
            .values()
            .filter(|s| s.state == ServiceState::Running)
            .count();
        let services_failed = self
            .services
            .values()
            .filter(|s| matches!(s.state, ServiceState::Failed(_)))
            .count();
        let total_restarts: u32 = self.services.values().map(|s| s.restart_count).sum();

        ArgonautStats {
            boot_mode: self.config.boot_mode,
            boot_duration_ms: self.boot_duration_ms(),
            services_running,
            services_failed,
            total_restarts,
            boot_complete: self.is_boot_complete(),
        }
    }

    /// Determine whether the system should drop to an emergency shell.
    /// Called after a critical boot step failure.
    pub fn should_drop_to_emergency(&self) -> bool {
        self.failed_steps()
            .iter()
            .any(|step| step.required && step.status == BootStepStatus::Failed)
    }

    /// Get the emergency shell configuration.
    pub fn emergency_shell_config(&self) -> EmergencyShellConfig {
        EmergencyShellConfig::default()
    }
}
