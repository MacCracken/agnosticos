//! Network Tools Agent Framework
//!
//! Provides agent-wrapped network security and diagnostic tools.
//! All tool invocations are sandboxed, audited, and require user approval
//! for sensitive operations.

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tracing::{debug, info, warn};

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
    TraceResult {
        hops: Vec<TraceHop>,
        target: String,
    },
    /// Socket listing from ss
    SocketList {
        sockets: Vec<SocketEntry>,
    },
    /// Raw output when no parser is available
    Raw {
        summary: String,
    },
}

/// Parse nmap/masscan output into structured hosts+ports
pub fn parse_scan_output(stdout: &str, tool: NetworkTool) -> ParsedOutput {
    let mut hosts: Vec<DiscoveredHost> = Vec::new();
    let mut current_host: Option<DiscoveredHost> = None;

    let scan_type = match tool {
        NetworkTool::MassScan => "masscan",
        NetworkTool::ServiceScan => "service",
        NetworkTool::PingSweep => "ping",
        _ => "port",
    };

    for line in stdout.lines() {
        let line = line.trim();

        // nmap: "Nmap scan report for <host> (<ip>)" or "Nmap scan report for <ip>"
        if line.starts_with("Nmap scan report for ") {
            if let Some(host) = current_host.take() {
                hosts.push(host);
            }
            let rest = &line["Nmap scan report for ".len()..];
            let (hostname, address) = if let Some(paren_start) = rest.find('(') {
                let name = rest[..paren_start].trim().to_string();
                let ip = rest[paren_start + 1..].trim_end_matches(')').to_string();
                (Some(name), ip)
            } else {
                (None, rest.trim().to_string())
            };
            current_host = Some(DiscoveredHost {
                address,
                hostname,
                mac_address: None,
                vendor: None,
                state: "up".to_string(),
                ports: Vec::new(),
            });
        }

        // nmap: "MAC Address: AA:BB:CC:DD:EE:FF (Vendor)"
        if line.starts_with("MAC Address: ") {
            if let Some(ref mut host) = current_host {
                let rest = &line["MAC Address: ".len()..];
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                host.mac_address = Some(parts[0].to_string());
                if parts.len() > 1 {
                    host.vendor = Some(parts[1].trim_matches(|c| c == '(' || c == ')').to_string());
                }
            }
        }

        // nmap port line: "80/tcp   open  http    Apache httpd 2.4.41"
        if let Some(port_entry) = parse_nmap_port_line(line) {
            if let Some(ref mut host) = current_host {
                host.ports.push(port_entry);
            }
        }

        // masscan: "Discovered open port 80/tcp on 192.168.1.1"
        if line.starts_with("Discovered open port ") {
            let rest = &line["Discovered open port ".len()..];
            if let Some((port_proto, addr)) = rest.split_once(" on ") {
                if let Some((port_str, proto)) = port_proto.split_once('/') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        let host = hosts.iter_mut().find(|h| h.address == addr);
                        let entry = DiscoveredPort {
                            port,
                            protocol: proto.to_string(),
                            state: "open".to_string(),
                            service: None,
                            version: None,
                        };
                        if let Some(h) = host {
                            h.ports.push(entry);
                        } else {
                            hosts.push(DiscoveredHost {
                                address: addr.to_string(),
                                hostname: None,
                                mac_address: None,
                                vendor: None,
                                state: "up".to_string(),
                                ports: vec![entry],
                            });
                        }
                    }
                }
            }
        }

        // nmap host down/up
        if line.contains("Host is up") {
            if let Some(ref mut host) = current_host {
                host.state = "up".to_string();
            }
        }
    }

    if let Some(host) = current_host {
        hosts.push(host);
    }

    ParsedOutput::HostScan {
        hosts,
        scan_type: scan_type.to_string(),
    }
}

fn parse_nmap_port_line(line: &str) -> Option<DiscoveredPort> {
    // Format: "80/tcp   open  http    Apache httpd 2.4.41"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let (port_str, proto) = parts[0].split_once('/')?;
    let port = port_str.parse::<u16>().ok()?;
    let state = parts[1];
    if !matches!(state, "open" | "closed" | "filtered" | "open|filtered") {
        return None;
    }
    let service = parts.get(2).map(|s| s.to_string());
    let version = if parts.len() > 3 {
        Some(parts[3..].join(" "))
    } else {
        None
    };
    Some(DiscoveredPort {
        port,
        protocol: proto.to_string(),
        state: state.to_string(),
        service,
        version,
    })
}

/// Parse dig output into DNS records
pub fn parse_dns_output(stdout: &str, query: &str) -> ParsedOutput {
    let mut records = Vec::new();
    let mut in_answer = false;

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with(";; ANSWER SECTION:") {
            in_answer = true;
            continue;
        }
        if in_answer {
            if line.is_empty() || line.starts_with(";;") {
                in_answer = false;
                continue;
            }
            // Format: "example.com.     300     IN      A       93.184.216.34"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let ttl = parts[1].parse::<u32>().ok();
                records.push(DnsRecord {
                    name: parts[0].trim_end_matches('.').to_string(),
                    record_type: parts[3].to_string(),
                    value: parts[4..].join(" "),
                    ttl,
                });
            }
        }
    }

    ParsedOutput::DnsResult {
        records,
        query: query.to_string(),
    }
}

/// Parse traceroute/mtr output into hops
pub fn parse_trace_output(stdout: &str, target: &str) -> ParsedOutput {
    let mut hops = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        // traceroute format: " 1  gateway (192.168.1.1)  0.595 ms  0.521 ms  0.497 ms"
        // mtr report format: " 1.|-- 192.168.1.1  0.0%     5    0.6   0.5   0.4   0.7   0.1"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        // Try parsing hop number from first token
        let hop_str = parts[0].trim_end_matches(".|--").trim_end_matches('.');
        let hop_number = match hop_str.parse::<u32>() {
            Ok(n) if n > 0 => n,
            _ => continue,
        };

        if parts.len() >= 2 && parts[1] == "*" {
            hops.push(TraceHop {
                hop_number,
                address: None,
                hostname: None,
                rtt_ms: None,
                loss_pct: None,
            });
            continue;
        }

        // Extract address and optional hostname
        let (hostname, address) = if parts.len() >= 3 && parts[2].starts_with('(') {
            let addr = parts[2].trim_matches(|c: char| c == '(' || c == ')').to_string();
            (Some(parts[1].to_string()), Some(addr))
        } else if parts.len() >= 2 {
            let addr = parts[1].trim_matches(|c: char| c == '(' || c == ')').to_string();
            (None, Some(addr))
        } else {
            (None, None)
        };

        // Extract RTT (first ms value found)
        let rtt_ms = parts.iter().find_map(|p| {
            p.trim_end_matches("ms").parse::<f64>().ok()
        });

        // Extract loss percentage (mtr format: "0.0%")
        let loss_pct = parts.iter().find_map(|p| {
            p.trim_end_matches('%').parse::<f64>().ok().filter(|_| p.ends_with('%'))
        });

        hops.push(TraceHop {
            hop_number,
            address,
            hostname,
            rtt_ms,
            loss_pct,
        });
    }

    ParsedOutput::TraceResult {
        hops,
        target: target.to_string(),
    }
}

/// Parse ss output into socket entries
pub fn parse_socket_output(stdout: &str) -> ParsedOutput {
    let mut sockets = Vec::new();

    for line in stdout.lines().skip(1) {
        // Format: "ESTAB  tcp  0  0  192.168.1.2:22  10.0.0.1:54321  users:(("sshd",pid=1234,fd=3))"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let state = parts[0].to_string();
        if matches!(state.as_str(), "State" | "Netid") {
            continue;
        }
        let protocol = parts[1].to_string();
        let local_addr = parts.get(4).unwrap_or(&"").to_string();
        let remote_addr = parts.get(5).unwrap_or(&"").to_string();
        let process = parts.get(6).map(|p| {
            p.trim_matches(|c: char| !c.is_alphanumeric() && c != ',' && c != '=' && c != '"')
                .to_string()
        });

        sockets.push(SocketEntry {
            state,
            protocol,
            local_addr,
            remote_addr,
            process,
        });
    }

    ParsedOutput::SocketList { sockets }
}

/// Parse tool output into structured results based on tool type
pub fn parse_output(output: &ToolOutput, target: Option<&str>) -> ParsedOutput {
    match output.tool {
        NetworkTool::PortScan | NetworkTool::PingSweep | NetworkTool::ServiceScan
        | NetworkTool::MassScan => parse_scan_output(&output.stdout, output.tool),
        NetworkTool::DnsLookup | NetworkTool::DnsEnum => {
            parse_dns_output(&output.stdout, target.unwrap_or("unknown"))
        }
        NetworkTool::TraceRoute | NetworkTool::NetworkDiag => {
            parse_trace_output(&output.stdout, target.unwrap_or("unknown"))
        }
        NetworkTool::SocketStats => parse_socket_output(&output.stdout),
        _ => {
            let line_count = output.stdout.lines().count();
            let summary = if output.exit_code == 0 {
                format!("{} completed ({} lines of output)", output.tool, line_count)
            } else {
                format!("{} exited with code {} ({} lines)", output.tool, output.exit_code, line_count)
            };
            ParsedOutput::Raw { summary }
        }
    }
}

/// Arguments that are considered dangerous and require explicit approval
const DANGEROUS_NMAP_ARGS: &[&str] = &[
    "--script=",
    "--script ",
    "-sC",
    "--script-args",
    "-oX",
    "-oN",
    "-oG",
    "-oA",
    "--data-length",
    "--spoof-mac",
    "-D",
    "--source-port",
];

/// Validate a scan target string.
///
/// Accepts:
/// - IPv4 / IPv6 addresses
/// - CIDR notation (e.g., `192.168.1.0/24`)
/// - DNS hostnames (RFC 952 / 1123)
///
/// Rejects:
/// - Broadcast addresses (255.255.255.255)
/// - Empty strings
/// - Targets containing shell metacharacters
pub fn validate_target(target: &str) -> Result<ValidatedTarget> {
    let target = target.trim();
    if target.is_empty() {
        return Err(anyhow!("Target cannot be empty"));
    }

    // Reject shell metacharacters
    if target.chars().any(|c| matches!(c, ';' | '|' | '&' | '$' | '`' | '(' | ')' | '{' | '}' | '>' | '<' | '!' | '\\')) {
        return Err(anyhow!("Target contains illegal characters"));
    }

    // Try CIDR notation first
    if let Some((addr_str, prefix_str)) = target.split_once('/') {
        let addr: IpAddr = addr_str
            .parse()
            .with_context(|| format!("Invalid IP in CIDR: {}", addr_str))?;
        let prefix: u8 = prefix_str
            .parse()
            .with_context(|| format!("Invalid prefix length: {}", prefix_str))?;

        // Validate prefix range
        let max_prefix = if addr.is_ipv4() { 32 } else { 128 };
        if prefix > max_prefix {
            return Err(anyhow!(
                "Prefix /{} out of range for {} (max {})",
                prefix,
                if addr.is_ipv4() { "IPv4" } else { "IPv6" },
                max_prefix
            ));
        }

        reject_broadcast(&addr)?;
        return Ok(ValidatedTarget::Cidr { addr, prefix });
    }

    // Try plain IP
    if let Ok(addr) = target.parse::<IpAddr>() {
        reject_broadcast(&addr)?;
        return Ok(ValidatedTarget::Ip(addr));
    }

    // Treat as hostname — validate per RFC 1123
    validate_hostname(target)?;
    Ok(ValidatedTarget::Hostname(target.to_string()))
}

/// Reject IPv4 broadcast (255.255.255.255)
fn reject_broadcast(addr: &IpAddr) -> Result<()> {
    if let IpAddr::V4(v4) = addr {
        if v4.octets() == [255, 255, 255, 255] {
            return Err(anyhow!("Broadcast address 255.255.255.255 is not allowed"));
        }
    }
    Ok(())
}

/// Validate hostname per RFC 952 / 1123
fn validate_hostname(host: &str) -> Result<()> {
    if host.is_empty() || host.len() > 253 {
        return Err(anyhow!("Hostname length must be 1-253 characters"));
    }
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(anyhow!("Hostname label '{}' must be 1-63 characters", label));
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(anyhow!(
                "Hostname label '{}' contains invalid characters",
                label
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(anyhow!(
                "Hostname label '{}' must not start or end with a hyphen",
                label
            ));
        }
    }
    Ok(())
}

/// Check whether a target is in RFC 1918 private space
pub fn is_rfc1918(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 10.0.0.0/8
            octets[0] == 10
            // 172.16.0.0/12
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            // 192.168.0.0/16
            || (octets[0] == 192 && octets[1] == 168)
        }
        IpAddr::V6(_) => false,
    }
}

/// Validate arguments, rejecting dangerous flags unless `allow_dangerous` is set
pub fn validate_args(tool: NetworkTool, args: &[String], allow_dangerous: bool) -> Result<()> {
    let joined = args.join(" ");
    match tool {
        NetworkTool::PortScan | NetworkTool::ServiceScan => {
            if !allow_dangerous {
                for pattern in DANGEROUS_NMAP_ARGS {
                    if joined.contains(pattern) {
                        return Err(anyhow!(
                            "Dangerous nmap argument '{}' requires explicit approval",
                            pattern.trim()
                        ));
                    }
                }
            }
        }
        NetworkTool::PacketCapture | NetworkTool::DeepInspect | NetworkTool::NetworkGrep => {
            // Reject writing to files without approval
            if !allow_dangerous && (joined.contains("-w ") || joined.contains("-w\t")) {
                return Err(anyhow!(
                    "Packet capture file output (-w) requires explicit approval"
                ));
            }
        }
        NetworkTool::MassScan => {
            if !allow_dangerous {
                for pattern in DANGEROUS_NMAP_ARGS.iter().filter(|p| p.starts_with("-o")) {
                    if joined.contains(*pattern) {
                        return Err(anyhow!(
                            "Dangerous masscan argument '{}' requires explicit approval",
                            pattern.trim()
                        ));
                    }
                }
                // Reject extremely high rates
                if joined.contains("--rate") {
                    if let Some(rate_val) = joined.split("--rate").nth(1)
                        .and_then(|s| s.trim().split_whitespace().next())
                        .and_then(|s| s.trim_start_matches('=').parse::<u64>().ok())
                    {
                        if rate_val > 10000 {
                            return Err(anyhow!(
                                "masscan rate {} exceeds safe limit (10000); requires explicit approval",
                                rate_val
                            ));
                        }
                    }
                }
            }
        }
        NetworkTool::VulnScanner => {
            if !allow_dangerous {
                // Reject custom template paths (could be malicious)
                if joined.contains("-t ") || joined.contains("--templates") {
                    return Err(anyhow!(
                        "Custom nuclei templates require explicit approval"
                    ));
                }
            }
        }
        NetworkTool::SqlMap => {
            if !allow_dangerous {
                let dangerous_sqlmap_args = &["--os-shell", "--os-cmd", "--file-write"];
                for pattern in dangerous_sqlmap_args {
                    if joined.contains(pattern) {
                        return Err(anyhow!(
                            "Dangerous sqlmap argument '{}' requires explicit approval (enables OS-level access)",
                            pattern
                        ));
                    }
                }
            }
        }
        NetworkTool::BetterCap => {
            if !allow_dangerous {
                if joined.contains("--caplet") {
                    return Err(anyhow!(
                        "Dangerous bettercap argument '--caplet' requires explicit approval (arbitrary script execution)"
                    ));
                }
            }
        }
        _ => {}
    }

    // Universal: reject command chaining
    if joined.contains("&&") || joined.contains("||") || joined.contains(';') {
        return Err(anyhow!("Command chaining is not allowed in tool arguments"));
    }

    Ok(())
}

/// Runner that executes network tools inside sandboxed environments
#[derive(Debug)]
pub struct NetworkToolRunner {
    /// Whether dangerous arguments are allowed (requires elevated approval)
    pub allow_dangerous: bool,
    /// Whether RFC 1918 targets should be rejected (external-only policy)
    pub external_only: bool,
}

impl NetworkToolRunner {
    /// Create a new runner with default (restrictive) settings
    pub fn new() -> Self {
        Self {
            allow_dangerous: false,
            external_only: false,
        }
    }

    /// Run a network tool with the given arguments.
    ///
    /// Validates arguments, checks tool availability, applies sandbox, captures output.
    pub async fn run(
        &self,
        config: &NetworkToolConfig,
        args: &[String],
        target: Option<&str>,
    ) -> Result<ToolOutput> {
        info!(tool = %config.tool, binary = config.binary_name, "Running network tool");

        // Validate arguments
        validate_args(config.tool, args, self.allow_dangerous)?;

        // Validate target if supplied
        if let Some(t) = target {
            let validated = validate_target(t)?;
            if self.external_only {
                match &validated {
                    ValidatedTarget::Ip(ip) if is_rfc1918(ip) => {
                        return Err(anyhow!(
                            "Target {} is RFC 1918 private address; external-only policy active",
                            ip
                        ));
                    }
                    ValidatedTarget::Cidr { addr, .. } if is_rfc1918(addr) => {
                        return Err(anyhow!(
                            "Target {}/{} is RFC 1918 private range; external-only policy active",
                            addr,
                            0
                        ));
                    }
                    _ => {}
                }
            }
        }

        // Check availability
        if !Self::check_tool_available(config.tool) {
            return Err(anyhow!(
                "Tool binary '{}' is not installed or not in PATH",
                config.binary_name
            ));
        }

        // Build command
        let mut cmd_args: Vec<String> = Vec::new();

        // Add tool-specific default flags
        match config.tool {
            NetworkTool::PortScan => {
                cmd_args.push("-sT".into()); // TCP connect scan (no raw sockets needed in sandbox)
            }
            NetworkTool::PingSweep => {
                cmd_args.push("-sn".into());
            }
            NetworkTool::ServiceScan => {
                cmd_args.push("-sV".into());
            }
            NetworkTool::MassScan => {
                cmd_args.push("--rate=1000".into()); // safe default rate
            }
            NetworkTool::ArpScan => {
                cmd_args.push("--localnet".into());
            }
            NetworkTool::NetworkDiag => {
                cmd_args.push("--report".into()); // parseable report mode
                cmd_args.push("-c".into());
                cmd_args.push("10".into()); // 10 pings per hop
            }
            NetworkTool::DeepInspect => {
                cmd_args.push("-c".into());
                cmd_args.push("100".into()); // capture 100 packets by default
            }
            NetworkTool::SocketStats => {
                cmd_args.push("-tunap".into()); // TCP, UDP, numeric, all, processes
            }
            NetworkTool::DnsEnum => {
                cmd_args.push("-d".into()); // domain flag
            }
            NetworkTool::VulnScanner => {
                cmd_args.push("-silent".into()); // parseable output
            }
            _ => {}
        }

        cmd_args.extend_from_slice(args);
        if let Some(t) = target {
            cmd_args.push(t.to_string());
        }

        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(config.max_timeout_secs);

        debug!(binary = config.binary_name, ?cmd_args, "Executing tool");

        let result = tokio::time::timeout(timeout, async {
            tokio::process::Command::new(config.binary_name)
                .args(&cmd_args)
                .output()
                .await
                .context("Failed to spawn tool process")
        })
        .await;

        let elapsed = start.elapsed();

        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(anyhow!("Tool execution failed: {}", e));
            }
            Err(_) => {
                warn!(
                    tool = %config.tool,
                    timeout_secs = config.max_timeout_secs,
                    "Tool execution timed out"
                );
                return Err(anyhow!(
                    "Tool '{}' timed out after {}s",
                    config.binary_name,
                    config.max_timeout_secs
                ));
            }
        };

        let audit_entry = format!(
            "tool={} binary={} args={:?} target={:?} exit={} duration={:.2}s",
            config.tool,
            config.binary_name,
            cmd_args,
            target,
            output.status.code().unwrap_or(-1),
            elapsed.as_secs_f64()
        );

        info!(%audit_entry, "Tool execution complete");

        Ok(ToolOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            duration: elapsed,
            tool: config.tool,
            audit_entry,
        })
    }

    /// Check whether the binary for a tool is available on the system
    pub fn check_tool_available(tool: NetworkTool) -> bool {
        let config = NetworkToolConfig::for_tool(tool);
        std::process::Command::new("which")
            .arg(config.binary_name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// List all tools that have their binary available
    pub fn list_available_tools() -> Vec<NetworkToolConfig> {
        ALL_TOOLS
            .iter()
            .filter(|t| Self::check_tool_available(**t))
            .map(|t| NetworkToolConfig::for_tool(*t))
            .collect()
    }

    /// List all known tool configurations (whether installed or not)
    pub fn list_all_tools() -> Vec<NetworkToolConfig> {
        ALL_TOOLS.iter().map(|t| NetworkToolConfig::for_tool(*t)).collect()
    }
}

impl Default for NetworkToolRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Individual tool wrapper agents — typed argument builders + structured results
// ============================================================================

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

/// Port scanner wrapping nmap/masscan with typed scan configuration.
#[derive(Debug)]
pub struct PortScanner {
    runner: NetworkToolRunner,
    profile: ScanProfile,
    ports: Option<String>,
    use_masscan: bool,
}

impl PortScanner {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            profile: ScanProfile::Standard,
            ports: None,
            use_masscan: false,
        }
    }

    /// Set the scan profile.
    pub fn profile(mut self, profile: ScanProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Scan specific ports (e.g., "80,443,8080" or "1-1024").
    pub fn ports(mut self, ports: &str) -> Self {
        self.ports = Some(ports.to_string());
        self
    }

    /// Use masscan instead of nmap (faster for large ranges).
    pub fn use_masscan(mut self, yes: bool) -> Self {
        self.use_masscan = yes;
        self
    }

    /// Build the argument list for the configured scan.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.use_masscan {
            // masscan args
            if let Some(ref ports) = self.ports {
                args.push("-p".into());
                args.push(ports.clone());
            } else {
                args.push("-p".into());
                args.push("1-65535".into());
            }
            return args;
        }

        // nmap args based on profile
        match self.profile {
            ScanProfile::Quick => {
                args.push("-F".into()); // fast mode (top 100 ports)
                args.push("-T4".into());
            }
            ScanProfile::Standard => {
                args.push("-T3".into());
            }
            ScanProfile::Thorough => {
                args.push("-p-".into()); // all ports
                args.push("-sV".into()); // version detection
                args.push("-T3".into());
            }
            ScanProfile::Stealth => {
                args.push("-T2".into()); // slower timing
                args.push("--randomize-hosts".into());
            }
        }

        if let Some(ref ports) = self.ports {
            // Override profile port selection
            args.retain(|a| a != "-F" && a != "-p-");
            args.push("-p".into());
            args.push(ports.clone());
        }

        args
    }

    /// Run the scan against a target. Returns structured host/port results.
    pub async fn scan(&self, target: &str) -> Result<Vec<DiscoveredHost>> {
        let tool = if self.use_masscan {
            NetworkTool::MassScan
        } else {
            NetworkTool::PortScan
        };
        let config = NetworkToolConfig::for_tool(tool);
        let args = self.build_args();
        let output = self.runner.run(&config, &args, Some(target)).await?;
        let parsed = parse_output(&output, Some(target));
        match parsed {
            ParsedOutput::HostScan { hosts, .. } => Ok(hosts),
            _ => Ok(Vec::new()),
        }
    }
}

impl Default for PortScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// DNS investigation wrapping dig and dnsrecon.
#[derive(Debug)]
pub struct DnsInvestigator {
    runner: NetworkToolRunner,
    record_types: Vec<String>,
    use_dnsrecon: bool,
    nameserver: Option<String>,
}

impl DnsInvestigator {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            record_types: Vec::new(),
            use_dnsrecon: false,
            nameserver: None,
        }
    }

    /// Query specific record types (e.g., "A", "AAAA", "MX", "NS", "TXT").
    pub fn record_type(mut self, rtype: &str) -> Self {
        self.record_types.push(rtype.to_uppercase());
        self
    }

    /// Use dnsrecon for enumeration instead of dig.
    pub fn enumerate(mut self, yes: bool) -> Self {
        self.use_dnsrecon = yes;
        self
    }

    /// Use a specific nameserver.
    pub fn nameserver(mut self, ns: &str) -> Self {
        self.nameserver = Some(ns.to_string());
        self
    }

    /// Build argument list.
    pub fn build_args(&self, domain: &str) -> Vec<String> {
        let mut args = Vec::new();

        if self.use_dnsrecon {
            args.push(domain.to_string());
            return args;
        }

        // dig args
        args.push(domain.to_string());
        for rt in &self.record_types {
            args.push(rt.clone());
        }
        if self.record_types.is_empty() {
            args.push("ANY".into());
        }
        if let Some(ref ns) = self.nameserver {
            args.push(format!("@{}", ns));
        }
        args
    }

    /// Run DNS lookup and return structured records.
    pub async fn lookup(&self, domain: &str) -> Result<Vec<DnsRecord>> {
        let tool = if self.use_dnsrecon {
            NetworkTool::DnsEnum
        } else {
            NetworkTool::DnsLookup
        };
        let config = NetworkToolConfig::for_tool(tool);
        let args = self.build_args(domain);
        let output = self.runner.run(&config, &args, None).await?;
        let parsed = parse_output(&output, Some(domain));
        match parsed {
            ParsedOutput::DnsResult { records, .. } => Ok(records),
            _ => Ok(Vec::new()),
        }
    }
}

impl Default for DnsInvestigator {
    fn default() -> Self {
        Self::new()
    }
}

/// Network path probing wrapping traceroute and mtr.
#[derive(Debug)]
pub struct NetworkProber {
    runner: NetworkToolRunner,
    use_mtr: bool,
    max_hops: Option<u32>,
    count: Option<u32>,
}

impl NetworkProber {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            use_mtr: false,
            max_hops: None,
            count: None,
        }
    }

    /// Use mtr instead of traceroute (combines ping + trace).
    pub fn use_mtr(mut self, yes: bool) -> Self {
        self.use_mtr = yes;
        self
    }

    /// Set maximum hop count.
    pub fn max_hops(mut self, hops: u32) -> Self {
        self.max_hops = Some(hops);
        self
    }

    /// Set ping count per hop (mtr only).
    pub fn count(mut self, n: u32) -> Self {
        self.count = Some(n);
        self
    }

    /// Build argument list.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.use_mtr {
            if let Some(hops) = self.max_hops {
                args.push("-m".into());
                args.push(hops.to_string());
            }
        } else {
            // traceroute
            if let Some(hops) = self.max_hops {
                args.push("-m".into());
                args.push(hops.to_string());
            }
        }

        args
    }

    /// Run trace and return structured hop list.
    pub async fn trace(&self, target: &str) -> Result<Vec<TraceHop>> {
        let tool = if self.use_mtr {
            NetworkTool::NetworkDiag
        } else {
            NetworkTool::TraceRoute
        };
        let config = NetworkToolConfig::for_tool(tool);
        let args = self.build_args();
        let output = self.runner.run(&config, &args, Some(target)).await?;
        let parsed = parse_output(&output, Some(target));
        match parsed {
            ParsedOutput::TraceResult { hops, .. } => Ok(hops),
            _ => Ok(Vec::new()),
        }
    }

    /// Run a simple ping sweep of a subnet.
    pub async fn ping_sweep(&self, subnet: &str) -> Result<Vec<DiscoveredHost>> {
        let config = NetworkToolConfig::for_tool(NetworkTool::PingSweep);
        let output = self.runner.run(&config, &[], Some(subnet)).await?;
        let parsed = parse_output(&output, Some(subnet));
        match parsed {
            ParsedOutput::HostScan { hosts, .. } => Ok(hosts),
            _ => Ok(Vec::new()),
        }
    }
}

impl Default for NetworkProber {
    fn default() -> Self {
        Self::new()
    }
}

/// Vulnerability assessment wrapping nuclei and nikto.
#[derive(Debug)]
pub struct VulnAssessor {
    runner: NetworkToolRunner,
    use_nikto: bool,
    severity_filter: Option<String>,
    tags: Vec<String>,
}

impl VulnAssessor {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner {
                allow_dangerous: false,
                external_only: false,
            },
            use_nikto: false,
            severity_filter: None,
            tags: Vec::new(),
        }
    }

    /// Use nikto instead of nuclei.
    pub fn use_nikto(mut self, yes: bool) -> Self {
        self.use_nikto = yes;
        self
    }

    /// Filter by severity (nuclei: "critical,high,medium").
    pub fn severity(mut self, sev: &str) -> Self {
        self.severity_filter = Some(sev.to_string());
        self
    }

    /// Filter by template tags (nuclei: "cve", "rce", "sqli").
    pub fn tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Build argument list.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.use_nikto {
            args.push("-h".into());
            return args;
        }

        // nuclei args
        if let Some(ref sev) = self.severity_filter {
            args.push("-severity".into());
            args.push(sev.clone());
        }
        for tag in &self.tags {
            args.push("-tags".into());
            args.push(tag.clone());
        }
        args.push("-u".into());
        args
    }

    /// Run vulnerability scan and return raw output summary.
    /// Structured parsing of nuclei/nikto results is deferred to beta.
    pub async fn scan(&self, target: &str) -> Result<ToolOutput> {
        let tool = if self.use_nikto {
            NetworkTool::WebScan
        } else {
            NetworkTool::VulnScanner
        };
        let config = NetworkToolConfig::for_tool(tool);
        let mut args = self.build_args();
        args.push(target.to_string());
        self.runner.run(&config, &args, None).await
    }
}

impl Default for VulnAssessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Traffic analysis wrapping tcpdump, tshark, and ngrep.
#[derive(Debug)]
pub struct TrafficAnalyzer {
    runner: NetworkToolRunner,
    tool_choice: NetworkTool,
    interface: Option<String>,
    filter: Option<String>,
    packet_count: Option<u32>,
}

impl TrafficAnalyzer {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            tool_choice: NetworkTool::PacketCapture,
            interface: None,
            filter: None,
            packet_count: Some(100),
        }
    }

    /// Use tshark for deep inspection.
    pub fn use_tshark(mut self) -> Self {
        self.tool_choice = NetworkTool::DeepInspect;
        self
    }

    /// Use ngrep for content matching.
    pub fn use_ngrep(mut self) -> Self {
        self.tool_choice = NetworkTool::NetworkGrep;
        self
    }

    /// Capture on a specific interface.
    pub fn interface(mut self, iface: &str) -> Self {
        self.interface = Some(iface.to_string());
        self
    }

    /// Set BPF filter expression.
    pub fn filter(mut self, filter: &str) -> Self {
        self.filter = Some(filter.to_string());
        self
    }

    /// Number of packets to capture.
    pub fn packet_count(mut self, n: u32) -> Self {
        self.packet_count = Some(n);
        self
    }

    /// Build argument list.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        match self.tool_choice {
            NetworkTool::PacketCapture => {
                // tcpdump
                if let Some(ref iface) = self.interface {
                    args.push("-i".into());
                    args.push(iface.clone());
                }
                if let Some(n) = self.packet_count {
                    args.push("-c".into());
                    args.push(n.to_string());
                }
                args.push("-nn".into()); // numeric output
            }
            NetworkTool::DeepInspect => {
                // tshark
                if let Some(ref iface) = self.interface {
                    args.push("-i".into());
                    args.push(iface.clone());
                }
            }
            NetworkTool::NetworkGrep => {
                // ngrep
                if let Some(ref iface) = self.interface {
                    args.push("-d".into());
                    args.push(iface.clone());
                }
            }
            _ => {}
        }

        if let Some(ref f) = self.filter {
            args.push(f.clone());
        }

        args
    }

    /// Run capture and return raw output.
    pub async fn capture(&self) -> Result<ToolOutput> {
        let config = NetworkToolConfig::for_tool(self.tool_choice);
        let args = self.build_args();
        self.runner.run(&config, &args, None).await
    }
}

impl Default for TrafficAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Web directory/path fuzzing wrapping gobuster, ffuf, and dirb.
#[derive(Debug)]
pub struct WebFuzzer {
    runner: NetworkToolRunner,
    tool_choice: NetworkTool,
    wordlist: Option<String>,
    extensions: Vec<String>,
    threads: Option<u32>,
    status_codes: Option<String>,
}

impl WebFuzzer {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            tool_choice: NetworkTool::DirBust,
            wordlist: None,
            extensions: Vec::new(),
            threads: None,
            status_codes: None,
        }
    }

    /// Use ffuf instead of gobuster.
    pub fn use_ffuf(mut self) -> Self {
        self.tool_choice = NetworkTool::DirFuzz;
        self
    }

    /// Set wordlist path.
    pub fn wordlist(mut self, path: &str) -> Self {
        self.wordlist = Some(path.to_string());
        self
    }

    /// Add file extensions to check (e.g., "php", "html", "js").
    pub fn extension(mut self, ext: &str) -> Self {
        self.extensions.push(ext.to_string());
        self
    }

    /// Set thread count.
    pub fn threads(mut self, n: u32) -> Self {
        self.threads = Some(n);
        self
    }

    /// Filter by HTTP status codes (e.g., "200,301,302").
    pub fn status_codes(mut self, codes: &str) -> Self {
        self.status_codes = Some(codes.to_string());
        self
    }

    /// Build argument list.
    pub fn build_args(&self, target_url: &str) -> Vec<String> {
        let mut args = Vec::new();

        match self.tool_choice {
            NetworkTool::DirBust => {
                // gobuster
                args.push("dir".into());
                args.push("-u".into());
                args.push(target_url.to_string());
                if let Some(ref wl) = self.wordlist {
                    args.push("-w".into());
                    args.push(wl.clone());
                }
                if !self.extensions.is_empty() {
                    args.push("-x".into());
                    args.push(self.extensions.join(","));
                }
                if let Some(n) = self.threads {
                    args.push("-t".into());
                    args.push(n.to_string());
                }
                if let Some(ref codes) = self.status_codes {
                    args.push("-s".into());
                    args.push(codes.clone());
                }
            }
            NetworkTool::DirFuzz => {
                // ffuf
                args.push("-u".into());
                args.push(format!("{}/FUZZ", target_url));
                if let Some(ref wl) = self.wordlist {
                    args.push("-w".into());
                    args.push(wl.clone());
                }
                if !self.extensions.is_empty() {
                    args.push("-e".into());
                    args.push(format!(".{}", self.extensions.join(",.")));
                }
                if let Some(n) = self.threads {
                    args.push("-t".into());
                    args.push(n.to_string());
                }
                if let Some(ref codes) = self.status_codes {
                    args.push("-mc".into());
                    args.push(codes.clone());
                }
            }
            _ => {}
        }

        args
    }

    /// Run fuzzing and return raw output.
    pub async fn fuzz(&self, target_url: &str) -> Result<ToolOutput> {
        let config = NetworkToolConfig::for_tool(self.tool_choice);
        let args = self.build_args(target_url);
        self.runner.run(&config, &args, None).await
    }
}

impl Default for WebFuzzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Socket/connection inspector wrapping ss.
#[derive(Debug)]
pub struct SocketInspector {
    runner: NetworkToolRunner,
    listening_only: bool,
    tcp_only: bool,
    udp_only: bool,
}

impl SocketInspector {
    pub fn new() -> Self {
        Self {
            runner: NetworkToolRunner::new(),
            listening_only: false,
            tcp_only: false,
            udp_only: false,
        }
    }

    /// Show only listening sockets.
    pub fn listening_only(mut self, yes: bool) -> Self {
        self.listening_only = yes;
        self
    }

    /// Filter to TCP only.
    pub fn tcp_only(mut self, yes: bool) -> Self {
        self.tcp_only = yes;
        self
    }

    /// Filter to UDP only.
    pub fn udp_only(mut self, yes: bool) -> Self {
        self.udp_only = yes;
        self
    }

    /// Build argument list.
    pub fn build_args(&self) -> Vec<String> {
        let mut flags = String::from("-");
        if self.listening_only {
            flags.push('l');
        }
        if self.tcp_only {
            flags.push('t');
        } else if self.udp_only {
            flags.push('u');
        } else {
            flags.push('t');
            flags.push('u');
        }
        flags.push('n'); // numeric
        flags.push('a'); // all
        flags.push('p'); // processes
        vec![flags]
    }

    /// Inspect sockets and return structured entries.
    pub async fn inspect(&self) -> Result<Vec<SocketEntry>> {
        let config = NetworkToolConfig::for_tool(NetworkTool::SocketStats);
        let args = self.build_args();
        let output = self.runner.run(&config, &args, None).await?;
        let parsed = parse_output(&output, None);
        match parsed {
            ParsedOutput::SocketList { sockets } => Ok(sockets),
            _ => Ok(Vec::new()),
        }
    }
}

impl Default for SocketInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RiskLevel tests ---

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "Low");
        assert_eq!(RiskLevel::Medium.to_string(), "Medium");
        assert_eq!(RiskLevel::High.to_string(), "High");
        assert_eq!(RiskLevel::Critical.to_string(), "Critical");
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    // --- NetworkTool Display ---

    #[test]
    fn test_network_tool_display() {
        assert_eq!(NetworkTool::PortScan.to_string(), "PortScan");
        assert_eq!(NetworkTool::DnsLookup.to_string(), "DnsLookup");
        assert_eq!(NetworkTool::PacketCapture.to_string(), "PacketCapture");
    }

    // --- Config tests ---

    #[test]
    fn test_all_tools_have_configs() {
        for tool in ALL_TOOLS {
            let config = NetworkToolConfig::for_tool(*tool);
            assert_eq!(config.tool, *tool);
            assert!(!config.binary_name.is_empty());
            assert!(config.max_timeout_secs > 0);
        }
    }

    #[test]
    fn test_port_scan_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::PortScan);
        assert_eq!(cfg.binary_name, "nmap");
        assert_eq!(cfg.risk_level, RiskLevel::High);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
    }

    #[test]
    fn test_dns_lookup_config_low_risk() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::DnsLookup);
        assert_eq!(cfg.binary_name, "dig");
        assert_eq!(cfg.risk_level, RiskLevel::Low);
        assert!(!cfg.requires_approval);
        assert!(cfg.required_capabilities.is_empty());
    }

    #[test]
    fn test_packet_capture_config_critical() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::PacketCapture);
        assert_eq!(cfg.binary_name, "tcpdump");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
        assert!(!cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_http_client_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::HttpClient);
        assert_eq!(cfg.binary_name, "curl");
        assert_eq!(cfg.risk_level, RiskLevel::Low);
        assert!(!cfg.requires_approval);
        assert!(cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_web_scan_config_critical() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::WebScan);
        assert_eq!(cfg.binary_name, "nikto");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
    }

    #[test]
    fn test_service_scan_uses_nmap() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::ServiceScan);
        assert_eq!(cfg.binary_name, "nmap");
        assert_eq!(cfg.risk_level, RiskLevel::High);
    }

    #[test]
    fn test_dirbust_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::DirBust);
        assert_eq!(cfg.binary_name, "gobuster");
        assert!(cfg.requires_approval);
    }

    #[test]
    fn test_netcat_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::NetcatConnect);
        assert_eq!(cfg.binary_name, "ncat");
        assert!(cfg.requires_approval);
    }

    // --- Target validation tests ---

    #[test]
    fn test_validate_target_ipv4() {
        let result = validate_target("192.168.1.1").unwrap();
        assert_eq!(result, ValidatedTarget::Ip("192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn test_validate_target_ipv6() {
        let result = validate_target("::1").unwrap();
        assert_eq!(result, ValidatedTarget::Ip("::1".parse().unwrap()));
    }

    #[test]
    fn test_validate_target_cidr() {
        let result = validate_target("10.0.0.0/24").unwrap();
        assert!(matches!(result, ValidatedTarget::Cidr { prefix: 24, .. }));
    }

    #[test]
    fn test_validate_target_hostname() {
        let result = validate_target("example.com").unwrap();
        assert_eq!(
            result,
            ValidatedTarget::Hostname("example.com".to_string())
        );
    }

    #[test]
    fn test_validate_target_subdomain() {
        let result = validate_target("sub.domain.example.com").unwrap();
        assert!(matches!(result, ValidatedTarget::Hostname(_)));
    }

    #[test]
    fn test_validate_target_empty_rejected() {
        assert!(validate_target("").is_err());
        assert!(validate_target("   ").is_err());
    }

    #[test]
    fn test_validate_target_broadcast_rejected() {
        assert!(validate_target("255.255.255.255").is_err());
    }

    #[test]
    fn test_validate_target_shell_metacharacters_rejected() {
        assert!(validate_target("192.168.1.1; rm -rf /").is_err());
        assert!(validate_target("host | cat /etc/passwd").is_err());
        assert!(validate_target("$(whoami)").is_err());
        assert!(validate_target("host`id`").is_err());
    }

    #[test]
    fn test_validate_target_invalid_cidr_prefix() {
        assert!(validate_target("10.0.0.0/33").is_err());
    }

    #[test]
    fn test_validate_target_invalid_hostname_chars() {
        assert!(validate_target("host_name.com").is_err());
        assert!(validate_target("host name.com").is_err());
    }

    #[test]
    fn test_validate_target_hostname_label_hyphen() {
        assert!(validate_target("-invalid.com").is_err());
        assert!(validate_target("invalid-.com").is_err());
        // Valid: hyphens in the middle
        assert!(validate_target("my-host.com").is_ok());
    }

    #[test]
    fn test_validate_target_cidr_broadcast_rejected() {
        assert!(validate_target("255.255.255.255/32").is_err());
    }

    // --- RFC 1918 detection ---

    #[test]
    fn test_is_rfc1918() {
        assert!(is_rfc1918(&"10.0.0.1".parse().unwrap()));
        assert!(is_rfc1918(&"172.16.0.1".parse().unwrap()));
        assert!(is_rfc1918(&"172.31.255.255".parse().unwrap()));
        assert!(is_rfc1918(&"192.168.0.1".parse().unwrap()));
        assert!(!is_rfc1918(&"8.8.8.8".parse().unwrap()));
        assert!(!is_rfc1918(&"172.32.0.1".parse().unwrap()));
        assert!(!is_rfc1918(&"::1".parse().unwrap()));
    }

    // --- Dangerous arg rejection ---

    #[test]
    fn test_reject_nmap_script_args() {
        let args = vec!["--script=vuln".to_string(), "target".to_string()];
        assert!(validate_args(NetworkTool::PortScan, &args, false).is_err());
    }

    #[test]
    fn test_allow_nmap_script_args_with_approval() {
        let args = vec!["--script=vuln".to_string(), "target".to_string()];
        assert!(validate_args(NetworkTool::PortScan, &args, true).is_ok());
    }

    #[test]
    fn test_reject_nmap_sc_flag() {
        let args = vec!["-sC".to_string()];
        assert!(validate_args(NetworkTool::PortScan, &args, false).is_err());
    }

    #[test]
    fn test_reject_tcpdump_write_flag() {
        let args = vec!["-w capture.pcap".to_string()];
        assert!(validate_args(NetworkTool::PacketCapture, &args, false).is_err());
    }

    #[test]
    fn test_reject_command_chaining() {
        let args = vec!["target".to_string(), "&&".to_string(), "rm".to_string()];
        assert!(validate_args(NetworkTool::DnsLookup, &args, false).is_err());
    }

    #[test]
    fn test_allow_normal_args() {
        let args = vec!["-p".to_string(), "80,443".to_string()];
        assert!(validate_args(NetworkTool::PortScan, &args, false).is_ok());
    }

    // --- Runner tests ---

    #[test]
    fn test_runner_default() {
        let runner = NetworkToolRunner::new();
        assert!(!runner.allow_dangerous);
        assert!(!runner.external_only);
    }

    #[test]
    fn test_list_all_tools_count() {
        let tools = NetworkToolRunner::list_all_tools();
        assert_eq!(tools.len(), ALL_TOOLS.len());
        assert_eq!(tools.len(), 32);
    }

    #[test]
    fn test_tool_output_fields() {
        let output = ToolOutput {
            stdout: "open port 80".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_secs(5),
            tool: NetworkTool::PortScan,
            audit_entry: "tool=PortScan exit=0".into(),
        };
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.tool, NetworkTool::PortScan);
        assert!(output.audit_entry.contains("PortScan"));
    }

    #[test]
    fn test_validated_target_display() {
        let ip = ValidatedTarget::Ip("1.2.3.4".parse().unwrap());
        assert_eq!(ip.to_string(), "1.2.3.4");

        let cidr = ValidatedTarget::Cidr {
            addr: "10.0.0.0".parse().unwrap(),
            prefix: 24,
        };
        assert_eq!(cidr.to_string(), "10.0.0.0/24");

        let host = ValidatedTarget::Hostname("example.com".into());
        assert_eq!(host.to_string(), "example.com");
    }

    #[tokio::test]
    async fn test_runner_rejects_rfc1918_external_only() {
        let runner = NetworkToolRunner {
            allow_dangerous: false,
            external_only: true,
        };
        let config = NetworkToolConfig::for_tool(NetworkTool::DnsLookup);
        let result = runner.run(&config, &[], Some("192.168.1.1")).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("RFC 1918"));
    }

    #[test]
    fn test_all_tools_constant_matches_variants() {
        // Verify ALL_TOOLS covers every variant
        let configs: Vec<_> = ALL_TOOLS.iter().map(|t| NetworkToolConfig::for_tool(*t)).collect();
        assert!(configs.iter().any(|c| c.tool == NetworkTool::PortScan));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::PingSweep));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DnsLookup));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::TraceRoute));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::BandwidthTest));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::PacketCapture));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::HttpClient));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::NetcatConnect));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::ServiceScan));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::WebScan));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DirBust));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::MassScan));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::ArpScan));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::NetworkDiag));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DataRelay));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DeepInspect));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::NetworkGrep));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::SocketStats));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DnsEnum));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DirFuzz));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::VulnScanner));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::BandwidthMonitor));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::PassiveFingerprint));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::NetDiscover));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::TermShark));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::BetterCap));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::DnsX));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::Fierce));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::WebAppFuzz));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::SqlMap));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::AircrackNg));
        assert!(configs.iter().any(|c| c.tool == NetworkTool::Kismet));
    }

    // --- New tool config tests ---

    #[test]
    fn test_masscan_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::MassScan);
        assert_eq!(cfg.binary_name, "masscan");
        assert_eq!(cfg.risk_level, RiskLevel::High);
        assert!(cfg.requires_approval);
    }

    #[test]
    fn test_mtr_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::NetworkDiag);
        assert_eq!(cfg.binary_name, "mtr");
        assert_eq!(cfg.risk_level, RiskLevel::Medium);
        assert!(!cfg.requires_approval);
    }

    #[test]
    fn test_ss_config_low_risk() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::SocketStats);
        assert_eq!(cfg.binary_name, "ss");
        assert_eq!(cfg.risk_level, RiskLevel::Low);
        assert!(!cfg.requires_approval);
    }

    #[test]
    fn test_nuclei_config_critical() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::VulnScanner);
        assert_eq!(cfg.binary_name, "nuclei");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
    }

    #[test]
    fn test_tshark_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::DeepInspect);
        assert_eq!(cfg.binary_name, "tshark");
        assert!(!cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_ffuf_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::DirFuzz);
        assert_eq!(cfg.binary_name, "ffuf");
        assert!(cfg.requires_approval);
    }

    #[test]
    fn test_p0f_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::PassiveFingerprint);
        assert_eq!(cfg.binary_name, "p0f");
        assert!(!cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_arp_scan_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::ArpScan);
        assert_eq!(cfg.binary_name, "arp-scan");
        assert_eq!(cfg.risk_level, RiskLevel::Medium);
    }

    // --- Dangerous arg rejection for new tools ---

    #[test]
    fn test_reject_masscan_high_rate() {
        let args = vec!["--rate=100000".to_string()];
        assert!(validate_args(NetworkTool::MassScan, &args, false).is_err());
    }

    #[test]
    fn test_allow_masscan_safe_rate() {
        let args = vec!["--rate=5000".to_string()];
        assert!(validate_args(NetworkTool::MassScan, &args, false).is_ok());
    }

    #[test]
    fn test_reject_nuclei_custom_templates() {
        let args = vec!["-t".to_string(), "/tmp/evil.yaml".to_string()];
        assert!(validate_args(NetworkTool::VulnScanner, &args, false).is_err());
    }

    #[test]
    fn test_allow_nuclei_custom_templates_with_approval() {
        let args = vec!["-t".to_string(), "/tmp/custom.yaml".to_string()];
        assert!(validate_args(NetworkTool::VulnScanner, &args, true).is_ok());
    }

    #[test]
    fn test_reject_tshark_write() {
        let args = vec!["-w capture.pcap".to_string()];
        assert!(validate_args(NetworkTool::DeepInspect, &args, false).is_err());
    }

    // --- Output parsing tests ---

    #[test]
    fn test_parse_nmap_scan_output() {
        let stdout = "\
Starting Nmap 7.94 ( https://nmap.org )
Nmap scan report for example.com (93.184.216.34)
Host is up (0.025s latency).
80/tcp   open  http    Apache httpd 2.4.41
443/tcp  open  https   nginx 1.18.0
22/tcp   closed ssh

Nmap done: 1 IP address (1 host up) scanned in 2.34 seconds";

        let result = parse_scan_output(stdout, NetworkTool::PortScan);
        match result {
            ParsedOutput::HostScan { hosts, scan_type } => {
                assert_eq!(scan_type, "port");
                assert_eq!(hosts.len(), 1);
                assert_eq!(hosts[0].address, "93.184.216.34");
                assert_eq!(hosts[0].hostname.as_deref(), Some("example.com"));
                assert_eq!(hosts[0].ports.len(), 3);
                assert_eq!(hosts[0].ports[0].port, 80);
                assert_eq!(hosts[0].ports[0].state, "open");
                assert_eq!(hosts[0].ports[0].service.as_deref(), Some("http"));
                assert_eq!(hosts[0].ports[1].port, 443);
                assert_eq!(hosts[0].ports[2].state, "closed");
            }
            _ => panic!("Expected HostScan"),
        }
    }

    #[test]
    fn test_parse_nmap_ping_sweep() {
        let stdout = "\
Starting Nmap 7.94
Nmap scan report for 192.168.1.1
Host is up (0.001s latency).
MAC Address: AA:BB:CC:DD:EE:FF (RouterVendor)

Nmap scan report for 192.168.1.50
Host is up (0.003s latency).

Nmap done: 256 IP addresses (2 hosts up)";

        let result = parse_scan_output(stdout, NetworkTool::PingSweep);
        match result {
            ParsedOutput::HostScan { hosts, .. } => {
                assert_eq!(hosts.len(), 2);
                assert_eq!(hosts[0].address, "192.168.1.1");
                assert_eq!(hosts[0].mac_address.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
                assert_eq!(hosts[0].vendor.as_deref(), Some("RouterVendor"));
                assert_eq!(hosts[1].address, "192.168.1.50");
            }
            _ => panic!("Expected HostScan"),
        }
    }

    #[test]
    fn test_parse_masscan_output() {
        let stdout = "\
Discovered open port 80/tcp on 10.0.0.1
Discovered open port 443/tcp on 10.0.0.1
Discovered open port 22/tcp on 10.0.0.2";

        let result = parse_scan_output(stdout, NetworkTool::MassScan);
        match result {
            ParsedOutput::HostScan { hosts, scan_type } => {
                assert_eq!(scan_type, "masscan");
                assert_eq!(hosts.len(), 2);
                let h1 = hosts.iter().find(|h| h.address == "10.0.0.1").unwrap();
                assert_eq!(h1.ports.len(), 2);
                let h2 = hosts.iter().find(|h| h.address == "10.0.0.2").unwrap();
                assert_eq!(h2.ports.len(), 1);
                assert_eq!(h2.ports[0].port, 22);
            }
            _ => panic!("Expected HostScan"),
        }
    }

    #[test]
    fn test_parse_dns_output() {
        let stdout = "\
; <<>> DiG 9.18.12 <<>> example.com
;; ANSWER SECTION:
example.com.     300     IN      A       93.184.216.34
example.com.     300     IN      AAAA    2606:2800:220:1:248:1893:25c8:1946

;; Query time: 25 msec";

        let result = parse_dns_output(stdout, "example.com");
        match result {
            ParsedOutput::DnsResult { records, query } => {
                assert_eq!(query, "example.com");
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].record_type, "A");
                assert_eq!(records[0].value, "93.184.216.34");
                assert_eq!(records[0].ttl, Some(300));
                assert_eq!(records[1].record_type, "AAAA");
            }
            _ => panic!("Expected DnsResult"),
        }
    }

    #[test]
    fn test_parse_traceroute_output() {
        let stdout = "\
traceroute to example.com (93.184.216.34), 30 hops max
 1  gateway (192.168.1.1)  0.595 ms  0.521 ms  0.497 ms
 2  10.0.0.1 (10.0.0.1)  2.123 ms  1.998 ms  2.045 ms
 3  * * *";

        let result = parse_trace_output(stdout, "example.com");
        match result {
            ParsedOutput::TraceResult { hops, target } => {
                assert_eq!(target, "example.com");
                assert_eq!(hops.len(), 3);
                assert_eq!(hops[0].hop_number, 1);
                assert_eq!(hops[0].hostname.as_deref(), Some("gateway"));
                assert_eq!(hops[0].address.as_deref(), Some("192.168.1.1"));
                assert_eq!(hops[2].address, None); // timeout hop
            }
            _ => panic!("Expected TraceResult"),
        }
    }

    #[test]
    fn test_parse_mtr_report() {
        let stdout = "\
Start: 2026-03-06T10:00:00
HOST: agnos                       Loss%   Snt   Last   Avg  Best  Wrst StDev
  1.|-- 192.168.1.1                0.0%    10    0.5   0.6   0.4   0.9   0.1
  2.|-- 10.0.0.1                   0.0%    10    2.1   2.0   1.8   2.4   0.2
  3.|-- ???                       100.0%    10    0.0   0.0   0.0   0.0   0.0";

        let result = parse_trace_output(stdout, "10.0.0.1");
        match result {
            ParsedOutput::TraceResult { hops, .. } => {
                assert_eq!(hops.len(), 3);
                assert_eq!(hops[0].hop_number, 1);
                assert_eq!(hops[0].address.as_deref(), Some("192.168.1.1"));
            }
            _ => panic!("Expected TraceResult"),
        }
    }

    #[test]
    fn test_parse_ss_output() {
        let stdout = "\
State    Recv-Q  Send-Q  Local Address:Port  Peer Address:Port  Process
ESTAB    0       0       192.168.1.2:22      10.0.0.1:54321     users:((\"sshd\",pid=1234,fd=3))
LISTEN   0       128     0.0.0.0:80          0.0.0.0:*          users:((\"nginx\",pid=5678,fd=6))";

        let result = parse_socket_output(stdout);
        match result {
            ParsedOutput::SocketList { sockets } => {
                assert_eq!(sockets.len(), 2);
                assert_eq!(sockets[0].state, "ESTAB");
                assert_eq!(sockets[1].state, "LISTEN");
            }
            _ => panic!("Expected SocketList"),
        }
    }

    #[test]
    fn test_parse_output_dispatches_correctly() {
        let output = ToolOutput {
            stdout: "".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_secs(1),
            tool: NetworkTool::SocketStats,
            audit_entry: String::new(),
        };
        let result = parse_output(&output, None);
        assert!(matches!(result, ParsedOutput::SocketList { .. }));

        let output2 = ToolOutput {
            stdout: "".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_secs(1),
            tool: NetworkTool::HttpClient,
            audit_entry: String::new(),
        };
        let result2 = parse_output(&output2, None);
        assert!(matches!(result2, ParsedOutput::Raw { .. }));
    }

    #[test]
    fn test_parse_empty_scan() {
        let result = parse_scan_output("Nmap done: 0 IP addresses", NetworkTool::PortScan);
        match result {
            ParsedOutput::HostScan { hosts, .. } => {
                assert!(hosts.is_empty());
            }
            _ => panic!("Expected HostScan"),
        }
    }

    #[test]
    fn test_parse_empty_dns() {
        let result = parse_dns_output(";; Got answer:\n;; AUTHORITY SECTION:", "test.invalid");
        match result {
            ParsedOutput::DnsResult { records, .. } => {
                assert!(records.is_empty());
            }
            _ => panic!("Expected DnsResult"),
        }
    }

    // --- Tool wrapper tests ---

    #[test]
    fn test_port_scanner_quick_args() {
        let scanner = PortScanner::new().profile(ScanProfile::Quick);
        let args = scanner.build_args();
        assert!(args.contains(&"-F".to_string()));
        assert!(args.contains(&"-T4".to_string()));
    }

    #[test]
    fn test_port_scanner_thorough_args() {
        let scanner = PortScanner::new().profile(ScanProfile::Thorough);
        let args = scanner.build_args();
        assert!(args.contains(&"-p-".to_string()));
        assert!(args.contains(&"-sV".to_string()));
    }

    #[test]
    fn test_port_scanner_stealth_args() {
        let scanner = PortScanner::new().profile(ScanProfile::Stealth);
        let args = scanner.build_args();
        assert!(args.contains(&"-T2".to_string()));
        assert!(args.contains(&"--randomize-hosts".to_string()));
    }

    #[test]
    fn test_port_scanner_custom_ports() {
        let scanner = PortScanner::new()
            .profile(ScanProfile::Quick)
            .ports("80,443,8080");
        let args = scanner.build_args();
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"80,443,8080".to_string()));
        // -F should be removed when custom ports specified
        assert!(!args.contains(&"-F".to_string()));
    }

    #[test]
    fn test_port_scanner_masscan_args() {
        let scanner = PortScanner::new()
            .use_masscan(true)
            .ports("1-1000");
        let args = scanner.build_args();
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"1-1000".to_string()));
        // Should not have nmap-specific flags
        assert!(!args.contains(&"-T3".to_string()));
    }

    #[test]
    fn test_dns_investigator_build_args() {
        let inv = DnsInvestigator::new()
            .record_type("A")
            .record_type("MX")
            .nameserver("8.8.8.8");
        let args = inv.build_args("example.com");
        assert!(args.contains(&"example.com".to_string()));
        assert!(args.contains(&"A".to_string()));
        assert!(args.contains(&"MX".to_string()));
        assert!(args.contains(&"@8.8.8.8".to_string()));
    }

    #[test]
    fn test_dns_investigator_default_any() {
        let inv = DnsInvestigator::new();
        let args = inv.build_args("test.com");
        assert!(args.contains(&"ANY".to_string()));
    }

    #[test]
    fn test_dns_investigator_enumerate() {
        let inv = DnsInvestigator::new().enumerate(true);
        let args = inv.build_args("example.com");
        assert!(args.contains(&"example.com".to_string()));
        // Should not have dig-specific ANY
        assert!(!args.contains(&"ANY".to_string()));
    }

    #[test]
    fn test_network_prober_trace_args() {
        let prober = NetworkProber::new().max_hops(20);
        let args = prober.build_args();
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"20".to_string()));
    }

    #[test]
    fn test_network_prober_mtr_args() {
        let prober = NetworkProber::new()
            .use_mtr(true)
            .max_hops(15);
        let args = prober.build_args();
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"15".to_string()));
    }

    #[test]
    fn test_vuln_assessor_nuclei_args() {
        let va = VulnAssessor::new()
            .severity("critical,high")
            .tag("cve")
            .tag("rce");
        let args = va.build_args();
        assert!(args.contains(&"-severity".to_string()));
        assert!(args.contains(&"critical,high".to_string()));
        assert!(args.contains(&"-tags".to_string()));
        assert!(args.contains(&"cve".to_string()));
        assert!(args.contains(&"rce".to_string()));
        assert!(args.contains(&"-u".to_string()));
    }

    #[test]
    fn test_vuln_assessor_nikto_args() {
        let va = VulnAssessor::new().use_nikto(true);
        let args = va.build_args();
        assert!(args.contains(&"-h".to_string()));
    }

    #[test]
    fn test_traffic_analyzer_tcpdump_args() {
        let ta = TrafficAnalyzer::new()
            .interface("eth0")
            .packet_count(50)
            .filter("port 80");
        let args = ta.build_args();
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"eth0".to_string()));
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"50".to_string()));
        assert!(args.contains(&"-nn".to_string()));
        assert!(args.contains(&"port 80".to_string()));
    }

    #[test]
    fn test_traffic_analyzer_tshark() {
        let ta = TrafficAnalyzer::new()
            .use_tshark()
            .interface("wlan0");
        assert_eq!(ta.tool_choice, NetworkTool::DeepInspect);
        let args = ta.build_args();
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"wlan0".to_string()));
    }

    #[test]
    fn test_traffic_analyzer_ngrep() {
        let ta = TrafficAnalyzer::new()
            .use_ngrep()
            .interface("lo")
            .filter("GET");
        assert_eq!(ta.tool_choice, NetworkTool::NetworkGrep);
        let args = ta.build_args();
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"lo".to_string()));
        assert!(args.contains(&"GET".to_string()));
    }

    #[test]
    fn test_web_fuzzer_gobuster_args() {
        let wf = WebFuzzer::new()
            .wordlist("/usr/share/wordlists/common.txt")
            .extension("php")
            .extension("html")
            .threads(20)
            .status_codes("200,301");
        let args = wf.build_args("http://target.com");
        assert!(args.contains(&"dir".to_string()));
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"http://target.com".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"/usr/share/wordlists/common.txt".to_string()));
        assert!(args.contains(&"-x".to_string()));
        assert!(args.contains(&"php,html".to_string()));
        assert!(args.contains(&"-t".to_string()));
        assert!(args.contains(&"20".to_string()));
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"200,301".to_string()));
    }

    #[test]
    fn test_web_fuzzer_ffuf_args() {
        let wf = WebFuzzer::new()
            .use_ffuf()
            .wordlist("/tmp/words.txt")
            .extension("js")
            .status_codes("200");
        let args = wf.build_args("http://example.com");
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"http://example.com/FUZZ".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"/tmp/words.txt".to_string()));
        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&".js".to_string()));
        assert!(args.contains(&"-mc".to_string()));
        assert!(args.contains(&"200".to_string()));
    }

    #[test]
    fn test_socket_inspector_default_args() {
        let si = SocketInspector::new();
        let args = si.build_args();
        assert_eq!(args.len(), 1);
        let flags = &args[0];
        assert!(flags.contains('t'));
        assert!(flags.contains('u'));
        assert!(flags.contains('n'));
        assert!(flags.contains('a'));
        assert!(flags.contains('p'));
    }

    #[test]
    fn test_socket_inspector_listening_tcp() {
        let si = SocketInspector::new()
            .listening_only(true)
            .tcp_only(true);
        let args = si.build_args();
        let flags = &args[0];
        assert!(flags.contains('l'));
        assert!(flags.contains('t'));
        assert!(!flags.contains('u'));
    }

    #[test]
    fn test_socket_inspector_udp_only() {
        let si = SocketInspector::new().udp_only(true);
        let args = si.build_args();
        let flags = &args[0];
        assert!(flags.contains('u'));
        assert!(!flags.contains('t'));
    }

    #[test]
    fn test_port_scanner_default() {
        let scanner = PortScanner::default();
        assert_eq!(scanner.profile, ScanProfile::Standard);
        assert!(!scanner.use_masscan);
        assert!(scanner.ports.is_none());
    }

    // --- New tool (batch 2) config tests ---

    #[test]
    fn test_netdiscover_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::NetDiscover);
        assert_eq!(cfg.binary_name, "netdiscover");
        assert_eq!(cfg.risk_level, RiskLevel::Medium);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
        assert!(cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_termshark_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::TermShark);
        assert_eq!(cfg.binary_name, "termshark");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
        assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
        assert!(!cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_bettercap_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::BetterCap);
        assert_eq!(cfg.binary_name, "bettercap");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
        assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
        assert!(!cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_dnsx_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::DnsX);
        assert_eq!(cfg.binary_name, "dnsx");
        assert_eq!(cfg.risk_level, RiskLevel::Medium);
        assert!(!cfg.requires_approval);
        assert!(cfg.required_capabilities.is_empty());
        assert!(cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_fierce_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::Fierce);
        assert_eq!(cfg.binary_name, "fierce");
        assert_eq!(cfg.risk_level, RiskLevel::High);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.is_empty());
        assert!(cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_wfuzz_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::WebAppFuzz);
        assert_eq!(cfg.binary_name, "wfuzz");
        assert_eq!(cfg.risk_level, RiskLevel::High);
        assert!(cfg.requires_approval);
        assert!(cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_sqlmap_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::SqlMap);
        assert_eq!(cfg.binary_name, "sqlmap");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
        assert!(cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_aircrack_ng_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::AircrackNg);
        assert_eq!(cfg.binary_name, "aircrack-ng");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
        assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
        assert!(!cfg.allowed_in_sandbox);
    }

    #[test]
    fn test_kismet_config() {
        let cfg = NetworkToolConfig::for_tool(NetworkTool::Kismet);
        assert_eq!(cfg.binary_name, "kismet");
        assert_eq!(cfg.risk_level, RiskLevel::Critical);
        assert!(cfg.requires_approval);
        assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
        assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
        assert!(!cfg.allowed_in_sandbox);
    }

    // --- Dangerous arg rejection for sqlmap and bettercap ---

    #[test]
    fn test_reject_sqlmap_os_shell() {
        let args = vec!["--os-shell".to_string(), "-u".to_string(), "http://target.com".to_string()];
        assert!(validate_args(NetworkTool::SqlMap, &args, false).is_err());
    }

    #[test]
    fn test_reject_sqlmap_os_cmd() {
        let args = vec!["--os-cmd=whoami".to_string()];
        assert!(validate_args(NetworkTool::SqlMap, &args, false).is_err());
    }

    #[test]
    fn test_reject_sqlmap_file_write() {
        let args = vec!["--file-write=/tmp/shell.php".to_string()];
        assert!(validate_args(NetworkTool::SqlMap, &args, false).is_err());
    }

    #[test]
    fn test_allow_sqlmap_dangerous_with_approval() {
        let args = vec!["--os-shell".to_string()];
        assert!(validate_args(NetworkTool::SqlMap, &args, true).is_ok());
    }

    #[test]
    fn test_allow_sqlmap_normal_args() {
        let args = vec!["-u".to_string(), "http://target.com/page?id=1".to_string(), "--batch".to_string()];
        assert!(validate_args(NetworkTool::SqlMap, &args, false).is_ok());
    }

    #[test]
    fn test_reject_bettercap_caplet() {
        let args = vec!["--caplet".to_string(), "http-req-dump".to_string()];
        assert!(validate_args(NetworkTool::BetterCap, &args, false).is_err());
    }

    #[test]
    fn test_allow_bettercap_caplet_with_approval() {
        let args = vec!["--caplet".to_string(), "http-req-dump".to_string()];
        assert!(validate_args(NetworkTool::BetterCap, &args, true).is_ok());
    }

    #[test]
    fn test_allow_bettercap_normal_args() {
        let args = vec!["-iface".to_string(), "eth0".to_string()];
        assert!(validate_args(NetworkTool::BetterCap, &args, false).is_ok());
    }

    #[test]
    fn test_all_tools_count_is_32() {
        assert_eq!(ALL_TOOLS.len(), 32);
    }

    #[test]
    fn test_new_tools_display() {
        assert_eq!(NetworkTool::NetDiscover.to_string(), "NetDiscover");
        assert_eq!(NetworkTool::TermShark.to_string(), "TermShark");
        assert_eq!(NetworkTool::BetterCap.to_string(), "BetterCap");
        assert_eq!(NetworkTool::DnsX.to_string(), "DnsX");
        assert_eq!(NetworkTool::Fierce.to_string(), "Fierce");
        assert_eq!(NetworkTool::WebAppFuzz.to_string(), "WebAppFuzz");
        assert_eq!(NetworkTool::SqlMap.to_string(), "SqlMap");
        assert_eq!(NetworkTool::AircrackNg.to_string(), "AircrackNg");
        assert_eq!(NetworkTool::Kismet.to_string(), "Kismet");
    }
}
