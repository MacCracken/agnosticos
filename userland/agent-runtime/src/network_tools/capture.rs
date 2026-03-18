//! Traffic capture, network probing, and socket inspection wrappers.

use anyhow::Result;

use super::parse::parse_output;
use super::runner::NetworkToolRunner;
use super::types::{
    DiscoveredHost, NetworkTool, NetworkToolConfig, ParsedOutput, SocketEntry, ToolOutput, TraceHop,
};

/// Network path probing wrapping traceroute and mtr.
#[derive(Debug)]
pub struct NetworkProber {
    pub(super) runner: NetworkToolRunner,
    pub(super) use_mtr: bool,
    pub(super) max_hops: Option<u32>,
    pub(super) count: Option<u32>,
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

/// Traffic analysis wrapping tcpdump, tshark, and ngrep.
#[derive(Debug)]
pub struct TrafficAnalyzer {
    pub(super) runner: NetworkToolRunner,
    pub tool_choice: NetworkTool,
    pub(super) interface: Option<String>,
    pub(super) filter: Option<String>,
    pub(super) packet_count: Option<u32>,
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

/// Socket/connection inspector wrapping ss.
#[derive(Debug)]
pub struct SocketInspector {
    pub(super) runner: NetworkToolRunner,
    pub(super) listening_only: bool,
    pub(super) tcp_only: bool,
    pub(super) udp_only: bool,
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
            let addr = parts[2]
                .trim_matches(|c: char| c == '(' || c == ')')
                .to_string();
            (Some(parts[1].to_string()), Some(addr))
        } else if parts.len() >= 2 {
            let addr = parts[1]
                .trim_matches(|c: char| c == '(' || c == ')')
                .to_string();
            (None, Some(addr))
        } else {
            (None, None)
        };

        // Extract RTT (first ms value found)
        let rtt_ms = parts
            .iter()
            .find_map(|p| p.trim_end_matches("ms").parse::<f64>().ok());

        // Extract loss percentage (mtr format: "0.0%")
        let loss_pct = parts.iter().find_map(|p| {
            p.trim_end_matches('%')
                .parse::<f64>()
                .ok()
                .filter(|_| p.ends_with('%'))
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
