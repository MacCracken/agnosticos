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
    /// Manage invoices in Aequi
    AequiInvoices {
        action: String,
        client: Option<String>,
    },
    /// Generate financial reports
    AequiReports {
        action: String,
        period: Option<String>,
    },
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
    /// Manage Photis boards
    PhotoisBoards {
        action: String,
        name: Option<String>,
    },
    /// Photis quick notes
    PhotoisNotes {
        action: String,
        content: Option<String>,
    },
    /// Run a QA test suite in Agnostic
    AgnosticRunSuite {
        suite: String,
        target_url: Option<String>,
    },
    /// Get test run status from Agnostic
    AgnosticTestStatus { run_id: String },
    /// Get test report from Agnostic
    AgnosticTestReport {
        run_id: String,
        format: Option<String>,
    },
    /// List available QA test suites
    AgnosticListSuites { category: Option<String> },
    /// Get QA agent status from Agnostic
    AgnosticAgentStatus { agent_type: Option<String> },
    /// Run an agent crew in Agnostic
    AgnosticRunCrew {
        title: String,
        preset: Option<String>,
    },
    /// Get crew run status
    AgnosticCrewStatus { crew_id: String },
    /// List agent crew presets
    AgnosticListPresets { domain: Option<String> },
    /// List agent definitions
    AgnosticListDefinitions { domain: Option<String> },
    /// Create a new agent definition
    AgnosticCreateAgent {
        agent_key: String,
        name: String,
        role: String,
    },
    /// Get code coverage from Agnostic
    AgnosticCoverage {
        action: String,
        suite: Option<String>,
    },
    /// Schedule recurring tests in Agnostic
    AgnosticSchedule {
        action: String,
        suite: Option<String>,
    },
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
    /// Manage Delta branches
    DeltaBranches {
        action: String,
        repo: Option<String>,
        name: Option<String>,
    },
    /// Delta code review
    DeltaReview {
        action: String,
        pr_id: Option<String>,
    },
    /// List edge nodes in the fleet
    EdgeListNodes { status: Option<String> },
    /// Deploy a task to an edge node
    EdgeDeploy { task: String, node: Option<String> },
    /// Trigger OTA update on an edge node
    EdgeUpdate {
        node: String,
        version: Option<String>,
    },
    /// Get edge node or fleet health status
    EdgeHealth { node: Option<String> },
    /// Decommission an edge node
    EdgeDecommission { node: String },
    /// Query edge node logs
    EdgeLogs {
        action: String,
        node: Option<String>,
    },
    /// Manage edge node config
    EdgeConfig {
        action: String,
        node: Option<String>,
        key: Option<String>,
    },
    /// Manage Shruti DAW sessions
    ShrutiSession {
        action: String,
        name: Option<String>,
    },
    /// Manage Shruti DAW tracks
    ShrutiTrack {
        action: String,
        name: Option<String>,
        kind: Option<String>,
    },
    /// Control Shruti mixer
    ShrutiMixer {
        track: String,
        gain: Option<f64>,
        mute: Option<bool>,
        solo: Option<bool>,
    },
    /// Control Shruti transport
    ShrutiTransport {
        action: String,
        value: Option<String>,
    },
    /// Manage Shruti audio plugins
    ShrutiPlugins {
        action: String,
        name: Option<String>,
    },
    /// Shruti AI audio features
    ShrutiAi {
        action: String,
        track: Option<String>,
    },
    /// Export Shruti session
    ShrutiExport {
        path: Option<String>,
        format: Option<String>,
    },
    /// Manage Tazama video projects
    TazamaProject {
        action: String,
        name: Option<String>,
    },
    /// Manage Tazama timeline clips
    TazamaTimeline {
        action: String,
        clip_id: Option<String>,
        position: Option<f64>,
    },
    /// Apply Tazama effects and transitions
    TazamaEffects {
        action: String,
        effect_type: Option<String>,
        clip_id: Option<String>,
    },
    /// Run Tazama AI video features
    TazamaAi {
        action: String,
        options: Option<String>,
    },
    /// Manage Tazama media library
    TazamaMedia {
        action: String,
        path: Option<String>,
    },
    /// Manage Tazama subtitles
    TazamaSubtitles {
        action: String,
        language: Option<String>,
    },
    /// Export Tazama video project
    TazamaExport {
        path: Option<String>,
        format: Option<String>,
    },
    /// Manage Rasa image canvases
    RasaCanvas {
        action: String,
        name: Option<String>,
    },
    /// Manage Rasa image layers
    RasaLayers {
        action: String,
        name: Option<String>,
        kind: Option<String>,
    },
    /// Apply Rasa image tools
    RasaTools {
        action: String,
        params: Option<String>,
    },
    /// Run Rasa AI image features
    RasaAi {
        action: String,
        prompt: Option<String>,
    },
    /// Rasa batch image operations
    RasaBatch {
        action: String,
        path: Option<String>,
    },
    /// Rasa design templates
    RasaTemplates {
        action: String,
        name: Option<String>,
    },
    /// Rasa non-destructive adjustment layers
    RasaAdjustments {
        action: String,
        adjustment_type: Option<String>,
    },
    /// Export Rasa image
    RasaExport {
        path: Option<String>,
        format: Option<String>,
    },
    /// Manage Mneme notebooks
    MnemeNotebook {
        action: String,
        name: Option<String>,
    },
    /// Manage Mneme notes
    MnemeNotes {
        action: String,
        title: Option<String>,
        notebook_id: Option<String>,
    },
    /// Search Mneme knowledge base
    MnemeSearch { query: String, mode: Option<String> },
    /// Run Mneme AI knowledge features
    MnemeAi {
        action: String,
        note_id: Option<String>,
    },
    /// Import documents into Mneme
    MnemeImport {
        action: String,
        path: Option<String>,
    },
    /// Manage Mneme tags
    MnemeTags { action: String, tag: Option<String> },
    /// Manage Mneme knowledge graph
    MnemeGraph {
        action: String,
        node_id: Option<String>,
    },
    /// Manage Synapse LLM models
    SynapseModels {
        action: String,
        name: Option<String>,
        source: Option<String>,
    },
    /// Start/stop/status Synapse model serving
    SynapseServe {
        action: String,
        model: Option<String>,
    },
    /// Manage Synapse fine-tuning jobs
    SynapseFinetune {
        action: String,
        model: Option<String>,
        method: Option<String>,
    },
    /// Run inference via Synapse
    SynapseChat {
        model: String,
        prompt: Option<String>,
    },
    /// Get Synapse health and GPU status
    SynapseStatus,
    /// Benchmark/compare LLM models
    SynapseBenchmark {
        action: String,
        models: Option<String>,
    },
    /// Quantize/convert models
    SynapseQuantize {
        action: String,
        model: Option<String>,
        format: Option<String>,
    },
    /// View BullShift portfolio and positions
    BullShiftPortfolio {
        action: String,
        period: Option<String>,
    },
    /// Place/cancel/list trading orders
    BullShiftOrders {
        action: String,
        symbol: Option<String>,
        side: Option<String>,
    },
    /// Get market data and quotes
    BullShiftMarket {
        action: String,
        symbol: Option<String>,
    },
    /// Manage price alerts
    BullShiftAlerts {
        action: String,
        symbol: Option<String>,
    },
    /// Manage trading strategies
    BullShiftStrategy {
        action: String,
        name: Option<String>,
    },
    /// Manage broker accounts
    BullShiftAccounts {
        action: String,
        broker: Option<String>,
    },
    /// View trade history
    BullShiftHistory {
        action: String,
        period: Option<String>,
    },
    /// Manage SecureYeoman AI agents
    YeomanAgents {
        action: String,
        agent_id: Option<String>,
        name: Option<String>,
    },
    /// Assign/manage SecureYeoman tasks
    YeomanTasks {
        action: String,
        description: Option<String>,
        task_id: Option<String>,
    },
    /// Query SecureYeoman MCP tools catalog
    YeomanTools {
        action: String,
        query: Option<String>,
    },
    /// Manage SecureYeoman integrations
    YeomanIntegrations {
        action: String,
        name: Option<String>,
    },
    /// Get SecureYeoman platform status
    YeomanStatus,
    /// Query SecureYeoman agent logs
    YeomanLogs {
        action: String,
        agent_id: Option<String>,
    },
    /// Manage SecureYeoman workflows
    YeomanWorkflows {
        action: String,
        name: Option<String>,
    },
    /// Scan a file or path for threats via phylax
    PhylaxScan {
        /// Target path to scan
        target: String,
        /// Scan mode: on_demand, pre_install, pre_exec
        mode: Option<String>,
    },
    /// Get phylax scan findings
    PhylaxFindings {
        /// Filter by severity: critical, high, medium, low
        severity: Option<String>,
    },
    /// View phylax scan history
    PhylaxHistory {
        /// Max results
        limit: Option<usize>,
    },
    /// Get phylax scanner status
    PhylaxStatus,
    /// List phylax detection rules
    PhylaxRules,
    // Tarang media framework
    /// Probe a media file
    TarangProbe { path: String },
    /// Analyze media content with AI
    TarangAnalyze { path: String },
    /// List supported codecs
    TarangCodecs,
    /// Prepare transcription request
    TarangTranscribe { path: String, language: Option<String> },
    /// Detect media format
    TarangFormats { path: String },

    // Jalwa media player
    /// Play a media file
    JalwaPlay { path: String },
    /// Pause playback
    JalwaPause,
    /// Get playback status
    JalwaStatus,
    /// Search media library
    JalwaSearch { query: String },
    /// Get AI recommendations
    JalwaRecommend { item_id: String, max: Option<u32> },

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
