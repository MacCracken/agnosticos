//! Tool Output Analysis via LLM Gateway
//!
//! Pipes structured network tool output through the LLM Gateway
//! for automated interpretation, threat identification, and reporting.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::network_tools::{NetworkTool, ParsedOutput, ToolOutput};

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

impl std::str::FromStr for FindingSeverity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "INFO" => Ok(Self::Info),
            "LOW" => Ok(Self::Low),
            "MEDIUM" | "MED" => Ok(Self::Medium),
            "HIGH" => Ok(Self::High),
            "CRITICAL" | "CRIT" => Ok(Self::Critical),
            other => Err(format!("unknown severity: {other}")),
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
                out.push_str(&format!(
                    "Host: {} ({})\n",
                    host.address,
                    host.hostname.as_deref().unwrap_or("no hostname")
                ));
                if let Some(mac) = &host.mac_address {
                    out.push_str(&format!(
                        "  MAC: {} ({})\n",
                        mac,
                        host.vendor.as_deref().unwrap_or("unknown vendor")
                    ));
                }
                for port in &host.ports {
                    out.push_str(&format!(
                        "  {}/{} {} - {} {}\n",
                        port.port,
                        port.protocol,
                        port.state,
                        port.service.as_deref().unwrap_or("unknown"),
                        port.version.as_deref().unwrap_or("")
                    ));
                }
                out.push('\n');
            }
            out
        }
        ParsedOutput::DnsResult { records, query } => {
            let mut out = format!("DNS query: {}\nRecords: {}\n\n", query, records.len());
            for record in records {
                out.push_str(&format!(
                    "{} {} {} (TTL: {})\n",
                    record.name,
                    record.record_type,
                    record.value,
                    record.ttl.map(|t| t.to_string()).unwrap_or("N/A".into())
                ));
            }
            out
        }
        ParsedOutput::TraceResult { hops, target } => {
            let mut out = format!("Trace to: {}\nHops: {}\n\n", target, hops.len());
            for hop in hops {
                let addr = hop.address.as_deref().unwrap_or("*");
                let host = hop.hostname.as_deref().unwrap_or("");
                let rtt = hop
                    .rtt_ms
                    .map(|r| format!("{:.1}ms", r))
                    .unwrap_or("timeout".into());
                let loss = hop
                    .loss_pct
                    .map(|l| format!(" ({:.0}% loss)", l))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {:>2}. {} {} {}{}\n",
                    hop.hop_number, addr, host, rtt, loss
                ));
            }
            out
        }
        ParsedOutput::SocketList { sockets } => {
            let mut out = format!("Active sockets: {}\n\n", sockets.len());
            for sock in sockets {
                out.push_str(&format!(
                    "{} {} {} -> {} {}\n",
                    sock.state,
                    sock.protocol,
                    sock.local_addr,
                    sock.remote_addr,
                    sock.process.as_deref().unwrap_or("")
                ));
            }
            out
        }
        ParsedOutput::Raw { summary } => summary.clone(),
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
        summary = response
            .lines()
            .next()
            .unwrap_or("Analysis complete")
            .to_string();
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

    let severity = severity_str.parse::<FindingSeverity>().unwrap_or(FindingSeverity::Info);

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

    let resp: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse LLM Gateway response")?;

    let text = resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("No analysis generated");

    Ok(parse_analysis_response(text, output.tool))
}

/// A structured reasoning trace capturing the full chain-of-thought
/// for an agent task: input -> reasoning -> tool calls -> output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTrace {
    /// Unique trace ID
    pub trace_id: String,
    /// Agent that executed this trace
    pub agent_id: String,
    /// Task or query that triggered this trace
    pub input: String,
    /// Ordered steps in the reasoning chain
    pub steps: Vec<ReasoningStep>,
    /// Final output/conclusion
    pub output: Option<String>,
    /// Overall status
    pub status: TraceStatus,
    /// When the trace started
    pub started_at: String,
    /// When the trace completed
    pub completed_at: Option<String>,
    /// Total duration in milliseconds
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step number (1-indexed)
    pub step: u32,
    /// What the agent decided to do
    pub action: String,
    /// Why (reasoning)
    pub rationale: String,
    /// Tool invoked (if any)
    pub tool_call: Option<String>,
    /// Tool output (if any)
    pub tool_output: Option<String>,
    /// Duration of this step in milliseconds
    pub duration_ms: u64,
    /// Whether this step succeeded
    pub success: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TraceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceStatus::InProgress => write!(f, "IN_PROGRESS"),
            TraceStatus::Completed => write!(f, "COMPLETED"),
            TraceStatus::Failed => write!(f, "FAILED"),
            TraceStatus::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

/// Builder for constructing reasoning traces incrementally
pub struct TraceBuilder {
    trace: ReasoningTrace,
    step_count: u32,
}

impl TraceBuilder {
    pub fn new(agent_id: &str, input: &str) -> Self {
        Self {
            trace: ReasoningTrace {
                trace_id: uuid::Uuid::new_v4().to_string(),
                agent_id: agent_id.to_string(),
                input: input.to_string(),
                steps: Vec::new(),
                output: None,
                status: TraceStatus::InProgress,
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: None,
                duration_ms: None,
            },
            step_count: 0,
        }
    }

    /// Add a reasoning step
    pub fn add_step(
        &mut self,
        action: &str,
        rationale: &str,
        tool_call: Option<&str>,
        tool_output: Option<&str>,
        duration_ms: u64,
        success: bool,
    ) {
        self.step_count += 1;
        self.trace.steps.push(ReasoningStep {
            step: self.step_count,
            action: action.to_string(),
            rationale: rationale.to_string(),
            tool_call: tool_call.map(String::from),
            tool_output: tool_output.map(String::from),
            duration_ms,
            success,
        });
    }

    /// Complete the trace successfully
    pub fn complete(mut self, output: &str) -> ReasoningTrace {
        self.trace.output = Some(output.to_string());
        self.trace.status = TraceStatus::Completed;
        self.trace.completed_at = Some(chrono::Utc::now().to_rfc3339());
        let total_ms: u64 = self.trace.steps.iter().map(|s| s.duration_ms).sum();
        self.trace.duration_ms = Some(total_ms);
        self.trace
    }

    /// Mark the trace as failed
    pub fn fail(mut self, error: &str) -> ReasoningTrace {
        self.trace.output = Some(error.to_string());
        self.trace.status = TraceStatus::Failed;
        self.trace.completed_at = Some(chrono::Utc::now().to_rfc3339());
        let total_ms: u64 = self.trace.steps.iter().map(|s| s.duration_ms).sum();
        self.trace.duration_ms = Some(total_ms);
        self.trace
    }

    /// Cancel the trace
    pub fn cancel(mut self) -> ReasoningTrace {
        self.trace.status = TraceStatus::Cancelled;
        self.trace.completed_at = Some(chrono::Utc::now().to_rfc3339());
        self.trace
    }

    /// Get current step count
    pub fn step_count(&self) -> u32 {
        self.step_count
    }

    /// Get trace ID
    pub fn trace_id(&self) -> &str {
        &self.trace.trace_id
    }
}

/// Format a reasoning trace for human-readable display
pub fn format_trace(trace: &ReasoningTrace) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Trace: {} [{}]", trace.trace_id, trace.status));
    lines.push(format!("Agent: {}", trace.agent_id));
    lines.push(format!("Input: {}", trace.input));
    lines.push(format!("Started: {}", trace.started_at));
    if let Some(ref completed) = trace.completed_at {
        lines.push(format!("Completed: {}", completed));
    }
    if let Some(ms) = trace.duration_ms {
        lines.push(format!("Duration: {}ms", ms));
    }
    lines.push("\u{2500}".repeat(60));

    for step in &trace.steps {
        let status_icon = if step.success { "OK" } else { "FAIL" };
        lines.push(format!(
            "Step {}: {} [{}] ({}ms)",
            step.step, step.action, status_icon, step.duration_ms
        ));
        lines.push(format!("  Rationale: {}", step.rationale));
        if let Some(ref tool) = step.tool_call {
            lines.push(format!("  Tool: {}", tool));
        }
        if let Some(ref output) = step.tool_output {
            let truncated = if output.len() > 200 {
                format!("{}...", &output[..200])
            } else {
                output.clone()
            };
            lines.push(format!("  Output: {}", truncated));
        }
    }

    lines.push("\u{2500}".repeat(60));
    if let Some(ref output) = trace.output {
        lines.push(format!("Result: {}", output));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_tools::{DiscoveredHost, DiscoveredPort, DnsRecord, SocketEntry, TraceHop};

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
                ports: vec![DiscoveredPort {
                    port: 22,
                    protocol: "tcp".into(),
                    state: "open".into(),
                    service: Some("ssh".into()),
                    version: Some("OpenSSH 8.9".into()),
                }],
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
                TraceHop {
                    hop_number: 1,
                    address: Some("10.0.0.1".into()),
                    hostname: None,
                    rtt_ms: Some(0.5),
                    loss_pct: None,
                },
                TraceHop {
                    hop_number: 2,
                    address: None,
                    hostname: None,
                    rtt_ms: None,
                    loss_pct: None,
                },
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

        assert_eq!(
            analysis.summary,
            "Found 3 open ports with 1 critical vulnerability."
        );
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
        let finding = parse_finding_line(
            "- [HIGH] Network: Exposed admin panel | /admin accessible without auth",
        )
        .unwrap();
        assert_eq!(finding.severity, FindingSeverity::High);
        assert_eq!(finding.category, "Network");
        assert!(finding.description.contains("admin panel"));
        assert!(finding.evidence.contains("/admin"));
    }

    #[test]
    fn test_parse_finding_no_evidence() {
        let finding =
            parse_finding_line("- [LOW] Configuration: Default credentials may be in use").unwrap();
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

    // ── ReasoningTrace / TraceBuilder tests ───────────────────────────

    #[test]
    fn test_trace_builder_new() {
        let builder = TraceBuilder::new("agent-1", "scan network");
        assert_eq!(builder.step_count(), 0);
        assert!(!builder.trace_id().is_empty());
        // trace_id should be a valid UUID
        assert!(uuid::Uuid::parse_str(builder.trace_id()).is_ok());
    }

    #[test]
    fn test_trace_builder_add_step() {
        let mut builder = TraceBuilder::new("agent-1", "scan network");
        builder.add_step(
            "lookup DNS",
            "need to resolve target",
            Some("dns_lookup"),
            Some("93.184.216.34"),
            50,
            true,
        );
        assert_eq!(builder.step_count(), 1);

        builder.add_step(
            "port scan",
            "check open ports",
            Some("nmap"),
            None,
            200,
            true,
        );
        assert_eq!(builder.step_count(), 2);
    }

    #[test]
    fn test_trace_builder_complete() {
        let mut builder = TraceBuilder::new("agent-1", "scan");
        builder.add_step("step1", "reason", None, None, 100, true);
        builder.add_step("step2", "reason2", None, None, 200, true);

        let trace = builder.complete("All done");
        assert_eq!(trace.status, TraceStatus::Completed);
        assert_eq!(trace.output.as_deref(), Some("All done"));
        assert!(trace.completed_at.is_some());
        assert_eq!(trace.duration_ms, Some(300));
        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.steps[0].step, 1);
        assert_eq!(trace.steps[1].step, 2);
    }

    #[test]
    fn test_trace_builder_fail() {
        let mut builder = TraceBuilder::new("agent-1", "scan");
        builder.add_step("attempt", "trying", Some("nmap"), None, 500, false);

        let trace = builder.fail("connection refused");
        assert_eq!(trace.status, TraceStatus::Failed);
        assert_eq!(trace.output.as_deref(), Some("connection refused"));
        assert!(trace.completed_at.is_some());
        assert_eq!(trace.duration_ms, Some(500));
    }

    #[test]
    fn test_trace_builder_cancel() {
        let builder = TraceBuilder::new("agent-1", "long task");
        let trace = builder.cancel();
        assert_eq!(trace.status, TraceStatus::Cancelled);
        assert!(trace.completed_at.is_some());
        assert!(trace.output.is_none());
        assert!(trace.duration_ms.is_none());
    }

    #[test]
    fn test_trace_builder_step_count() {
        let mut builder = TraceBuilder::new("a", "b");
        assert_eq!(builder.step_count(), 0);
        builder.add_step("x", "y", None, None, 10, true);
        assert_eq!(builder.step_count(), 1);
        builder.add_step("x2", "y2", None, None, 20, true);
        assert_eq!(builder.step_count(), 2);
    }

    #[test]
    fn test_trace_builder_trace_id() {
        let b1 = TraceBuilder::new("a", "b");
        let b2 = TraceBuilder::new("a", "b");
        // Each builder gets a unique trace ID
        assert_ne!(b1.trace_id(), b2.trace_id());
    }

    #[test]
    fn test_format_trace_output() {
        let mut builder = TraceBuilder::new("agent-1", "check host");
        builder.add_step(
            "ping",
            "verify host is up",
            Some("ping"),
            Some("64 bytes from 10.0.0.1"),
            30,
            true,
        );
        let trace = builder.complete("Host is up");

        let formatted = format_trace(&trace);
        assert!(formatted.contains("agent-1"));
        assert!(formatted.contains("check host"));
        assert!(formatted.contains("Step 1: ping [OK] (30ms)"));
        assert!(formatted.contains("Rationale: verify host is up"));
        assert!(formatted.contains("Tool: ping"));
        assert!(formatted.contains("Output: 64 bytes from 10.0.0.1"));
        assert!(formatted.contains("Result: Host is up"));
        assert!(formatted.contains("Duration: 30ms"));
    }

    #[test]
    fn test_reasoning_trace_serialization_roundtrip() {
        let mut builder = TraceBuilder::new("agent-x", "test task");
        builder.add_step(
            "do thing",
            "because",
            Some("tool"),
            Some("output"),
            42,
            true,
        );
        let trace = builder.complete("done");

        let json = serde_json::to_string(&trace).unwrap();
        let restored: ReasoningTrace = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.trace_id, trace.trace_id);
        assert_eq!(restored.agent_id, "agent-x");
        assert_eq!(restored.input, "test task");
        assert_eq!(restored.steps.len(), 1);
        assert_eq!(restored.steps[0].action, "do thing");
        assert_eq!(restored.steps[0].tool_call.as_deref(), Some("tool"));
        assert_eq!(restored.status, TraceStatus::Completed);
        assert_eq!(restored.output.as_deref(), Some("done"));
    }

    #[test]
    fn test_trace_status_display() {
        assert_eq!(TraceStatus::InProgress.to_string(), "IN_PROGRESS");
        assert_eq!(TraceStatus::Completed.to_string(), "COMPLETED");
        assert_eq!(TraceStatus::Failed.to_string(), "FAILED");
        assert_eq!(TraceStatus::Cancelled.to_string(), "CANCELLED");
    }

    #[test]
    fn test_trace_multiple_steps() {
        let mut builder = TraceBuilder::new("agent", "multi-step");
        for i in 0..5 {
            builder.add_step(
                &format!("step{}", i),
                &format!("reason{}", i),
                None,
                None,
                10 * (i as u64 + 1),
                i < 4, // last step fails
            );
        }
        let trace = builder.complete("partial success");
        assert_eq!(trace.steps.len(), 5);
        assert!(trace.steps[0].success);
        assert!(!trace.steps[4].success);
        // Duration should be sum: 10+20+30+40+50 = 150
        assert_eq!(trace.duration_ms, Some(150));
    }

    #[test]
    fn test_format_trace_tool_output_truncation() {
        let long_output = "A".repeat(300);
        let mut builder = TraceBuilder::new("agent", "truncation test");
        builder.add_step(
            "action",
            "reason",
            Some("tool"),
            Some(&long_output),
            10,
            true,
        );
        let trace = builder.complete("done");

        let formatted = format_trace(&trace);
        // The formatted output should truncate at 200 chars + "..."
        assert!(formatted.contains(&format!("{}...", &"A".repeat(200))));
        // Should NOT contain the full 300-char string
        assert!(!formatted.contains(&"A".repeat(300)));
    }

    #[test]
    fn test_trace_duration_calculation() {
        let mut builder = TraceBuilder::new("agent", "test");
        builder.add_step("a", "r", None, None, 100, true);
        builder.add_step("b", "r", None, None, 250, true);
        builder.add_step("c", "r", None, None, 50, true);
        let trace = builder.complete("ok");
        assert_eq!(trace.duration_ms, Some(400));
    }

    #[test]
    fn test_format_trace_empty() {
        let builder = TraceBuilder::new("agent", "empty task");
        let trace = builder.complete("nothing to do");

        let formatted = format_trace(&trace);
        assert!(formatted.contains("empty task"));
        assert!(formatted.contains("Result: nothing to do"));
        assert!(formatted.contains("Duration: 0ms"));
        // Should not contain any "Step" lines
        assert!(!formatted.contains("Step "));
    }

    #[test]
    fn test_reasoning_step_serialization() {
        let step = ReasoningStep {
            step: 1,
            action: "scan".into(),
            rationale: "need info".into(),
            tool_call: Some("nmap".into()),
            tool_output: Some("port 22 open".into()),
            duration_ms: 150,
            success: true,
        };

        let json = serde_json::to_string(&step).unwrap();
        let restored: ReasoningStep = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.step, 1);
        assert_eq!(restored.action, "scan");
        assert_eq!(restored.tool_call.as_deref(), Some("nmap"));
        assert!(restored.success);
    }
}
