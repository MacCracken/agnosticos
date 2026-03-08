use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use uuid::Uuid;

use agnos_common::{audit::AuditChain, telemetry::SpanCollector};

use crate::ipc::RpcRegistry;
use crate::knowledge_base::KnowledgeBase;
use crate::learning::AnomalyDetector;
use crate::rag::{RagConfig, RagPipeline};

use super::handlers::audit::AuditEvent;
use super::handlers::dashboard::StoredDashboardSnapshot;
use super::handlers::profiles::EnvironmentProfile;
use super::handlers::reasoning::StoredReasoningTrace;
use super::handlers::webhooks::WebhookRegistration;
use super::types::AgentDetail;

#[derive(Debug, Clone)]
pub struct RegisteredAgentEntry {
    pub detail: AgentDetail,
}

/// In-memory per-agent key-value store for the REST API bridge.
/// Maps agent_id -> key -> value.
#[derive(Debug, Clone, Default)]
pub struct ApiMemoryStore {
    data: Arc<RwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl ApiMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, agent_id: &str, key: &str) -> Option<serde_json::Value> {
        let data = self.data.read().await;
        data.get(agent_id).and_then(|m| m.get(key).cloned())
    }

    pub async fn set(&self, agent_id: &str, key: &str, value: serde_json::Value) {
        let mut data = self.data.write().await;
        data.entry(agent_id.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    pub async fn delete(&self, agent_id: &str, key: &str) -> bool {
        let mut data = self.data.write().await;
        if let Some(agent_map) = data.get_mut(agent_id) {
            agent_map.remove(key).is_some()
        } else {
            false
        }
    }

    pub async fn list_keys(&self, agent_id: &str) -> Vec<String> {
        let data = self.data.read().await;
        data.get(agent_id)
            .map(|m| {
                let mut keys: Vec<String> = m.keys().cloned().collect();
                keys.sort();
                keys
            })
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct ApiState {
    agents: Arc<RwLock<HashMap<Uuid, RegisteredAgentEntry>>>,
    started_at: DateTime<Utc>,
    pub webhooks: Arc<RwLock<Vec<WebhookRegistration>>>,
    pub audit_buffer: Arc<RwLock<VecDeque<AuditEvent>>>,
    pub audit_chain: Arc<RwLock<AuditChain>>,
    pub memory_store: ApiMemoryStore,
    pub traces: Arc<RwLock<VecDeque<serde_json::Value>>>,
    pub rag_pipeline: Arc<RwLock<RagPipeline>>,
    pub knowledge_base: Arc<RwLock<KnowledgeBase>>,
    /// Distributed tracing span collector (OpenTelemetry-like).
    pub span_collector: Arc<SpanCollector>,
    /// Agent-to-agent RPC method registry.
    pub rpc_registry: Arc<RwLock<RpcRegistry>>,
    /// Behavior anomaly detector for agent monitoring.
    pub anomaly_detector: Arc<RwLock<AnomalyDetector>>,
    /// Marketplace local registry for package management.
    pub marketplace_registry: Arc<RwLock<crate::marketplace::local_registry::LocalRegistry>>,
    /// Per-agent reasoning trace store (agent_id -> traces).
    pub reasoning_traces: Arc<RwLock<HashMap<String, VecDeque<StoredReasoningTrace>>>>,
    /// Dashboard sync snapshots from external consumers.
    pub dashboard_snapshots: Arc<RwLock<VecDeque<StoredDashboardSnapshot>>>,
    /// Named environment profiles (dev/staging/prod).
    pub environment_profiles: Arc<RwLock<HashMap<String, EnvironmentProfile>>>,
    /// Named vector collections for semantic search.
    pub vector_collections: Arc<RwLock<HashMap<String, crate::vector_store::VectorIndex>>>,
    /// Optional Bearer token for API authentication.
    /// When `Some`, all endpoints except `GET /v1/health` require it.
    pub api_key: Option<String>,
}

impl std::fmt::Debug for ApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiState")
            .field("started_at", &self.started_at)
            .finish()
    }
}

impl ApiState {
    pub fn new() -> Self {
        let api_key = std::env::var("AGNOS_RUNTIME_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        let marketplace_dir = std::env::var("AGNOS_MARKETPLACE_DIR").unwrap_or_else(|_| {
            crate::marketplace::local_registry::DEFAULT_MARKETPLACE_DIR.to_string()
        });
        let marketplace_registry = crate::marketplace::local_registry::LocalRegistry::new(
            std::path::Path::new(&marketplace_dir),
        )
        .unwrap_or_else(|_| {
            // Fallback to temp dir if default path is not writable
            crate::marketplace::local_registry::LocalRegistry::new(
                &std::env::temp_dir().join("agnos-marketplace"),
            )
            .expect("Failed to create marketplace registry")
        });
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            started_at: Utc::now(),
            webhooks: Arc::new(RwLock::new(Vec::new())),
            audit_buffer: Arc::new(RwLock::new(VecDeque::new())),
            audit_chain: Arc::new(RwLock::new(AuditChain::new())),
            memory_store: ApiMemoryStore::new(),
            traces: Arc::new(RwLock::new(VecDeque::new())),
            rag_pipeline: Arc::new(RwLock::new(RagPipeline::new(RagConfig::default()))),
            knowledge_base: Arc::new(RwLock::new(KnowledgeBase::new())),
            span_collector: Arc::new(SpanCollector::new()),
            rpc_registry: Arc::new(RwLock::new(RpcRegistry::new())),
            anomaly_detector: Arc::new(RwLock::new(AnomalyDetector::new(100, 2.0))),
            marketplace_registry: Arc::new(RwLock::new(marketplace_registry)),
            reasoning_traces: Arc::new(RwLock::new(HashMap::new())),
            dashboard_snapshots: Arc::new(RwLock::new(VecDeque::new())),
            environment_profiles: Arc::new(RwLock::new(
                super::handlers::profiles::default_profiles(),
            )),
            vector_collections: Arc::new(RwLock::new(HashMap::new())),
            api_key,
        }
    }

    /// Create a new `ApiState` with an explicit API key (useful for testing).
    pub fn with_api_key(api_key: Option<String>) -> Self {
        let tmp_marketplace = crate::marketplace::local_registry::LocalRegistry::new(
            &std::env::temp_dir().join("agnos-marketplace-test"),
        )
        .expect("Failed to create test marketplace registry");
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            started_at: Utc::now(),
            webhooks: Arc::new(RwLock::new(Vec::new())),
            audit_buffer: Arc::new(RwLock::new(VecDeque::new())),
            audit_chain: Arc::new(RwLock::new(AuditChain::new())),
            memory_store: ApiMemoryStore::new(),
            traces: Arc::new(RwLock::new(VecDeque::new())),
            rag_pipeline: Arc::new(RwLock::new(RagPipeline::new(RagConfig::default()))),
            knowledge_base: Arc::new(RwLock::new(KnowledgeBase::new())),
            span_collector: Arc::new(SpanCollector::new()),
            rpc_registry: Arc::new(RwLock::new(RpcRegistry::new())),
            anomaly_detector: Arc::new(RwLock::new(AnomalyDetector::new(100, 2.0))),
            marketplace_registry: Arc::new(RwLock::new(tmp_marketplace)),
            reasoning_traces: Arc::new(RwLock::new(HashMap::new())),
            dashboard_snapshots: Arc::new(RwLock::new(VecDeque::new())),
            environment_profiles: Arc::new(RwLock::new(
                super::handlers::profiles::default_profiles(),
            )),
            vector_collections: Arc::new(RwLock::new(HashMap::new())),
            api_key,
        }
    }

    /// Acquire a read lock on the agents map.
    pub async fn agents_read(
        &self,
    ) -> tokio::sync::RwLockReadGuard<'_, HashMap<Uuid, RegisteredAgentEntry>> {
        self.agents.read().await
    }

    /// Acquire a write lock on the agents map.
    pub async fn agents_write(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<Uuid, RegisteredAgentEntry>> {
        self.agents.write().await
    }

    /// Return the instant the API state was created.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }
}

impl Default for ApiState {
    fn default() -> Self {
        Self::new()
    }
}
