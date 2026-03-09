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
// Tests
// ---------------------------------------------------------------------------

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
