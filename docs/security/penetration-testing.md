# AGNOS Penetration Testing Framework

**Last Updated**: 2026-03-07

This document outlines the penetration testing methodology and procedures for AGNOS security audits.

## Scope

### In Scope
- **Daimon** — Agent Runtime Daemon (port 8090)
- **Hoosh** — LLM Gateway Service (port 8088)
- **Agnoshi** — AI Shell (`agnsh`)
- **Aethersafha** — Desktop Environment (Wayland compositor)
- **Aegis** — Security Daemon
- **Sigil** — Trust Verification System
- **Mela** — Agent/App Marketplace
- **MCP Server** — Model Context Protocol endpoints
- **Ark/Nous** — Package management and resolution
- Kernel Modules (agnos-security, agent-subsystem, llm)
- Inter-process Communication (Unix domain sockets at `/run/agnos/agents/`)
- Authentication & Authorization (Bearer tokens, mTLS)
- Sandbox Mechanisms (Landlock, seccomp-bpf, namespaces)
- Multi-node Federation (if enabled)
- Post-Quantum Cryptographic primitives

### Out of Scope
- Physical security
- Social engineering
- Client-side vulnerabilities (browser, SDKs)
- Third-party cloud services (unless AGNOS-managed)

## Testing Methodology

### Phase 1: Reconnaissance

#### Network Discovery
```bash
# Port scanning
nmap -sS -sV -O target

# Service enumeration
nmap -sC target

# SSL/TLS analysis
testssl target
```

#### Information Gathering
- OSINT on AGNOS components
- Source code review (if whitebox)
- Documentation review
- Architecture analysis

### Phase 2: Vulnerability Assessment

#### Authentication & Authorization
- [ ] Default credentials check
- [ ] Authentication bypass
- [ ] Privilege escalation
- [ ] Session management
- [ ] Token handling
- [ ] Password policy enforcement

#### Agent Runtime
- [ ] Agent isolation bypass
- [ ] Resource limit circumvention
- [ ] Sandbox escape (Landlock, seccomp)
- [ ] IPC manipulation
- [ ] Agent spoofing

#### LLM Gateway
- [ ] Prompt injection
- [ ] Model manipulation
- [ ] Cache poisoning
- [ ] Rate limiting bypass
- [ ] API key exposure

#### Desktop Environment
- [ ] Wayland compositor vulnerabilities
- [ ] Window management attacks
- [ ] AI feature abuse
- [ ] Permission escalation

#### Marketplace (Mela)
- [ ] Malicious package upload
- [ ] Package signature bypass (sigil verification)
- [ ] Supply chain attacks via dependency confusion
- [ ] Sandbox escape from marketplace agents
- [ ] Trust chain manipulation

#### MCP Server
- [ ] Unauthorized tool invocation
- [ ] Input injection via MCP tool parameters
- [ ] MCP session hijacking
- [ ] Tool capability escalation

#### Federation
- [ ] Cross-node authentication bypass
- [ ] Federation message tampering
- [ ] Node impersonation
- [ ] Distributed DoS via federation

#### Post-Quantum Cryptography
- [ ] Hybrid handshake downgrade attacks
- [ ] PQC key exchange manipulation
- [ ] Signature algorithm confusion
- [ ] Side-channel attacks on PQC primitives

#### Kernel Modules
- [ ] Privilege escalation via kernel modules
- [ ] Memory corruption
- [ ] Race conditions
- [ ] Denial of service

### Phase 3: Exploitation

#### Privilege Escalation
```bash
# Check current privileges
id
whoami
# Check sudo permissions
sudo -l
# Check capabilities
getcap -r /
# Check SUID binaries
find / -perm -4000 2>/dev/null
```

#### Container/Sandbox Escape
```bash
# Check namespace isolation
ls -la /proc/self/ns
# Check cgroup
cat /proc/self/cgroup
# Check seccomp
cat /proc/self/status | grep Seccomp
```

### Phase 4: Post-Exploitation

#### Lateral Movement
- IPC channel abuse
- Agent-to-agent communication exploitation
- Service account pivot

#### Persistence
- Malicious agent registration
- Backdoor creation
- Modified security policies

#### Data Exfiltration
- Audit log manipulation
- Model data theft
- Agent memory extraction

## Testing Techniques

### Black Box Testing
- No prior knowledge of system
- Focus on external interfaces
- Network-based attacks only

### White Box Testing
- Full source code access
- Architecture documentation provided
- Can perform deeper analysis

### Gray Box Testing
- Limited documentation
- Standard user access
- Focus on common vulnerabilities

## Test Cases

### Authentication Bypass
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| AUTH-01 | Default credentials | Critical | |
| AUTH-02 | Session hijacking | High | |
| AUTH-03 | OAuth/Token theft | High | |

### Agent Runtime
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| AGENT-01 | Sandbox escape | Critical | |
| AGENT-02 | Resource limit bypass | Medium | |
| AGENT-03 | IPC injection | High | |

### LLM Gateway
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| LLM-01 | Prompt injection | High | |
| LLM-02 | Model extraction | Medium | |
| LLM-03 | Cache poisoning | Medium | |

### Marketplace (Mela)
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| MELA-01 | Package signature bypass | Critical | |
| MELA-02 | Malicious agent installation | High | |
| MELA-03 | Supply chain dependency confusion | High | |
| MELA-04 | Sandbox escape from marketplace agent | Critical | |

### MCP Server
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| MCP-01 | Unauthorized tool invocation | High | |
| MCP-02 | Input injection via tool params | High | |
| MCP-03 | Session hijacking | Medium | |

### Federation
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| FED-01 | Cross-node auth bypass | Critical | |
| FED-02 | Node impersonation | High | |
| FED-03 | Federation message tampering | High | |

### Desktop (Aethersafha)
| ID | Description | Severity | Status |
|----|-------------|----------|--------|
| DESK-01 | Wayland compositor exploit | High | |
| DESK-02 | AI feature abuse | Medium | |

## Tools

### Network
- nmap
- netcat
- wireshark
- burp suite

### Web
- OWASP ZAP
- SQLMap
- XSSer

### Exploitation
- Metasploit
- SQLMap
- custom exploits

### Analysis
- gdb
- radare2
- IDA Pro
- Ghidra

## Reporting

### Vulnerability Format
```json
{
  "id": "AGNOS-2026-001",
  "title": "Agent Sandbox Escape via Landlock Misconfiguration",
  "severity": "Critical",
  "cvss": 9.8,
  "description": "...",
  "steps_to_reproduce": "...",
  "impact": "...",
  "remediation": "..."
}
```

### Severity Ratings
- **Critical**: Immediate risk of system compromise
- **High**: Significant impact, requires urgent fix
- **Medium**: Moderate impact, should be addressed
- **Low**: Minor issue, address when possible
- **Info**: Informational, no action required

## Timeline

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Planning | 1 day | Test plan |
| Reconnaissance | 2 days | Findings report |
| Vulnerability Assessment | 5 days | Vulnerabilities list |
| Exploitation | 3 days | Exploit proof-of-concepts |
| Reporting | 2 days | Final report |

## References
- OWASP Testing Guide
- CIS Controls
- NIST Cybersecurity Framework
- CVE Database
