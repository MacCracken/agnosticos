# Security Testing Checklist

**Last Updated**: 2026-03-07

## Pre-Engagement
- [ ] Scope defined and documented
- [ ] Rules of engagement agreed
- [ ] Legal authorization obtained
- [ ] Contact information exchanged
- [ ] Emergency contacts established
- [ ] Timeline agreed
- [ ] NDA signed

## Information Gathering
- [ ] Network mapping complete
- [ ] Service enumeration done
- [ ] Version information collected
- [ ] Documentation reviewed
- [ ] Source code available (if whitebox)
- [ ] Architecture diagram obtained

## Authentication & Authorization
### Password Security
- [ ] Default credentials checked
- [ ] Password policy enforcement verified
- [ ] Password storage mechanism reviewed
- [ ] Password reset functionality tested
- [ ] Brute-force protection verified

### Session Management
- [ ] Session token randomness verified
- [ ] Session timeout verified
- [ ] Session fixation tested
- [ ] Session hijacking scenarios tested
- [ ] Concurrent session handling verified

### Access Control
- [ ] Privilege escalation tested
- [ ] Horizontal privilege escalation tested
- [ ] Vertical privilege escalation tested
- [ ] IDOR vulnerabilities checked
- [ ] Broken access control verified

## Agent Runtime Security
### Agent Isolation
- [ ] Agent process isolation verified
- [ ] Namespace isolation tested
- [ ] Cgroup limits enforced
- [ ] Resource limits enforced

### Sandbox
- [ ] Landlock enforcement verified
- [ ] Seccomp filters tested
- [ ] Syscall allowlist enforced
- [ ] Filesystem restrictions verified
- [ ] Network namespace isolation tested

### IPC Security
- [ ] Message authentication verified
- [ ] IPC channel encryption tested
- [ ] Message validation enforced
- [ ] Rate limiting implemented
- [ ] Replay attack protection verified

## LLM Gateway Security
### Input Validation
- [ ] Prompt injection prevention verified
- [ ] Input length limits enforced
- [ ] Special character sanitization tested
- [ ] Unicode handling verified

### Model Security
- [ ] Model access control verified
- [ ] Model extraction attempts blocked
- [ ] Training data protection verified
- [ ] Model output filtering tested

### API Security
- [ ] API key protection verified
- [ ] Rate limiting enforced
- [ ] Request validation tested
- [ ] Response filtering verified

## Desktop Environment Security
### Compositor
- [ ] Wayland protocol security verified
- [ ] Input handling reviewed
- [ ] Window isolation tested
- [ ] Buffer overflows checked

### AI Features
- [ ] Context detection reviewed
- [ ] Suggestion generation tested
- [ ] Privacy implications checked
- [ ] Prompt leakage prevented

### Permission System
- [ ] Permission escalation tested
- [ ] Default permissions restricted
- [ ] Permission revocation verified

## Kernel Module Security
### Memory Safety
- [ ] Buffer overflow checks passed
- [ ] Use-after-free prevented
- [ ] Race conditions mitigated
- [ ] Integer overflows handled

### Privilege Handling
- [ ] Capability checks verified
- [ ] User/kernel boundary protected
- [ ] Privilege escalation prevented

## Network Security
### Protocol Security
- [ ] TLS configuration verified
- [ ] Certificate validation tested
- [ ] Protocol downgrade prevented
- [ ] Perfect forward secrecy enabled

### Service Hardening
- [ ] Unnecessary services disabled
- [ ] Port scanning mitigated
- [ ] Firewall rules enforced
- [ ] Network segmentation verified

## Data Protection
### Encryption
- [ ] Data at rest encryption verified
- [ ] Data in transit encryption tested
- [ ] Key management reviewed
- [ ] Key rotation implemented

### Privacy
- [ ] PII handling reviewed
- [ ] Audit logging verified
- [ ] Log sanitization implemented

## Incident Response
### Detection
- [ ] Intrusion detection operational
- [ ] Anomaly detection configured
- [ ] Alerting mechanism tested
- [ ] Monitoring dashboard functional

### Response
- [ ] Incident response plan documented
- [ ] Escalation procedures defined
- [ ] Communication plan established
- [ ] Forensic capabilities available

## Post-Testing
- [ ] All vulnerabilities documented
- [ ] PoC scripts provided
- [ ] Remediation recommendations delivered
- [ ] Risk ratings assigned
- [ ] Executive summary provided
- [ ] Retest scheduled (if needed)

## Aegis Security Daemon
- [ ] Aegis daemon running and enforcing policies
- [ ] Real-time agent behavior monitoring active
- [ ] Automated threat response configured
- [ ] Security policy distribution verified
- [ ] Aegis ↔ kernel module coordination tested

## Sigil Trust System
- [ ] Package signature verification operational
- [ ] Trust chain integrity validated
- [ ] TPM-backed attestation functional
- [ ] Key rotation procedures tested
- [ ] Transparency log entries verified

## Marketplace (Mela) Security
- [ ] Package signature verification enforced on install
- [ ] Sandbox profiles applied to marketplace agents
- [ ] Supply chain dependency validation active
- [ ] Malicious package detection tested
- [ ] Package removal/quarantine procedures verified

## Post-Quantum Cryptography
- [ ] Hybrid ML-KEM + X25519 key exchange verified
- [ ] Hybrid ML-DSA + Ed25519 signatures verified
- [ ] PQC algorithm negotiation tested
- [ ] Downgrade attack prevention verified
- [ ] Crypto-agile API migration path tested

## MCP Server Security
- [ ] MCP tool invocation authorization enforced
- [ ] MCP input validation and sanitization tested
- [ ] MCP session management secure
- [ ] MCP tool capability boundaries verified
- [ ] MCP rate limiting configured

## Federation Security
- [ ] Cross-node authentication verified
- [ ] Federation message integrity (mTLS) tested
- [ ] Node identity verification operational
- [ ] Federation traffic encryption enforced
- [ ] Distributed denial-of-service mitigations tested

## Vulnerability Severity Criteria

### Critical (CVSS 9.0-10.0)
- Remote code execution
- Complete system compromise
- Data exfiltration

### High (CVSS 7.0-8.9)
- Privilege escalation
- Sensitive data access
- Service disruption

### Medium (CVSS 4.0-6.9)
- Limited privilege escalation
- Limited data access
- Temporary service impact

### Low (CVSS 0.1-3.9)
- Information disclosure
- Minor configuration issue
- Minimal impact
