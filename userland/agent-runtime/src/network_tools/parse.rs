//! Output parsing dispatcher — routes tool output to the appropriate parser.

use super::capture::{parse_socket_output, parse_trace_output};
use super::dns::parse_dns_output;
use super::nmap::parse_scan_output;
use super::types::{NetworkTool, ParsedOutput, ToolOutput};

/// Parse tool output into structured results based on tool type
pub fn parse_output(output: &ToolOutput, target: Option<&str>) -> ParsedOutput {
    match output.tool {
        NetworkTool::PortScan
        | NetworkTool::PingSweep
        | NetworkTool::ServiceScan
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
                format!(
                    "{} exited with code {} ({} lines)",
                    output.tool, output.exit_code, line_count
                )
            };
            ParsedOutput::Raw { summary }
        }
    }
}
