pub mod agent;
pub mod http_api;
pub mod ipc;
pub mod lifecycle;
pub mod orchestrator;
pub mod package_manager;
pub mod pubsub;
pub mod registry;
pub mod resource;
pub mod rollback;
pub mod sandbox;
pub mod seccomp_profiles;
pub mod service_manager;
pub mod supervisor;
pub mod network_tools;
pub mod tool_analysis;
pub mod swarm;
pub mod learning;
pub mod memory_store;
pub mod multimodal;
pub mod wasm_runtime;
pub mod vector_store;
pub mod rag;
pub mod knowledge_base;
pub mod file_watcher;
pub mod capability;
pub mod resource_forecast;
pub mod mtls;
pub mod integrity;
pub mod aegis;
pub mod agnova;
pub mod ark;
pub mod marketplace;
pub mod mcp_server;
pub mod nous;
pub mod sigil;
pub mod takumi;
pub mod argonaut;

pub use agent::{Agent, AgentHandle};
pub use lifecycle::LifecycleManager;
pub use orchestrator::Orchestrator;
pub use pubsub::TopicBroker;
pub use package_manager::PackageManager;
pub use registry::AgentRegistry;
pub use rollback::RollbackManager;
pub use service_manager::ServiceManager;
pub use supervisor::Supervisor;
pub use swarm::SwarmCoordinator;
pub use learning::AgentLearner;
pub use memory_store::AgentMemoryStore;
pub use multimodal::ModalityRegistry;
pub use vector_store::VectorIndex;
pub use rag::{RagPipeline, RagConfig};
pub use knowledge_base::KnowledgeBase;
pub use file_watcher::FileWatcher;
pub use ipc::{RpcRegistry, RpcRouter, RpcRequest, RpcResponse};
pub use learning::{AnomalyDetector, BehaviorSample, AnomalyAlert, AnomalySeverity};
pub use marketplace::{
    MarketplaceCategory, MarketplaceManifest, PublisherInfo,
    DependencyGraph, DepNode,
    trust::{PublisherKeyring, KeyVersion},
    transparency::TransparencyLog,
    local_registry::LocalRegistry,
    remote_client::RegistryClient,
    flutter_agpkg::{FlutterBuildDir, PackFlutterConfig, SandboxProfile, LandlockRule, NetworkRule},
    sandbox_profiles::{SandboxPreset, PredefinedProfile},
};
pub use nous::{
    NousResolver, PackageSource, ResolvedPackage, InstalledPackage,
    AvailableUpdate, UnifiedSearchResult, SystemPackageDb,
};
pub use ark::{ArkPackageManager, ArkCommand, ArkConfig, ArkResult, ArkOutput, InstallPlan, InstallStep};
pub use aegis::{
    AegisSecurityDaemon, AegisConfig, AegisStats,
    ThreatLevel, SecurityEvent, SecurityEventType,
    QuarantineEntry, QuarantineAction,
    SecurityScanResult, ScanType, SecurityFinding,
};
pub use sigil::{
    SigilVerifier, TrustLevel, TrustPolicy, TrustEnforcement, ArtifactType,
    TrustedArtifact, VerificationResult, TrustCheck, RevocationEntry, RevocationList, SigilStats,
};
pub use takumi::{
    TakumiBuildSystem, BuildRecipe, PackageMetadata, SourceSpec, DependencySpec,
    BuildSteps, SecurityFlags, HardeningFlag, ArkPackage, ArkManifest, ArkFileEntry,
    ArkFileType, BuildContext, BuildStatus, BuildLogEntry,
};
pub use agnova::{
    AgnovaInstaller, InstallConfig, InstallMode, InstallPhase, InstallProgress,
    InstallResult, InstallError, DiskLayout, PartitionSpec, Filesystem, PartitionFlag,
    BootloaderConfig, BootloaderType, NetworkConfig, UserConfig, SecurityConfig,
    PackageSelection,
};
pub use argonaut::{
    ArgonautInit, ArgonautConfig, ArgonautStats, BootMode, BootStage, BootStep,
    BootStepStatus, ServiceDefinition, ServiceState, ManagedService, RestartPolicy,
    HealthCheck, HealthCheckType, ReadyCheck,
};
