//! Boot sequence construction — builds the ordered list of [`BootStep`]s
//! for each [`BootMode`].

use super::types::{BootMode, BootStage, BootStep, BootStepStatus};

impl super::ArgonautInit {
    /// Build the ordered boot sequence for a given mode.
    #[allow(clippy::vec_init_then_push)]
    pub fn build_boot_sequence(mode: BootMode) -> Vec<BootStep> {
        let mut steps = Vec::new();

        // Common early stages (all modes).
        steps.push(BootStep {
            stage: BootStage::MountFilesystems,
            description: "Mount essential filesystems (proc, sys, dev, tmp)".into(),
            required: true,
            timeout_ms: 2000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        steps.push(BootStep {
            stage: BootStage::StartDeviceManager,
            description: "Start udev device manager".into(),
            required: true,
            timeout_ms: 3000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        steps.push(BootStep {
            stage: BootStage::VerifyRootfs,
            description: "Verify rootfs integrity via dm-verity".into(),
            required: true,
            timeout_ms: 5000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        steps.push(BootStep {
            stage: BootStage::StartSecurity,
            description: "Initialize Landlock, seccomp, and MAC policies".into(),
            required: true,
            timeout_ms: 2000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });
        // Edge mode: enforce dm-verity, TPM attestation, then straight to
        // agent-runtime. No databases, no LLM gateway, no compositor/shell.
        if mode == BootMode::Edge {
            steps.push(BootStep {
                stage: BootStage::StartAgentRuntime,
                description: "Start daimon (agent-runtime) in edge mode on port 8090".into(),
                required: true,
                timeout_ms: 3000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            steps.push(BootStep {
                stage: BootStage::BootComplete,
                description: "Edge boot complete — agent ready".into(),
                required: true,
                timeout_ms: 500,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            return steps;
        }

        // Server and Desktop get database services.
        if mode == BootMode::Server || mode == BootMode::Desktop {
            steps.push(BootStep {
                stage: BootStage::StartDatabaseServices,
                description: "Start PostgreSQL and Redis database services".into(),
                required: true,
                timeout_ms: 15_000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
        }

        steps.push(BootStep {
            stage: BootStage::StartAgentRuntime,
            description: "Start daimon (agent-runtime) on port 8090".into(),
            required: true,
            timeout_ms: 5000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });

        // Server and Desktop get llm-gateway.
        if mode == BootMode::Server || mode == BootMode::Desktop {
            steps.push(BootStep {
                stage: BootStage::StartLlmGateway,
                description: "Start hoosh (llm-gateway) on port 8088".into(),
                required: false,
                timeout_ms: 5000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            steps.push(BootStep {
                stage: BootStage::StartModelServices,
                description: "Start Synapse LLM model manager".into(),
                required: false,
                timeout_ms: 15_000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
        }

        // Desktop-only stages.
        if mode == BootMode::Desktop {
            steps.push(BootStep {
                stage: BootStage::StartCompositor,
                description: "Start aethersafha (Wayland compositor)".into(),
                required: true,
                timeout_ms: 5000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
            steps.push(BootStep {
                stage: BootStage::StartShell,
                description: "Start agnoshi (AI shell)".into(),
                required: false,
                timeout_ms: 3000,
                status: BootStepStatus::Pending,
                started_at: None,
                completed_at: None,
                error: None,
            });
        }

        // Final stage.
        steps.push(BootStep {
            stage: BootStage::BootComplete,
            description: "All boot stages finished".into(),
            required: true,
            timeout_ms: 1000,
            status: BootStepStatus::Pending,
            started_at: None,
            completed_at: None,
            error: None,
        });

        steps
    }
}
