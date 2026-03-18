//! Core types, enums, and structs for network tools.

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

/// Risk level for network tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    /// Safe read-only operations (e.g., DNS lookup)
    Low,
    /// Operations that generate network traffic (e.g., ping, traceroute)
    Medium,
    /// Active scanning that may trigger IDS (e.g., port scan, service scan)
    High,
    /// Potentially destructive or highly intrusive (e.g., web scan, packet capture)
    Critical,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "Low"),
            RiskLevel::Medium => write!(f, "Medium"),
            RiskLevel::High => write!(f, "High"),
            RiskLevel::Critical => write!(f, "Critical"),
        }
    }
}

/// Supported network tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkTool {
    /// Port scanning (nmap)
    PortScan,
    /// ICMP sweep (ping/nmap -sn)
    PingSweep,
    /// DNS resolution (dig/nslookup)
    DnsLookup,
    /// Route tracing (traceroute)
    TraceRoute,
    /// Bandwidth measurement (iperf3)
    BandwidthTest,
    /// Packet capture (tcpdump)
    PacketCapture,
    /// HTTP client (curl)
    HttpClient,
    /// Raw TCP/UDP connection (ncat)
    NetcatConnect,
    /// Service version detection (nmap -sV)
    ServiceScan,
    /// Web vulnerability scanning (nikto)
    WebScan,
    /// Directory brute-forcing (gobuster)
    DirBust,
    /// High-speed port scanning (masscan)
    MassScan,
    /// ARP-based local network discovery (arp-scan)
    ArpScan,
    /// Network diagnostics combining traceroute + ping (mtr)
    NetworkDiag,
    /// Bidirectional data relay (socat)
    DataRelay,
    /// Deep packet inspection (tshark)
    DeepInspect,
    /// Network-level grep for packet content (ngrep)
    NetworkGrep,
    /// Socket statistics and connection info (ss)
    SocketStats,
    /// DNS enumeration and zone info (dnsrecon)
    DnsEnum,
    /// Directory/subdomain fuzzing (ffuf)
    DirFuzz,
    /// Template-based vulnerability scanning (nuclei)
    VulnScanner,
    /// Per-process bandwidth monitoring (nethogs)
    BandwidthMonitor,
    /// Passive OS fingerprinting (p0f)
    PassiveFingerprint,
    /// ARP network scanning — passive/active modes (netdiscover)
    NetDiscover,
    /// TUI Wireshark frontend (termshark wrapping tshark)
    TermShark,
    /// Network monitoring and MITM analysis framework (bettercap)
    BetterCap,
    /// Fast multi-purpose DNS toolkit (dnsx)
    DnsX,
    /// DNS zone traversal and subdomain discovery (fierce)
    Fierce,
    /// Web application fuzzer (wfuzz)
    WebAppFuzz,
    /// SQL injection detection and exploitation (sqlmap)
    SqlMap,
    /// 802.11 wireless analysis suite (aircrack-ng)
    AircrackNg,
    /// Wireless network detector and sniffer (kismet)
    Kismet,
}

impl fmt::Display for NetworkTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkTool::PortScan => write!(f, "PortScan"),
            NetworkTool::PingSweep => write!(f, "PingSweep"),
            NetworkTool::DnsLookup => write!(f, "DnsLookup"),
            NetworkTool::TraceRoute => write!(f, "TraceRoute"),
            NetworkTool::BandwidthTest => write!(f, "BandwidthTest"),
            NetworkTool::PacketCapture => write!(f, "PacketCapture"),
            NetworkTool::HttpClient => write!(f, "HttpClient"),
            NetworkTool::NetcatConnect => write!(f, "NetcatConnect"),
            NetworkTool::ServiceScan => write!(f, "ServiceScan"),
            NetworkTool::WebScan => write!(f, "WebScan"),
            NetworkTool::DirBust => write!(f, "DirBust"),
            NetworkTool::MassScan => write!(f, "MassScan"),
            NetworkTool::ArpScan => write!(f, "ArpScan"),
            NetworkTool::NetworkDiag => write!(f, "NetworkDiag"),
            NetworkTool::DataRelay => write!(f, "DataRelay"),
            NetworkTool::DeepInspect => write!(f, "DeepInspect"),
            NetworkTool::NetworkGrep => write!(f, "NetworkGrep"),
            NetworkTool::SocketStats => write!(f, "SocketStats"),
            NetworkTool::DnsEnum => write!(f, "DnsEnum"),
            NetworkTool::DirFuzz => write!(f, "DirFuzz"),
            NetworkTool::VulnScanner => write!(f, "VulnScanner"),
            NetworkTool::BandwidthMonitor => write!(f, "BandwidthMonitor"),
            NetworkTool::PassiveFingerprint => write!(f, "PassiveFingerprint"),
            NetworkTool::NetDiscover => write!(f, "NetDiscover"),
            NetworkTool::TermShark => write!(f, "TermShark"),
            NetworkTool::BetterCap => write!(f, "BetterCap"),
            NetworkTool::DnsX => write!(f, "DnsX"),
            NetworkTool::Fierce => write!(f, "Fierce"),
            NetworkTool::WebAppFuzz => write!(f, "WebAppFuzz"),
            NetworkTool::SqlMap => write!(f, "SqlMap"),
            NetworkTool::AircrackNg => write!(f, "AircrackNg"),
            NetworkTool::Kismet => write!(f, "Kismet"),
        }
    }
}

/// Configuration for a specific network tool
#[derive(Debug, Clone)]
pub struct NetworkToolConfig {
    /// Which tool this config is for
    pub tool: NetworkTool,
    /// Binary name on disk (e.g., "nmap", "tcpdump")
    pub binary_name: &'static str,
    /// Linux capabilities required (e.g., "NET_RAW", "NET_ADMIN")
    pub required_capabilities: Vec<String>,
    /// Risk classification
    pub risk_level: RiskLevel,
    /// Whether user approval is required before execution
    pub requires_approval: bool,
    /// Maximum allowed execution time in seconds
    pub max_timeout_secs: u64,
    /// Whether this tool can run inside a sandbox
    pub allowed_in_sandbox: bool,
}

impl NetworkToolConfig {
    /// Build the default configuration for a given tool variant
    pub fn for_tool(tool: NetworkTool) -> Self {
        match tool {
            NetworkTool::PortScan => Self {
                tool,
                binary_name: "nmap",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: true,
            },
            NetworkTool::PingSweep => Self {
                tool,
                binary_name: "nmap",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: false,
                max_timeout_secs: 120,
                allowed_in_sandbox: true,
            },
            NetworkTool::DnsLookup => Self {
                tool,
                binary_name: "dig",
                required_capabilities: vec![],
                risk_level: RiskLevel::Low,
                requires_approval: false,
                max_timeout_secs: 30,
                allowed_in_sandbox: true,
            },
            NetworkTool::TraceRoute => Self {
                tool,
                binary_name: "traceroute",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: false,
                max_timeout_secs: 120,
                allowed_in_sandbox: true,
            },
            NetworkTool::BandwidthTest => Self {
                tool,
                binary_name: "iperf3",
                required_capabilities: vec![],
                risk_level: RiskLevel::Medium,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: true,
            },
            NetworkTool::PacketCapture => Self {
                tool,
                binary_name: "tcpdump",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: false,
            },
            NetworkTool::HttpClient => Self {
                tool,
                binary_name: "curl",
                required_capabilities: vec![],
                risk_level: RiskLevel::Low,
                requires_approval: false,
                max_timeout_secs: 60,
                allowed_in_sandbox: true,
            },
            NetworkTool::NetcatConnect => Self {
                tool,
                binary_name: "ncat",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 120,
                allowed_in_sandbox: true,
            },
            NetworkTool::ServiceScan => Self {
                tool,
                binary_name: "nmap",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 900,
                allowed_in_sandbox: true,
            },
            NetworkTool::WebScan => Self {
                tool,
                binary_name: "nikto",
                required_capabilities: vec![],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 1800,
                allowed_in_sandbox: true,
            },
            NetworkTool::DirBust => Self {
                tool,
                binary_name: "gobuster",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: true,
            },
            NetworkTool::MassScan => Self {
                tool,
                binary_name: "masscan",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: true,
            },
            NetworkTool::ArpScan => Self {
                tool,
                binary_name: "arp-scan",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: true,
                max_timeout_secs: 120,
                allowed_in_sandbox: true,
            },
            NetworkTool::NetworkDiag => Self {
                tool,
                binary_name: "mtr",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: false,
                max_timeout_secs: 120,
                allowed_in_sandbox: true,
            },
            NetworkTool::DataRelay => Self {
                tool,
                binary_name: "socat",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: true,
            },
            NetworkTool::DeepInspect => Self {
                tool,
                binary_name: "tshark",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: false,
            },
            NetworkTool::NetworkGrep => Self {
                tool,
                binary_name: "ngrep",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: false,
            },
            NetworkTool::SocketStats => Self {
                tool,
                binary_name: "ss",
                required_capabilities: vec![],
                risk_level: RiskLevel::Low,
                requires_approval: false,
                max_timeout_secs: 30,
                allowed_in_sandbox: true,
            },
            NetworkTool::DnsEnum => Self {
                tool,
                binary_name: "dnsrecon",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: true,
            },
            NetworkTool::DirFuzz => Self {
                tool,
                binary_name: "ffuf",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: true,
            },
            NetworkTool::VulnScanner => Self {
                tool,
                binary_name: "nuclei",
                required_capabilities: vec![],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 1800,
                allowed_in_sandbox: true,
            },
            NetworkTool::BandwidthMonitor => Self {
                tool,
                binary_name: "nethogs",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: false,
            },
            NetworkTool::PassiveFingerprint => Self {
                tool,
                binary_name: "p0f",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: false,
            },
            NetworkTool::NetDiscover => Self {
                tool,
                binary_name: "netdiscover",
                required_capabilities: vec!["NET_RAW".into()],
                risk_level: RiskLevel::Medium,
                requires_approval: true,
                max_timeout_secs: 300,
                allowed_in_sandbox: true,
            },
            NetworkTool::TermShark => Self {
                tool,
                binary_name: "termshark",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: false,
            },
            NetworkTool::BetterCap => Self {
                tool,
                binary_name: "bettercap",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 1800,
                allowed_in_sandbox: false,
            },
            NetworkTool::DnsX => Self {
                tool,
                binary_name: "dnsx",
                required_capabilities: vec![],
                risk_level: RiskLevel::Medium,
                requires_approval: false,
                max_timeout_secs: 300,
                allowed_in_sandbox: true,
            },
            NetworkTool::Fierce => Self {
                tool,
                binary_name: "fierce",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 600,
                allowed_in_sandbox: true,
            },
            NetworkTool::WebAppFuzz => Self {
                tool,
                binary_name: "wfuzz",
                required_capabilities: vec![],
                risk_level: RiskLevel::High,
                requires_approval: true,
                max_timeout_secs: 900,
                allowed_in_sandbox: true,
            },
            NetworkTool::SqlMap => Self {
                tool,
                binary_name: "sqlmap",
                required_capabilities: vec![],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 1800,
                allowed_in_sandbox: true,
            },
            NetworkTool::AircrackNg => Self {
                tool,
                binary_name: "aircrack-ng",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 1800,
                allowed_in_sandbox: false,
            },
            NetworkTool::Kismet => Self {
                tool,
                binary_name: "kismet",
                required_capabilities: vec!["NET_RAW".into(), "NET_ADMIN".into()],
                risk_level: RiskLevel::Critical,
                requires_approval: true,
                max_timeout_secs: 1800,
                allowed_in_sandbox: false,
            },
        }
    }
}

/// All known tool variants, for iteration
pub const ALL_TOOLS: &[NetworkTool] = &[
    NetworkTool::PortScan,
    NetworkTool::PingSweep,
    NetworkTool::DnsLookup,
    NetworkTool::TraceRoute,
    NetworkTool::BandwidthTest,
    NetworkTool::PacketCapture,
    NetworkTool::HttpClient,
    NetworkTool::NetcatConnect,
    NetworkTool::ServiceScan,
    NetworkTool::WebScan,
    NetworkTool::DirBust,
    NetworkTool::MassScan,
    NetworkTool::ArpScan,
    NetworkTool::NetworkDiag,
    NetworkTool::DataRelay,
    NetworkTool::DeepInspect,
    NetworkTool::NetworkGrep,
    NetworkTool::SocketStats,
    NetworkTool::DnsEnum,
    NetworkTool::DirFuzz,
    NetworkTool::VulnScanner,
    NetworkTool::BandwidthMonitor,
    NetworkTool::PassiveFingerprint,
    NetworkTool::NetDiscover,
    NetworkTool::TermShark,
    NetworkTool::BetterCap,
    NetworkTool::DnsX,
    NetworkTool::Fierce,
    NetworkTool::WebAppFuzz,
    NetworkTool::SqlMap,
    NetworkTool::AircrackNg,
    NetworkTool::Kismet,
];

/// Output captured from a tool execution
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Process exit code
    pub exit_code: i32,
    /// Wall-clock duration of the execution
    pub duration: Duration,
    /// Which tool produced this output
    pub tool: NetworkTool,
    /// Human-readable audit trail entry
    pub audit_entry: String,
}

/// A validated scan target
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatedTarget {
    /// Single IP address
    Ip(IpAddr),
    /// CIDR notation (address + prefix length)
    Cidr { addr: IpAddr, prefix: u8 },
    /// DNS hostname
    Hostname(String),
}

impl fmt::Display for ValidatedTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidatedTarget::Ip(ip) => write!(f, "{}", ip),
            ValidatedTarget::Cidr { addr, prefix } => write!(f, "{}/{}", addr, prefix),
            ValidatedTarget::Hostname(h) => write!(f, "{}", h),
        }
    }
}

// ============================================================================
// Output parsing — structured results from raw tool stdout
// ============================================================================

/// A discovered open port from a scan
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPort {
    pub port: u16,
    pub protocol: String,
    pub state: String,
    pub service: Option<String>,
    pub version: Option<String>,
}

/// A discovered host from a network scan
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredHost {
    pub address: String,
    pub hostname: Option<String>,
    pub mac_address: Option<String>,
    pub vendor: Option<String>,
    pub state: String,
    pub ports: Vec<DiscoveredPort>,
}

/// A DNS record from a lookup
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    pub name: String,
    pub record_type: String,
    pub value: String,
    pub ttl: Option<u32>,
}

/// A hop from a traceroute/mtr
#[derive(Debug, Clone, PartialEq)]
pub struct TraceHop {
    pub hop_number: u32,
    pub address: Option<String>,
    pub hostname: Option<String>,
    pub rtt_ms: Option<f64>,
    pub loss_pct: Option<f64>,
}

/// A socket/connection entry from ss
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketEntry {
    pub state: String,
    pub protocol: String,
    pub local_addr: String,
    pub remote_addr: String,
    pub process: Option<String>,
}

/// Structured results parsed from tool output
#[derive(Debug, Clone)]
pub enum ParsedOutput {
    /// Hosts and ports from nmap/masscan scans
    HostScan {
        hosts: Vec<DiscoveredHost>,
        scan_type: String,
    },
    /// DNS records from dig/dnsrecon
    DnsResult {
        records: Vec<DnsRecord>,
        query: String,
    },
    /// Route hops from traceroute/mtr
    TraceResult { hops: Vec<TraceHop>, target: String },
    /// Socket listing from ss
    SocketList { sockets: Vec<SocketEntry> },
    /// Raw output when no parser is available
    Raw { summary: String },
}

/// Scan profile controlling speed/stealth tradeoffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProfile {
    /// Quick scan — common ports only.
    Quick,
    /// Standard scan — top 1000 ports.
    Standard,
    /// Thorough scan — all 65535 ports, service detection.
    Thorough,
    /// Stealth scan — slower timing, randomized order.
    Stealth,
}
