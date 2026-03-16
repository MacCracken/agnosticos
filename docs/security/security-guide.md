# Security Guide

**Last Updated**: 2026-03-16

AGNOS is designed with security as a foundational principle. This guide documents the security architecture, threat model, and best practices.

## Security Architecture

### Defense in Depth

AGNOS implements multiple layers of security:

```
┌─────────────────────────────────────────┐
│  Layer 6: Application Security          │
│  - Agent sandboxing, Permission system  │
├─────────────────────────────────────────┤
│  Layer 5: Desktop Security              │
│  - Screen lock, Secure notifications    │
├─────────────────────────────────────────┤
│  Layer 4: Service Security              │
│  - Agent Runtime, LLM Gateway          │
├─────────────────────────────────────────┤
│  Layer 3: System Security               │
│  - Namespaces, Seccomp, Landlock       │
├─────────────────────────────────────────┤
│  Layer 2: Kernel Security                 │
│  - Hardened kernel, LSM modules         │
├─────────────────────────────────────────┤
│  Layer 1: Hardware Security               │
│  - Secure boot, TPM, Disk encryption   │
└─────────────────────────────────────────┘
```

## Threat Model

### Assets

1. **User Data** - Files, credentials, personal information
2. **System Integrity** - OS components, kernel, configuration
3. **Agent Operations** - AI agent tasks and decisions
4. **Communication** - Network traffic, IPC messages
5. **Resources** - CPU, GPU, memory, storage

### Threat Actors

1. **Malicious Agents** - Compromised or malicious AI agents
2. **External Attackers** - Network-based attacks
3. **Insider Threats** - Malicious users or processes
4. **Supply Chain** - Compromised dependencies

### Attack Vectors

| Vector | Risk | Mitigation |
|--------|------|------------|
| Agent escape | High | Sandboxing, seccomp, namespaces |
| Privilege escalation | High | Capabilities, no root for agents |
| Data exfiltration | Medium | Network policies, audit logging |
| Prompt injection | Medium | Input validation, context limits |
| Denial of service | Medium | Resource quotas, rate limiting |

## Security Features

### 1. Agent Sandboxing

Agents run in isolated environments:

- **Landlock**: Filesystem sandboxing
- **Seccomp**: System call filtering
- **Namespaces**: PID, network, mount isolation
- **Cgroups**: Resource limits

```rust
use agent_runtime::sandbox::{Sandbox, FilesystemRule, Access};
use agnos_sys::landlock;
use agnos_sys::seccomp;

let sandbox = Sandbox::new();
sandbox.apply_landlock(&[
    FilesystemRule {
        path: "/home/user/project".to_string(),
        access: Access::ReadWrite,
    },
    FilesystemRule {
        path: "/tmp".to_string(),
        access: Access::ReadWrite,
    },
])?;

sandbox.load_seccomp(basic_filter)?;
```

### 2. Permission System

Fine-grained capability-based permissions:

| Permission | Description | Risk Level |
|------------|-------------|------------|
| file:read | Read files | Low |
| file:write | Modify files | Medium |
| file:delete | Delete files | High |
| network:outbound | External connections | Medium |
| process:spawn | Start processes | High |
| agent:delegate | Create sub-agents | Critical |

### 3. Human Override

Critical operations require human approval:

```rust
// Agent requests permission
security.request_permission(PermissionRequest {
    agent_id,
    permission: "file:delete".to_string(),
    resource: "/home/user/important.txt".to_string(),
    reason: "Cleaning up old files".to_string(),
})?;

// User sees notification and approves/denies
```

### 4. Audit Logging

All security-relevant events are logged:

```rust
// Logged automatically
{
    "timestamp": "2026-02-12T10:30:00Z",
    "event_type": "permission_request",
    "agent_id": "...",
    "agent_name": "file-manager",
    "permission": "file:delete",
    "resource": "/home/user/file.txt",
    "result": "granted",
    "user": "alice"
}
```

### 5. Emergency Kill Switch

Immediate termination of all agents:

```rust
// Trigger emergency mode
security.emergency_kill_switch();

// All agents terminated
// All permissions revoked
// System locked down
```

### 6. Aegis Security Daemon

Aegis (`agent_runtime::aegis`) is the centralized security policy daemon that enforces system-wide security rules. It provides:

- Real-time agent behavior monitoring and anomaly detection
- Automated sandbox enforcement and threat response
- Security policy distribution across the system
- Coordination with kernel security modules (Landlock, seccomp, IMA)

### 7. Phylax Threat Detection

Phylax (`agent_runtime::phylax`) is the native threat detection engine — pure Rust, no external AV dependency:

- **YARA-compatible rule engine** — Hex-pattern matching for known malware signatures, crypto miners, reverse shells, base64 droppers
- **Shannon entropy analysis** — Detects ransomware patterns and encrypted payloads (configurable threshold)
- **File content inspection** — Magic byte detection (ELF, PE, suspicious shebangs), polyglot file detection, embedded payload analysis
- **Aegis integration** — Findings above configurable severity are forwarded to aegis for quarantine decisions
- **Scan modes** — On-demand, real-time (fanotify), scheduled, pre-install, pre-exec
- **Built-in rules** — 5 default rules (EICAR test, reverse shell, crypto miner, base64 dropper, credential access); extensible via `.phylax-db` signature database

### 8. Sigil Trust System

Sigil (`agent_runtime::sigil`) provides cryptographic trust verification:

- Package and agent signature verification (Ed25519 + ML-DSA hybrid)
- Trust chain management for `.ark` and `.agnos-agent` packages
- TPM-backed hardware attestation for system integrity
- Transparency log integration for auditable trust decisions

### 9. Post-Quantum Cryptography

AGNOS implements hybrid post-quantum cryptographic schemes:

- **Key exchange**: ML-KEM (Kyber) + X25519 hybrid for forward secrecy
- **Digital signatures**: ML-DSA (Dilithium) + Ed25519 hybrid
- **Hash-based signatures**: SPHINCS+ for long-lived signing keys
- **Crypto-agility**: Algorithm-agnostic APIs in `agnos_common::secrets` allow seamless migration

### 10. Mutual TLS (mTLS)

All inter-service communication uses mutual TLS (`agent_runtime::mtls`):

- Certificate pinning via `agnos_sys::certpin` (SPKI SHA-256 hashes)
- Per-agent client certificates issued by the local CA
- Certificate rotation with zero-downtime rollover
- CORS restricted to localhost; Bearer token auth for API endpoints

### 11. Zero-Trust Architecture (ADR-003)

AGNOS follows zero-trust principles:

- **Never trust, always verify**: Every agent request is authenticated and authorized
- **Least privilege**: Capabilities granted per-agent, per-resource
- **Micro-segmentation**: Network namespaces isolate agent traffic
- **Continuous verification**: Aegis monitors agent behavior throughout lifecycle
- **Assume breach**: Cryptographic audit chain ensures forensic capability

## Security Best Practices

### For Users

1. **Keep system updated** - Apply security patches promptly
2. **Review permissions** - Regularly audit agent permissions
3. **Monitor audit logs** - Check for suspicious activity
4. **Use strong passwords** - Protect user account
5. **Enable full disk encryption** - Protect data at rest

### For Developers

1. **Principle of least privilege** - Request minimal permissions
2. **Validate all inputs** - Sanitize agent inputs
3. **Secure defaults** - Deny by default, allow explicitly
4. **Defense in depth** - Multiple security layers
5. **Fail secure** - Fail to safe state, not open

### For Administrators

1. **Network segmentation** - Isolate agent network traffic
2. **Regular backups** - Maintain offline backups
3. **Incident response plan** - Document procedures
4. **Security monitoring** - Deploy SIEM tools
5. **Penetration testing** - Regular security audits

## Vulnerability Reporting & Compliance

For vulnerability disclosure policy, bug bounty, response timelines, compliance status, and contact information, see [SECURITY.md](../../SECURITY.md).

For the security checklist, see [security-checklist.md](security-checklist.md).

## References

- [Linux Security Modules](https://www.kernel.org/doc/html/latest/admin-guide/LSM/index.html)
- [Landlock Documentation](https://docs.kernel.org/userspace-api/landlock.html)
- [Seccomp BPF](https://www.kernel.org/doc/Documentation/prctl/seccomp_filter.txt)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
