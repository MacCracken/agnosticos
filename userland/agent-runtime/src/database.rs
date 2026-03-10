//! Database Manager for AGNOS Agent Runtime
//!
//! Manages per-agent PostgreSQL databases and shared Redis connections.
//! Each agent can request an isolated database schema and a namespaced
//! Redis key prefix, provisioned automatically during agent startup.
//!
//! This module provides the in-process management layer. Actual database
//! servers (PostgreSQL, Redis) run as system services managed by argonaut.

use std::collections::HashMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use agnos_common::AgentId;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Database manager configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection string for the management connection.
    /// This connection is used to create/drop per-agent databases.
    #[serde(default = "default_postgres_url")]
    pub postgres_url: String,

    /// Redis connection string for shared cache.
    #[serde(default = "default_redis_url")]
    pub redis_url: String,

    /// Maximum number of per-agent PostgreSQL databases.
    #[serde(default = "default_max_databases")]
    pub max_databases: usize,

    /// Maximum number of Redis connections (across all agents).
    #[serde(default = "default_max_redis_connections")]
    pub max_redis_connections: usize,

    /// Default storage quota per agent database (bytes).
    #[serde(default = "default_storage_quota")]
    pub default_storage_quota: u64,
}

fn default_postgres_url() -> String {
    "postgresql://agnos@localhost/agnos".to_string()
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}

fn default_max_databases() -> usize {
    100
}

fn default_max_redis_connections() -> usize {
    50
}

fn default_storage_quota() -> u64 {
    512 * 1024 * 1024 // 512 MB
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            postgres_url: default_postgres_url(),
            redis_url: default_redis_url(),
            max_databases: default_max_databases(),
            max_redis_connections: default_max_redis_connections(),
            default_storage_quota: default_storage_quota(),
        }
    }
}

// ---------------------------------------------------------------------------
// Agent database requirements (from agent manifest metadata)
// ---------------------------------------------------------------------------

/// Database requirements declared in an agent's manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentDatabaseRequirements {
    /// Whether the agent needs a PostgreSQL database.
    #[serde(default)]
    pub postgres: bool,

    /// Whether the agent needs Redis access.
    #[serde(default)]
    pub redis: bool,

    /// Optional custom schema name (defaults to `agent_{id}`).
    #[serde(default)]
    pub schema: Option<String>,

    /// Storage quota override (bytes). Uses default if not set.
    #[serde(default)]
    pub storage_quota: Option<u64>,

    /// PostgreSQL extensions to enable (e.g. `["vector", "uuid-ossp"]`).
    #[serde(default)]
    pub extensions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Provisioned database info
// ---------------------------------------------------------------------------

/// Information about a provisioned per-agent database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionedDatabase {
    /// Agent that owns this database.
    pub agent_id: AgentId,

    /// PostgreSQL connection URL for the agent.
    /// Format: `postgresql://agent_{id}@localhost/agent_{id}`
    pub postgres_url: Option<String>,

    /// PostgreSQL database name.
    pub postgres_database: Option<String>,

    /// PostgreSQL schema name.
    pub postgres_schema: Option<String>,

    /// Redis key prefix for this agent.
    /// Format: `agent:{id}:`
    pub redis_prefix: Option<String>,

    /// Storage quota (bytes).
    pub storage_quota: u64,

    /// Extensions enabled.
    pub extensions: Vec<String>,

    /// When the database was provisioned.
    pub provisioned_at: String,
}

// ---------------------------------------------------------------------------
// Database manager
// ---------------------------------------------------------------------------

/// Manages per-agent database provisioning and lifecycle.
///
/// In the current implementation, this tracks provisioning state in-memory
/// and generates the SQL/commands needed. Actual execution requires a live
/// PostgreSQL/Redis connection (handled at integration layer).
pub struct DatabaseManager {
    config: DatabaseConfig,
    provisioned: HashMap<AgentId, ProvisionedDatabase>,
}

impl Default for DatabaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseManager {
    /// Create a new database manager with default configuration.
    pub fn new() -> Self {
        Self {
            config: DatabaseConfig::default(),
            provisioned: HashMap::new(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: DatabaseConfig) -> Self {
        Self {
            config,
            provisioned: HashMap::new(),
        }
    }

    /// Provision database resources for an agent.
    ///
    /// Returns the provisioned database info including connection URLs.
    /// The caller is responsible for executing the generated SQL against
    /// the actual PostgreSQL/Redis instances.
    pub fn provision(
        &mut self,
        agent_id: AgentId,
        requirements: &AgentDatabaseRequirements,
    ) -> Result<ProvisionedDatabase> {
        if self.provisioned.contains_key(&agent_id) {
            bail!("Database already provisioned for agent {}", agent_id);
        }

        if self.provisioned.len() >= self.config.max_databases {
            bail!(
                "Maximum database limit reached ({})",
                self.config.max_databases
            );
        }

        let db_name = Self::database_name(&agent_id);
        let schema = requirements
            .schema
            .clone()
            .unwrap_or_else(|| "public".to_string());
        let quota = requirements
            .storage_quota
            .unwrap_or(self.config.default_storage_quota);

        let postgres_url = if requirements.postgres {
            Some(format!("postgresql://{}@localhost/{}", db_name, db_name))
        } else {
            None
        };

        let redis_prefix = if requirements.redis {
            Some(format!("agent:{}:", agent_id))
        } else {
            None
        };

        let provisioned = ProvisionedDatabase {
            agent_id,
            postgres_url,
            postgres_database: if requirements.postgres {
                Some(db_name.clone())
            } else {
                None
            },
            postgres_schema: if requirements.postgres {
                Some(schema)
            } else {
                None
            },
            redis_prefix,
            storage_quota: quota,
            extensions: requirements.extensions.clone(),
            provisioned_at: chrono::Utc::now().to_rfc3339(),
        };

        info!(
            agent_id = %agent_id,
            postgres = requirements.postgres,
            redis = requirements.redis,
            "Provisioned database resources"
        );

        self.provisioned.insert(agent_id, provisioned.clone());
        Ok(provisioned)
    }

    /// Deprovision database resources for an agent.
    ///
    /// Returns the SQL commands needed to clean up. The caller is
    /// responsible for executing them.
    pub fn deprovision(&mut self, agent_id: &AgentId) -> Result<Vec<String>> {
        let info = match self.provisioned.remove(agent_id) {
            Some(info) => info,
            None => {
                warn!(agent_id = %agent_id, "No database provisioned for agent");
                return Ok(vec![]);
            }
        };

        let mut commands = Vec::new();

        if let Some(ref db_name) = info.postgres_database {
            // Terminate connections, then drop database and role
            commands.push(format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}';",
                db_name
            ));
            commands.push(format!("DROP DATABASE IF EXISTS \"{}\";", db_name));
            commands.push(format!("DROP ROLE IF EXISTS \"{}\";", db_name));
        }

        info!(
            agent_id = %agent_id,
            commands = commands.len(),
            "Deprovisioned database resources"
        );

        Ok(commands)
    }

    /// Generate SQL commands to create a per-agent database.
    ///
    /// These commands should be executed by a PostgreSQL superuser.
    pub fn provision_sql(&self, agent_id: &AgentId) -> Result<Vec<String>> {
        let info = self
            .provisioned
            .get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("Agent {} not provisioned", agent_id))?;

        let mut commands = Vec::new();

        if let Some(ref db_name) = info.postgres_database {
            // Create role (no superuser, no createdb)
            commands.push(format!(
                "CREATE ROLE \"{}\" WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;",
                db_name
            ));

            // Create database owned by the agent role
            commands.push(format!(
                "CREATE DATABASE \"{}\" OWNER \"{}\" ENCODING 'UTF8';",
                db_name, db_name
            ));

            // Enable requested extensions (must be done as superuser)
            for ext in &info.extensions {
                commands.push(format!(
                    "\\c \"{}\"\nCREATE EXTENSION IF NOT EXISTS \"{}\" SCHEMA public;",
                    db_name, ext
                ));
            }

            debug!(
                agent_id = %agent_id,
                database = db_name,
                extensions = ?info.extensions,
                "Generated provisioning SQL"
            );
        }

        Ok(commands)
    }

    /// Get provisioned database info for an agent.
    pub fn get(&self, agent_id: &AgentId) -> Option<&ProvisionedDatabase> {
        self.provisioned.get(agent_id)
    }

    /// List all provisioned databases.
    pub fn list(&self) -> Vec<&ProvisionedDatabase> {
        self.provisioned.values().collect()
    }

    /// Get usage statistics.
    pub fn stats(&self) -> DatabaseStats {
        let postgres_count = self
            .provisioned
            .values()
            .filter(|p| p.postgres_url.is_some())
            .count();
        let redis_count = self
            .provisioned
            .values()
            .filter(|p| p.redis_prefix.is_some())
            .count();

        DatabaseStats {
            total_provisioned: self.provisioned.len(),
            postgres_databases: postgres_count,
            redis_namespaces: redis_count,
            max_databases: self.config.max_databases,
            max_redis_connections: self.config.max_redis_connections,
        }
    }

    /// Generate database name from agent ID.
    fn database_name(agent_id: &AgentId) -> String {
        // Use a safe, short identifier derived from agent ID
        let id_str = agent_id.to_string().replace('-', "_");
        format!("agent_{}", &id_str[..8.min(id_str.len())])
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Database subsystem statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub total_provisioned: usize,
    pub postgres_databases: usize,
    pub redis_namespaces: usize,
    pub max_databases: usize,
    pub max_redis_connections: usize,
}

// ---------------------------------------------------------------------------
// PostgreSQL + pgvector vector store backend
// ---------------------------------------------------------------------------

/// PostgreSQL + pgvector vector store backend.
/// Generates SQL for vector operations — caller executes against live connection.
pub struct PostgresVectorBackend {
    database_name: String,
    table_name: String,
    dimension: usize,
}

impl PostgresVectorBackend {
    pub fn new(database_name: &str, table_name: &str, dimension: usize) -> Self {
        Self {
            database_name: database_name.to_string(),
            table_name: table_name.to_string(),
            dimension,
        }
    }

    /// SQL to create the vector table (idempotent).
    pub fn create_table_sql(&self) -> Vec<String> {
        vec![
            "CREATE EXTENSION IF NOT EXISTS vector;".to_string(),
            format!(
                "CREATE TABLE IF NOT EXISTS {} (\
                \n  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),\
                \n  embedding vector({}),\
                \n  content TEXT NOT NULL,\
                \n  metadata JSONB DEFAULT '{{}}',\
                \n  created_at TIMESTAMPTZ DEFAULT NOW()\
                \n);",
                self.table_name, self.dimension
            ),
            format!(
                "CREATE INDEX IF NOT EXISTS {table}_embedding_idx ON {table} \
                USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);",
                table = self.table_name
            ),
        ]
    }

    /// SQL to insert a vector entry. Returns (sql, params_description).
    pub fn insert_sql(&self) -> (String, &'static str) {
        (
            format!(
                "INSERT INTO {} (id, embedding, content, metadata) VALUES ($1, $2, $3, $4)",
                self.table_name
            ),
            "(uuid, vector_string, text, jsonb)",
        )
    }

    /// SQL for nearest-neighbor search. Returns (sql, params_description).
    pub fn search_sql(&self, top_k: usize) -> (String, &'static str) {
        (
            format!(
                "SELECT id, content, metadata, created_at, 1 - (embedding <=> $1) AS similarity \
                FROM {} \
                ORDER BY embedding <=> $1 \
                LIMIT {}",
                self.table_name, top_k
            ),
            "(vector_string)",
        )
    }

    /// SQL to delete a vector entry by ID.
    pub fn delete_sql(&self) -> (String, &'static str) {
        (
            format!("DELETE FROM {} WHERE id = $1", self.table_name),
            "(uuid)",
        )
    }

    /// SQL to drop the vector table.
    pub fn drop_table_sql(&self) -> String {
        format!("DROP TABLE IF EXISTS {}", self.table_name)
    }

    /// Format a vector as pgvector string literal: '[1.0,2.0,3.0]'
    pub fn format_vector(embedding: &[f64]) -> String {
        format!(
            "[{}]",
            embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

// ---------------------------------------------------------------------------
// Redis session/cache store
// ---------------------------------------------------------------------------

/// Redis session/cache store for agents.
/// Generates Redis commands — caller executes against live connection.
pub struct RedisSessionStore {
    prefix: String,
    default_ttl_secs: u64,
}

impl RedisSessionStore {
    pub fn new(agent_prefix: &str, default_ttl_secs: u64) -> Self {
        Self {
            prefix: agent_prefix.to_string(),
            default_ttl_secs,
        }
    }

    /// Create a session store for the given agent with a 1-hour default TTL.
    pub fn from_agent_id(agent_id: &AgentId) -> Self {
        Self::new(&format!("agent:{}:", agent_id), 3600)
    }

    /// Redis SET command with optional TTL.
    pub fn set_command(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> Vec<String> {
        let full_key = format!("{}{}", self.prefix, key);
        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);
        vec![
            "SET".into(),
            full_key,
            value.to_string(),
            "EX".into(),
            ttl.to_string(),
        ]
    }

    /// Redis GET command.
    pub fn get_command(&self, key: &str) -> Vec<String> {
        vec!["GET".into(), format!("{}{}", self.prefix, key)]
    }

    /// Redis DEL command.
    pub fn del_command(&self, key: &str) -> Vec<String> {
        vec!["DEL".into(), format!("{}{}", self.prefix, key)]
    }

    /// Redis KEYS command to list all keys for this agent.
    pub fn list_keys_command(&self) -> Vec<String> {
        vec!["KEYS".into(), format!("{}*", self.prefix)]
    }

    /// Redis HSET for hash maps (e.g., session data).
    pub fn hset_command(&self, key: &str, field: &str, value: &str) -> Vec<String> {
        vec![
            "HSET".into(),
            format!("{}{}", self.prefix, key),
            field.to_string(),
            value.to_string(),
        ]
    }

    /// Redis HGET for hash maps.
    pub fn hget_command(&self, key: &str, field: &str) -> Vec<String> {
        vec![
            "HGET".into(),
            format!("{}{}", self.prefix, key),
            field.to_string(),
        ]
    }

    /// Redis HGETALL for full hash retrieval.
    pub fn hgetall_command(&self, key: &str) -> Vec<String> {
        vec!["HGETALL".into(), format!("{}{}", self.prefix, key)]
    }

    /// Redis EXPIRE to set TTL on a key.
    pub fn expire_command(&self, key: &str, ttl_secs: u64) -> Vec<String> {
        vec![
            "EXPIRE".into(),
            format!("{}{}", self.prefix, key),
            ttl_secs.to_string(),
        ]
    }

    /// Redis PUBLISH for pub/sub messaging between agents.
    pub fn publish_command(&self, channel: &str, message: &str) -> Vec<String> {
        vec![
            "PUBLISH".into(),
            format!("{}{}", self.prefix, channel),
            message.to_string(),
        ]
    }

    /// Cleanup: delete all keys with this agent's prefix using SCAN (safer than KEYS).
    pub fn cleanup_commands(&self) -> Vec<Vec<String>> {
        vec![vec![
            "SCAN".into(),
            "0".into(),
            "MATCH".into(),
            format!("{}*", self.prefix),
            "COUNT".into(),
            "100".into(),
        ]]
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn default_ttl(&self) -> u64 {
        self.default_ttl_secs
    }
}

// ---------------------------------------------------------------------------
// HuggingFace model registry
// ---------------------------------------------------------------------------

/// HuggingFace model registry integration for Synapse.
/// Generates download URLs and manages model metadata — caller handles actual downloads.
pub struct ModelRegistry {
    /// Base directory for model storage.
    model_dir: String,
    /// HuggingFace Hub base URL.
    hub_url: String,
}

impl ModelRegistry {
    pub fn new(model_dir: &str) -> Self {
        Self {
            model_dir: model_dir.to_string(),
            hub_url: "https://huggingface.co".to_string(),
        }
    }

    /// Default registry for AGNOS system Synapse instance.
    pub fn system_default() -> Self {
        Self::new("/var/lib/synapse/models")
    }

    /// Generate the download URL for a HuggingFace model file.
    pub fn hf_download_url(&self, repo_id: &str, filename: &str) -> String {
        format!("{}/{}/resolve/main/{}", self.hub_url, repo_id, filename)
    }

    /// Generate the API URL for model metadata.
    pub fn hf_api_url(&self, repo_id: &str) -> String {
        format!("https://huggingface.co/api/models/{}", repo_id)
    }

    /// Expected local path for a downloaded model.
    pub fn local_model_path(&self, repo_id: &str, filename: &str) -> String {
        format!("{}/{}/{}", self.model_dir, repo_id, filename)
    }

    /// Expected local directory for a model repo.
    pub fn local_repo_dir(&self, repo_id: &str) -> String {
        format!("{}/{}", self.model_dir, repo_id)
    }

    /// Generate a model manifest entry for tracking downloaded models.
    pub fn model_manifest_entry(&self, repo_id: &str, filename: &str, size_bytes: u64) -> serde_json::Value {
        serde_json::json!({
            "repo_id": repo_id,
            "filename": filename,
            "local_path": self.local_model_path(repo_id, filename),
            "size_bytes": size_bytes,
            "hub_url": self.hf_download_url(repo_id, filename),
            "downloaded_at": chrono::Utc::now().to_rfc3339(),
        })
    }

    pub fn model_dir(&self) -> &str { &self.model_dir }
    pub fn hub_url(&self) -> &str { &self.hub_url }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod model_registry_tests {
    use super::*;

    #[test]
    fn hf_download_url_format() {
        let reg = ModelRegistry::new("/models");
        let url = reg.hf_download_url("TheBloke/Llama-2-7B-GGUF", "llama-2-7b.Q4_K_M.gguf");
        assert_eq!(url, "https://huggingface.co/TheBloke/Llama-2-7B-GGUF/resolve/main/llama-2-7b.Q4_K_M.gguf");
    }

    #[test]
    fn hf_api_url_format() {
        let reg = ModelRegistry::new("/models");
        assert_eq!(reg.hf_api_url("meta-llama/Llama-2-7b"), "https://huggingface.co/api/models/meta-llama/Llama-2-7b");
    }

    #[test]
    fn local_model_path() {
        let reg = ModelRegistry::new("/var/lib/synapse/models");
        let path = reg.local_model_path("TheBloke/Llama-2-7B-GGUF", "model.gguf");
        assert_eq!(path, "/var/lib/synapse/models/TheBloke/Llama-2-7B-GGUF/model.gguf");
    }

    #[test]
    fn local_repo_dir() {
        let reg = ModelRegistry::new("/models");
        assert_eq!(reg.local_repo_dir("meta-llama/Llama-2-7b"), "/models/meta-llama/Llama-2-7b");
    }

    #[test]
    fn system_default_uses_synapse_dir() {
        let reg = ModelRegistry::system_default();
        assert_eq!(reg.model_dir(), "/var/lib/synapse/models");
    }

    #[test]
    fn model_manifest_entry_has_required_fields() {
        let reg = ModelRegistry::new("/models");
        let entry = reg.model_manifest_entry("org/model", "weights.bin", 1024);
        assert_eq!(entry["repo_id"], "org/model");
        assert_eq!(entry["filename"], "weights.bin");
        assert_eq!(entry["size_bytes"], 1024);
        assert!(entry["downloaded_at"].as_str().is_some());
        assert!(entry["hub_url"].as_str().unwrap().contains("huggingface.co"));
        assert!(entry["local_path"].as_str().unwrap().contains("/models/org/model/weights.bin"));
    }
}

#[cfg(test)]
mod pgvector_tests {
    use super::*;

    #[test]
    fn pgvector_create_table_sql() {
        let backend = PostgresVectorBackend::new("test_db", "embeddings", 1536);
        let sql = backend.create_table_sql();
        assert_eq!(sql.len(), 3);
        assert!(sql[0].contains("CREATE EXTENSION IF NOT EXISTS vector"));
        assert!(sql[1].contains("CREATE TABLE IF NOT EXISTS embeddings"));
        assert!(sql[1].contains("vector(1536)"));
        assert!(sql[1].contains("content TEXT NOT NULL"));
        assert!(sql[1].contains("JSONB"));
        assert!(sql[2].contains("CREATE INDEX IF NOT EXISTS embeddings_embedding_idx"));
        assert!(sql[2].contains("ivfflat"));
        assert!(sql[2].contains("vector_cosine_ops"));
    }

    #[test]
    fn pgvector_insert_sql() {
        let backend = PostgresVectorBackend::new("test_db", "embeddings", 768);
        let (sql, params) = backend.insert_sql();
        assert!(sql.contains("INSERT INTO embeddings"));
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(sql.contains("$3"));
        assert!(sql.contains("$4"));
        assert!(params.contains("uuid"));
        assert!(params.contains("vector_string"));
    }

    #[test]
    fn pgvector_search_sql() {
        let backend = PostgresVectorBackend::new("test_db", "embeddings", 1536);
        let (sql, params) = backend.search_sql(10);
        assert!(sql.contains("ORDER BY embedding <=> $1"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("similarity"));
        assert!(params.contains("vector_string"));
    }

    #[test]
    fn pgvector_delete_sql() {
        let backend = PostgresVectorBackend::new("test_db", "embeddings", 1536);
        let (sql, params) = backend.delete_sql();
        assert!(sql.contains("DELETE FROM embeddings WHERE id = $1"));
        assert!(params.contains("uuid"));
    }

    #[test]
    fn pgvector_format_vector() {
        let vec = PostgresVectorBackend::format_vector(&[1.0, 2.5, 3.0]);
        assert_eq!(vec, "[1,2.5,3]");
    }

    #[test]
    fn pgvector_drop_table() {
        let backend = PostgresVectorBackend::new("test_db", "embeddings", 1536);
        let sql = backend.drop_table_sql();
        assert_eq!(sql, "DROP TABLE IF EXISTS embeddings");
    }

    #[test]
    fn pgvector_custom_table_name() {
        let backend = PostgresVectorBackend::new("mydb", "custom_vectors", 256);
        assert_eq!(backend.table_name(), "custom_vectors");
        let (sql, _) = backend.insert_sql();
        assert!(sql.contains("custom_vectors"));
        let create = backend.create_table_sql();
        assert!(create[1].contains("custom_vectors"));
        assert!(create[2].contains("custom_vectors_embedding_idx"));
    }

    #[test]
    fn pgvector_dimension() {
        let backend = PostgresVectorBackend::new("db", "t", 384);
        assert_eq!(backend.dimension(), 384);
    }
}

#[cfg(test)]
mod redis_tests {
    use super::*;
    use uuid::Uuid;

    fn test_store() -> RedisSessionStore {
        RedisSessionStore::new("agent:test:", 3600)
    }

    #[test]
    fn redis_set_with_default_ttl() {
        let store = test_store();
        let cmd = store.set_command("session", "data123", None);
        assert_eq!(cmd, vec!["SET", "agent:test:session", "data123", "EX", "3600"]);
    }

    #[test]
    fn redis_set_with_custom_ttl() {
        let store = test_store();
        let cmd = store.set_command("session", "data123", Some(60));
        assert_eq!(cmd, vec!["SET", "agent:test:session", "data123", "EX", "60"]);
    }

    #[test]
    fn redis_get_command() {
        let store = test_store();
        let cmd = store.get_command("session");
        assert_eq!(cmd, vec!["GET", "agent:test:session"]);
    }

    #[test]
    fn redis_del_command() {
        let store = test_store();
        let cmd = store.del_command("session");
        assert_eq!(cmd, vec!["DEL", "agent:test:session"]);
    }

    #[test]
    fn redis_list_keys() {
        let store = test_store();
        let cmd = store.list_keys_command();
        assert_eq!(cmd, vec!["KEYS", "agent:test:*"]);
    }

    #[test]
    fn redis_hset_hget() {
        let store = test_store();
        let hset = store.hset_command("sess", "user", "alice");
        assert_eq!(hset, vec!["HSET", "agent:test:sess", "user", "alice"]);
        let hget = store.hget_command("sess", "user");
        assert_eq!(hget, vec!["HGET", "agent:test:sess", "user"]);
    }

    #[test]
    fn redis_hgetall() {
        let store = test_store();
        let cmd = store.hgetall_command("sess");
        assert_eq!(cmd, vec!["HGETALL", "agent:test:sess"]);
    }

    #[test]
    fn redis_expire() {
        let store = test_store();
        let cmd = store.expire_command("key1", 120);
        assert_eq!(cmd, vec!["EXPIRE", "agent:test:key1", "120"]);
    }

    #[test]
    fn redis_publish() {
        let store = test_store();
        let cmd = store.publish_command("events", "hello");
        assert_eq!(cmd, vec!["PUBLISH", "agent:test:events", "hello"]);
    }

    #[test]
    fn redis_cleanup() {
        let store = test_store();
        let cmds = store.cleanup_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0][0], "SCAN");
        assert!(cmds[0].contains(&"agent:test:*".to_string()));
    }

    #[test]
    fn redis_prefix_format() {
        let store = test_store();
        assert_eq!(store.prefix(), "agent:test:");
        assert_eq!(store.default_ttl(), 3600);
    }

    #[test]
    fn redis_from_agent_id() {
        let id = AgentId(Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap());
        let store = RedisSessionStore::from_agent_id(&id);
        assert!(store.prefix().starts_with("agent:"));
        assert!(store.prefix().ends_with(':'));
        assert!(store.prefix().contains("12345678"));
        assert_eq!(store.default_ttl(), 3600);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_agent_id() -> AgentId {
        AgentId(Uuid::new_v4())
    }

    fn test_requirements() -> AgentDatabaseRequirements {
        AgentDatabaseRequirements {
            postgres: true,
            redis: true,
            schema: None,
            storage_quota: None,
            extensions: vec!["vector".to_string()],
        }
    }

    #[test]
    fn provision_creates_database_info() {
        let mut mgr = DatabaseManager::new();
        let agent_id = test_agent_id();
        let reqs = test_requirements();

        let result = mgr.provision(agent_id, &reqs);
        assert!(result.is_ok());

        let info = result.unwrap();
        assert!(info.postgres_url.is_some());
        assert!(info.redis_prefix.is_some());
        assert!(info.postgres_database.is_some());
        assert_eq!(info.extensions, vec!["vector"]);
    }

    #[test]
    fn provision_rejects_duplicate() {
        let mut mgr = DatabaseManager::new();
        let agent_id = test_agent_id();
        let reqs = test_requirements();

        mgr.provision(agent_id, &reqs).unwrap();
        let result = mgr.provision(agent_id, &reqs);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already provisioned"));
    }

    #[test]
    fn provision_respects_max_limit() {
        let config = DatabaseConfig {
            max_databases: 2,
            ..Default::default()
        };
        let mut mgr = DatabaseManager::with_config(config);
        let reqs = test_requirements();

        mgr.provision(test_agent_id(), &reqs).unwrap();
        mgr.provision(test_agent_id(), &reqs).unwrap();
        let result = mgr.provision(test_agent_id(), &reqs);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Maximum"));
    }

    #[test]
    fn provision_postgres_only() {
        let mut mgr = DatabaseManager::new();
        let reqs = AgentDatabaseRequirements {
            postgres: true,
            redis: false,
            ..Default::default()
        };
        let info = mgr.provision(test_agent_id(), &reqs).unwrap();
        assert!(info.postgres_url.is_some());
        assert!(info.redis_prefix.is_none());
    }

    #[test]
    fn provision_redis_only() {
        let mut mgr = DatabaseManager::new();
        let reqs = AgentDatabaseRequirements {
            postgres: false,
            redis: true,
            ..Default::default()
        };
        let info = mgr.provision(test_agent_id(), &reqs).unwrap();
        assert!(info.postgres_url.is_none());
        assert!(info.redis_prefix.is_some());
    }

    #[test]
    fn provision_custom_schema() {
        let mut mgr = DatabaseManager::new();
        let reqs = AgentDatabaseRequirements {
            postgres: true,
            schema: Some("custom_schema".to_string()),
            ..Default::default()
        };
        let info = mgr.provision(test_agent_id(), &reqs).unwrap();
        assert_eq!(info.postgres_schema.unwrap(), "custom_schema");
    }

    #[test]
    fn provision_custom_quota() {
        let mut mgr = DatabaseManager::new();
        let reqs = AgentDatabaseRequirements {
            postgres: true,
            storage_quota: Some(1024 * 1024 * 1024),
            ..Default::default()
        };
        let info = mgr.provision(test_agent_id(), &reqs).unwrap();
        assert_eq!(info.storage_quota, 1024 * 1024 * 1024);
    }

    #[test]
    fn deprovision_returns_cleanup_sql() {
        let mut mgr = DatabaseManager::new();
        let agent_id = test_agent_id();
        let reqs = test_requirements();
        mgr.provision(agent_id, &reqs).unwrap();

        let commands = mgr.deprovision(&agent_id).unwrap();
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("DROP DATABASE")));
        assert!(commands.iter().any(|c| c.contains("DROP ROLE")));
    }

    #[test]
    fn deprovision_missing_agent_returns_empty() {
        let mut mgr = DatabaseManager::new();
        let commands = mgr.deprovision(&test_agent_id()).unwrap();
        assert!(commands.is_empty());
    }

    #[test]
    fn provision_sql_generates_create_statements() {
        let mut mgr = DatabaseManager::new();
        let agent_id = test_agent_id();
        let reqs = AgentDatabaseRequirements {
            postgres: true,
            extensions: vec!["vector".to_string(), "uuid-ossp".to_string()],
            ..Default::default()
        };
        mgr.provision(agent_id, &reqs).unwrap();

        let sql = mgr.provision_sql(&agent_id).unwrap();
        assert!(sql.iter().any(|s| s.contains("CREATE ROLE")));
        assert!(sql.iter().any(|s| s.contains("CREATE DATABASE")));
        assert!(sql.iter().any(|s| s.contains("vector")));
        assert!(sql.iter().any(|s| s.contains("uuid-ossp")));
    }

    #[test]
    fn get_returns_provisioned_info() {
        let mut mgr = DatabaseManager::new();
        let agent_id = test_agent_id();
        mgr.provision(agent_id, &test_requirements()).unwrap();

        assert!(mgr.get(&agent_id).is_some());
        assert!(mgr.get(&test_agent_id()).is_none());
    }

    #[test]
    fn list_returns_all() {
        let mut mgr = DatabaseManager::new();
        let reqs = test_requirements();
        mgr.provision(test_agent_id(), &reqs).unwrap();
        mgr.provision(test_agent_id(), &reqs).unwrap();

        assert_eq!(mgr.list().len(), 2);
    }

    #[test]
    fn stats_counts_correctly() {
        let mut mgr = DatabaseManager::new();
        mgr.provision(
            test_agent_id(),
            &AgentDatabaseRequirements {
                postgres: true,
                redis: true,
                ..Default::default()
            },
        )
        .unwrap();
        mgr.provision(
            test_agent_id(),
            &AgentDatabaseRequirements {
                postgres: false,
                redis: true,
                ..Default::default()
            },
        )
        .unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.total_provisioned, 2);
        assert_eq!(stats.postgres_databases, 1);
        assert_eq!(stats.redis_namespaces, 2);
    }

    #[test]
    fn database_name_is_safe() {
        let id = AgentId(Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap());
        let name = DatabaseManager::database_name(&id);
        assert!(name.starts_with("agent_"));
        assert!(!name.contains('-'));
    }

    #[test]
    fn default_config_values() {
        let config = DatabaseConfig::default();
        assert!(config.postgres_url.contains("localhost"));
        assert!(config.redis_url.contains("127.0.0.1"));
        assert_eq!(config.max_databases, 100);
        assert_eq!(config.default_storage_quota, 512 * 1024 * 1024);
    }

    #[test]
    fn redis_prefix_format() {
        let mut mgr = DatabaseManager::new();
        let agent_id = test_agent_id();
        let info = mgr
            .provision(
                agent_id,
                &AgentDatabaseRequirements {
                    redis: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let prefix = info.redis_prefix.unwrap();
        assert!(prefix.starts_with("agent:"));
        assert!(prefix.ends_with(':'));
    }
}
