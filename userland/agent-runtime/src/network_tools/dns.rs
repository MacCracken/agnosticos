//! DNS investigation wrappers — dig and dnsrecon.

use anyhow::Result;

use super::parse::parse_output;
use super::runner::NetworkToolRunner;
use super::types::{DnsRecord, NetworkTool, NetworkToolConfig, ParsedOutput};

/// DNS investigation wrapping dig and dnsrecon.
#[derive(Debug)]
pub struct DnsInvestigator {
    pub(super) runner: NetworkToolRunner,
    pub(super) record_types: Vec<String>,
    pub(super) use_dnsrecon: bool,
    pub(super) nameserver: Option<String>,
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
