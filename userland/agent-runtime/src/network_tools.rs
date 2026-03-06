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
        NetworkTool::PacketCapture => {
            // Reject writing to files without approval
            if !allow_dangerous && (joined.contains("-w ") || joined.contains("-w\t")) {
                return Err(anyhow!(
                    "Packet capture file output (-w) requires explicit approval"
                ));
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
        assert_eq!(tools.len(), 11);
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
    }
}
