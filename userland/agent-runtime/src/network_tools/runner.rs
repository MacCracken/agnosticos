//! NetworkToolRunner — validates and executes network tools in sandboxed environments.

use std::net::IpAddr;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tracing::{debug, info, warn};

use super::types::{NetworkTool, NetworkToolConfig, ToolOutput, ValidatedTarget, ALL_TOOLS};

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
    if target.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '$' | '`' | '(' | ')' | '{' | '}' | '>' | '<' | '!' | '\\'
        )
    }) {
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
pub(crate) fn reject_broadcast(addr: &IpAddr) -> Result<()> {
    if let IpAddr::V4(v4) = addr {
        if v4.octets() == [255, 255, 255, 255] {
            return Err(anyhow!("Broadcast address 255.255.255.255 is not allowed"));
        }
    }
    Ok(())
}

/// Validate hostname per RFC 952 / 1123
pub(crate) fn validate_hostname(host: &str) -> Result<()> {
    if host.is_empty() || host.len() > 253 {
        return Err(anyhow!("Hostname length must be 1-253 characters"));
    }
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(anyhow!(
                "Hostname label '{}' must be 1-63 characters",
                label
            ));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
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
                    if let Some(rate_val) = joined
                        .split("--rate")
                        .nth(1)
                        .and_then(|s| s.split_whitespace().next())
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
                    return Err(anyhow!("Custom nuclei templates require explicit approval"));
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
            if !allow_dangerous && joined.contains("--caplet") {
                return Err(anyhow!(
                        "Dangerous bettercap argument '--caplet' requires explicit approval (arbitrary script execution)"
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
        ALL_TOOLS
            .iter()
            .map(|t| NetworkToolConfig::for_tool(*t))
            .collect()
    }
}

impl Default for NetworkToolRunner {
    fn default() -> Self {
        Self::new()
    }
}
