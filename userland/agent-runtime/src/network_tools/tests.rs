use std::net::IpAddr;
use std::time::Duration;

use super::capture::{parse_socket_output, parse_trace_output};
use super::dns::parse_dns_output;
use super::nmap::{parse_nmap_port_line, parse_scan_output};
use super::parse::parse_output;
use super::runner::{
    is_rfc1918, reject_broadcast, validate_args, validate_hostname, validate_target,
    NetworkToolRunner,
};
use super::types::{
    DiscoveredHost, DiscoveredPort, DnsRecord, NetworkTool, NetworkToolConfig, ParsedOutput,
    RiskLevel, ScanProfile, SocketEntry, ToolOutput, TraceHop, ValidatedTarget, ALL_TOOLS,
};
use super::{
    DnsInvestigator, NetworkProber, PortScanner, SocketInspector, TrafficAnalyzer, VulnAssessor,
    WebFuzzer,
};

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
    assert_eq!(result, ValidatedTarget::Hostname("example.com".to_string()));
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
    assert_eq!(tools.len(), 32);
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
    let configs: Vec<_> = ALL_TOOLS
        .iter()
        .map(|t| NetworkToolConfig::for_tool(*t))
        .collect();
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
    assert!(configs.iter().any(|c| c.tool == NetworkTool::MassScan));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::ArpScan));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::NetworkDiag));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::DataRelay));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::DeepInspect));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::NetworkGrep));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::SocketStats));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::DnsEnum));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::DirFuzz));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::VulnScanner));
    assert!(configs
        .iter()
        .any(|c| c.tool == NetworkTool::BandwidthMonitor));
    assert!(configs
        .iter()
        .any(|c| c.tool == NetworkTool::PassiveFingerprint));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::NetDiscover));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::TermShark));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::BetterCap));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::DnsX));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::Fierce));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::WebAppFuzz));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::SqlMap));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::AircrackNg));
    assert!(configs.iter().any(|c| c.tool == NetworkTool::Kismet));
}

// --- New tool config tests ---

#[test]
fn test_masscan_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::MassScan);
    assert_eq!(cfg.binary_name, "masscan");
    assert_eq!(cfg.risk_level, RiskLevel::High);
    assert!(cfg.requires_approval);
}

#[test]
fn test_mtr_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::NetworkDiag);
    assert_eq!(cfg.binary_name, "mtr");
    assert_eq!(cfg.risk_level, RiskLevel::Medium);
    assert!(!cfg.requires_approval);
}

#[test]
fn test_ss_config_low_risk() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::SocketStats);
    assert_eq!(cfg.binary_name, "ss");
    assert_eq!(cfg.risk_level, RiskLevel::Low);
    assert!(!cfg.requires_approval);
}

#[test]
fn test_nuclei_config_critical() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::VulnScanner);
    assert_eq!(cfg.binary_name, "nuclei");
    assert_eq!(cfg.risk_level, RiskLevel::Critical);
    assert!(cfg.requires_approval);
}

#[test]
fn test_tshark_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::DeepInspect);
    assert_eq!(cfg.binary_name, "tshark");
    assert!(!cfg.allowed_in_sandbox);
}

#[test]
fn test_ffuf_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::DirFuzz);
    assert_eq!(cfg.binary_name, "ffuf");
    assert!(cfg.requires_approval);
}

#[test]
fn test_p0f_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::PassiveFingerprint);
    assert_eq!(cfg.binary_name, "p0f");
    assert!(!cfg.allowed_in_sandbox);
}

#[test]
fn test_arp_scan_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::ArpScan);
    assert_eq!(cfg.binary_name, "arp-scan");
    assert_eq!(cfg.risk_level, RiskLevel::Medium);
}

// --- Dangerous arg rejection for new tools ---

#[test]
fn test_reject_masscan_high_rate() {
    let args = vec!["--rate=100000".to_string()];
    assert!(validate_args(NetworkTool::MassScan, &args, false).is_err());
}

#[test]
fn test_allow_masscan_safe_rate() {
    let args = vec!["--rate=5000".to_string()];
    assert!(validate_args(NetworkTool::MassScan, &args, false).is_ok());
}

#[test]
fn test_reject_nuclei_custom_templates() {
    let args = vec!["-t".to_string(), "/tmp/evil.yaml".to_string()];
    assert!(validate_args(NetworkTool::VulnScanner, &args, false).is_err());
}

#[test]
fn test_allow_nuclei_custom_templates_with_approval() {
    let args = vec!["-t".to_string(), "/tmp/custom.yaml".to_string()];
    assert!(validate_args(NetworkTool::VulnScanner, &args, true).is_ok());
}

#[test]
fn test_reject_tshark_write() {
    let args = vec!["-w capture.pcap".to_string()];
    assert!(validate_args(NetworkTool::DeepInspect, &args, false).is_err());
}

// --- Output parsing tests ---

#[test]
fn test_parse_nmap_scan_output() {
    let stdout = "\
Starting Nmap 7.94 ( https://nmap.org )
Nmap scan report for example.com (93.184.216.34)
Host is up (0.025s latency).
80/tcp   open  http    Apache httpd 2.4.41
443/tcp  open  https   nginx 1.18.0
22/tcp   closed ssh

Nmap done: 1 IP address (1 host up) scanned in 2.34 seconds";

    let result = parse_scan_output(stdout, NetworkTool::PortScan);
    match result {
        ParsedOutput::HostScan { hosts, scan_type } => {
            assert_eq!(scan_type, "port");
            assert_eq!(hosts.len(), 1);
            assert_eq!(hosts[0].address, "93.184.216.34");
            assert_eq!(hosts[0].hostname.as_deref(), Some("example.com"));
            assert_eq!(hosts[0].ports.len(), 3);
            assert_eq!(hosts[0].ports[0].port, 80);
            assert_eq!(hosts[0].ports[0].state, "open");
            assert_eq!(hosts[0].ports[0].service.as_deref(), Some("http"));
            assert_eq!(hosts[0].ports[1].port, 443);
            assert_eq!(hosts[0].ports[2].state, "closed");
        }
        _ => panic!("Expected HostScan"),
    }
}

#[test]
fn test_parse_nmap_ping_sweep() {
    let stdout = "\
Starting Nmap 7.94
Nmap scan report for 192.168.1.1
Host is up (0.001s latency).
MAC Address: AA:BB:CC:DD:EE:FF (RouterVendor)

Nmap scan report for 192.168.1.50
Host is up (0.003s latency).

Nmap done: 256 IP addresses (2 hosts up)";

    let result = parse_scan_output(stdout, NetworkTool::PingSweep);
    match result {
        ParsedOutput::HostScan { hosts, .. } => {
            assert_eq!(hosts.len(), 2);
            assert_eq!(hosts[0].address, "192.168.1.1");
            assert_eq!(hosts[0].mac_address.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
            assert_eq!(hosts[0].vendor.as_deref(), Some("RouterVendor"));
            assert_eq!(hosts[1].address, "192.168.1.50");
        }
        _ => panic!("Expected HostScan"),
    }
}

#[test]
fn test_parse_masscan_output() {
    let stdout = "\
Discovered open port 80/tcp on 10.0.0.1
Discovered open port 443/tcp on 10.0.0.1
Discovered open port 22/tcp on 10.0.0.2";

    let result = parse_scan_output(stdout, NetworkTool::MassScan);
    match result {
        ParsedOutput::HostScan { hosts, scan_type } => {
            assert_eq!(scan_type, "masscan");
            assert_eq!(hosts.len(), 2);
            let h1 = hosts.iter().find(|h| h.address == "10.0.0.1").unwrap();
            assert_eq!(h1.ports.len(), 2);
            let h2 = hosts.iter().find(|h| h.address == "10.0.0.2").unwrap();
            assert_eq!(h2.ports.len(), 1);
            assert_eq!(h2.ports[0].port, 22);
        }
        _ => panic!("Expected HostScan"),
    }
}

#[test]
fn test_parse_dns_output() {
    let stdout = "\
; <<>> DiG 9.18.12 <<>> example.com
;; ANSWER SECTION:
example.com.     300     IN      A       93.184.216.34
example.com.     300     IN      AAAA    2606:2800:220:1:248:1893:25c8:1946

;; Query time: 25 msec";

    let result = parse_dns_output(stdout, "example.com");
    match result {
        ParsedOutput::DnsResult { records, query } => {
            assert_eq!(query, "example.com");
            assert_eq!(records.len(), 2);
            assert_eq!(records[0].record_type, "A");
            assert_eq!(records[0].value, "93.184.216.34");
            assert_eq!(records[0].ttl, Some(300));
            assert_eq!(records[1].record_type, "AAAA");
        }
        _ => panic!("Expected DnsResult"),
    }
}

#[test]
fn test_parse_traceroute_output() {
    let stdout = "\
traceroute to example.com (93.184.216.34), 30 hops max
 1  gateway (192.168.1.1)  0.595 ms  0.521 ms  0.497 ms
 2  10.0.0.1 (10.0.0.1)  2.123 ms  1.998 ms  2.045 ms
 3  * * *";

    let result = parse_trace_output(stdout, "example.com");
    match result {
        ParsedOutput::TraceResult { hops, target } => {
            assert_eq!(target, "example.com");
            assert_eq!(hops.len(), 3);
            assert_eq!(hops[0].hop_number, 1);
            assert_eq!(hops[0].hostname.as_deref(), Some("gateway"));
            assert_eq!(hops[0].address.as_deref(), Some("192.168.1.1"));
            assert_eq!(hops[2].address, None); // timeout hop
        }
        _ => panic!("Expected TraceResult"),
    }
}

#[test]
fn test_parse_mtr_report() {
    let stdout = "\
Start: 2026-03-06T10:00:00
HOST: agnos                       Loss%   Snt   Last   Avg  Best  Wrst StDev
  1.|-- 192.168.1.1                0.0%    10    0.5   0.6   0.4   0.9   0.1
  2.|-- 10.0.0.1                   0.0%    10    2.1   2.0   1.8   2.4   0.2
  3.|-- ???                       100.0%    10    0.0   0.0   0.0   0.0   0.0";

    let result = parse_trace_output(stdout, "10.0.0.1");
    match result {
        ParsedOutput::TraceResult { hops, .. } => {
            assert_eq!(hops.len(), 3);
            assert_eq!(hops[0].hop_number, 1);
            assert_eq!(hops[0].address.as_deref(), Some("192.168.1.1"));
        }
        _ => panic!("Expected TraceResult"),
    }
}

#[test]
fn test_parse_ss_output() {
    let stdout = "\
State    Recv-Q  Send-Q  Local Address:Port  Peer Address:Port  Process
ESTAB    0       0       192.168.1.2:22      10.0.0.1:54321     users:((\"sshd\",pid=1234,fd=3))
LISTEN   0       128     0.0.0.0:80          0.0.0.0:*          users:((\"nginx\",pid=5678,fd=6))";

    let result = parse_socket_output(stdout);
    match result {
        ParsedOutput::SocketList { sockets } => {
            assert_eq!(sockets.len(), 2);
            assert_eq!(sockets[0].state, "ESTAB");
            assert_eq!(sockets[1].state, "LISTEN");
        }
        _ => panic!("Expected SocketList"),
    }
}

#[test]
fn test_parse_output_dispatches_correctly() {
    let output = ToolOutput {
        stdout: "".into(),
        stderr: String::new(),
        exit_code: 0,
        duration: Duration::from_secs(1),
        tool: NetworkTool::SocketStats,
        audit_entry: String::new(),
    };
    let result = parse_output(&output, None);
    assert!(matches!(result, ParsedOutput::SocketList { .. }));

    let output2 = ToolOutput {
        stdout: "".into(),
        stderr: String::new(),
        exit_code: 0,
        duration: Duration::from_secs(1),
        tool: NetworkTool::HttpClient,
        audit_entry: String::new(),
    };
    let result2 = parse_output(&output2, None);
    assert!(matches!(result2, ParsedOutput::Raw { .. }));
}

#[test]
fn test_parse_empty_scan() {
    let result = parse_scan_output("Nmap done: 0 IP addresses", NetworkTool::PortScan);
    match result {
        ParsedOutput::HostScan { hosts, .. } => {
            assert!(hosts.is_empty());
        }
        _ => panic!("Expected HostScan"),
    }
}

#[test]
fn test_parse_empty_dns() {
    let result = parse_dns_output(";; Got answer:\n;; AUTHORITY SECTION:", "test.invalid");
    match result {
        ParsedOutput::DnsResult { records, .. } => {
            assert!(records.is_empty());
        }
        _ => panic!("Expected DnsResult"),
    }
}

// --- Tool wrapper tests ---

#[test]
fn test_port_scanner_quick_args() {
    let scanner = PortScanner::new().profile(ScanProfile::Quick);
    let args = scanner.build_args();
    assert!(args.contains(&"-F".to_string()));
    assert!(args.contains(&"-T4".to_string()));
}

#[test]
fn test_port_scanner_thorough_args() {
    let scanner = PortScanner::new().profile(ScanProfile::Thorough);
    let args = scanner.build_args();
    assert!(args.contains(&"-p-".to_string()));
    assert!(args.contains(&"-sV".to_string()));
}

#[test]
fn test_port_scanner_stealth_args() {
    let scanner = PortScanner::new().profile(ScanProfile::Stealth);
    let args = scanner.build_args();
    assert!(args.contains(&"-T2".to_string()));
    assert!(args.contains(&"--randomize-hosts".to_string()));
}

#[test]
fn test_port_scanner_custom_ports() {
    let scanner = PortScanner::new()
        .profile(ScanProfile::Quick)
        .ports("80,443,8080");
    let args = scanner.build_args();
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"80,443,8080".to_string()));
    // -F should be removed when custom ports specified
    assert!(!args.contains(&"-F".to_string()));
}

#[test]
fn test_port_scanner_masscan_args() {
    let scanner = PortScanner::new().use_masscan(true).ports("1-1000");
    let args = scanner.build_args();
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"1-1000".to_string()));
    // Should not have nmap-specific flags
    assert!(!args.contains(&"-T3".to_string()));
}

#[test]
fn test_dns_investigator_build_args() {
    let inv = DnsInvestigator::new()
        .record_type("A")
        .record_type("MX")
        .nameserver("8.8.8.8");
    let args = inv.build_args("example.com");
    assert!(args.contains(&"example.com".to_string()));
    assert!(args.contains(&"A".to_string()));
    assert!(args.contains(&"MX".to_string()));
    assert!(args.contains(&"@8.8.8.8".to_string()));
}

#[test]
fn test_dns_investigator_default_any() {
    let inv = DnsInvestigator::new();
    let args = inv.build_args("test.com");
    assert!(args.contains(&"ANY".to_string()));
}

#[test]
fn test_dns_investigator_enumerate() {
    let inv = DnsInvestigator::new().enumerate(true);
    let args = inv.build_args("example.com");
    assert!(args.contains(&"example.com".to_string()));
    // Should not have dig-specific ANY
    assert!(!args.contains(&"ANY".to_string()));
}

#[test]
fn test_network_prober_trace_args() {
    let prober = NetworkProber::new().max_hops(20);
    let args = prober.build_args();
    assert!(args.contains(&"-m".to_string()));
    assert!(args.contains(&"20".to_string()));
}

#[test]
fn test_network_prober_mtr_args() {
    let prober = NetworkProber::new().use_mtr(true).max_hops(15);
    let args = prober.build_args();
    assert!(args.contains(&"-m".to_string()));
    assert!(args.contains(&"15".to_string()));
}

#[test]
fn test_vuln_assessor_nuclei_args() {
    let va = VulnAssessor::new()
        .severity("critical,high")
        .tag("cve")
        .tag("rce");
    let args = va.build_args();
    assert!(args.contains(&"-severity".to_string()));
    assert!(args.contains(&"critical,high".to_string()));
    assert!(args.contains(&"-tags".to_string()));
    assert!(args.contains(&"cve".to_string()));
    assert!(args.contains(&"rce".to_string()));
    assert!(args.contains(&"-u".to_string()));
}

#[test]
fn test_vuln_assessor_nikto_args() {
    let va = VulnAssessor::new().use_nikto(true);
    let args = va.build_args();
    assert!(args.contains(&"-h".to_string()));
}

#[test]
fn test_traffic_analyzer_tcpdump_args() {
    let ta = TrafficAnalyzer::new()
        .interface("eth0")
        .packet_count(50)
        .filter("port 80");
    let args = ta.build_args();
    assert!(args.contains(&"-i".to_string()));
    assert!(args.contains(&"eth0".to_string()));
    assert!(args.contains(&"-c".to_string()));
    assert!(args.contains(&"50".to_string()));
    assert!(args.contains(&"-nn".to_string()));
    assert!(args.contains(&"port 80".to_string()));
}

#[test]
fn test_traffic_analyzer_tshark() {
    let ta = TrafficAnalyzer::new().use_tshark().interface("wlan0");
    assert_eq!(ta.tool_choice, NetworkTool::DeepInspect);
    let args = ta.build_args();
    assert!(args.contains(&"-i".to_string()));
    assert!(args.contains(&"wlan0".to_string()));
}

#[test]
fn test_traffic_analyzer_ngrep() {
    let ta = TrafficAnalyzer::new()
        .use_ngrep()
        .interface("lo")
        .filter("GET");
    assert_eq!(ta.tool_choice, NetworkTool::NetworkGrep);
    let args = ta.build_args();
    assert!(args.contains(&"-d".to_string()));
    assert!(args.contains(&"lo".to_string()));
    assert!(args.contains(&"GET".to_string()));
}

#[test]
fn test_web_fuzzer_gobuster_args() {
    let wf = WebFuzzer::new()
        .wordlist("/usr/share/wordlists/common.txt")
        .extension("php")
        .extension("html")
        .threads(20)
        .status_codes("200,301");
    let args = wf.build_args("http://target.com");
    assert!(args.contains(&"dir".to_string()));
    assert!(args.contains(&"-u".to_string()));
    assert!(args.contains(&"http://target.com".to_string()));
    assert!(args.contains(&"-w".to_string()));
    assert!(args.contains(&"/usr/share/wordlists/common.txt".to_string()));
    assert!(args.contains(&"-x".to_string()));
    assert!(args.contains(&"php,html".to_string()));
    assert!(args.contains(&"-t".to_string()));
    assert!(args.contains(&"20".to_string()));
    assert!(args.contains(&"-s".to_string()));
    assert!(args.contains(&"200,301".to_string()));
}

#[test]
fn test_web_fuzzer_ffuf_args() {
    let wf = WebFuzzer::new()
        .use_ffuf()
        .wordlist("/tmp/words.txt")
        .extension("js")
        .status_codes("200");
    let args = wf.build_args("http://example.com");
    assert!(args.contains(&"-u".to_string()));
    assert!(args.contains(&"http://example.com/FUZZ".to_string()));
    assert!(args.contains(&"-w".to_string()));
    assert!(args.contains(&"/tmp/words.txt".to_string()));
    assert!(args.contains(&"-e".to_string()));
    assert!(args.contains(&".js".to_string()));
    assert!(args.contains(&"-mc".to_string()));
    assert!(args.contains(&"200".to_string()));
}

#[test]
fn test_socket_inspector_default_args() {
    let si = SocketInspector::new();
    let args = si.build_args();
    assert_eq!(args.len(), 1);
    let flags = &args[0];
    assert!(flags.contains('t'));
    assert!(flags.contains('u'));
    assert!(flags.contains('n'));
    assert!(flags.contains('a'));
    assert!(flags.contains('p'));
}

#[test]
fn test_socket_inspector_listening_tcp() {
    let si = SocketInspector::new().listening_only(true).tcp_only(true);
    let args = si.build_args();
    let flags = &args[0];
    assert!(flags.contains('l'));
    assert!(flags.contains('t'));
    assert!(!flags.contains('u'));
}

#[test]
fn test_socket_inspector_udp_only() {
    let si = SocketInspector::new().udp_only(true);
    let args = si.build_args();
    let flags = &args[0];
    assert!(flags.contains('u'));
    assert!(!flags.contains('t'));
}

#[test]
fn test_port_scanner_default() {
    let scanner = PortScanner::default();
    assert_eq!(scanner.profile, ScanProfile::Standard);
    assert!(!scanner.use_masscan);
    assert!(scanner.ports.is_none());
}

// --- New tool (batch 2) config tests ---

#[test]
fn test_netdiscover_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::NetDiscover);
    assert_eq!(cfg.binary_name, "netdiscover");
    assert_eq!(cfg.risk_level, RiskLevel::Medium);
    assert!(cfg.requires_approval);
    assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
    assert!(cfg.allowed_in_sandbox);
}

#[test]
fn test_termshark_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::TermShark);
    assert_eq!(cfg.binary_name, "termshark");
    assert_eq!(cfg.risk_level, RiskLevel::Critical);
    assert!(cfg.requires_approval);
    assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
    assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
    assert!(!cfg.allowed_in_sandbox);
}

#[test]
fn test_bettercap_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::BetterCap);
    assert_eq!(cfg.binary_name, "bettercap");
    assert_eq!(cfg.risk_level, RiskLevel::Critical);
    assert!(cfg.requires_approval);
    assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
    assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
    assert!(!cfg.allowed_in_sandbox);
}

#[test]
fn test_dnsx_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::DnsX);
    assert_eq!(cfg.binary_name, "dnsx");
    assert_eq!(cfg.risk_level, RiskLevel::Medium);
    assert!(!cfg.requires_approval);
    assert!(cfg.required_capabilities.is_empty());
    assert!(cfg.allowed_in_sandbox);
}

#[test]
fn test_fierce_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::Fierce);
    assert_eq!(cfg.binary_name, "fierce");
    assert_eq!(cfg.risk_level, RiskLevel::High);
    assert!(cfg.requires_approval);
    assert!(cfg.required_capabilities.is_empty());
    assert!(cfg.allowed_in_sandbox);
}

#[test]
fn test_wfuzz_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::WebAppFuzz);
    assert_eq!(cfg.binary_name, "wfuzz");
    assert_eq!(cfg.risk_level, RiskLevel::High);
    assert!(cfg.requires_approval);
    assert!(cfg.allowed_in_sandbox);
}

#[test]
fn test_sqlmap_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::SqlMap);
    assert_eq!(cfg.binary_name, "sqlmap");
    assert_eq!(cfg.risk_level, RiskLevel::Critical);
    assert!(cfg.requires_approval);
    assert!(cfg.allowed_in_sandbox);
}

#[test]
fn test_aircrack_ng_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::AircrackNg);
    assert_eq!(cfg.binary_name, "aircrack-ng");
    assert_eq!(cfg.risk_level, RiskLevel::Critical);
    assert!(cfg.requires_approval);
    assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
    assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
    assert!(!cfg.allowed_in_sandbox);
}

#[test]
fn test_kismet_config() {
    let cfg = NetworkToolConfig::for_tool(NetworkTool::Kismet);
    assert_eq!(cfg.binary_name, "kismet");
    assert_eq!(cfg.risk_level, RiskLevel::Critical);
    assert!(cfg.requires_approval);
    assert!(cfg.required_capabilities.contains(&"NET_RAW".to_string()));
    assert!(cfg.required_capabilities.contains(&"NET_ADMIN".to_string()));
    assert!(!cfg.allowed_in_sandbox);
}

// --- Dangerous arg rejection for sqlmap and bettercap ---

#[test]
fn test_reject_sqlmap_os_shell() {
    let args = vec![
        "--os-shell".to_string(),
        "-u".to_string(),
        "http://target.com".to_string(),
    ];
    assert!(validate_args(NetworkTool::SqlMap, &args, false).is_err());
}

#[test]
fn test_reject_sqlmap_os_cmd() {
    let args = vec!["--os-cmd=whoami".to_string()];
    assert!(validate_args(NetworkTool::SqlMap, &args, false).is_err());
}

#[test]
fn test_reject_sqlmap_file_write() {
    let args = vec!["--file-write=/tmp/shell.php".to_string()];
    assert!(validate_args(NetworkTool::SqlMap, &args, false).is_err());
}

#[test]
fn test_allow_sqlmap_dangerous_with_approval() {
    let args = vec!["--os-shell".to_string()];
    assert!(validate_args(NetworkTool::SqlMap, &args, true).is_ok());
}

#[test]
fn test_allow_sqlmap_normal_args() {
    let args = vec![
        "-u".to_string(),
        "http://target.com/page?id=1".to_string(),
        "--batch".to_string(),
    ];
    assert!(validate_args(NetworkTool::SqlMap, &args, false).is_ok());
}

#[test]
fn test_reject_bettercap_caplet() {
    let args = vec!["--caplet".to_string(), "http-req-dump".to_string()];
    assert!(validate_args(NetworkTool::BetterCap, &args, false).is_err());
}

#[test]
fn test_allow_bettercap_caplet_with_approval() {
    let args = vec!["--caplet".to_string(), "http-req-dump".to_string()];
    assert!(validate_args(NetworkTool::BetterCap, &args, true).is_ok());
}

#[test]
fn test_allow_bettercap_normal_args() {
    let args = vec!["-iface".to_string(), "eth0".to_string()];
    assert!(validate_args(NetworkTool::BetterCap, &args, false).is_ok());
}

#[test]
fn test_all_tools_count_is_32() {
    assert_eq!(ALL_TOOLS.len(), 32);
}

#[test]
fn test_new_tools_display() {
    assert_eq!(NetworkTool::NetDiscover.to_string(), "NetDiscover");
    assert_eq!(NetworkTool::TermShark.to_string(), "TermShark");
    assert_eq!(NetworkTool::BetterCap.to_string(), "BetterCap");
    assert_eq!(NetworkTool::DnsX.to_string(), "DnsX");
    assert_eq!(NetworkTool::Fierce.to_string(), "Fierce");
    assert_eq!(NetworkTool::WebAppFuzz.to_string(), "WebAppFuzz");
    assert_eq!(NetworkTool::SqlMap.to_string(), "SqlMap");
    assert_eq!(NetworkTool::AircrackNg.to_string(), "AircrackNg");
    assert_eq!(NetworkTool::Kismet.to_string(), "Kismet");
}

// --- NetworkToolConfig coverage for all tool variants ---

#[test]
fn test_config_for_all_tools_has_binary() {
    for tool in ALL_TOOLS.iter() {
        let config = NetworkToolConfig::for_tool(*tool);
        assert!(
            !config.binary_name.is_empty(),
            "Tool {:?} has empty binary",
            tool
        );
        assert_eq!(config.tool, *tool);
    }
}

#[test]
fn test_critical_tools_require_approval() {
    let critical_tools = [NetworkTool::PacketCapture, NetworkTool::WebScan];
    for tool in &critical_tools {
        let config = NetworkToolConfig::for_tool(*tool);
        assert_eq!(config.risk_level, RiskLevel::Critical);
        assert!(
            config.requires_approval,
            "{:?} should require approval",
            tool
        );
    }
}

#[test]
fn test_low_risk_tools_no_approval() {
    let low_tools = [NetworkTool::DnsLookup, NetworkTool::HttpClient];
    for tool in &low_tools {
        let config = NetworkToolConfig::for_tool(*tool);
        assert_eq!(config.risk_level, RiskLevel::Low);
        assert!(
            !config.requires_approval,
            "{:?} should not require approval",
            tool
        );
    }
}

// --- Struct construction ---

#[test]
fn test_tool_output_construction() {
    let output = ToolOutput {
        stdout: "PORT   STATE SERVICE\n80/tcp open  http".to_string(),
        stderr: String::new(),
        exit_code: 0,
        duration: Duration::from_millis(1500),
        tool: NetworkTool::PortScan,
        audit_entry: "scan 192.168.1.1".to_string(),
    };
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.tool, NetworkTool::PortScan);
    assert!(output.stdout.contains("80/tcp"));
}

#[test]
fn test_validated_target_display_all_variants() {
    let ip = ValidatedTarget::Ip("10.0.0.1".parse().unwrap());
    assert_eq!(ip.to_string(), "10.0.0.1");

    let cidr = ValidatedTarget::Cidr {
        addr: "192.168.1.0".parse().unwrap(),
        prefix: 24,
    };
    assert_eq!(cidr.to_string(), "192.168.1.0/24");

    let host = ValidatedTarget::Hostname("example.com".to_string());
    assert_eq!(host.to_string(), "example.com");
}

#[test]
fn test_discovered_port_construction() {
    let port = DiscoveredPort {
        port: 443,
        protocol: "tcp".to_string(),
        state: "open".to_string(),
        service: Some("https".to_string()),
        version: Some("OpenSSL 1.1.1".to_string()),
    };
    assert_eq!(port.port, 443);
    assert_eq!(port.service.as_deref(), Some("https"));
}

#[test]
fn test_discovered_host_with_ports() {
    let host = DiscoveredHost {
        address: "10.0.0.1".to_string(),
        hostname: Some("server.local".to_string()),
        mac_address: Some("AA:BB:CC:DD:EE:FF".to_string()),
        vendor: Some("ACME Corp".to_string()),
        state: "up".to_string(),
        ports: vec![DiscoveredPort {
            port: 22,
            protocol: "tcp".to_string(),
            state: "open".to_string(),
            service: Some("ssh".to_string()),
            version: None,
        }],
    };
    assert_eq!(host.ports.len(), 1);
    assert_eq!(host.hostname.as_deref(), Some("server.local"));
}

#[test]
fn test_dns_record_construction() {
    let rec = DnsRecord {
        name: "example.com".to_string(),
        record_type: "A".to_string(),
        value: "93.184.216.34".to_string(),
        ttl: Some(300),
    };
    assert_eq!(rec.ttl, Some(300));
}

#[test]
fn test_trace_hop_construction() {
    let hop = TraceHop {
        hop_number: 3,
        address: Some("10.0.0.1".to_string()),
        hostname: Some("gateway.local".to_string()),
        rtt_ms: Some(1.5),
        loss_pct: Some(0.0),
    };
    assert_eq!(hop.hop_number, 3);
    assert_eq!(hop.rtt_ms, Some(1.5));
}

#[test]
fn test_socket_entry_construction() {
    let entry = SocketEntry {
        state: "ESTAB".to_string(),
        protocol: "tcp".to_string(),
        local_addr: "127.0.0.1:8080".to_string(),
        remote_addr: "10.0.0.1:54321".to_string(),
        process: Some("sshd".to_string()),
    };
    assert_eq!(entry.state, "ESTAB");
}

// --- Parse functions ---

#[test]
fn test_parse_nmap_port_line_valid() {
    let port = parse_nmap_port_line("80/tcp   open  http    Apache httpd 2.4.41");
    assert!(port.is_some());
    let p = port.unwrap();
    assert_eq!(p.port, 80);
    assert_eq!(p.protocol, "tcp");
    assert_eq!(p.state, "open");
    assert_eq!(p.service.as_deref(), Some("http"));
    assert_eq!(p.version.as_deref(), Some("Apache httpd 2.4.41"));
}

#[test]
fn test_parse_nmap_port_line_closed() {
    let port = parse_nmap_port_line("22/tcp   closed  ssh");
    assert!(port.is_some());
    assert_eq!(port.unwrap().state, "closed");
}

#[test]
fn test_parse_nmap_port_line_filtered() {
    let port = parse_nmap_port_line("443/tcp   filtered  https");
    assert!(port.is_some());
    assert_eq!(port.unwrap().state, "filtered");
}

#[test]
fn test_parse_nmap_port_line_invalid() {
    assert!(parse_nmap_port_line("not a port line").is_none());
    assert!(parse_nmap_port_line("").is_none());
    assert!(parse_nmap_port_line("80/tcp running http").is_none()); // invalid state
}

#[test]
fn test_parse_output_raw_fallback() {
    let output = ToolOutput {
        stdout: "line1\nline2\nline3\n".to_string(),
        stderr: String::new(),
        exit_code: 0,
        duration: Duration::from_secs(1),
        tool: NetworkTool::HttpClient,
        audit_entry: "curl example.com".to_string(),
    };
    let parsed = parse_output(&output, None);
    match parsed {
        ParsedOutput::Raw { summary } => {
            assert!(summary.contains("HttpClient"));
            assert!(summary.contains("3 lines"));
        }
        _ => panic!("Expected Raw variant"),
    }
}

#[test]
fn test_parse_output_raw_error_exit() {
    let output = ToolOutput {
        stdout: "error output\n".to_string(),
        stderr: "failed".to_string(),
        exit_code: 1,
        duration: Duration::from_secs(1),
        tool: NetworkTool::DirBust,
        audit_entry: "gobuster".to_string(),
    };
    let parsed = parse_output(&output, None);
    match parsed {
        ParsedOutput::Raw { summary } => {
            assert!(summary.contains("exited with code 1"));
        }
        _ => panic!("Expected Raw variant"),
    }
}

// --- Validation helpers ---

#[test]
fn test_reject_broadcast() {
    let bcast: IpAddr = "255.255.255.255".parse().unwrap();
    assert!(reject_broadcast(&bcast).is_err());

    let normal: IpAddr = "192.168.1.1".parse().unwrap();
    assert!(reject_broadcast(&normal).is_ok());

    let v6: IpAddr = "::1".parse().unwrap();
    assert!(reject_broadcast(&v6).is_ok());
}

#[test]
fn test_validate_hostname_valid() {
    assert!(validate_hostname("example.com").is_ok());
    assert!(validate_hostname("sub.domain.example.com").is_ok());
    assert!(validate_hostname("my-host").is_ok());
}

#[test]
fn test_validate_hostname_invalid() {
    assert!(validate_hostname("").is_err());
    assert!(validate_hostname("-leading").is_err());
    assert!(validate_hostname("trailing-").is_err());
    assert!(validate_hostname("has space.com").is_err());
    let long_host = "a".repeat(254);
    assert!(validate_hostname(&long_host).is_err());
}

// --- Builder patterns ---

#[test]
fn test_port_scanner_build_args_quick() {
    let scanner = PortScanner::new().profile(ScanProfile::Quick);
    let args = scanner.build_args();
    assert!(args.contains(&"-F".to_string()));
    assert!(args.contains(&"-T4".to_string()));
}

#[test]
fn test_port_scanner_build_args_thorough() {
    let scanner = PortScanner::new().profile(ScanProfile::Thorough);
    let args = scanner.build_args();
    assert!(args.contains(&"-p-".to_string()));
    assert!(args.contains(&"-sV".to_string()));
}

#[test]
fn test_port_scanner_build_args_stealth() {
    let scanner = PortScanner::new().profile(ScanProfile::Stealth);
    let args = scanner.build_args();
    assert!(args.contains(&"-T2".to_string()));
    assert!(args.contains(&"--randomize-hosts".to_string()));
}

#[test]
fn test_port_scanner_build_args_masscan() {
    let scanner = PortScanner::new().use_masscan(true).ports("80,443");
    let args = scanner.build_args();
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"80,443".to_string()));
}

#[test]
fn test_port_scanner_custom_ports_override() {
    let scanner = PortScanner::new()
        .profile(ScanProfile::Thorough)
        .ports("22,80,443");
    let args = scanner.build_args();
    // -p- and -F should be removed, replaced by custom ports
    assert!(!args.contains(&"-p-".to_string()));
    assert!(args.contains(&"22,80,443".to_string()));
}

#[test]
fn test_port_scanner_default_fields() {
    let scanner = PortScanner::default();
    assert_eq!(scanner.profile, ScanProfile::Standard);
    assert!(!scanner.use_masscan);
    assert!(scanner.ports.is_none());
}

#[test]
fn test_dns_investigator_builder() {
    let dns = DnsInvestigator::new()
        .record_type("MX")
        .record_type("TXT")
        .nameserver("8.8.8.8")
        .enumerate(true);
    assert!(dns.use_dnsrecon);
    assert_eq!(dns.record_types, vec!["MX", "TXT"]);
    assert_eq!(dns.nameserver.as_deref(), Some("8.8.8.8"));
}

#[test]
fn test_network_tool_runner_default_impl() {
    let _runner = NetworkToolRunner::default();
    let tools = NetworkToolRunner::list_all_tools();
    assert_eq!(tools.len(), 32);
}

#[test]
fn test_network_tool_runner_list_all_configs() {
    let tools = NetworkToolRunner::list_all_tools();
    // Every tool should have a non-empty binary and valid risk level
    for config in &tools {
        assert!(!config.binary_name.is_empty());
        assert!(config.max_timeout_secs > 0);
    }
}
