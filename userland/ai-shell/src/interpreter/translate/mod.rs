mod aequi;
mod agnos;
mod agnostic;
mod delta;
mod filesystem;
mod knowledge;
mod marketplace;
mod misc;
mod network;
mod package;
mod photis;
mod process;
mod system;

use anyhow::{anyhow, Result};

use super::intent::{Intent, Translation};
use super::Interpreter;

impl Interpreter {
    /// Translate intent into shell command
    pub fn translate(&self, intent: &Intent) -> Result<Translation> {
        match intent {
            // Filesystem operations
            Intent::ListFiles { .. }
            | Intent::ShowFile { .. }
            | Intent::ChangeDirectory { .. }
            | Intent::CreateDirectory { .. }
            | Intent::Copy { .. }
            | Intent::Move { .. }
            | Intent::FindFiles { .. }
            | Intent::SearchContent { .. }
            | Intent::Remove { .. } => filesystem::translate_filesystem(intent),

            // Process / system info / network info / disk
            Intent::ShowProcesses
            | Intent::SystemInfo
            | Intent::KillProcess { .. }
            | Intent::DiskUsage { .. }
            | Intent::NetworkInfo => process::translate_process(intent),

            // Network scanning
            Intent::NetworkScan { .. } => network::translate_network(intent),

            // AGNOS agent/audit/service
            Intent::AuditView { .. } | Intent::AgentInfo { .. } | Intent::ServiceControl { .. } => {
                agnos::translate_agnos(intent)
            }

            // System: journal, device, mount, boot, update
            Intent::JournalView { .. }
            | Intent::DeviceInfo { .. }
            | Intent::MountControl { .. }
            | Intent::BootConfig { .. }
            | Intent::SystemUpdate { .. } => system::translate_system(intent),

            // Knowledge / RAG
            Intent::KnowledgeSearch { .. } | Intent::RagQuery { .. } => {
                knowledge::translate_knowledge(intent)
            }

            // Package management
            Intent::InstallPackage { .. }
            | Intent::ArkInstall { .. }
            | Intent::ArkRemove { .. }
            | Intent::ArkSearch { .. }
            | Intent::ArkInfo { .. }
            | Intent::ArkUpdate
            | Intent::ArkUpgrade { .. }
            | Intent::ArkStatus => package::translate_package(intent),

            // Marketplace
            Intent::MarketplaceInstall { .. }
            | Intent::MarketplaceUninstall { .. }
            | Intent::MarketplaceSearch { .. }
            | Intent::MarketplaceList
            | Intent::MarketplaceUpdate => marketplace::translate_marketplace(intent),

            // Aequi accounting
            Intent::AequiTaxEstimate { .. }
            | Intent::AequiScheduleC { .. }
            | Intent::AequiImportBank { .. }
            | Intent::AequiBalance
            | Intent::AequiReceipts { .. } => aequi::translate_aequi(intent),

            // Agnostic QA platform
            Intent::AgnosticRunSuite { .. }
            | Intent::AgnosticTestStatus { .. }
            | Intent::AgnosticTestReport { .. }
            | Intent::AgnosticListSuites { .. }
            | Intent::AgnosticAgentStatus { .. } => agnostic::translate_agnostic(intent),

            // Delta code hosting
            Intent::DeltaCreateRepo { .. }
            | Intent::DeltaListRepos
            | Intent::DeltaPr { .. }
            | Intent::DeltaPush { .. }
            | Intent::DeltaCiStatus { .. } => delta::translate_delta(intent),

            // Photis Nadi
            Intent::TaskList { .. }
            | Intent::TaskCreate { .. }
            | Intent::TaskUpdate { .. }
            | Intent::RitualCheck { .. }
            | Intent::ProductivityStats { .. } => photis::translate_photis(intent),

            // Shell / pipeline
            Intent::ShellCommand { .. } | Intent::Pipeline { .. } => misc::translate_misc(intent),

            // Error cases
            Intent::Ambiguous { alternatives } => Err(anyhow!(
                "Ambiguous request. Did you mean one of: {}?",
                alternatives.join(", ")
            )),

            Intent::Question { query: _ } => Err(anyhow!(
                "Questions should be handled by LLM, not translated to commands"
            )),

            Intent::Unknown => Err(anyhow!("Cannot translate unknown intent")),
        }
    }
}
