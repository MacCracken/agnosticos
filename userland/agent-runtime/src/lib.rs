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
pub mod marketplace;
pub mod mcp_server;

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
};
