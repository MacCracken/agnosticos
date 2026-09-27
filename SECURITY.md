# Security Policy

This document outlines the security policies, procedures, and best practices for AGNOS (A General Networked Operating System).

## Supported Versions

Security updates are provided for the following versions:

| Version | Supported | Status |
|---------|-----------|--------|
| Latest stable | ✅ Yes | Full support |
| Previous stable | ✅ Yes | 6 months after new release |
| Beta/RC | ⚠️ Limited | Critical fixes only |
| Development | ❌ No | Use at your own risk |

## Notable Hardening — Structural Immunity

The AGNOS-native kernel is **structurally immune** to **CVE-2026-31431** (Linux LPE in `algif_aead`/AF_ALG via `splice()`, disclosed 2026-04-29). The sovereign AGNOS syscall table has no generic BSD socket family, no `socket()` over arbitrary domains, no `splice`, no AF_ALG — the bug class is unreachable. AGNOS *does* ship networking, but as a fixed TCP/UDP/ICMP band with slot-indexed connections: `sock_connect`, `sock_send`, `udp_bind` and their peers are purpose-built and take no address-family argument, so there is no family to select. This is a durable property of the *shape* of the surface, not its size: the table has grown several times over as the kernel matured and the immunity holds regardless, which is why this section anchors on the absent families rather than a count that drifts. (Since agnos 1.57.7 `sock_listen` carries an address *class* — any vs loopback-only — which is not a family selector.) Re-verified 2026-09-27 against agnos 1.57.10's `kernel/core/syscall.cyr`; the live enumeration lives in `agnos/docs/development/agnos-userland-abi.md` and is deliberately not restated here.

This is the canonical example of the **absence-by-design** security pattern: the kernel doesn't *patch* the vulnerability, doesn't *contain* the vulnerable subsystem — whole categories of Linux kernel CVEs become structurally inapplicable. The Linux host defconfigs in this repo (`kernel/{6.6-lts,6.x-stable,7.0-devel}/configs/` and `kernel/configs/edge-*.config`) are meant to pin `# CONFIG_CRYPTO_USER_API{,_HASH,_SKCIPHER,_AEAD,_RNG} is not set` to remove the equivalent surface from host kernels used in the bootstrap path. ⚠️ **That pin has not been applied**: none of those host config files carries a `CRYPTO_USER_API` line (re-checked 2026-09-27). It is an open *Do* item in [`docs/development/roadmap.md`](docs/development/roadmap.md), recorded as a known gap in [`docs/security/cis-benchmarks.md`](docs/security/cis-benchmarks.md). The AGNOS kernel's own immunity does not depend on it.

⚠️ **Absence-by-design covers only the classes AGNOS never added — not the surface it does add.** That surface is tracked, not assumed. The GPU/compositor and shared-memory bands grew with the desktop arc; kernel-side modeset writes sit behind an arm-once on-disk latch (`/.modeset-armed`), and block-device writes refuse until deliberately armed — see *Defense in Depth* below for what that gate is and is not. Shared-memory slots gained an owner and a generation, with `shm_free` owner-gated, in agnos 1.56.40 — cross-owner reads *and writes* remain permitted and counted rather than refused, because the compositor reads a client's buffer every frame. (When this was first written 1.56.40 was open and unburned; it has since been released, and every iron burn since 2026-08-07 (the 1.56.41 kernel) has run a kernel that carries it.) Since agnos **1.57.7** every TCP and UDP id is owned by the process that opened it, and a parent can cap a child's memory and CPU (`spawn_limits`#107) — built and gated in QEMU, **not burned** (the last iron burn is 1.57.1). Current status: [`docs/development/state.md`](docs/development/state.md).

For more on the pattern, see [`docs/security/security-guide.md`](docs/security/security-guide.md).

## Security Principles

### 1. Defense in Depth

AGNOS layers security natively. ⚠️ **No layer below is a Linux mechanism, and none will be**: per [`docs/development/roadmap.md`](docs/development/roadmap.md), confinement "when it lands, is **capability-scoped and native** — never Landlock, seccomp or `unshare` ABI emulation." Naming a Linux primitive here would advertise something the project has ruled out permanently, so layers that are not yet built are marked 📋 rather than dressed in a borrowed name.

- **Kernel Level**: ring 0 / ring 3 separation with a per-process address space; W^X — data segments, every user stack and all `mmap` memory mapped NX, and ELF text mapped read-only + executable since agnos 1.56.51 (through 1.56.50 text pages were writable *and* executable), with an RWX segment refused outright since 1.57.2 (built and gated, not burned). Two 1.56.52 fixes belong here because both were holes when this page was first written: user-pointer validation became a page-table walk (until then ring 3 could name kernel pages inside the user window), and ring-3 port I/O is now refused (until then a misplaced TSS field left ports `0x000–0x33F` open). 📋 **KASLR is not active**: gnoboot can slide an `ET_DYN` kernel to an RDRAND-chosen 2 MB-aligned base, but that needs an opt-in `CYRIUS_PIE=1` build nothing sets — the kernel that ships is `ET_EXEC` (`readelf -h build/agnos`, re-checked at 1.57.10). An earlier revision of this line listed the slide as a live control. The physical allocators are deterministic by design (both were made directional to fix a boot-flaky page fault), so there is no heap or allocation randomization either — see [`security-guide.md` §5](docs/security/security-guide.md)
- **System Level**: irreversible operations gated at the syscall boundary rather than by a MAC label store — raw block-device writes refuse until deliberately armed, and kernel-side modeset writes sit behind an arm-once on-disk latch. 📋 Per-process capabilities are **not** in the kernel, and per the 2026-05-12 identity-model ruling that agnos 1.56.56 adopted they will not be a kernel credential: the block-write gate is an arm token (cleared on `blk_close` since 1.56.51, and when the arming process dies since 1.57.1 / 1.57.7) whose named successor is a **kavach capability gate in userland**. 📋 Encryption at rest is installer-side only (agnova's LUKS root option) — the AGNOS kernel reads no encrypted volume
- **Application Level**: signature verification before *installation* — ark checks a SHA-256 root hash, an Ed25519 signature and per-file hashes (sigil) before any file is written — plus binary/archive scanning and auto-quarantine (aegis, kavach). ⚠️ Nothing verifies a signature at *exec* time: the AGNOS ELF loader performs no signature check, and per-agent capability grants are a design target, not a mechanism that exists today. ⚠️ kavach's Landlock / seccomp / namespace backends are **Linux-target only** — on AGNOS they compile out and report "not supported"; native confinement primitives are a roadmap item, not a shipped one
- **Network Level**: a fixed TCP/UDP/ICMP kernel band with slot-indexed connections — each owned since agnos 1.57.7 by the process that opened it (built and gated, not burned; through 1.57.6 any process could send on, drain or close any connection) — plus ingress hardening (a claimed IPv4 total-length is clamped to the bytes actually received). TLS is a **ring-3** concern — the Cyrius stdlib's `tls_native` over sigil — unblocked by the kernel prerequisites `getrandom`#45, `time_unix`#46 and `sock_*`#47–50. 📋 There is no in-kernel packet filter; a firewall layer is unbuilt, not merely unconfigured
- **Audit Level**: hash-chained audit records with opt-in per-entry Ed25519 signing (libro + sigil), with ML-DSA-65 post-quantum signing default-on (compiled in) in sigil
- **Structural Level**: Whole CVE classes unreachable because the syscall surface doesn't expose them (see *Notable Hardening* above)

### 2. Least Privilege

- Agents run with minimal required permissions — 📋 the intended model, and the design goal every item below is measured against. It is **not a control AGNOS enforces today**: every process runs unconfined, and the three bullets that follow name exactly where the gap is
- Capabilities are granted per-agent, not system-wide — 📋 the intended model. No per-agent capability enforcement exists yet: not in the kernel, not in daimon, and not behind the agent config schema below
- Identity is sigil keys, not Unix accounts — AGNOS is single-owner and does no Unix login by default. ⚠️ There is no uid model in the kernel, and by design there will not be one: `getuid` returns 0 for every process, and the ruling in [`planning/identity-and-authorization-model.md`](docs/development/planning/identity-and-authorization-model.md) (2026-05-12), which agnos 1.56.56 adopted in its kernel comments, makes this the decision, not a gap — authorization is meant to come from capabilities (kavach, t-ron) and multi-user from avatara overlays. So "restrict root" is not a control AGNOS offers. What actually limits an agent today is signature verification at install and aegis quarantine — not a privilege boundary the kernel enforces
- Privilege escalation requires explicit authorization (shakti) — ⚠️ shakti is **pre-1.0 (0.7.2)**, Linux-only, and authenticates against a policy file in userland. The kernel has no seam behind it, so nothing today prevents a process from simply not asking. Since agnos 1.56.56 the kernel says so by design: the comment that once named an aegis/shakti check landing "when agnos grows per-proc caps" now names a **kavach capability gate in userland** as the successor, and shakti — a uid-de-escalating sudo equivalent — is recorded as N/A on agnos by architecture (shakti's own roadmap has not caught up with that ruling yet)

### 3. Transparency

- All security-relevant events are logged
- Audit logs are cryptographically signed
- Source code is open for review
- Security decisions are documented

### 4. Human Sovereignty

- Humans retain ultimate control
- All agent actions can be audited
- Override mechanisms exist for critical operations
- No autonomous privileged operations

## Reporting Security Vulnerabilities

### Private Disclosure (Preferred)

If you discover a security vulnerability, please report it privately:

**Email**: security@agnos.io

**GPG Key**: Contact security@agnos.io for the GPG public key.

**Include in your report**:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)
- Your contact information

### Response Timeline

| Timeframe | Action |
|-----------|--------|
| 24 hours | Acknowledgment of receipt |
| 72 hours | Initial assessment |
| 1 week | Fix development begins |
| 30 days | Target fix completion |
| 90 days | Public disclosure (coordinated) |

### Public Disclosure

After the vulnerability is fixed:

1. Security advisory published
2. CVE assigned (if applicable)
3. Credit given to reporter (with permission)
4. Fix backported to supported versions

### Bug Bounty

We offer a bug bounty program for eligible vulnerabilities:

| Severity | Reward |
|----------|--------|
| Critical | $5,000 - $10,000 |
| High | $1,000 - $5,000 |
| Medium | $500 - $1,000 |
| Low | $100 - $500 |

Scope: the AGNOS kernel and its syscall surface (there are no loadable kernel modules — the kernel is a single Cyrius binary), agent runtime, sandbox implementation, cryptographic systems

## Security Architecture

For the full security architecture, threat model, and technical details (process isolation, the syscall trust boundary, W^X, sandboxing, permissions, audit logging, aegis, sigil, post-quantum cryptography, mTLS, zero-trust), see [docs/security/security-guide.md](docs/security/security-guide.md). That guide — and the checklist, penetration-testing, vulnerability-management and CIS documents linked at the bottom of this page — was **rewritten 2026-08-05 for the Cyrius/agnos era** against agnos 1.56.40 / cyrius 6.5.7 / gnoboot 0.6.1, and **re-verified 2026-09-27** against agnos 1.57.10 / cyrius 6.6.6 / gnoboot 0.7.2 (⚠️ agnos 1.57.2–1.57.10 are built and gated in QEMU, not burned; the last iron burn is 1.57.1), and is no longer a fossil. The Rust code samples are gone; Landlock, seccomp and namespaces survive only as 🐧 *Linux-target-only* mechanisms that [`docs/development/roadmap.md`](docs/development/roadmap.md) rules out for AGNOS permanently. Sandboxing, per-agent permissions, the human-approval gate, mTLS and post-quantum key exchange are now recorded there as **absent**, not shipped. This page states policy and summarizes; the guide carries a `path:line` citation into live source for every control it claims — where the two differ on a specific control, verify against the guide's citation.

## Security Hardening Guide

### System Installation

⚠️ `agnos-install` / `agnos-secureboot` / `agnos-audit` were Rust-era placeholder names — **no such binaries exist**. The installer is **agnova**; the audit chain is **libro**. Of the three items below, only the first ships today.

1. **Encrypt the root volume** — agnova's LUKS option, chosen at install time:
   ```bash
   agnova plan --device /dev/nvme0n1 --mode server --encrypt --verbose
   ```
   📋 The AGNOS kernel cannot yet *read* an encrypted volume, so this applies to host-staged installs. TPM sealing is unbuilt — agnova has no `--tpm` flag.

2. **Secure Boot** — 📋 no enrolment tool ships. The pieces that exist are gnoboot (a PE32+ EFI application) and `cyrius sign-efi`, which Authenticode-signs a PE for UEFI Secure Boot. Chaining them into a first-boot enrolment flow has not been built.

3. **Audit logging** — libro provides the hash-chained, signed audit chain the subsystems write into. 📋 There is no system-wide enable switch and no remote-log shipper.

### Agent Security

⚠️ Agent lifecycle lives in **daimon** and sandbox policy in **kavach**; `agnos-agent` is another Rust-era placeholder name with no binary behind it. The policy fields below are kavach's **portable** ones — deliberately not `landlock:` / `seccomp:` / `network_isolation:`, which are Linux-target fields kavach compiles out on AGNOS and which the roadmap rules out permanently.

1. **Create Restricted Agent**
   ```yaml
   # /agnos/agents/my-agent/config.yaml
   name: my-agent
   capabilities:
     - file.read:/home/user/documents/**
     - network.connect:api.example.com:443
   sandbox:
     preset: strict          # kavach policy_strict()
     network_enabled: false
     read_only_rootfs: true
     memory_limit_mb: 512
   audit_level: verbose
   ```
   📋 On AGNOS the resource limits and the confinement backend are not enforceable yet; the `sandbox:` block is honoured on Linux targets. 📋 The `capabilities:` and `audit_level:` keys are a Rust-era schema with no parser behind them anywhere in the ecosystem — they document the intended grant shape, not a check that runs today.

2. **Verify Agent Permissions** — 📋 not verifiable today. daimon supervises agent lifecycle but performs no capability check, and nothing else parses the grant keys above, so there is nothing for a verify CLI to report on yet.

3. **Monitor Agent Activity** — agent actions land in the libro audit chain. 📋 There is no follow-mode log CLI.

### Network Security

📋 **AGNOS has no firewall.** There is no netfilter, no nftables and no packet-filter hook anywhere in the kernel; an `nft` ruleset previously shown here described a Linux host, not AGNOS. Because the kernel exposes only a fixed TCP/UDP/ICMP band, inbound reachability is decided entirely by *what you run*, not by a rule table. The base system's listening set is small enough to enumerate:

| Service | Port | Notes |
|---------|------|-------|
| daimon (agent runtime) | 8090 | HTTP API with **no authentication** (daimon's roadmap puts auth at 2.5.x); asks for `127.0.0.1` by default since daimon 2.3.0 — through 2.2.x it bound `0.0.0.0` |
| hoosh (LLM gateway) | 8088 | OpenAI-compatible API; asks for `127.0.0.1` by default; auth fails open when no tokens are configured |

Anything else you install opens its own port and nothing can close it again — bote's MCP transports (`127.0.0.1:8390` HTTP, `:8391` bridge, `:8392` streamable, `:8393` WebSocket) and agora's telnet listener (`2323`) are the two you are most likely to meet. Enumerate the daemons you actually started; there is no rule table to consult instead.

⚠️ **On AGNOS itself, "binds `127.0.0.1`" is a Linux-host fact.** Through cyrius 6.6.6 the stdlib's agnos arm of `sock_bind` ignores the address (`cyrius/lib/net.cyr:295-302`) and `sock_listen`#56 is issued with the port alone (`lib/syscalls_x86_64_agnos.cyr:1633-1641`), so any server built on it for AGNOS listens on the NIC's address whatever address it asked for, reachable from the network — daimon's `serve` does exactly this, and warns and audits `http.listen.not_loopback` when it does. agnos 1.57.7 added a loopback-only listen class (`SOCK_LISTEN_LOOPBACK`, built and gated, not burned); the stdlib does not pass it yet.

⚠️ Three consequences worth stating plainly. A port is closed here only because no program opened it — that is not the same guarantee a default-deny filter gives. A compromised daemon is not contained by it, and neither is a second daemon you forgot you started. And where AGNOS is staged from a Linux host during bootstrap, that host's own firewall still applies and remains the operator's responsibility.

## Security Testing

### Automated Testing

```bash
# Boot pipeline security checks
cd scripts && ./build/boot --test --kernel /path/to/agnos

# Subsystem security tests (in respective repos)
cd /path/to/subsystem
cyrius tests  # every .tcyr suite under tests/ (bare `cyrius test` does the same since cyrius 6.5.11; given file arguments it runs only those)
cyrius fuzz   # the .fcyr harnesses under fuzz/
cyrius audit  # project sweep: fmt / lint / docs / tests / bench
```

⚠️ kavach's nine CWE-class findings (CWE-208, -116, -59, -276, -532, -88, -316, -190, -252) came from its **ADR-005 hardening pass**, not from fuzzing — an earlier revision of this file credited them to `cyrius fuzz`.

### Manual Testing

- [ ] Attempt privilege escalation
- [ ] Test sandbox escape vectors
- [ ] Verify audit log integrity
- [ ] Confirm the listening-port set matches the daemons you actually started (there is no filter to test)
- [ ] Test human override mechanisms
- [ ] Verify encryption at rest
- [ ] Test recovery procedures

## Incident Response

### Severity Levels

| Level | Criteria | Response Time |
|-------|----------|---------------|
| Critical | RCE, privilege escalation, data breach | 1 hour |
| High | DoS, information disclosure | 4 hours |
| Medium | Security bypass, misconfiguration | 24 hours |
| Low | Documentation, hardening | 7 days |

### Response Process

1. **Detection**: Automated monitoring or user report
2. **Assessment**: Determine severity and impact
3. **Containment**: Limit damage and exposure
4. **Investigation**: Root cause analysis
5. **Remediation**: Develop and test fix
6. **Recovery**: Deploy fix and verify
7. **Post-incident**: Review and improve

### Emergency Procedures

⚠️ **There is no incident-response tooling yet.** Earlier revisions of this section listed an `agnos-agent` / `agnos-audit` / `agnos-security` / `agnos-forensics` command set; none of those binaries exist, and an incident is the worst possible moment to discover that. The steps below are what a responder can actually do today, with the gaps named.

**Agent Compromise**:
- **Isolate** — 📋 no isolate verb. Stop the agent through daimon, which owns agent lifecycle and process supervision. ⚠️ On AGNOS a stop only *acts* since agnos 1.57.7 (built and gated, not burned): through 1.57.6 `kill`#16 set a pending bit and nothing more. daimon signals the agent's own pid only — its children survive — although `KILL_TREE` (`sig | 0x100`) exists since 1.57.7.
- **Preserve evidence** — the libro audit chain is hash-chained (per-entry Ed25519 signing is opt-in), so a tampered record is detectable. Copy the chain files before anything else; 📋 there is no forensic-capture command that bundles process state.
- **Contain** — quarantine and scanning live in aegis and kavach. 📋 On AGNOS the confinement backends are not enforceable (see *Defense in Depth*), so containment is currently "stop the process", not "restrict it in place".

**System Compromise**:
- 📋 There is no lockdown switch. The blunt controls that do exist are the block-write arm (raw device writes refuse until deliberately armed) and orderly shutdown via ACPI S5.
- Report through the private-disclosure channel below.

## Compliance

### Standards Alignment

| Standard | Status | Notes |
|----------|--------|-------|
| CIS Benchmarks | 📋 Planned | **No certification is held and no compliance percentage has been measured.** The mapping in [`docs/security/cis-benchmarks.md`](docs/security/cis-benchmarks.md) was rewritten 2026-08-05 and *is* scoped to the AGNOS kernel — its predecessor marked ~55 controls ✅ against subsystems that do not exist, and those were removed rather than updated. Its finding is that most of the benchmark is **categorically inapplicable**: CIS presupposes uid/gid separation, PAM, systemd, `/etc/fstab`, sysctl, auditd and nftables, none of which AGNOS has. Rows that hold are marked 🐧 **host-bootstrap only** |
| NIST 800-53 | 📋 Planned | Moderate impact |
| Common Criteria | 📋 Planned | Target EAL4+ |
| FIPS 140-2 | 📋 Planned | Cryptographic modules |

### Certifications

Planned security certifications:

- **Common Criteria (EAL4+)**: Targeting government/enterprise use
- **FIPS 140-2 Level 2**: For cryptographic operations
- **CIS Benchmarks**: Hardening compliance

## Security Resources

- [Security Guide](docs/security/security-guide.md)
- [Penetration Testing](docs/security/penetration-testing.md)
- [Security Checklist](docs/security/security-checklist.md)
- [Vulnerability Management](docs/security/vulnerability-management.md)
- [CIS Benchmarks](docs/security/cis-benchmarks.md)

## Contact

- **Security Team**: security@agnos.io
- **GPG Key**: Contact security@agnos.io for the GPG public key
- **Bug Bounty**: [bugcrowd.com/agnos](https://bugcrowd.com/agnos)
- **Security Advisories**: [agnos.io/security/advisories](https://agnos.io/security/advisories)

## Acknowledgments

We thank the security researchers who have responsibly disclosed vulnerabilities:

- *No disclosures yet - be the first!*

---

**Last Updated**: 2026-09-27
**Version**: 0.1.0 (SemVer — tracks the repo `VERSION` file; the former `2026.5.9` stamp was a pre-switch CalVer fossil)
**Next Review**: 2026-11-05
