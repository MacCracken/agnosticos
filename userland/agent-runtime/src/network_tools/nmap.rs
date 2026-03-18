//! Port scanning wrappers — nmap and masscan.

use anyhow::Result;

use super::parse::parse_output;
use super::runner::NetworkToolRunner;
use super::types::{
    DiscoveredHost, DiscoveredPort, NetworkTool, NetworkToolConfig, ParsedOutput, ScanProfile,
};

/// Port scanner wrapping nmap/masscan with typed scan configuration.
#[derive(Debug)]
pub struct PortScanner {
    pub(super) runner: NetworkToolRunner,
    pub(super) profile: ScanProfile,
    pub(super) ports: Option<String>,
    pub(super) use_masscan: bool,
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
        if let Some(rest) = line.strip_prefix("Nmap scan report for ") {
            if let Some(host) = current_host.take() {
                hosts.push(host);
            }
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
        if let Some(rest) = line.strip_prefix("MAC Address: ") {
            if let Some(ref mut host) = current_host {
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
        if let Some(rest) = line.strip_prefix("Discovered open port ") {
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

pub(crate) fn parse_nmap_port_line(line: &str) -> Option<DiscoveredPort> {
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
