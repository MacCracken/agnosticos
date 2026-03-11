use crate::security::PermissionLevel;

/// Parsed intent from natural language
#[derive(Debug, Clone)]
pub enum Intent {
    /// Show files or directories
    ListFiles {
        path: Option<String>,
        options: ListOptions,
    },
    /// Display file contents
    ShowFile { path: String, lines: Option<usize> },
    /// Search for files
    FindFiles {
        pattern: String,
        path: Option<String>,
    },
    /// Search within files
    SearchContent {
        pattern: String,
        path: Option<String>,
    },
    /// Change directory
    ChangeDirectory { path: String },
    /// Create directory
    CreateDirectory { path: String },
    /// Copy files
    Copy { source: String, destination: String },
    /// Move/rename files
    Move { source: String, destination: String },
    /// Remove files
    Remove { path: String, recursive: bool },
    /// View process information
    ShowProcesses,
    /// Kill a process
    KillProcess { pid: u32 },
    /// Show system information
    SystemInfo,
    /// Network operations
    NetworkInfo,
    /// Disk usage
    DiskUsage { path: Option<String> },
    /// Install package
    InstallPackage { packages: Vec<String> },
    /// General shell command
    ShellCommand { command: String, args: Vec<String> },
    /// View audit log entries
    AuditView {
        /// Optional agent ID to filter by
        agent_id: Option<String>,
        /// Time window (e.g., "1h", "30m", "1d")
        time_window: Option<String>,
        /// Maximum number of entries
        count: Option<usize>,
    },
    /// View agent status and information
    AgentInfo {
        /// Optional agent ID (if None, list all agents)
        agent_id: Option<String>,
    },
    /// Manage system services
    ServiceControl {
        /// Action: list, start, stop, restart, status
        action: String,
        /// Service name (for start/stop/restart/status)
        service_name: Option<String>,
    },
    /// Network scanning and diagnostics
    NetworkScan {
        /// Tool action: port_scan, ping_sweep, dns_lookup, trace_route, packet_capture, web_scan,
        /// mass_scan, arp_scan, network_diag, service_scan, dir_bust, dir_fuzz, vuln_scan,
        /// socket_stats, dns_enum, deep_inspect, bandwidth_monitor
        action: String,
        /// Target host, IP, or network
        target: Option<String>,
    },
    /// View journald log entries
    JournalView {
        /// Optional systemd unit to filter by
        unit: Option<String>,
        /// Optional priority filter (e.g., "err", "warning", "info")
        priority: Option<String>,
        /// Maximum number of lines to display
        lines: Option<usize>,
        /// Time window (e.g., "1h ago", "today")
        since: Option<String>,
    },
    /// View device information (udev)
    DeviceInfo {
        /// Optional subsystem filter (e.g., "usb", "block", "net")
        subsystem: Option<String>,
        /// Optional specific device path
        device_path: Option<String>,
    },
    /// Mount/unmount filesystems (including FUSE)
    MountControl {
        /// Action: list, mount, unmount
        action: String,
        /// Optional mountpoint path
        mountpoint: Option<String>,
        /// Optional filesystem type
        filesystem: Option<String>,
    },
    /// Bootloader configuration
    BootConfig {
        /// Action: list, default, timeout
        action: String,
        /// Optional boot entry identifier
        entry: Option<String>,
        /// Optional value (for set operations)
        value: Option<String>,
    },
    /// System update management
    SystemUpdate {
        /// Action: check, apply, rollback, status
        action: String,
    },
    /// Search the knowledge base
    KnowledgeSearch {
        query: String,
        source: Option<String>,
    },
    /// Query RAG pipeline for context-augmented answers
    RagQuery { query: String },
    /// Unified package install via ark
    ArkInstall {
        packages: Vec<String>,
        source: Option<String>,
    },
    /// Unified package remove via ark
    ArkRemove { packages: Vec<String> },
    /// Unified package search via ark
    ArkSearch { query: String },
    /// Show ark package info
    ArkInfo { package: String },
    /// Check for updates via ark
    ArkUpdate,
    /// Upgrade packages via ark
    ArkUpgrade { packages: Option<Vec<String>> },
    /// Show ark status
    ArkStatus,
    /// Install a marketplace package
    MarketplaceInstall { package: String },
    /// Uninstall a marketplace package
    MarketplaceUninstall { package: String },
    /// Search the marketplace
    MarketplaceSearch { query: String },
    /// List installed marketplace packages
    MarketplaceList,
    /// Update marketplace packages
    MarketplaceUpdate,
    /// Show quarterly tax estimate from Aequi
    AequiTaxEstimate { quarter: Option<String> },
    /// Show Schedule C preview from Aequi
    AequiScheduleC { year: Option<String> },
    /// Import a bank statement into Aequi
    AequiImportBank { file_path: String },
    /// Show account balances from Aequi
    AequiBalance,
    /// List receipts from Aequi
    AequiReceipts { status: Option<String> },
    /// List tasks from Photis Nadi
    TaskList { status: Option<String> },
    /// Create a task in Photis Nadi
    TaskCreate {
        title: String,
        priority: Option<String>,
    },
    /// Update a task in Photis Nadi
    TaskUpdate {
        task_id: String,
        status: Option<String>,
    },
    /// Check daily rituals/habits
    RitualCheck { date: Option<String> },
    /// Show productivity statistics
    ProductivityStats { period: Option<String> },
    /// Create a repository in Delta
    DeltaCreateRepo {
        name: String,
        description: Option<String>,
    },
    /// List repositories in Delta
    DeltaListRepos,
    /// Create or manage a pull request in Delta
    DeltaPr {
        action: String,
        repo: Option<String>,
        title: Option<String>,
    },
    /// Push code to Delta
    DeltaPush {
        repo: Option<String>,
        branch: Option<String>,
    },
    /// Show Delta CI pipeline status
    DeltaCiStatus { repo: Option<String> },
    /// Piped command chain (cmd1 | cmd2)
    Pipeline { commands: Vec<String> },
    /// Question/Information request
    Question { query: String },
    /// Ambiguous - needs clarification
    Ambiguous { alternatives: Vec<String> },
    /// Unknown intent
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub all: bool,
    pub long: bool,
    pub human_readable: bool,
    pub sort_by_time: bool,
    pub recursive: bool,
}

/// Command translation result
#[derive(Debug, Clone)]
pub struct Translation {
    pub command: String,
    pub args: Vec<String>,
    pub description: String,
    pub permission: PermissionLevel,
    pub explanation: String,
}
