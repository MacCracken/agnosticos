use anyhow::{anyhow, Result};

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_network(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::NetworkScan { action, target } => {
            let (command, args, desc, permission) = match action.as_str() {
                "port_scan" => {
                    let t = target.as_deref().unwrap_or("localhost");
                    (
                        "nmap".to_string(),
                        vec!["-sT".to_string(), t.to_string()],
                        format!("Port scan on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "ping_sweep" => {
                    let t = target.as_deref().unwrap_or("192.168.1.0/24");
                    (
                        "nmap".to_string(),
                        vec!["-sn".to_string(), t.to_string()],
                        format!("Ping sweep on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "dns_lookup" => {
                    let t = target.as_deref().unwrap_or("localhost");
                    (
                        "dig".to_string(),
                        vec![t.to_string()],
                        format!("DNS lookup for {}", t),
                        PermissionLevel::Safe,
                    )
                }
                "trace_route" => {
                    let t = target.as_deref().unwrap_or("localhost");
                    (
                        "traceroute".to_string(),
                        vec![t.to_string()],
                        format!("Trace route to {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "packet_capture" => {
                    let iface = target.as_deref().unwrap_or("eth0");
                    (
                        "tcpdump".to_string(),
                        vec![
                            "-i".to_string(),
                            iface.to_string(),
                            "-c".to_string(),
                            "100".to_string(),
                        ],
                        format!("Capture packets on {}", iface),
                        PermissionLevel::Admin,
                    )
                }
                "web_scan" => {
                    let t = target.as_deref().unwrap_or("http://localhost");
                    (
                        "nikto".to_string(),
                        vec!["-h".to_string(), t.to_string()],
                        format!("Web scan on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "mass_scan" => {
                    let t = target.as_deref().unwrap_or("192.168.1.0/24");
                    (
                        "masscan".to_string(),
                        vec![
                            "--rate=1000".to_string(),
                            "-p1-65535".to_string(),
                            t.to_string(),
                        ],
                        format!("Mass scan on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "arp_scan" => {
                    let args = if let Some(t) = target.as_deref() {
                        vec![t.to_string()]
                    } else {
                        vec!["--localnet".to_string()]
                    };
                    (
                        "arp-scan".to_string(),
                        args,
                        "ARP scan local network".to_string(),
                        PermissionLevel::Admin,
                    )
                }
                "network_diag" => {
                    let t = target.as_deref().unwrap_or("localhost");
                    (
                        "mtr".to_string(),
                        vec![
                            "--report".to_string(),
                            "-c".to_string(),
                            "10".to_string(),
                            t.to_string(),
                        ],
                        format!("Network diagnostics to {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "service_scan" => {
                    let t = target.as_deref().unwrap_or("localhost");
                    (
                        "nmap".to_string(),
                        vec!["-sV".to_string(), t.to_string()],
                        format!("Service detection on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "dir_fuzz" => {
                    let t = target.as_deref().unwrap_or("http://localhost");
                    (
                        "ffuf".to_string(),
                        vec![
                            "-u".to_string(),
                            format!("{}/FUZZ", t),
                            "-w".to_string(),
                            "/usr/share/wordlists/common.txt".to_string(),
                        ],
                        format!("Directory fuzzing on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "vuln_scan" => {
                    let t = target.as_deref().unwrap_or("http://localhost");
                    (
                        "nuclei".to_string(),
                        vec!["-u".to_string(), t.to_string(), "-silent".to_string()],
                        format!("Vulnerability scan on {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "socket_stats" => (
                    "ss".to_string(),
                    vec!["-tunap".to_string()],
                    "Show network sockets and connections".to_string(),
                    PermissionLevel::Safe,
                ),
                "dns_enum" => {
                    let t = target.as_deref().unwrap_or("localhost");
                    (
                        "dnsrecon".to_string(),
                        vec!["-d".to_string(), t.to_string()],
                        format!("DNS enumeration for {}", t),
                        PermissionLevel::Admin,
                    )
                }
                "deep_inspect" => {
                    let iface = target.as_deref().unwrap_or("eth0");
                    (
                        "tshark".to_string(),
                        vec![
                            "-i".to_string(),
                            iface.to_string(),
                            "-c".to_string(),
                            "100".to_string(),
                        ],
                        format!("Deep packet inspection on {}", iface),
                        PermissionLevel::Admin,
                    )
                }
                "bandwidth_monitor" => (
                    "nethogs".to_string(),
                    vec![],
                    "Monitor per-process bandwidth usage".to_string(),
                    PermissionLevel::Admin,
                ),
                other => {
                    return Err(anyhow!("Unknown network scan action: {}", other));
                }
            };
            Ok(Translation {
                command,
                args,
                description: desc.clone(),
                permission,
                explanation: desc,
            })
        }

        _ => unreachable!("translate_network called with non-network intent"),
    }
}
