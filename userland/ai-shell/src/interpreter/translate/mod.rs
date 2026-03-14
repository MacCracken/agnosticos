mod aequi;
mod agnos;
mod agnostic;
mod bullshift;
mod delta;
mod edge;
mod filesystem;
mod knowledge;
mod marketplace;
mod misc;
mod mneme;
mod network;
mod package;
mod photis;
mod process;
mod rasa;
mod shruti;
mod synapse;
mod system;
mod tazama;
mod yeoman;

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
            | Intent::AequiReceipts { .. }
            | Intent::AequiInvoices { .. }
            | Intent::AequiReports { .. } => aequi::translate_aequi(intent),

            // Agnostic QA platform
            Intent::AgnosticRunSuite { .. }
            | Intent::AgnosticTestStatus { .. }
            | Intent::AgnosticTestReport { .. }
            | Intent::AgnosticListSuites { .. }
            | Intent::AgnosticAgentStatus { .. }
            | Intent::AgnosticRunCrew { .. }
            | Intent::AgnosticCrewStatus { .. }
            | Intent::AgnosticListPresets { .. }
            | Intent::AgnosticListDefinitions { .. }
            | Intent::AgnosticCreateAgent { .. }
            | Intent::AgnosticCoverage { .. }
            | Intent::AgnosticSchedule { .. } => agnostic::translate_agnostic(intent),

            // Delta code hosting
            Intent::DeltaCreateRepo { .. }
            | Intent::DeltaListRepos
            | Intent::DeltaPr { .. }
            | Intent::DeltaPush { .. }
            | Intent::DeltaCiStatus { .. }
            | Intent::DeltaBranches { .. }
            | Intent::DeltaReview { .. } => delta::translate_delta(intent),

            // Edge fleet management
            Intent::EdgeListNodes { .. }
            | Intent::EdgeDeploy { .. }
            | Intent::EdgeUpdate { .. }
            | Intent::EdgeHealth { .. }
            | Intent::EdgeDecommission { .. }
            | Intent::EdgeLogs { .. }
            | Intent::EdgeConfig { .. } => edge::translate_edge(intent),

            // Shruti DAW
            Intent::ShrutiSession { .. }
            | Intent::ShrutiTrack { .. }
            | Intent::ShrutiMixer { .. }
            | Intent::ShrutiTransport { .. }
            | Intent::ShrutiPlugins { .. }
            | Intent::ShrutiAi { .. }
            | Intent::ShrutiExport { .. } => shruti::translate_shruti(intent),

            // Tazama video editor
            Intent::TazamaProject { .. }
            | Intent::TazamaTimeline { .. }
            | Intent::TazamaEffects { .. }
            | Intent::TazamaAi { .. }
            | Intent::TazamaMedia { .. }
            | Intent::TazamaSubtitles { .. }
            | Intent::TazamaExport { .. } => tazama::translate_tazama(intent),

            // Rasa image editor
            Intent::RasaCanvas { .. }
            | Intent::RasaLayers { .. }
            | Intent::RasaTools { .. }
            | Intent::RasaAi { .. }
            | Intent::RasaBatch { .. }
            | Intent::RasaTemplates { .. }
            | Intent::RasaExport { .. } => rasa::translate_rasa(intent),

            // Mneme knowledge base
            Intent::MnemeNotebook { .. }
            | Intent::MnemeNotes { .. }
            | Intent::MnemeSearch { .. }
            | Intent::MnemeAi { .. }
            | Intent::MnemeImport { .. }
            | Intent::MnemeTags { .. }
            | Intent::MnemeGraph { .. } => mneme::translate_mneme(intent),

            // Synapse LLM management
            Intent::SynapseModels { .. }
            | Intent::SynapseServe { .. }
            | Intent::SynapseFinetune { .. }
            | Intent::SynapseChat { .. }
            | Intent::SynapseStatus
            | Intent::SynapseBenchmark { .. }
            | Intent::SynapseQuantize { .. } => synapse::translate_synapse(intent),

            // BullShift trading
            Intent::BullShiftPortfolio { .. }
            | Intent::BullShiftOrders { .. }
            | Intent::BullShiftMarket { .. }
            | Intent::BullShiftAlerts { .. }
            | Intent::BullShiftStrategy { .. }
            | Intent::BullShiftAccounts { .. }
            | Intent::BullShiftHistory { .. } => bullshift::translate_bullshift(intent),

            // SecureYeoman AI platform
            Intent::YeomanAgents { .. }
            | Intent::YeomanTasks { .. }
            | Intent::YeomanTools { .. }
            | Intent::YeomanIntegrations { .. }
            | Intent::YeomanStatus
            | Intent::YeomanLogs { .. }
            | Intent::YeomanWorkflows { .. } => yeoman::translate_yeoman(intent),

            // Photis Nadi
            Intent::TaskList { .. }
            | Intent::TaskCreate { .. }
            | Intent::TaskUpdate { .. }
            | Intent::RitualCheck { .. }
            | Intent::ProductivityStats { .. }
            | Intent::PhotoisBoards { .. }
            | Intent::PhotoisNotes { .. } => photis::translate_photis(intent),

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
