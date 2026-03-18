//! Runlevel management — shutdown planning and runlevel switching.

use anyhow::Result;

use super::types::{
    BootMode, Runlevel, RunlevelSwitchPlan, ServiceState, ServiceTarget, ShutdownAction,
    ShutdownPlan, ShutdownStep, ShutdownStepStatus, ShutdownType,
};

impl super::ArgonautInit {
    /// Build a shutdown plan for the given shutdown type.
    /// Services are stopped in reverse dependency order.
    pub fn plan_shutdown(&self, shutdown_type: ShutdownType) -> Result<ShutdownPlan> {
        let service_order = self.shutdown_order()?;
        let mut steps = Vec::new();

        // Step 1: Wall message
        let wall_msg = format!(
            "AGNOS system {} in {} seconds",
            shutdown_type,
            self.config.shutdown_timeout_ms / 1000
        );
        steps.push(ShutdownStep {
            description: "Broadcast shutdown warning".into(),
            action: ShutdownAction::WallMessage(wall_msg.clone()),
            timeout_ms: 1000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 2: Notify agents to save state
        steps.push(ShutdownStep {
            description: "Notify agents to save state".into(),
            action: ShutdownAction::NotifyAgents,
            timeout_ms: 5000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 3: Stop services in reverse dependency order
        for svc_name in &service_order {
            if let Some(svc) = self.services.get(svc_name) {
                if svc.state == ServiceState::Running || svc.state == ServiceState::Starting {
                    steps.push(ShutdownStep {
                        description: format!("Stop service: {}", svc_name),
                        action: ShutdownAction::StopService {
                            name: svc_name.clone(),
                            signal: 15, // SIGTERM
                        },
                        timeout_ms: 5000,
                        status: ShutdownStepStatus::Pending,
                    });
                }
            }
        }

        // Step 4: Sync filesystems
        steps.push(ShutdownStep {
            description: "Sync filesystem buffers".into(),
            action: ShutdownAction::SyncFilesystems,
            timeout_ms: 3000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 5: Unmount
        steps.push(ShutdownStep {
            description: "Unmount filesystems".into(),
            action: ShutdownAction::UnmountFilesystems,
            timeout_ms: 5000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 6: Deactivate swap
        steps.push(ShutdownStep {
            description: "Deactivate swap".into(),
            action: ShutdownAction::SwapOff,
            timeout_ms: 2000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 7: Close LUKS volumes
        steps.push(ShutdownStep {
            description: "Close encrypted volumes".into(),
            action: ShutdownAction::CloseLuks,
            timeout_ms: 3000,
            status: ShutdownStepStatus::Pending,
        });

        // Step 8: Final kernel action
        steps.push(ShutdownStep {
            description: format!("Execute {}", shutdown_type),
            action: ShutdownAction::KernelAction(shutdown_type),
            timeout_ms: 1000,
            status: ShutdownStepStatus::Pending,
        });

        Ok(ShutdownPlan {
            shutdown_type,
            steps,
            timeout_ms: self.config.shutdown_timeout_ms,
            wall_message: Some(wall_msg),
        })
    }

    /// Compute which services need to start/stop when switching runlevels.
    pub fn plan_runlevel_switch(
        &self,
        target: Runlevel,
        targets: &[ServiceTarget],
    ) -> RunlevelSwitchPlan {
        let mut services_to_start = Vec::new();
        let mut services_to_stop = Vec::new();

        // Determine which services should be running at the target runlevel
        let mut desired: std::collections::HashSet<String> = std::collections::HashSet::new();
        for tgt in targets {
            if tgt.is_active_in(target) {
                for svc in &tgt.requires {
                    desired.insert(svc.clone());
                }
                for svc in &tgt.wants {
                    desired.insert(svc.clone());
                }
            }
        }

        // Services that need to start (desired but not running)
        for svc_name in &desired {
            if let Some(svc) = self.services.get(svc_name) {
                if svc.state != ServiceState::Running && svc.state != ServiceState::Starting {
                    services_to_start.push(svc_name.clone());
                }
            } else {
                // Service not registered, still mark it as needed
                services_to_start.push(svc_name.clone());
            }
        }

        // Services that need to stop (running but not desired)
        for (name, svc) in &self.services {
            if (svc.state == ServiceState::Running || svc.state == ServiceState::Starting)
                && !desired.contains(name)
            {
                services_to_stop.push(name.clone());
            }
        }

        // Emergency/rescue: stop everything except basic shell
        if target == Runlevel::Emergency {
            services_to_stop.clear();
            services_to_start.clear();
            for (name, svc) in &self.services {
                if svc.state == ServiceState::Running || svc.state == ServiceState::Starting {
                    services_to_stop.push(name.clone());
                }
            }
        }

        RunlevelSwitchPlan {
            from: Runlevel::from_boot_mode(self.config.boot_mode),
            to: target,
            services_to_start,
            services_to_stop,
            drop_to_shell: target == Runlevel::Emergency || target == Runlevel::Rescue,
        }
    }

    /// Determine which boot mode to use for a given runlevel.
    pub fn runlevel_boot_mode(runlevel: Runlevel) -> Option<BootMode> {
        runlevel.to_boot_mode()
    }
}
