//! Tool Output Analysis via LLM Gateway
//!
//! Pipes structured network tool output through the LLM Gateway
//! for automated interpretation, threat identification, and reporting.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tracing::info;

use crate::network_tools::{
    NetworkTool, ParsedOutput, ToolOutput,
};

/// Analysis result from the LLM.
#[derive(Debug, Clone)]
pub struct ToolAnalysis {
    /// Human-readable summary of findings.
    pub summary: String,
    /// Risk assessment (0.0 = safe, 1.0 = critical).
    pub risk_score: f64,
    /// Identified threats or concerns.
    pub findings: Vec<Finding>,
    /// Recommended next actions.
    pub recommendations: Vec<String>,
    /// The tool that produced the original output.
    pub tool: NetworkTool,
    /// Raw LLM response text.
    pub raw_response: String,
}

/// A specific finding from the analysis.
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: FindingSeverity,
    pub category: String,
    pub description: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for FindingSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FindingSeverity::Info => write!(f, "INFO"),
            FindingSeverity::Low => write!(f, "LOW"),
            FindingSeverity::Medium => write!(f, "MEDIUM"),
            FindingSeverity::High => write!(f, "HIGH"),
            FindingSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Builds the system prompt for LLM analysis of network tool output.
fn build_system_prompt(tool: NetworkTool) -> String {
    let tool_context = match tool {
        NetworkTool::PortScan | NetworkTool::MassScan | NetworkTool::ServiceScan => {
            "You are analyzing port scan results. Identify open services, \
             potential vulnerabilities (known CVEs for service versions), \
             unnecessary exposed ports, and security risks."
        }
        NetworkTool::PingSweep | NetworkTool::ArpScan => {
            "You are analyzing network discovery results. Identify active hosts, \
             rogue devices, unexpected hosts on the network, and network topology insights."
        }
        NetworkTool::DnsLookup | NetworkTool::DnsEnum => {
            "You are analyzing DNS records. Identify misconfigurations, \
             dangling records, potential subdomain takeover risks, \
             and information disclosure through DNS."
        }
        NetworkTool::TraceRoute | NetworkTool::NetworkDiag => {
            "You are analyzing network path/routing data. Identify \
             high-latency hops, packet loss points, unexpected routing paths, \
             and potential network issues."
        }
        NetworkTool::SocketStats => {
            "You are analyzing network socket/connection data. Identify \
             unexpected listening services, suspicious outbound connections, \
             and potential backdoors or data exfiltration."
        }
        NetworkTool::PacketCapture | NetworkTool::DeepInspect | NetworkTool::NetworkGrep => {
            "You are analyzing captured network traffic. Identify \
             unencrypted sensitive data, suspicious traffic patterns, \
             protocol anomalies, and potential attacks."
        }
        NetworkTool::WebScan | NetworkTool::VulnScanner => {
            "You are analyzing vulnerability scan results. Prioritize \
             findings by severity, identify false positives where possible, \
             and recommend remediation steps."
        }
        NetworkTool::DirBust | NetworkTool::DirFuzz => {
            "You are analyzing web directory/endpoint discovery results. \
             Identify sensitive exposed paths, admin panels, backup files, \
             and potential information disclosure."
        }
        _ => {
            "You are a security analyst reviewing network tool output. \
             Identify security risks, misconfigurations, and anomalies."
        }
    };

    format!(
        "{}\n\n\
         Respond in this exact format:\n\
         SUMMARY: <1-2 sentence overview>\n\
         RISK: <0.0 to 1.0>\n\
         FINDINGS:\n\
         - [SEVERITY] CATEGORY: description | evidence\n\
         RECOMMENDATIONS:\n\
         - <action item>\n\n\
         Severity levels: INFO, LOW, MEDIUM, HIGH, CRITICAL\n\
         Be concise and actionable. Focus on real security implications.",
        tool_context
    )
}

/// Format parsed output as structured text for the LLM prompt.
fn format_parsed_output(parsed: &ParsedOutput) -> String {
    match parsed {
        ParsedOutput::HostScan { hosts, scan_type } => {
            let mut out = format!("Scan type: {}\nHosts found: {}\n\n", scan_type, hosts.len());
            for host in hosts {
                out.push_str(&format!("Host: {} ({})\n", host.address,
                    host.hostname.as_deref().unwrap_or("no hostname")));
                if let Some(mac) = &host.mac_address {
                    out.push_str(&format!("  MAC: {} ({})\n", mac,
                        host.vendor.as_deref().unwrap_or("unknown vendor")));
                }
                for port in &host.ports {
                    out.push_str(&format!("  {}/{} {} - {} {}\n",
                        port.port, port.protocol, port.state,
                        port.service.as_deref().unwrap_or("unknown"),
                        port.version.as_deref().unwrap_or("")));
                }
                out.push('\n');
            }
            out
        }
        ParsedOutput::DnsResult { records, query } => {
            let mut out = format!("DNS query: {}\nRecords: {}\n\n", query, records.len());
            for record in records {
                out.push_str(&format!("{} {} {} (TTL: {})\n",
                    record.name, record.record_type, record.value,
                    record.ttl.map(|t| t.to_string()).unwrap_or("N/A".into())));
            }
            out
        }
        ParsedOutput::TraceResult { hops, target } => {
            let mut out = format!("Trace to: {}\nHops: {}\n\n", target, hops.len());
            for hop in hops {
                let addr = hop.address.as_deref().unwrap_or("*");
                let host = hop.hostname.as_deref().unwrap_or("");
                let rtt = hop.rtt_ms.map(|r| format!("{:.1}ms", r)).unwrap_or("timeout".into());
                let loss = hop.loss_pct.map(|l| format!(" ({:.0}% loss)", l)).unwrap_or_default();
                out.push_str(&format!("  {:>2}. {} {} {}{}\n", hop.hop_number, addr, host, rtt, loss));
            }
            out
        }
        ParsedOutput::SocketList { sockets } => {
            let mut out = format!("Active sockets: {}\n\n", sockets.len());
            for sock in sockets {
                out.push_str(&format!("{} {} {} -> {} {}\n",
                    sock.state, sock.protocol, sock.local_addr, sock.remote_addr,
                    sock.process.as_deref().unwrap_or("")));
            }
            out
        }
        ParsedOutput::Raw { summary } => {
            summary.clone()
        }
    }
}

/// Build a complete analysis prompt from tool output.
pub fn build_analysis_prompt(output: &ToolOutput, parsed: &ParsedOutput) -> (String, String) {
    let system = build_system_prompt(output.tool);
    let user = format!(
        "Analyze the following {} output:\n\n{}\n\nExecution details:\n- Duration: {:.1}s\n- Exit code: {}",
        output.tool,
        format_parsed_output(parsed),
        output.duration.as_secs_f64(),
        output.exit_code
    );
    (system, user)
}

/// Parse the LLM response into a structured ToolAnalysis.
pub fn parse_analysis_response(response: &str, tool: NetworkTool) -> ToolAnalysis {
    let mut summary = String::new();
    let mut risk_score = 0.0;
    let mut findings = Vec::new();
    let mut recommendations = Vec::new();
    let mut in_findings = false;
    let mut in_recommendations = false;

    for line in response.lines() {
        let line = line.trim();

        if let Some(s) = line.strip_prefix("SUMMARY:") {
            summary = s.trim().to_string();
            in_findings = false;
            in_recommendations = false;
            continue;
        }

        if let Some(r) = line.strip_prefix("RISK:") {
            risk_score = r.trim().parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0);
            in_findings = false;
            in_recommendations = false;
            continue;
        }

        if line == "FINDINGS:" {
            in_findings = true;
            in_recommendations = false;
            continue;
        }

        if line == "RECOMMENDATIONS:" {
            in_findings = false;
            in_recommendations = true;
            continue;
        }

        if in_findings {
            if let Some(finding) = parse_finding_line(line) {
                findings.push(finding);
            }
        }

        if in_recommendations {
            if let Some(rec) = line.strip_prefix("- ") {
                if !rec.is_empty() {
                    recommendations.push(rec.to_string());
                }
            }
        }
    }

    // Fallback if parsing found nothing
    if summary.is_empty() {
        summary = response.lines().next().unwrap_or("Analysis complete").to_string();
    }

    ToolAnalysis {
        summary,
        risk_score,
        findings,
        recommendations,
        tool,
        raw_response: response.to_string(),
    }
}

fn parse_finding_line(line: &str) -> Option<Finding> {
    let line = line.strip_prefix("- ")?;
    // Format: [SEVERITY] CATEGORY: description | evidence
    let (severity_str, rest) = if line.starts_with('[') {
        let end = line.find(']')?;
        (&line[1..end], line[end + 1..].trim())
    } else {
        return None;
    };

    let severity = match severity_str.to_uppercase().as_str() {
        "INFO" => FindingSeverity::Info,
        "LOW" => FindingSeverity::Low,
        "MEDIUM" | "MED" => FindingSeverity::Medium,
        "HIGH" => FindingSeverity::High,
        "CRITICAL" | "CRIT" => FindingSeverity::Critical,
        _ => FindingSeverity::Info,
    };

    let (category, desc_evidence) = rest.split_once(':')?;
    let (description, evidence) = if let Some((d, e)) = desc_evidence.split_once('|') {
        (d.trim().to_string(), e.trim().to_string())
    } else {
        (desc_evidence.trim().to_string(), String::new())
    };

    Some(Finding {
        severity,
        category: category.trim().to_string(),
        description,
        evidence,
    })
}

/// Send tool output to the LLM Gateway for analysis via HTTP.
pub async fn analyze_via_gateway(
    output: &ToolOutput,
    parsed: &ParsedOutput,
    gateway_url: &str,
    model: &str,
) -> Result<ToolAnalysis> {
    let (system_prompt, user_prompt) = build_analysis_prompt(output, parsed);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("Failed to create HTTP client")?;

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 2048
    });

    info!(tool = %output.tool, model, "Sending tool output to LLM for analysis");

    let response = client
        .post(format!("{}/v1/chat/completions", gateway_url))
        .json(&body)
        .send()
        .await
        .context("Failed to reach LLM Gateway")?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow!("LLM Gateway returned {}: {}", status, text));
    }

    let resp: serde_json::Value = response.json().await
        .context("Failed to parse LLM Gateway response")?;

    let text = resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("No analysis generated");

    Ok(parse_analysis_response(text, output.tool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_tools::{DiscoveredHost, DiscoveredPort, DnsRecord, TraceHop, SocketEntry};

    #[test]
    fn test_build_system_prompt_port_scan() {
        let prompt = build_system_prompt(NetworkTool::PortScan);
        assert!(prompt.contains("port scan"));
        assert!(prompt.contains("SUMMARY:"));
        assert!(prompt.contains("RISK:"));
    }

    #[test]
    fn test_build_system_prompt_dns() {
        let prompt = build_system_prompt(NetworkTool::DnsLookup);
        assert!(prompt.contains("DNS"));
    }

    #[test]
    fn test_build_system_prompt_vuln() {
        let prompt = build_system_prompt(NetworkTool::VulnScanner);
        assert!(prompt.contains("vulnerability"));
    }

    #[test]
    fn test_format_host_scan() {
        let parsed = ParsedOutput::HostScan {
            hosts: vec![DiscoveredHost {
                address: "10.0.0.1".into(),
                hostname: Some("server.local".into()),
                mac_address: None,
                vendor: None,
                state: "up".into(),
                ports: vec![
                    DiscoveredPort {
                        port: 22, protocol: "tcp".into(), state: "open".into(),
                        service: Some("ssh".into()), version: Some("OpenSSH 8.9".into()),
                    },
                ],
            }],
            scan_type: "port".into(),
        };
        let text = format_parsed_output(&parsed);
        assert!(text.contains("10.0.0.1"));
        assert!(text.contains("server.local"));
        assert!(text.contains("22/tcp"));
        assert!(text.contains("OpenSSH 8.9"));
    }

    #[test]
    fn test_format_dns_result() {
        let parsed = ParsedOutput::DnsResult {
            records: vec![DnsRecord {
                name: "example.com".into(),
                record_type: "A".into(),
                value: "93.184.216.34".into(),
                ttl: Some(300),
            }],
            query: "example.com".into(),
        };
        let text = format_parsed_output(&parsed);
        assert!(text.contains("example.com"));
        assert!(text.contains("93.184.216.34"));
    }

    #[test]
    fn test_format_trace_result() {
        let parsed = ParsedOutput::TraceResult {
            hops: vec![
                TraceHop { hop_number: 1, address: Some("10.0.0.1".into()), hostname: None, rtt_ms: Some(0.5), loss_pct: None },
                TraceHop { hop_number: 2, address: None, hostname: None, rtt_ms: None, loss_pct: None },
            ],
            target: "example.com".into(),
        };
        let text = format_parsed_output(&parsed);
        assert!(text.contains("10.0.0.1"));
        assert!(text.contains("0.5ms"));
        assert!(text.contains("timeout"));
    }

    #[test]
    fn test_format_socket_list() {
        let parsed = ParsedOutput::SocketList {
            sockets: vec![SocketEntry {
                state: "ESTAB".into(),
                protocol: "tcp".into(),
                local_addr: "0.0.0.0:22".into(),
                remote_addr: "10.0.0.5:55555".into(),
                process: Some("sshd".into()),
            }],
        };
        let text = format_parsed_output(&parsed);
        assert!(text.contains("ESTAB"));
        assert!(text.contains("sshd"));
    }

    #[test]
    fn test_parse_analysis_response() {
        let response = "\
SUMMARY: Found 3 open ports with 1 critical vulnerability.
RISK: 0.7
FINDINGS:
- [CRITICAL] Exposed Service: SSH on port 22 running outdated OpenSSH | OpenSSH 7.4 has known CVEs
- [MEDIUM] Configuration: HTTP without TLS on port 80 | Unencrypted traffic
- [INFO] Service: DNS resolver responding on port 53 | Expected for DNS server
RECOMMENDATIONS:
- Upgrade OpenSSH to version 9.0 or later
- Enable TLS for HTTP traffic (port 80 → 443)
- Restrict DNS access to internal networks only";

        let analysis = parse_analysis_response(response, NetworkTool::PortScan);

        assert_eq!(analysis.summary, "Found 3 open ports with 1 critical vulnerability.");
        assert!((analysis.risk_score - 0.7).abs() < 0.01);
        assert_eq!(analysis.findings.len(), 3);
        assert_eq!(analysis.findings[0].severity, FindingSeverity::Critical);
        assert_eq!(analysis.findings[0].category, "Exposed Service");
        assert!(analysis.findings[0].evidence.contains("CVE"));
        assert_eq!(analysis.findings[1].severity, FindingSeverity::Medium);
        assert_eq!(analysis.findings[2].severity, FindingSeverity::Info);
        assert_eq!(analysis.recommendations.len(), 3);
        assert!(analysis.recommendations[0].contains("OpenSSH"));
    }

    #[test]
    fn test_parse_analysis_response_minimal() {
        let response = "No significant findings.";
        let analysis = parse_analysis_response(response, NetworkTool::DnsLookup);
        assert_eq!(analysis.summary, "No significant findings.");
        assert_eq!(analysis.risk_score, 0.0);
        assert!(analysis.findings.is_empty());
    }

    #[test]
    fn test_parse_finding_line() {
        let finding = parse_finding_line("- [HIGH] Network: Exposed admin panel | /admin accessible without auth").unwrap();
        assert_eq!(finding.severity, FindingSeverity::High);
        assert_eq!(finding.category, "Network");
        assert!(finding.description.contains("admin panel"));
        assert!(finding.evidence.contains("/admin"));
    }

    #[test]
    fn test_parse_finding_no_evidence() {
        let finding = parse_finding_line("- [LOW] Configuration: Default credentials may be in use").unwrap();
        assert_eq!(finding.severity, FindingSeverity::Low);
        assert!(finding.evidence.is_empty());
    }

    #[test]
    fn test_risk_score_clamped() {
        let response = "SUMMARY: test\nRISK: 5.0\n";
        let analysis = parse_analysis_response(response, NetworkTool::PortScan);
        assert_eq!(analysis.risk_score, 1.0);
    }

    #[test]
    fn test_build_analysis_prompt() {
        let output = ToolOutput {
            stdout: "test output".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_secs(5),
            tool: NetworkTool::PortScan,
            audit_entry: String::new(),
        };
        let parsed = ParsedOutput::HostScan {
            hosts: vec![],
            scan_type: "port".into(),
        };
        let (system, user) = build_analysis_prompt(&output, &parsed);
        assert!(system.contains("port scan"));
        assert!(user.contains("5.0s"));
        assert!(user.contains("Exit code: 0"));
    }

    #[test]
    fn test_finding_severity_display() {
        assert_eq!(FindingSeverity::Critical.to_string(), "CRITICAL");
        assert_eq!(FindingSeverity::Info.to_string(), "INFO");
    }

    #[test]
    fn test_finding_severity_ordering() {
        assert!(FindingSeverity::Info < FindingSeverity::Low);
        assert!(FindingSeverity::Low < FindingSeverity::Medium);
        assert!(FindingSeverity::High < FindingSeverity::Critical);
    }
}
