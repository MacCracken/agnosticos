# Stiva — Rust-Native Container Runtime

> **Status**: Scaffolded (0.1.0) | **Name**: stiva (Romanian: "stack")
>
> **Dependencies**: kavach (isolation), nein (networking), ark (images), libro (audit)

---

## State of the Art (2025–2026)

No existing system combines composable multi-layer isolation with quantitative security scoring. The industry either does VM isolation (Firecracker, Kata) OR OS-level hardening (gVisor, seccomp) — not both, and never with an attested purpose-built runtime in between.

### Industry Landscape

| Solution | Layers | Composable? | Quantitative Score? | Open Source? | Notes |
|----------|--------|------------|--------------------|----|-------|
| Docker/Podman | 1 (process) | No | No | Yes | General-purpose, CVE-prone shim chain |
| gVisor (Google) | 1 (user-space kernel) | No | No | Yes | Strong syscall interception, ~20% overhead |
| Kata Containers | 2 (VM + container) | Partial | No | Yes | QEMU/Firecracker/Cloud-Hypervisor backends |
| Firecracker (AWS) | 2 (VM + jailer) | Partial | No | Yes | Minimal KVM microVM, ~100ms boot |
| AWS Nitro Enclaves | 2 (hypervisor + enclave) | Partial | No | No | Proprietary, AWS-only, not auditable |
| CoCo (CNCF) | 3 (TEE + container + attestation) | Yes | Partial (EAR format) | Yes | Closest to stiva — but Kubernetes-native, no single score |
| Constellation/Contrast (Edgeless) | 2-3 (confidential VMs) | Yes | No | Yes | Constellation deprecated 2025, shifted to per-workload Contrast |
| Enarx (Profian) | 2 (TEE abstraction) | Partial | No | Yes | SGX/SEV/ARM TEE, WebAssembly focus |
| Intel TDX | 1 (hardware TEE) | No | No | N/A | CPU feature, not a runtime |
| AMD SEV-SNP + SVSM | 1-2 (VM + firmware) | Partial | No | N/A | Hardware feature + firmware layer |
| Youki | 1 (OCI runtime, Rust) | No | No | Yes | CVE-2025-54867 — Rust ≠ logic-safe |
| **Kavach + Stiva** | **5** | **Yes** | **Yes (0-100)** | **Yes** | **Only system with quantitative composite scoring** |

### What Nobody Else Does

1. **Quantitative composite scoring.** CoCo's Veraison produces structured attestation results (IETF RATS EAR format) but does not reduce them to a single score. Kavach is the only system that says "this configuration = 98" and can explain which layers contribute what.

2. **9 backends under one API.** Kata supports 3 VMMs. CoCo supports 3+ TEEs. Kavach unifies Process, gVisor, Firecracker, WASM, OCI, SGX, SEV, SyAgnos, and Noop under a single `SandboxBackend` trait with identical policy enforcement.

3. **Purpose-built runtime with no override mechanism.** Docker has `--privileged`. Podman has `--cap-add`. Even CoCo's Kata agent accepts runtime configuration. Stiva has no privilege escalation API — the policy is the only interface.

4. **Agent-native threat model.** Existing isolation systems protect "containers" or "workloads." Kavach/stiva protects AI agents — the threat model includes prompt injection leading to sandbox escape, credential exfiltration via output, and autonomous action without attestation.

### Academic Support

- **LATTE (EuroS&P 2025)**: Proposes layered attestation for composed TEE stacks but does not quantify composite strength. Kavach's scoring framework addresses this gap.
- **RContainer (NDSS 2025)**: Secure container architecture using hardware-backed isolation. Validates the approach of eliminating shim chains.
- **Confidential VMs Explained (ACM SIGMETRICS 2025)**: Comprehensive measurement of Intel TDX vs AMD SEV-SNP performance. Neither achieves composability — they're single-layer hardware isolation.

### CVE Evidence (2024–2025)

The container runtime shim chain (containerd → runc) has produced 5 critical CVEs in 14 months, all in the mount/namespace initialization path that stiva eliminates entirely:

| CVE | Date | Root Cause | Stiva Mitigation |
|-----|------|-----------|-----------------|
| CVE-2024-21626 | Feb 2024 | runc FD leak during init | No FD inheritance from untrusted code |
| CVE-2025-31133 | Nov 2025 | Symlink race on /dev/null | No symlink-based mount system |
| CVE-2025-52565 | Nov 2025 | /dev/console bind-mount race | Mounts pre-validated, no race window |
| CVE-2025-52881 | Nov 2025 | Procfs write redirect | No procfs mounting in container init |
| CVE-2025-54867 | Jan 2025 | Youki symlink trick (Rust!) | No general-purpose mount system |

The last entry is critical: **Youki is written in Rust**, proving that memory safety alone does not prevent logic errors in security-critical mount/namespace code. Stiva addresses this by not having a general-purpose mount system at all — mounts are static and pre-validated before any untrusted code executes.

---

## Problem

AGNOS depends on Docker/Podman (100MB+ Go daemon) for container workloads — sy-agnos sandbox images, edge deployment, development containers. The container runtime is the one major system component that isn't Rust-native.

Docker and Podman are general-purpose runtimes designed for developer ergonomics. They ship with escape hatches (`--privileged`, `--cap-add`), a persistent root daemon with REST API, and a multi-process shim chain (containerd → runc → container) that has produced repeated CVEs:

| CVE | Date | Impact |
|-----|------|--------|
| CVE-2024-21626 | Feb 2024 | runc FD leak → host filesystem access → arbitrary binary overwrite |
| CVE-2025-31133 | Nov 2025 | runc symlink race on /dev/null → bind-mount arbitrary target RW |
| CVE-2025-52565 | Nov 2025 | runc /dev/console race → procfs write access |
| CVE-2025-52881 | Nov 2025 | runc procfs redirect → host root escape |
| CVE-2025-54867 | Jan 2025 | Youki (Rust OCI runtime) symlink trick → host pseudo-fs mount |

The last entry is critical: **being written in Rust does not prevent logic errors in security-critical mount/namespace code.** Stiva addresses this by not having a general-purpose mount system at all — mounts are pre-validated and static.

---

## Architecture

```
stiva (container runtime, <5MB, daemonless, signed)
  ├── kavach  (isolation: namespaces, cgroups, seccomp, landlock, caps)
  ├── nein    (networking: nfnetlink sockets, bridge/host/none, no CNI)
  ├── ark     (images: signed squashfs layers, registry client)
  └── libro   (audit: container lifecycle events, cryptographic chain)
```

### Comparison to Docker/Podman

```
Docker model:                              Stiva model:
  dockerd (50MB+ Go daemon, root, REST)      stiva run (single binary, <5MB, signed)
    → containerd (shim manager)                → direct clone(NEWPID|NEWNS|NEWNET|NEWUSER)
      → runc (OCI runtime)                      → kavach policy (seccomp + landlock + caps)
        → container                                → container
    → CNI plugins (pluggable = attackable)     → nein (Rust-native nfnetlink)
    → docker pull (trust manifest)             → ark (signed squashfs, reject unsigned)
```

| Property | Docker/Podman | Stiva |
|----------|--------------|-------|
| Binary size | 50MB+ daemon | <5MB single binary |
| Process model | Persistent root daemon | Daemonless, exits after container |
| Shim chain | 3 processes (dockerd → containerd → runc) | 1 process (direct clone) |
| Override flags | `--privileged`, `--cap-add`, `--security-opt` | None — policy is the only API |
| Seccomp | Runtime-applied, overridable via config | Baked into binary, no override |
| Image trust | Registry manifest (MITM-able) | ark-signed squashfs (reject unsigned) |
| Network | CNI plugins (pluggable, configurable) | nein (Rust-native nfnetlink, static) |
| Attack surface | REST API, FD inheritance, mount races | None of the above |
| Self-attestation | None | Signed binary hash verified at launch |

---

## Security Uplift for sy-agnos

When stiva replaces docker/podman as the sy-agnos container runtime:

| Feature | Strength Boost | Why |
|---------|---------------|-----|
| Runtime attestation | +3 | Signed binary hash verified before first container |
| Image verification | +2 | ark-signed squashfs, reject unsigned/tampered images |
| Seccomp enforcement | +2 | Baked into runtime binary, no override API |
| No escape hatches | +2 | No `--privileged`, `--cap-add` — flags don't exist |
| No daemon | +2 | No persistent root process to attack |
| Minimal syscall surface | +1 | Direct clone() → exec, no shim chain |
| **Total** | **+12** | |

### Strength Scores by Configuration

```
sy-agnos minimal + docker           = 80
sy-agnos minimal + stiva            = 92
sy-agnos dm-verity + stiva          = 94
sy-agnos tpm_measured + stiva       = 95
Firecracker + jailer                = 93
Firecracker + jailer + stiva + sy-agnos TPM = 98
```

---

## Composable Isolation Stacks

Stiva enables explicit layer composition. Each layer catches what the one above misses:

```
Firecracker (KVM microVM)              — hardware isolation boundary
  └── jailer (cgroup, seccomp, chroot)    — privilege reduction
      └── stiva (attested runtime)        — no daemon, signed binary, no overrides
          └── sy-agnos (OS sandbox)       — immutable rootfs, baked seccomp/nftables
              └── TPM measured boot       — hardware-attested integrity chain
```

### Industry Comparison

| Solution | Composable? | Quantitative? | Layers | Score |
|----------|------------|---------------|--------|-------|
| Docker default | No | No | 1 | ~30 |
| gVisor (Google) | No | No | 1 | ~70 |
| Kata Containers | Partial | No | 2 (VM + container) | ~75 |
| Firecracker (AWS) | Partial | No | 2 (VM + jailer) | ~85 |
| AWS Nitro Enclaves | Partial | No | 2 (hypervisor + enclave) | ~90 |
| CoCo (CNCF) | Yes | Partial (EAR) | 3 (TEE + container + attestation) | ~85-92 |
| **Kavach + Stiva** | **Yes** | **Yes (0-100)** | **5 (VM + jailer + runtime + OS + TPM)** | **98** |

**What's unique about kavach/stiva:**
1. **Only system with quantitative composite scoring** — single number (0-100) for any configuration
2. **9 backends under one API** — broadest backend coverage of any sandbox framework
3. **Purpose-built runtime** — not a general-purpose Docker/Podman retrofitted with security
4. **Explicit composition model** — layers stack with measured strength, degradation is quantified
5. **Agent-native threat model** — designed for AI/agent sandboxing, not just container isolation

---

## Adversarial Test Plan

Stiva's security claims are proven by **purpose-driven adversarial testing** — specific attacks that must fail at each layer. See [kavach/tests/adversarial.rs](https://github.com/MacCracken/kavach) and [kavach/docs/development/stiva.md](https://github.com/MacCracken/kavach).

### Tests per Layer (~300 total)

| Layer | Tests | Attack Class |
|-------|-------|-------------|
| Stiva runtime | ~50 | Binary tampering, privilege escalation, daemon presence |
| Sy-agnos container | ~60 | Shell escape, seccomp bypass, network egress, fs write |
| Firecracker VM | ~40 | VM escape, memory isolation, device fuzzing |
| Jailer | ~30 | Chroot breakout, cgroup escape, capability escalation |
| TPM attestation | ~30 | PCR forgery, replay attacks, HMAC bypass |
| Externalization gate | ~40 | Secret leakage, oversized output, pattern evasion |
| Cross-layer composition | ~20 | End-to-end bypass, layer skip, score verification |
| Kavach policy | ~30 | Policy override, config injection, state machine violation |

**Methodology:** Each test is a specific attack vector. If all ~300 pass, the composed stack is demonstrably secure against known attack classes. The remaining 2 points to 100 represent "attacks we haven't thought of yet" — addressed by bug bounties and red teams.

---

## Kavach Integration

Kavach's `SyAgnosBackend` already detects docker/podman. When stiva is available:

```rust
// kavach auto-detects stiva as preferred runtime
fn detect_runtime() -> Option<(String, RuntimeKind)> {
    if which_exists("stiva") { return Some(("stiva".into(), RuntimeKind::Stiva)); }
    if which_exists("docker") { return Some(("docker".into(), RuntimeKind::Docker)); }
    if which_exists("podman") { return Some(("podman".into(), RuntimeKind::Podman)); }
    None
}

// Strength modifier applied based on runtime
fn runtime_strength_modifier(runtime: RuntimeKind) -> u8 {
    match runtime {
        RuntimeKind::Stiva => 12,   // attested, signed, no overrides, no daemon
        RuntimeKind::Docker => 0,
        RuntimeKind::Podman => 0,
    }
}
```

---

## Implementation Phases

| Phase | Delivers | Depends On |
|-------|---------|------------|
| **stiva v0.1** | Daemonless container lifecycle (create/start/exec/stop/rm), kavach isolation | kavach v1.0 |
| **stiva v0.2** | ark image signing + verification, registry pull | ark v1.0 |
| **stiva v0.3** | nein networking (bridge/host/none via nfnetlink) | nein v0.1 |
| **stiva v0.4** | Runtime self-attestation (signed binary hash) | libro v1.0 |
| **stiva v0.5** | Full composition with Firecracker + jailer | kavach post-v1 |
| **stiva v1.0** | Production-ready, adversarial test suite passing | All above |

---

## Non-Goals

- **General-purpose container runtime** — stiva runs kavach sandbox images, not arbitrary containers
- **Docker CLI compatibility** — no `docker build`, `docker compose`, `docker swarm`
- **Multi-tenant orchestration** — stiva runs one container at a time; fleet orchestration is daimon/sutra
- **Image building** — stiva runs images; ark builds them
- **Backwards compatibility** — no commitment to Docker socket API or OCI runtime spec compliance where it conflicts with security

---

## References

- [kavach stiva spec](https://github.com/MacCracken/kavach/blob/main/docs/development/stiva.md)
- [kavach adversarial tests](https://github.com/MacCracken/kavach/blob/main/tests/adversarial.rs)
- [Confidential Containers (CoCo)](https://confidentialcontainers.org/)
- [Veraison RATS attestation](https://www.ietf.org/wg/rats/)
- [LATTE: Layered Attestation (EuroS&P 2025)](https://donnod.github.io/assets/papers/eurosp25.pdf)
- [runc CVEs 2024-2025](https://www.sysdig.com/blog/runc-container-escape-vulnerabilities)
- [RContainer Secure Architecture (NDSS 2025)](https://www.ndss-symposium.org/wp-content/uploads/2025-328-paper.pdf)
