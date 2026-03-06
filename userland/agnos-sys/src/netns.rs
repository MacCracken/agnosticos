//! Per-Agent Network Namespace and Firewall Interface
//!
//! Creates isolated network namespaces with veth pairs and nftables firewall
//! rules for AGNOS agents. Shells out to `ip` and `nft` (standard practice —
//! avoids ~1000 LOC of raw netlink code).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Configuration for creating a per-agent network namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetNamespaceConfig {
    /// Agent identifier (used for naming)
    pub name: String,
    /// IP address assigned to the agent side of the veth pair
    pub agent_ip: String,
    /// IP address assigned to the host side of the veth pair
    pub host_ip: String,
    /// CIDR prefix length (typically 30 for a /30 point-to-point link)
    pub prefix_len: u8,
    /// Firewall policy applied inside the namespace
    pub firewall_policy: FirewallPolicy,
    /// Whether to enable NAT (masquerade) for outbound traffic
    pub enable_nat: bool,
    /// DNS servers to configure inside the namespace
    pub dns_servers: Vec<String>,
}

impl NetNamespaceConfig {
    /// Create a config with auto-generated IPs based on the agent name hash.
    pub fn for_agent(name: impl Into<String>) -> Self {
        let name = name.into();
        let (host_ip, agent_ip) = generate_agent_ips(&name);
        Self {
            name,
            agent_ip,
            host_ip,
            prefix_len: 30,
            firewall_policy: FirewallPolicy::default(),
            enable_nat: true,
            dns_servers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(SysError::InvalidArgument("Namespace name cannot be empty".into()));
        }
        if self.name.len() > 64 {
            return Err(SysError::InvalidArgument("Namespace name too long (max 64)".into()));
        }
        if self.agent_ip.is_empty() || self.host_ip.is_empty() {
            return Err(SysError::InvalidArgument("IP addresses cannot be empty".into()));
        }
        if self.prefix_len == 0 || self.prefix_len > 32 {
            return Err(SysError::InvalidArgument(format!(
                "Invalid prefix length: {} (must be 1-32)",
                self.prefix_len
            )));
        }
        // Validate IPs are well-formed
        self.agent_ip.parse::<std::net::Ipv4Addr>().map_err(|_| {
            SysError::InvalidArgument(format!("Invalid agent IP: {}", self.agent_ip))
        })?;
        self.host_ip.parse::<std::net::Ipv4Addr>().map_err(|_| {
            SysError::InvalidArgument(format!("Invalid host IP: {}", self.host_ip))
        })?;
        Ok(())
    }
}

/// Firewall policy for an agent namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallPolicy {
    /// Default action for inbound traffic
    pub default_inbound: FirewallAction,
    /// Default action for outbound traffic
    pub default_outbound: FirewallAction,
    /// Specific rules (evaluated before defaults)
    pub rules: Vec<FirewallRule>,
}

impl Default for FirewallPolicy {
    fn default() -> Self {
        Self {
            default_inbound: FirewallAction::Drop,
            default_outbound: FirewallAction::Accept,
            rules: Vec::new(),
        }
    }
}

/// A specific firewall rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    /// Traffic direction
    pub direction: TrafficDirection,
    /// Protocol to match
    pub protocol: Protocol,
    /// Port to match (0 = any)
    pub port: u16,
    /// Remote address to match (empty = any)
    pub remote_addr: String,
    /// Action to take
    pub action: FirewallAction,
    /// Human-readable comment
    pub comment: String,
}

/// Traffic direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrafficDirection {
    Inbound,
    Outbound,
}

/// Network protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Any,
}

impl Protocol {
    /// Return the nftables protocol string.
    pub fn as_nft_str(&self) -> &str {
        match self {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
            Protocol::Icmp => "icmp",
            Protocol::Any => "ip",
        }
    }
}

/// Firewall action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirewallAction {
    Accept,
    Drop,
    Reject,
}

impl FirewallAction {
    /// Return the nftables action string.
    pub fn as_nft_str(&self) -> &str {
        match self {
            FirewallAction::Accept => "accept",
            FirewallAction::Drop => "drop",
            FirewallAction::Reject => "reject",
        }
    }
}

impl std::fmt::Display for FirewallAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_nft_str())
    }
}

/// Handle for an active network namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetNamespaceHandle {
    /// Namespace name (`agnos-agent-{id}`)
    pub name: String,
    /// Host-side veth interface name
    pub veth_host: String,
    /// Agent-side veth interface name
    pub veth_agent: String,
    /// Path to the namespace (`/var/run/netns/agnos-agent-{id}`)
    pub netns_path: String,
}

/// Generate IP addresses for an agent based on a hash of its name.
///
/// Uses the 10.100.x.y/30 range. Returns (host_ip, agent_ip).
pub fn generate_agent_ips(agent_name: &str) -> (String, String) {
    let mut hasher = DefaultHasher::new();
    agent_name.hash(&mut hasher);
    let hash = hasher.finish();
    let third_octet = (hash % 255) as u8;
    // Use .1 and .2 within a /30 block (base .0 is network, .3 is broadcast)
    (
        format!("10.100.{}.1", third_octet),
        format!("10.100.{}.2", third_octet),
    )
}

/// Truncate a veth interface name to the max allowed length (15 chars on Linux).
fn truncate_veth_name(base: &str) -> String {
    if base.len() > 15 {
        base[..15].to_string()
    } else {
        base.to_string()
    }
}

/// Create a network namespace for an agent with veth pair and IP configuration.
///
/// Steps:
/// 1. `ip netns add agnos-agent-{name}`
/// 2. `ip link add veth-{name}-h type veth peer name veth-{name}-a`
/// 3. Move agent-side veth into namespace
/// 4. Assign IPs to both sides
/// 5. Bring both interfaces up
///
/// Requires root or `CAP_NET_ADMIN`.
pub fn create_agent_netns(config: &NetNamespaceConfig) -> Result<NetNamespaceHandle> {
    #[cfg(target_os = "linux")]
    {
        config.validate()?;

        let ns_name = format!("agnos-agent-{}", config.name);
        let veth_host = truncate_veth_name(&format!("veth-{}-h", config.name));
        let veth_agent = truncate_veth_name(&format!("veth-{}-a", config.name));

        // 1. Create namespace
        run_ip_cmd(&["netns", "add", &ns_name])?;

        // 2. Create veth pair
        if let Err(e) = run_ip_cmd(&[
            "link", "add", &veth_host, "type", "veth", "peer", "name", &veth_agent,
        ]) {
            // Cleanup namespace on failure
            let _ = run_ip_cmd(&["netns", "delete", &ns_name]);
            return Err(e);
        }

        // 3. Move agent-side veth into namespace
        if let Err(e) = run_ip_cmd(&["link", "set", &veth_agent, "netns", &ns_name]) {
            let _ = run_ip_cmd(&["link", "delete", &veth_host]);
            let _ = run_ip_cmd(&["netns", "delete", &ns_name]);
            return Err(e);
        }

        // 4. Assign IPs
        let host_cidr = format!("{}/{}", config.host_ip, config.prefix_len);
        let agent_cidr = format!("{}/{}", config.agent_ip, config.prefix_len);

        if let Err(e) = run_ip_cmd(&["addr", "add", &host_cidr, "dev", &veth_host]) {
            let _ = cleanup_netns(&ns_name, &veth_host);
            return Err(e);
        }
        if let Err(e) = run_ip_netns_cmd(
            &ns_name,
            &["addr", "add", &agent_cidr, "dev", &veth_agent],
        ) {
            let _ = cleanup_netns(&ns_name, &veth_host);
            return Err(e);
        }

        // 5. Bring interfaces up
        let _ = run_ip_cmd(&["link", "set", &veth_host, "up"]);
        let _ = run_ip_netns_cmd(&ns_name, &["link", "set", &veth_agent, "up"]);
        let _ = run_ip_netns_cmd(&ns_name, &["link", "set", "lo", "up"]);

        // Set default route inside namespace
        let _ = run_ip_netns_cmd(
            &ns_name,
            &["route", "add", "default", "via", &config.host_ip],
        );

        // Enable NAT on host side if requested
        if config.enable_nat {
            let _ = run_cmd("sysctl", &["-w", "net.ipv4.ip_forward=1"]);
            let _ = run_cmd(
                "iptables",
                &[
                    "-t", "nat", "-A", "POSTROUTING",
                    "-s", &format!("{}/{}", config.agent_ip, config.prefix_len),
                    "-j", "MASQUERADE",
                ],
            );
        }

        let handle = NetNamespaceHandle {
            name: ns_name.clone(),
            veth_host,
            veth_agent,
            netns_path: format!("/var/run/netns/{}", ns_name),
        };

        tracing::info!(
            "Created network namespace '{}' (host={}, agent={})",
            ns_name,
            config.host_ip,
            config.agent_ip
        );

        Ok(handle)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(SysError::NotSupported)
    }
}

/// Apply firewall rules to an agent's network namespace using nftables.
pub fn apply_firewall_rules(handle: &NetNamespaceHandle, policy: &FirewallPolicy) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let ruleset = generate_nftables_ruleset(policy, &handle.veth_agent);

        // Write ruleset to a temp file and load it inside the namespace
        let tmp_path = format!("/tmp/agnos-nft-{}.conf", handle.name);
        std::fs::write(&tmp_path, &ruleset)
            .map_err(|e| SysError::Unknown(format!("Failed to write nft rules: {}", e)))?;

        let result = run_cmd(
            "ip",
            &["netns", "exec", &handle.name, "nft", "-f", &tmp_path],
        );

        let _ = std::fs::remove_file(&tmp_path);
        result?;

        tracing::info!(
            "Applied {} firewall rules to namespace '{}'",
            policy.rules.len(),
            handle.name
        );
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (handle, policy);
        Err(SysError::NotSupported)
    }
}

/// Destroy an agent's network namespace and clean up veth pairs.
pub fn destroy_agent_netns(handle: &NetNamespaceHandle) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Delete host-side veth (peer is auto-deleted)
        let _ = run_ip_cmd(&["link", "delete", &handle.veth_host]);

        // Delete namespace
        run_ip_cmd(&["netns", "delete", &handle.name])?;

        tracing::info!("Destroyed network namespace '{}'", handle.name);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = handle;
        Err(SysError::NotSupported)
    }
}

/// List all AGNOS agent network namespaces.
///
/// Returns names of namespaces matching the `agnos-agent-*` pattern.
pub fn list_agent_netns() -> Result<Vec<String>> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("ip")
            .args(["netns", "list"])
            .output()
            .map_err(|e| SysError::Unknown(format!("Failed to run 'ip netns list': {}", e)))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let namespaces: Vec<String> = stdout
            .lines()
            .filter_map(|line| {
                // Format: "name (id: N)" or just "name"
                let name = line.split_whitespace().next()?;
                if name.starts_with("agnos-agent-") {
                    Some(name.to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(namespaces)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(SysError::NotSupported)
    }
}

/// Generate a complete nftables ruleset string from a firewall policy.
///
/// This is a **pure function** — fully testable without root or network namespaces.
pub fn generate_nftables_ruleset(policy: &FirewallPolicy, veth_name: &str) -> String {
    let mut lines = Vec::new();

    lines.push("#!/usr/sbin/nft -f".to_string());
    lines.push(String::new());
    lines.push("# AGNOS agent firewall rules (auto-generated)".to_string());
    lines.push(format!("# Interface: {}", veth_name));
    lines.push(String::new());

    // Flush existing rules
    lines.push("flush ruleset".to_string());
    lines.push(String::new());

    // Create table and chains
    lines.push("table inet agnos_agent {".to_string());

    // Input chain (inbound)
    lines.push("    chain input {".to_string());
    lines.push(format!(
        "        type filter hook input priority 0; policy {};",
        policy.default_inbound.as_nft_str()
    ));
    lines.push(String::new());

    // Allow established/related connections
    lines.push("        ct state established,related accept".to_string());

    // Allow loopback
    lines.push("        iifname \"lo\" accept".to_string());

    // Specific inbound rules
    for rule in &policy.rules {
        if rule.direction == TrafficDirection::Inbound {
            lines.push(format!("        {}", format_nft_rule(rule)));
        }
    }

    lines.push("    }".to_string());
    lines.push(String::new());

    // Output chain (outbound)
    lines.push("    chain output {".to_string());
    lines.push(format!(
        "        type filter hook output priority 0; policy {};",
        policy.default_outbound.as_nft_str()
    ));
    lines.push(String::new());

    // Allow established/related
    lines.push("        ct state established,related accept".to_string());

    // Allow loopback
    lines.push("        oifname \"lo\" accept".to_string());

    // Allow DNS (UDP 53) for resolution
    lines.push("        udp dport 53 accept".to_string());

    // Specific outbound rules
    for rule in &policy.rules {
        if rule.direction == TrafficDirection::Outbound {
            lines.push(format!("        {}", format_nft_rule(rule)));
        }
    }

    lines.push("    }".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

/// Format a single firewall rule as an nftables rule string.
fn format_nft_rule(rule: &FirewallRule) -> String {
    let mut parts = Vec::new();

    // Protocol
    match rule.protocol {
        Protocol::Tcp => parts.push("tcp".to_string()),
        Protocol::Udp => parts.push("udp".to_string()),
        Protocol::Icmp => parts.push("meta l4proto icmp".to_string()),
        Protocol::Any => {}
    }

    // Port
    if rule.port > 0 {
        match rule.direction {
            TrafficDirection::Inbound => parts.push(format!("dport {}", rule.port)),
            TrafficDirection::Outbound => parts.push(format!("dport {}", rule.port)),
        }
    }

    // Remote address
    if !rule.remote_addr.is_empty() {
        match rule.direction {
            TrafficDirection::Inbound => parts.push(format!("ip saddr {}", rule.remote_addr)),
            TrafficDirection::Outbound => parts.push(format!("ip daddr {}", rule.remote_addr)),
        }
    }

    // Action
    parts.push(rule.action.as_nft_str().to_string());

    // Comment
    if !rule.comment.is_empty() {
        let escaped = rule.comment.replace('\\', "\\\\").replace('"', "\\\"");
        parts.push(format!("comment \"{}\"", escaped));
    }

    parts.join(" ")
}

/// Run an `ip` command with the given arguments.
#[cfg(target_os = "linux")]
fn run_ip_cmd(args: &[&str]) -> Result<()> {
    run_cmd("ip", args)
}

/// Run an `ip netns exec <ns> ip <args>` command.
#[cfg(target_os = "linux")]
fn run_ip_netns_cmd(ns_name: &str, args: &[&str]) -> Result<()> {
    let mut full_args = vec!["netns", "exec", ns_name, "ip"];
    full_args.extend_from_slice(args);
    run_cmd("ip", &full_args)
}

/// Run a command and check for success.
#[cfg(target_os = "linux")]
fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run '{}': {}", cmd, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "{} {} failed: {}",
            cmd,
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(())
}

/// Clean up a namespace and veth pair on error.
#[cfg(target_os = "linux")]
fn cleanup_netns(ns_name: &str, veth_host: &str) -> Result<()> {
    let _ = run_ip_cmd(&["link", "delete", veth_host]);
    run_ip_cmd(&["netns", "delete", ns_name])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_agent_ips() {
        let (host, agent) = generate_agent_ips("test-agent-1");
        assert!(host.starts_with("10.100."));
        assert!(host.ends_with(".1"));
        assert!(agent.starts_with("10.100."));
        assert!(agent.ends_with(".2"));
    }

    #[test]
    fn test_generate_agent_ips_deterministic() {
        let (h1, a1) = generate_agent_ips("same-agent");
        let (h2, a2) = generate_agent_ips("same-agent");
        assert_eq!(h1, h2);
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_generate_agent_ips_different() {
        let (h1, _) = generate_agent_ips("agent-a");
        let (h2, _) = generate_agent_ips("agent-b");
        // Different agents should (likely) get different IPs
        // This could theoretically collide but is extremely unlikely
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_truncate_veth_name() {
        assert_eq!(truncate_veth_name("short"), "short");
        assert_eq!(truncate_veth_name("exactly15chars!"), "exactly15chars!");
        assert_eq!(
            truncate_veth_name("this-is-a-very-long-name"),
            "this-is-a-very-"
        );
    }

    #[test]
    fn test_net_namespace_config_for_agent() {
        let config = NetNamespaceConfig::for_agent("test-1");
        assert_eq!(config.name, "test-1");
        assert_eq!(config.prefix_len, 30);
        assert!(config.enable_nat);
        assert!(!config.dns_servers.is_empty());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_net_namespace_config_validate_empty_name() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.name = String::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_net_namespace_config_validate_bad_ip() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.agent_ip = "not-an-ip".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_net_namespace_config_validate_bad_prefix() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.prefix_len = 0;
        assert!(config.validate().is_err());

        config.prefix_len = 33;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_firewall_policy_default() {
        let policy = FirewallPolicy::default();
        assert_eq!(policy.default_inbound, FirewallAction::Drop);
        assert_eq!(policy.default_outbound, FirewallAction::Accept);
        assert!(policy.rules.is_empty());
    }

    #[test]
    fn test_firewall_action_as_nft_str() {
        assert_eq!(FirewallAction::Accept.as_nft_str(), "accept");
        assert_eq!(FirewallAction::Drop.as_nft_str(), "drop");
        assert_eq!(FirewallAction::Reject.as_nft_str(), "reject");
    }

    #[test]
    fn test_protocol_as_nft_str() {
        assert_eq!(Protocol::Tcp.as_nft_str(), "tcp");
        assert_eq!(Protocol::Udp.as_nft_str(), "udp");
        assert_eq!(Protocol::Icmp.as_nft_str(), "icmp");
        assert_eq!(Protocol::Any.as_nft_str(), "ip");
    }

    #[test]
    fn test_generate_nftables_ruleset_default() {
        let policy = FirewallPolicy::default();
        let ruleset = generate_nftables_ruleset(&policy, "veth-test-a");

        assert!(ruleset.contains("flush ruleset"));
        assert!(ruleset.contains("table inet agnos_agent"));
        assert!(ruleset.contains("chain input"));
        assert!(ruleset.contains("chain output"));
        assert!(ruleset.contains("policy drop"));
        assert!(ruleset.contains("policy accept"));
        assert!(ruleset.contains("ct state established,related accept"));
        assert!(ruleset.contains("udp dport 53 accept"));
    }

    #[test]
    fn test_generate_nftables_ruleset_with_rules() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Drop,
            default_outbound: FirewallAction::Drop,
            rules: vec![
                FirewallRule {
                    direction: TrafficDirection::Outbound,
                    protocol: Protocol::Tcp,
                    port: 443,
                    remote_addr: String::new(),
                    action: FirewallAction::Accept,
                    comment: "Allow HTTPS".to_string(),
                },
                FirewallRule {
                    direction: TrafficDirection::Inbound,
                    protocol: Protocol::Tcp,
                    port: 8080,
                    remote_addr: "10.100.0.1".to_string(),
                    action: FirewallAction::Accept,
                    comment: "Allow API from host".to_string(),
                },
            ],
        };

        let ruleset = generate_nftables_ruleset(&policy, "veth-test-a");

        assert!(ruleset.contains("tcp dport 443"));
        assert!(ruleset.contains("Allow HTTPS"));
        assert!(ruleset.contains("tcp dport 8080"));
        assert!(ruleset.contains("ip saddr 10.100.0.1"));
    }

    #[test]
    fn test_generate_nftables_ruleset_icmp_rule() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Drop,
            default_outbound: FirewallAction::Accept,
            rules: vec![FirewallRule {
                direction: TrafficDirection::Outbound,
                protocol: Protocol::Icmp,
                port: 0,
                remote_addr: String::new(),
                action: FirewallAction::Accept,
                comment: "Allow ping".to_string(),
            }],
        };

        let ruleset = generate_nftables_ruleset(&policy, "veth-test-a");
        assert!(ruleset.contains("meta l4proto icmp"));
    }

    #[test]
    fn test_net_namespace_handle_serialization() {
        let handle = NetNamespaceHandle {
            name: "agnos-agent-test".to_string(),
            veth_host: "veth-test-h".to_string(),
            veth_agent: "veth-test-a".to_string(),
            netns_path: "/var/run/netns/agnos-agent-test".to_string(),
        };

        let json = serde_json::to_string(&handle).unwrap();
        let deserialized: NetNamespaceHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "agnos-agent-test");
        assert_eq!(deserialized.veth_host, "veth-test-h");
    }

    #[test]
    fn test_format_nft_rule_tcp_outbound() {
        let rule = FirewallRule {
            direction: TrafficDirection::Outbound,
            protocol: Protocol::Tcp,
            port: 443,
            remote_addr: String::new(),
            action: FirewallAction::Accept,
            comment: String::new(),
        };
        let formatted = format_nft_rule(&rule);
        assert_eq!(formatted, "tcp dport 443 accept");
    }

    #[test]
    fn test_format_nft_rule_with_addr_and_comment() {
        let rule = FirewallRule {
            direction: TrafficDirection::Inbound,
            protocol: Protocol::Udp,
            port: 5353,
            remote_addr: "192.168.1.0/24".to_string(),
            action: FirewallAction::Drop,
            comment: "Block mDNS".to_string(),
        };
        let formatted = format_nft_rule(&rule);
        assert!(formatted.contains("udp"));
        assert!(formatted.contains("dport 5353"));
        assert!(formatted.contains("ip saddr 192.168.1.0/24"));
        assert!(formatted.contains("drop"));
        assert!(formatted.contains("Block mDNS"));
    }

    #[test]
    #[ignore = "Requires root and ip/nft tools"]
    fn test_create_and_destroy_netns() {
        let config = NetNamespaceConfig::for_agent("integration-test");
        let handle = create_agent_netns(&config).unwrap();
        assert!(handle.name.contains("integration-test"));

        let namespaces = list_agent_netns().unwrap();
        assert!(namespaces.iter().any(|n| n.contains("integration-test")));

        destroy_agent_netns(&handle).unwrap();
    }

    // --- Additional coverage tests ---

    #[test]
    fn test_net_namespace_config_validate_long_name() {
        let mut config = NetNamespaceConfig::for_agent("x");
        config.name = "a".repeat(65);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_net_namespace_config_validate_exactly_64_chars() {
        let mut config = NetNamespaceConfig::for_agent("x");
        config.name = "a".repeat(64);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_net_namespace_config_validate_empty_agent_ip() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.agent_ip = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_net_namespace_config_validate_empty_host_ip() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.host_ip = String::new();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_net_namespace_config_validate_bad_host_ip() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.host_ip = "not-an-ip".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Invalid host IP"));
    }

    #[test]
    fn test_net_namespace_config_validate_prefix_len_32() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.prefix_len = 32;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_net_namespace_config_validate_prefix_len_1() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.prefix_len = 1;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_net_namespace_config_clone() {
        let config = NetNamespaceConfig::for_agent("test-clone");
        let cloned = config.clone();
        assert_eq!(cloned.name, config.name);
        assert_eq!(cloned.agent_ip, config.agent_ip);
        assert_eq!(cloned.host_ip, config.host_ip);
        assert_eq!(cloned.prefix_len, config.prefix_len);
        assert_eq!(cloned.enable_nat, config.enable_nat);
        assert_eq!(cloned.dns_servers, config.dns_servers);
    }

    #[test]
    fn test_net_namespace_config_debug() {
        let config = NetNamespaceConfig::for_agent("dbg");
        let dbg = format!("{:?}", config);
        assert!(dbg.contains("NetNamespaceConfig"));
        assert!(dbg.contains("dbg"));
    }

    #[test]
    fn test_net_namespace_config_serialization_roundtrip() {
        let config = NetNamespaceConfig::for_agent("serde-test");
        let json = serde_json::to_string(&config).unwrap();
        let back: NetNamespaceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "serde-test");
        assert_eq!(back.agent_ip, config.agent_ip);
        assert_eq!(back.host_ip, config.host_ip);
        assert_eq!(back.prefix_len, 30);
        assert!(back.enable_nat);
        assert_eq!(back.dns_servers.len(), 2);
    }

    #[test]
    fn test_firewall_policy_clone() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Reject,
            default_outbound: FirewallAction::Drop,
            rules: vec![FirewallRule {
                direction: TrafficDirection::Inbound,
                protocol: Protocol::Tcp,
                port: 80,
                remote_addr: "1.2.3.4".to_string(),
                action: FirewallAction::Accept,
                comment: "test".to_string(),
            }],
        };
        let cloned = policy.clone();
        assert_eq!(cloned.default_inbound, FirewallAction::Reject);
        assert_eq!(cloned.default_outbound, FirewallAction::Drop);
        assert_eq!(cloned.rules.len(), 1);
    }

    #[test]
    fn test_firewall_policy_serialization_roundtrip() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Reject,
            default_outbound: FirewallAction::Accept,
            rules: vec![FirewallRule {
                direction: TrafficDirection::Outbound,
                protocol: Protocol::Udp,
                port: 53,
                remote_addr: String::new(),
                action: FirewallAction::Accept,
                comment: "DNS".to_string(),
            }],
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: FirewallPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.default_inbound, FirewallAction::Reject);
        assert_eq!(back.rules.len(), 1);
        assert_eq!(back.rules[0].comment, "DNS");
    }

    #[test]
    fn test_firewall_rule_debug_and_clone() {
        let rule = FirewallRule {
            direction: TrafficDirection::Inbound,
            protocol: Protocol::Icmp,
            port: 0,
            remote_addr: String::new(),
            action: FirewallAction::Accept,
            comment: "ping".to_string(),
        };
        let dbg = format!("{:?}", rule);
        assert!(dbg.contains("FirewallRule"));
        let cloned = rule.clone();
        assert_eq!(cloned.comment, "ping");
    }

    #[test]
    fn test_traffic_direction_serde_roundtrip() {
        for d in &[TrafficDirection::Inbound, TrafficDirection::Outbound] {
            let json = serde_json::to_string(d).unwrap();
            let back: TrafficDirection = serde_json::from_str(&json).unwrap();
            assert_eq!(*d, back);
        }
    }

    #[test]
    fn test_protocol_serde_roundtrip() {
        for p in &[Protocol::Tcp, Protocol::Udp, Protocol::Icmp, Protocol::Any] {
            let json = serde_json::to_string(p).unwrap();
            let back: Protocol = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, back);
        }
    }

    #[test]
    fn test_firewall_action_serde_roundtrip() {
        for a in &[FirewallAction::Accept, FirewallAction::Drop, FirewallAction::Reject] {
            let json = serde_json::to_string(a).unwrap();
            let back: FirewallAction = serde_json::from_str(&json).unwrap();
            assert_eq!(*a, back);
        }
    }

    #[test]
    fn test_firewall_action_display() {
        assert_eq!(format!("{}", FirewallAction::Accept), "accept");
        assert_eq!(format!("{}", FirewallAction::Drop), "drop");
        assert_eq!(format!("{}", FirewallAction::Reject), "reject");
    }

    #[test]
    fn test_net_namespace_handle_clone_and_debug() {
        let handle = NetNamespaceHandle {
            name: "agnos-agent-test".to_string(),
            veth_host: "veth-test-h".to_string(),
            veth_agent: "veth-test-a".to_string(),
            netns_path: "/var/run/netns/agnos-agent-test".to_string(),
        };
        let cloned = handle.clone();
        assert_eq!(cloned.name, handle.name);
        assert_eq!(cloned.netns_path, handle.netns_path);
        let dbg = format!("{:?}", handle);
        assert!(dbg.contains("NetNamespaceHandle"));
    }

    #[test]
    fn test_generate_agent_ips_range() {
        // Verify the IPs are in valid 10.100.x.{1,2} range
        for name in &["a", "bb", "ccc", "dddd", "eeeeee"] {
            let (host, agent) = generate_agent_ips(name);
            assert!(host.starts_with("10.100."));
            assert!(host.ends_with(".1"));
            assert!(agent.starts_with("10.100."));
            assert!(agent.ends_with(".2"));
            // Third octet is 0..254
            let parts: Vec<&str> = host.split('.').collect();
            let third: u8 = parts[2].parse().unwrap();
            assert!(third <= 254);
        }
    }

    #[test]
    fn test_truncate_veth_name_exact_15() {
        let name = "123456789012345"; // exactly 15
        assert_eq!(truncate_veth_name(name), name);
    }

    #[test]
    fn test_truncate_veth_name_16() {
        let name = "1234567890123456"; // 16 chars
        assert_eq!(truncate_veth_name(name), "123456789012345");
    }

    #[test]
    fn test_truncate_veth_name_empty() {
        assert_eq!(truncate_veth_name(""), "");
    }

    #[test]
    fn test_generate_nftables_ruleset_contains_interface_comment() {
        let policy = FirewallPolicy::default();
        let ruleset = generate_nftables_ruleset(&policy, "veth-myagent-a");
        assert!(ruleset.contains("# Interface: veth-myagent-a"));
    }

    #[test]
    fn test_generate_nftables_ruleset_shebang() {
        let policy = FirewallPolicy::default();
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        assert!(ruleset.starts_with("#!/usr/sbin/nft -f"));
    }

    #[test]
    fn test_generate_nftables_ruleset_loopback_rules() {
        let policy = FirewallPolicy::default();
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        assert!(ruleset.contains("iifname \"lo\" accept"));
        assert!(ruleset.contains("oifname \"lo\" accept"));
    }

    #[test]
    fn test_generate_nftables_ruleset_reject_policy() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Reject,
            default_outbound: FirewallAction::Reject,
            rules: Vec::new(),
        };
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        // Both chains should have reject policy
        let input_policy = "policy reject;";
        assert!(ruleset.contains(input_policy));
    }

    #[test]
    fn test_generate_nftables_ruleset_multiple_inbound_rules() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Drop,
            default_outbound: FirewallAction::Accept,
            rules: vec![
                FirewallRule {
                    direction: TrafficDirection::Inbound,
                    protocol: Protocol::Tcp,
                    port: 22,
                    remote_addr: String::new(),
                    action: FirewallAction::Accept,
                    comment: "SSH".to_string(),
                },
                FirewallRule {
                    direction: TrafficDirection::Inbound,
                    protocol: Protocol::Tcp,
                    port: 80,
                    remote_addr: String::new(),
                    action: FirewallAction::Accept,
                    comment: "HTTP".to_string(),
                },
                FirewallRule {
                    direction: TrafficDirection::Inbound,
                    protocol: Protocol::Tcp,
                    port: 443,
                    remote_addr: String::new(),
                    action: FirewallAction::Accept,
                    comment: "HTTPS".to_string(),
                },
            ],
        };
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        assert!(ruleset.contains("tcp dport 22 accept"));
        assert!(ruleset.contains("tcp dport 80 accept"));
        assert!(ruleset.contains("tcp dport 443 accept"));
        assert!(ruleset.contains("SSH"));
        assert!(ruleset.contains("HTTP"));
        assert!(ruleset.contains("HTTPS"));
    }

    #[test]
    fn test_format_nft_rule_any_protocol_no_port() {
        let rule = FirewallRule {
            direction: TrafficDirection::Outbound,
            protocol: Protocol::Any,
            port: 0,
            remote_addr: String::new(),
            action: FirewallAction::Drop,
            comment: String::new(),
        };
        let formatted = format_nft_rule(&rule);
        assert_eq!(formatted, "drop");
    }

    #[test]
    fn test_format_nft_rule_any_protocol_with_addr() {
        let rule = FirewallRule {
            direction: TrafficDirection::Outbound,
            protocol: Protocol::Any,
            port: 0,
            remote_addr: "10.0.0.0/8".to_string(),
            action: FirewallAction::Reject,
            comment: String::new(),
        };
        let formatted = format_nft_rule(&rule);
        assert_eq!(formatted, "ip daddr 10.0.0.0/8 reject");
    }

    #[test]
    fn test_format_nft_rule_inbound_with_addr() {
        let rule = FirewallRule {
            direction: TrafficDirection::Inbound,
            protocol: Protocol::Any,
            port: 0,
            remote_addr: "192.168.1.1".to_string(),
            action: FirewallAction::Accept,
            comment: String::new(),
        };
        let formatted = format_nft_rule(&rule);
        assert_eq!(formatted, "ip saddr 192.168.1.1 accept");
    }

    #[test]
    fn test_format_nft_rule_udp_outbound_with_comment() {
        let rule = FirewallRule {
            direction: TrafficDirection::Outbound,
            protocol: Protocol::Udp,
            port: 123,
            remote_addr: String::new(),
            action: FirewallAction::Accept,
            comment: "NTP".to_string(),
        };
        let formatted = format_nft_rule(&rule);
        assert_eq!(formatted, "udp dport 123 accept comment \"NTP\"");
    }

    #[test]
    fn test_format_nft_rule_icmp_inbound() {
        let rule = FirewallRule {
            direction: TrafficDirection::Inbound,
            protocol: Protocol::Icmp,
            port: 0,
            remote_addr: String::new(),
            action: FirewallAction::Drop,
            comment: String::new(),
        };
        let formatted = format_nft_rule(&rule);
        assert_eq!(formatted, "meta l4proto icmp drop");
    }

    #[test]
    fn test_format_nft_rule_full_rule() {
        let rule = FirewallRule {
            direction: TrafficDirection::Inbound,
            protocol: Protocol::Tcp,
            port: 8080,
            remote_addr: "172.16.0.0/12".to_string(),
            action: FirewallAction::Accept,
            comment: "Private API".to_string(),
        };
        let formatted = format_nft_rule(&rule);
        assert!(formatted.contains("tcp"));
        assert!(formatted.contains("dport 8080"));
        assert!(formatted.contains("ip saddr 172.16.0.0/12"));
        assert!(formatted.contains("accept"));
        assert!(formatted.contains("comment \"Private API\""));
    }

    #[test]
    fn test_generate_nftables_ruleset_no_rules_structure() {
        let policy = FirewallPolicy::default();
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        // Verify structural elements
        assert!(ruleset.contains("table inet agnos_agent {"));
        assert!(ruleset.contains("chain input {"));
        assert!(ruleset.contains("chain output {"));
        // Ends with closing brace
        assert!(ruleset.trim().ends_with('}'));
    }

    #[test]
    fn test_generate_nftables_ruleset_mixed_directions() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Drop,
            default_outbound: FirewallAction::Drop,
            rules: vec![
                FirewallRule {
                    direction: TrafficDirection::Inbound,
                    protocol: Protocol::Tcp,
                    port: 22,
                    remote_addr: String::new(),
                    action: FirewallAction::Accept,
                    comment: "SSH in".to_string(),
                },
                FirewallRule {
                    direction: TrafficDirection::Outbound,
                    protocol: Protocol::Tcp,
                    port: 443,
                    remote_addr: String::new(),
                    action: FirewallAction::Accept,
                    comment: "HTTPS out".to_string(),
                },
            ],
        };
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        // Inbound rule should appear in input chain, outbound in output chain
        assert!(ruleset.contains("SSH in"));
        assert!(ruleset.contains("HTTPS out"));
    }

    #[test]
    fn test_net_namespace_config_for_agent_dns_defaults() {
        let config = NetNamespaceConfig::for_agent("dns-test");
        assert_eq!(config.dns_servers, vec!["8.8.8.8", "8.8.4.4"]);
    }

    #[test]
    fn test_net_namespace_config_for_agent_nat_default() {
        let config = NetNamespaceConfig::for_agent("nat-test");
        assert!(config.enable_nat);
    }

    #[test]
    fn test_firewall_policy_debug() {
        let policy = FirewallPolicy::default();
        let dbg = format!("{:?}", policy);
        assert!(dbg.contains("FirewallPolicy"));
        assert!(dbg.contains("Drop"));
        assert!(dbg.contains("Accept"));
    }

    #[test]
    fn test_traffic_direction_eq() {
        assert_eq!(TrafficDirection::Inbound, TrafficDirection::Inbound);
        assert_ne!(TrafficDirection::Inbound, TrafficDirection::Outbound);
    }

    #[test]
    fn test_protocol_eq() {
        assert_eq!(Protocol::Tcp, Protocol::Tcp);
        assert_ne!(Protocol::Tcp, Protocol::Udp);
        assert_ne!(Protocol::Icmp, Protocol::Any);
    }

    #[test]
    fn test_firewall_action_eq() {
        assert_eq!(FirewallAction::Accept, FirewallAction::Accept);
        assert_ne!(FirewallAction::Accept, FirewallAction::Drop);
        assert_ne!(FirewallAction::Drop, FirewallAction::Reject);
    }

    // --- New coverage tests ---

    #[test]
    fn test_generate_agent_ips_empty_name() {
        // Should not panic, just return some valid IPs
        let (host, agent) = generate_agent_ips("");
        assert!(host.starts_with("10.100."));
        assert!(agent.starts_with("10.100."));
    }

    #[test]
    fn test_generate_agent_ips_long_name() {
        let name = "a".repeat(1000);
        let (host, agent) = generate_agent_ips(&name);
        assert!(host.starts_with("10.100."));
        assert!(agent.ends_with(".2"));
    }

    #[test]
    fn test_net_namespace_config_for_agent_from_owned_string() {
        let config = NetNamespaceConfig::for_agent(String::from("owned-name"));
        assert_eq!(config.name, "owned-name");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_net_namespace_config_validate_prefix_len_zero_error_msg() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.prefix_len = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("prefix length"));
    }

    #[test]
    fn test_net_namespace_config_validate_prefix_len_33_error_msg() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.prefix_len = 33;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("prefix length"));
    }

    #[test]
    fn test_net_namespace_config_validate_agent_ip_invalid_format() {
        let mut config = NetNamespaceConfig::for_agent("test");
        config.agent_ip = "999.999.999.999".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_format_nft_rule_reject_action() {
        let rule = FirewallRule {
            direction: TrafficDirection::Inbound,
            protocol: Protocol::Tcp,
            port: 22,
            remote_addr: String::new(),
            action: FirewallAction::Reject,
            comment: String::new(),
        };
        let formatted = format_nft_rule(&rule);
        assert!(formatted.contains("reject"));
    }

    #[test]
    fn test_generate_nftables_ruleset_accept_inbound_policy() {
        let policy = FirewallPolicy {
            default_inbound: FirewallAction::Accept,
            default_outbound: FirewallAction::Drop,
            rules: Vec::new(),
        };
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        // Input chain should have accept policy
        assert!(ruleset.contains("hook input priority 0; policy accept;"));
        // Output chain should have drop policy
        assert!(ruleset.contains("hook output priority 0; policy drop;"));
    }

    #[test]
    fn test_generate_nftables_ruleset_contains_dns_rule() {
        let policy = FirewallPolicy::default();
        let ruleset = generate_nftables_ruleset(&policy, "veth0");
        assert!(ruleset.contains("udp dport 53 accept"));
    }

    #[test]
    fn test_net_namespace_config_for_agent_ip_determinism() {
        // Same name always produces same IPs
        let c1 = NetNamespaceConfig::for_agent("deterministic");
        let c2 = NetNamespaceConfig::for_agent("deterministic");
        assert_eq!(c1.agent_ip, c2.agent_ip);
        assert_eq!(c1.host_ip, c2.host_ip);
    }

    #[test]
    fn test_truncate_veth_name_single_char() {
        assert_eq!(truncate_veth_name("x"), "x");
    }

    #[test]
    fn test_firewall_rule_serialization_roundtrip() {
        let rule = FirewallRule {
            direction: TrafficDirection::Outbound,
            protocol: Protocol::Tcp,
            port: 443,
            remote_addr: "1.2.3.4".to_string(),
            action: FirewallAction::Accept,
            comment: "HTTPS".to_string(),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let de: FirewallRule = serde_json::from_str(&json).unwrap();
        assert_eq!(de.port, 443);
        assert_eq!(de.remote_addr, "1.2.3.4");
        assert_eq!(de.comment, "HTTPS");
    }
}
