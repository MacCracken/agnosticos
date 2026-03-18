//! Network Tools Agent Framework
//!
//! Provides agent-wrapped network security and diagnostic tools.
//! All tool invocations are sandboxed, audited, and require user approval
//! for sensitive operations.
//!
//! Submodules:
//! - **types**: Enums, structs, and tool configuration
//! - **runner**: Argument validation, target validation, and tool execution
//! - **nmap**: Port scanning (nmap/masscan) and host scan output parsing
//! - **dns**: DNS investigation (dig/dnsrecon) and DNS output parsing
//! - **capture**: Traffic capture, network probing, socket inspection, and trace/socket output parsing
//! - **scanners**: Vulnerability assessment (nuclei/nikto) and web fuzzing (gobuster/ffuf)
//! - **parse**: Output dispatcher routing tool output to the appropriate parser

pub mod types;

mod capture;
mod dns;
mod nmap;
mod parse;
mod runner;
mod scanners;

#[cfg(test)]
mod tests;

// Re-export everything that was previously `pub` in the flat network_tools.rs
// so that all external consumers see an identical public API.

pub use types::{
    DiscoveredHost, DiscoveredPort, DnsRecord, NetworkTool, NetworkToolConfig, ParsedOutput,
    RiskLevel, ScanProfile, SocketEntry, ToolOutput, TraceHop, ValidatedTarget, ALL_TOOLS,
};

pub use runner::{is_rfc1918, validate_args, validate_target, NetworkToolRunner};

pub use parse::parse_output;

pub use nmap::{parse_scan_output, PortScanner};

pub use dns::{parse_dns_output, DnsInvestigator};

pub use capture::{
    parse_socket_output, parse_trace_output, NetworkProber, SocketInspector, TrafficAnalyzer,
};

pub use scanners::{VulnAssessor, WebFuzzer};
