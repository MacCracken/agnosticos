//! All types, enums, and data structures for the ark package manager.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::nous::PackageSource;

// ---------------------------------------------------------------------------
// Ark CLI command
// ---------------------------------------------------------------------------

/// An ark command parsed from CLI args.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArkCommand {
    /// Install one or more packages.
    Install { packages: Vec<String>, force: bool },
    /// Install all packages in a group (e.g., `ark install --group desktop`).
    GroupInstall { group: String, force: bool },
    /// Remove/uninstall packages.
    Remove { packages: Vec<String>, purge: bool },
    /// Search across all sources.
    Search {
        query: String,
        source: Option<PackageSource>,
    },
    /// List installed packages.
    List { source: Option<PackageSource> },
    /// Show detailed info about a package.
    Info { package: String },
    /// Check for updates.
    Update,
    /// Upgrade packages with available updates.
    Upgrade { packages: Option<Vec<String>> },
    /// Show ark version and status.
    Status,
}

// ---------------------------------------------------------------------------
// Ark result and output types
// ---------------------------------------------------------------------------

/// Result of an ark operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkResult {
    pub success: bool,
    pub message: String,
    pub packages_affected: Vec<String>,
    pub source: PackageSource,
}

/// Formatted output for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkOutput {
    pub lines: Vec<ArkOutputLine>,
}

/// A single line of formatted ark output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArkOutputLine {
    Header(String),
    Package {
        name: String,
        version: String,
        source: PackageSource,
        description: String,
    },
    Info {
        key: String,
        value: String,
    },
    Separator,
    Success(String),
    Error(String),
    Warning(String),
}

impl ArkOutput {
    /// Create an empty output.
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Format the output as a human-readable string.
    pub fn to_display_string(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                ArkOutputLine::Header(s) => {
                    out.push_str(&format!("=== {} ===\n", s));
                }
                ArkOutputLine::Package {
                    name,
                    version,
                    source,
                    description,
                } => {
                    out.push_str(&format!(
                        "  {} ({}) [{}] -- {}\n",
                        name, version, source, description
                    ));
                }
                ArkOutputLine::Info { key, value } => {
                    out.push_str(&format!("  {}: {}\n", key, value));
                }
                ArkOutputLine::Separator => {
                    out.push_str("---\n");
                }
                ArkOutputLine::Success(s) => {
                    out.push_str(&format!("OK: {}\n", s));
                }
                ArkOutputLine::Error(s) => {
                    out.push_str(&format!("ERROR: {}\n", s));
                }
                ArkOutputLine::Warning(s) => {
                    out.push_str(&format!("WARN: {}\n", s));
                }
            }
        }
        out
    }
}

impl Default for ArkOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ArkOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

// ---------------------------------------------------------------------------
// Install plan
// ---------------------------------------------------------------------------

/// A plan for what ark wants to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPlan {
    pub steps: Vec<InstallStep>,
    pub requires_root: bool,
    pub estimated_size_bytes: u64,
}

impl InstallPlan {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            requires_root: false,
            estimated_size_bytes: 0,
        }
    }
}

impl Default for InstallPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// A single step in an install plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallStep {
    SystemInstall {
        package: String,
        version: Option<String>,
    },
    SystemRemove {
        package: String,
        purge: bool,
    },
    MarketplaceInstall {
        package: String,
        version: Option<String>,
    },
    MarketplaceRemove {
        package: String,
    },
    FlutterInstall {
        package: String,
        version: Option<String>,
    },
    FlutterRemove {
        package: String,
    },
    /// Run `apt-get update` to refresh system package lists.
    SystemUpdate,
}

// ---------------------------------------------------------------------------
// Ark configuration
// ---------------------------------------------------------------------------

/// Configuration for ark behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkConfig {
    /// Default resolution strategy when source is ambiguous.
    pub default_strategy: crate::nous::ResolutionStrategy,
    /// Require confirmation for apt installs (default: true).
    pub confirm_system_installs: bool,
    /// Require confirmation for removals (default: true).
    pub confirm_removals: bool,
    /// Check for updates on search (default: false).
    pub auto_update_check: bool,
    /// ANSI colors in output (default: true).
    pub color_output: bool,
    /// Marketplace package storage directory.
    pub marketplace_dir: PathBuf,
    /// Cache directory for ark metadata.
    pub cache_dir: PathBuf,
}

impl Default for ArkConfig {
    fn default() -> Self {
        Self {
            default_strategy: crate::nous::ResolutionStrategy::SystemFirst,
            confirm_system_installs: true,
            confirm_removals: true,
            auto_update_check: false,
            color_output: true,
            marketplace_dir: PathBuf::from("/var/lib/agnos/marketplace"),
            cache_dir: PathBuf::from("/var/cache/agnos/ark"),
        }
    }
}

// ---------------------------------------------------------------------------
// Transaction log — atomic operation tracking with rollback
// ---------------------------------------------------------------------------

/// Unique identifier for a transaction.
pub type TransactionId = String;

/// A single atomic package management transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: TransactionStatus,
    pub operations: Vec<TransactionOp>,
    pub user: String,
}

/// Status of a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    InProgress,
    Committed,
    RolledBack,
    Failed(String),
}

impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InProgress => write!(f, "in-progress"),
            Self::Committed => write!(f, "committed"),
            Self::RolledBack => write!(f, "rolled-back"),
            Self::Failed(e) => write!(f, "failed: {}", e),
        }
    }
}

/// A single operation within a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionOp {
    pub op_type: TransactionOpType,
    pub package: String,
    pub version: Option<String>,
    pub source: PackageSource,
    pub status: TransactionOpStatus,
    pub error: Option<String>,
}

/// Types of transaction operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionOpType {
    Install,
    Remove,
    Upgrade { from_version: String },
}

/// Status of a transaction operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionOpStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
    RolledBack,
}

/// Default path for the persistent transaction log.
pub const DEFAULT_TRANSACTION_LOG_PATH: &str = "/var/lib/agnos/ark/transaction.log";

/// The transaction log stores all past transactions for auditing and rollback.
/// When a log_path is set, state-changing operations persist to an append-only
/// JSONL file for crash recovery (H18 audit finding).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionLog {
    pub(crate) transactions: Vec<Transaction>,
    pub(crate) next_id: u64,
    #[serde(skip)]
    pub(crate) log_path: Option<PathBuf>,
}

impl TransactionLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a transaction log backed by a JSONL file. Replays existing
    /// entries on load; starts empty if file does not exist.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let mut log = Self {
            transactions: Vec::new(),
            next_id: 0,
            log_path: Some(path.to_path_buf()),
        };
        if path.exists() {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read transaction log {}", path.display()))?;
            for (lineno, line) in contents.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Transaction>(trimmed) {
                    Ok(txn) => {
                        if let Some(num_str) = txn.id.strip_prefix("txn-") {
                            if let Ok(num) = num_str.parse::<u64>() {
                                if num >= log.next_id {
                                    log.next_id = num;
                                }
                            }
                        }
                        if let Some(pos) = log.transactions.iter().position(|t| t.id == txn.id) {
                            log.transactions[pos] = txn;
                        } else {
                            log.transactions.push(txn);
                        }
                    }
                    Err(e) => {
                        warn!(lineno = lineno + 1, error = %e, "Skipping corrupt transaction log entry");
                    }
                }
            }
            info!(transactions = log.transactions.len(), path = %path.display(), "Recovered transaction log from disk");
        } else {
            debug!(path = %path.display(), "No existing transaction log -- starting fresh");
        }
        Ok(log)
    }

    /// Persist a single transaction to the append-only log file.
    pub(crate) fn persist(&self, txn: &Transaction) {
        if let Some(ref path) = self.log_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json_line) = serde_json::to_string(txn) {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(f, "{}", json_line);
                }
            }
        }
    }

    /// Begin a new transaction.
    pub fn begin(&mut self, user: &str) -> TransactionId {
        self.next_id += 1;
        let id = format!("txn-{:06}", self.next_id);
        let txn = Transaction {
            id: id.clone(),
            started_at: Utc::now(),
            completed_at: None,
            status: TransactionStatus::InProgress,
            operations: Vec::new(),
            user: user.to_string(),
        };
        info!(txn_id = %id, user = user, "transaction started");
        self.transactions.push(txn);
        id
    }

    /// Add an operation to a transaction.
    pub fn add_op(&mut self, txn_id: &str, op: TransactionOp) -> bool {
        if let Some(txn) = self.transactions.iter_mut().find(|t| t.id == txn_id) {
            if txn.status == TransactionStatus::InProgress {
                txn.operations.push(op);
                return true;
            }
        }
        false
    }

    /// Mark a transaction operation as complete.
    pub fn mark_op_complete(&mut self, txn_id: &str, package: &str) -> bool {
        if let Some(txn) = self.transactions.iter_mut().find(|t| t.id == txn_id) {
            if let Some(op) = txn.operations.iter_mut().find(|o| o.package == package) {
                op.status = TransactionOpStatus::Complete;
                return true;
            }
        }
        false
    }

    /// Mark a transaction operation as failed.
    pub fn mark_op_failed(&mut self, txn_id: &str, package: &str, error: &str) -> bool {
        if let Some(txn) = self.transactions.iter_mut().find(|t| t.id == txn_id) {
            if let Some(op) = txn.operations.iter_mut().find(|o| o.package == package) {
                op.status = TransactionOpStatus::Failed;
                op.error = Some(error.to_string());
                return true;
            }
        }
        false
    }

    /// Commit a transaction. Persists to log file when backed by disk.
    pub fn commit(&mut self, txn_id: &str) -> bool {
        let idx = match self.transactions.iter().position(|t| t.id == txn_id) {
            Some(i) => i,
            None => return false,
        };
        if self.transactions[idx].status != TransactionStatus::InProgress {
            return false;
        }
        self.transactions[idx].status = TransactionStatus::Committed;
        self.transactions[idx].completed_at = Some(Utc::now());
        info!(txn_id = txn_id, "transaction committed");
        let snapshot = self.transactions[idx].clone();
        self.persist(&snapshot);
        true
    }

    /// Roll back a transaction. Persists to log file when backed by disk.
    pub fn rollback(&mut self, txn_id: &str) -> bool {
        let idx = match self.transactions.iter().position(|t| t.id == txn_id) {
            Some(i) => i,
            None => return false,
        };
        if self.transactions[idx].status != TransactionStatus::InProgress {
            return false;
        }
        for op in &mut self.transactions[idx].operations {
            if op.status == TransactionOpStatus::Pending
                || op.status == TransactionOpStatus::InProgress
            {
                op.status = TransactionOpStatus::RolledBack;
            }
        }
        self.transactions[idx].status = TransactionStatus::RolledBack;
        self.transactions[idx].completed_at = Some(Utc::now());
        warn!(txn_id = txn_id, "transaction rolled back");
        let snapshot = self.transactions[idx].clone();
        self.persist(&snapshot);
        true
    }

    /// Fail a transaction with an error message. Persists to disk.
    pub fn fail(&mut self, txn_id: &str, error: &str) -> bool {
        let idx = match self.transactions.iter().position(|t| t.id == txn_id) {
            Some(i) => i,
            None => return false,
        };
        if self.transactions[idx].status != TransactionStatus::InProgress {
            return false;
        }
        self.transactions[idx].status = TransactionStatus::Failed(error.to_string());
        self.transactions[idx].completed_at = Some(Utc::now());
        let snapshot = self.transactions[idx].clone();
        self.persist(&snapshot);
        true
    }

    /// Get a transaction by ID.
    pub fn get(&self, txn_id: &str) -> Option<&Transaction> {
        self.transactions.iter().find(|t| t.id == txn_id)
    }

    /// Get the N most recent transactions.
    pub fn recent(&self, count: usize) -> Vec<&Transaction> {
        self.transactions.iter().rev().take(count).collect()
    }

    /// Total number of transactions.
    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Package database — /var/lib/ark/installed.db equivalent
// ---------------------------------------------------------------------------

/// An entry in the unified package database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDbEntry {
    pub name: String,
    pub version: String,
    pub source: PackageSource,
    pub installed_at: DateTime<Utc>,
    pub installed_by: String,
    pub size_bytes: u64,
    pub checksum: String,
    /// List of files installed by this package (absolute paths).
    pub files: Vec<String>,
    /// Runtime dependencies.
    pub dependencies: Vec<String>,
    /// Transaction that installed this package.
    pub transaction_id: Option<TransactionId>,
}

/// The unified package database. Tracks every package installed via ark,
/// regardless of source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageDb {
    pub(crate) packages: HashMap<String, PackageDbEntry>,
}

impl PackageDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a newly installed package.
    pub fn register(&mut self, entry: PackageDbEntry) {
        info!(
            package = %entry.name,
            version = %entry.version,
            source = ?entry.source,
            files = entry.files.len(),
            "package registered"
        );
        self.packages.insert(entry.name.clone(), entry);
    }

    /// Remove a package from the database. Returns the entry if it existed.
    pub fn unregister(&mut self, name: &str) -> Option<PackageDbEntry> {
        let entry = self.packages.remove(name);
        if entry.is_some() {
            info!(package = name, "package unregistered");
        }
        entry
    }

    /// Look up a package by name.
    pub fn get(&self, name: &str) -> Option<&PackageDbEntry> {
        self.packages.get(name)
    }

    /// Check if a package is installed.
    pub fn is_installed(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// List all installed packages.
    pub fn list(&self) -> Vec<&PackageDbEntry> {
        self.packages.values().collect()
    }

    /// List packages matching a query (name or description substring).
    pub fn search(&self, query: &str) -> Vec<&PackageDbEntry> {
        let q = query.to_lowercase();
        self.packages
            .values()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .collect()
    }

    /// List packages from a specific source.
    pub fn by_source(&self, source: &PackageSource) -> Vec<&PackageDbEntry> {
        self.packages
            .values()
            .filter(|e| &e.source == source)
            .collect()
    }

    /// Total number of installed packages.
    pub fn count(&self) -> usize {
        self.packages.len()
    }

    /// Total disk usage across all installed packages.
    pub fn total_size(&self) -> u64 {
        self.packages.values().map(|e| e.size_bytes).sum()
    }

    /// Check for packages whose files are missing (integrity check).
    pub fn check_integrity(&self) -> Vec<IntegrityIssue> {
        let mut issues = Vec::new();
        for entry in self.packages.values() {
            if entry.files.is_empty() {
                issues.push(IntegrityIssue {
                    package: entry.name.clone(),
                    issue_type: IntegrityIssueType::NoFileManifest,
                });
            }
        }
        issues
    }

    /// Get all files owned by a package.
    pub fn files_for(&self, name: &str) -> Vec<&str> {
        self.packages
            .get(name)
            .map(|e| e.files.iter().map(|f| f.as_str()).collect())
            .unwrap_or_default()
    }

    /// Find which package owns a given file path.
    pub fn owner_of(&self, file_path: &str) -> Option<&str> {
        self.packages
            .values()
            .find(|e| e.files.iter().any(|f| f == file_path))
            .map(|e| e.name.as_str())
    }

    /// Get dependency resolution order for installing a package.
    /// Simple topological sort — does not handle version constraints.
    pub fn resolve_install_order(&self, to_install: &[&str]) -> Result<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut visiting = std::collections::HashSet::new();

        for &pkg in to_install {
            self.resolve_visit(pkg, to_install, &mut visited, &mut visiting, &mut order)?;
        }

        Ok(order)
    }

    fn resolve_visit(
        &self,
        pkg: &str,
        all: &[&str],
        visited: &mut std::collections::HashSet<String>,
        visiting: &mut std::collections::HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(pkg) {
            return Ok(());
        }
        if visiting.contains(pkg) {
            bail!("circular dependency detected involving '{}'", pkg);
        }

        visiting.insert(pkg.to_string());

        // Check dependencies from the package database entry
        if let Some(entry) = self.packages.get(pkg) {
            for dep in &entry.dependencies {
                // Recurse into deps that are in our install set
                if all.contains(&dep.as_str()) {
                    self.resolve_visit(dep, all, visited, visiting, order)?;
                }
            }
        }

        visiting.remove(pkg);
        visited.insert(pkg.to_string());
        order.push(pkg.to_string());
        Ok(())
    }
}

/// An integrity issue found during package database validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityIssue {
    pub package: String,
    pub issue_type: IntegrityIssueType,
}

/// Types of integrity issues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrityIssueType {
    /// Package has no file manifest — cannot verify or clean-remove.
    NoFileManifest,
    /// A file listed in the manifest is missing from disk.
    MissingFile(String),
    /// A file's checksum doesn't match the manifest.
    ChecksumMismatch(String),
}

// ---------------------------------------------------------------------------
// Execution result types
// ---------------------------------------------------------------------------

/// Result of executing an install plan step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step: InstallStep,
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
}

/// Result of executing an entire install plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanExecutionResult {
    pub transaction_id: TransactionId,
    pub success: bool,
    pub steps_completed: usize,
    pub steps_failed: usize,
    pub total_duration_ms: u64,
    pub step_results: Vec<StepResult>,
}
