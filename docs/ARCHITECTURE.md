# AGNOS System Architecture

> **Last Updated**: 2026-03-07 | **Version**: 2026.3.7

This document provides the technical architecture of AGNOS (AI-Native General Operating System).

## Table of Contents

1. [System Overview](#system-overview)
2. [Named Subsystems](#named-subsystems)
3. [Kernel Architecture](#kernel-architecture)
4. [User Space Architecture](#user-space-architecture)
5. [Security Architecture](#security-architecture)
6. [Data Flow](#data-flow)
7. [Technology Stack](#technology-stack)
8. [Design Decisions](#design-decisions)

## System Overview

AGNOS is a specialized Linux distribution designed for AI agent execution with human oversight. The architecture consists of three main layers with four named subsystems spanning them:

```
+======================================================================+
|                        AGNOS Architecture                             |
+======================================================================+
|                                                                       |
|  Named Subsystems:                                                    |
|  +-------+ +-------+ +-------+ +-------+ +-------+ +-------+        |
|  |  ark  | | nous  | |takumi | | mela  | | aegis | | sigil |        |
|  |  pkg  | |resolve| | build | |market | |secure | | trust |        |
|  |  mgr  | |daemon | |system | | place | |daemon | |system |        |
|  +-------+ +-------+ +-------+ +-------+ +-------+ +-------+        |
|  +----------+ +---------+                                             |
|  | argonaut | | agnova  |                                             |
|  |   init   | |installer|                                             |
|  |  system  | |         |                                             |
|  +----------+ +---------+                                             |
|       |              |              |              |                   |
+-------+--------------+--------------+--------------+------------------+
|                                                                       |
|  +---------------------------------------------------------------+   |
|  |                     User Space Layer                           |   |
|  |  +-------------+ +-------------+ +------------------------+   |   |
|  |  | aethersafha | |  agnoshi   | |  Agent Applications    |   |   |
|  |  |  (desktop)  | |   (shell)   | | (mela + flutter)       |   |   |
|  |  +------+------+ +------+------+ +-----------+------------+   |   |
|  |         +----------------+------------------------+           |   |
|  |                          |                                    |   |
|  |  +-------------+  +-----+------+  +-----------+              |   |
|  |  | LLM Gateway |  |   Agent   |  | agnos-sys |              |   |
|  |  |  (port 8088)|  |  Runtime  |  | (syscalls)|              |   |
|  |  |             |  | (port 8090)|  |           |              |   |
|  |  +-------------+  +-----+------+  +-----+-----+              |   |
|  +---------------------------------------------------------------+   |
|                             |                                         |
+-----------------------------+-----------------------------------------+
|                             |                                         |
|  +---------------------------------------------------------------+   |
|  |                    Kernel Space Layer                          |   |
|  |  +-------------------+------------------------------+         |   |
|  |  |            Linux 6.6 LTS (Hardened)              |         |   |
|  |  |  +-----------+ +-----------+ +----------------+  |         |   |
|  |  |  |   AGNOS   | |   Agent   | |     LLM        |  |         |   |
|  |  |  |  Security | |   Kernel  | |    Kernel      |  |         |   |
|  |  |  |  Module   | | Subsystem | |    Module      |  |         |   |
|  |  |  +-----------+ +-----------+ +----------------+  |         |   |
|  |  +--------------------------------------------------+         |   |
|  |                          |                                    |   |
|  |  +------------------------------------------------------+    |   |
|  |  |           Hardware Abstraction Layer                   |    |   |
|  |  |      (CPU, GPU, NPU, Memory, Storage, I/O)            |    |   |
|  |  +------------------------------------------------------+    |   |
|  +---------------------------------------------------------------+   |
|                                                                       |
+======================================================================+
```

## Named Subsystems

AGNOS uses named subsystems for its major cross-cutting concerns. Each subsystem has a distinct identity, purpose, and API surface.

### ark — Unified Package Manager

The user-facing CLI for all package operations. Users never interact with `apt`, `dpkg`, or marketplace APIs directly.

```
ark install nginx          # system package (via apt backend)
ark install acme/scanner   # mela agent package
ark install photis-nadi    # Flutter desktop app
ark search "web server"    # searches all sources
ark update                 # checks all sources for updates
ark list                   # unified view of everything installed
ark status                 # package system health
```

**Components:**
- `agent-runtime/src/ark.rs` — CLI command parser, install planner, output formatter
- HTTP API: `/v1/ark/*` routes on port 8090
- agnoshi: `ark install ...` natural language intents

**Design principle:** ark generates *install plans* but does not execute privileged operations directly. Execution flows through `agnos-sudo` for system packages or the marketplace installer for agents.

### nous — Package Resolver Daemon

The intelligence layer behind ark. Given a package name, nous determines which source to use.

```
"nginx"          -> PackageSource::System       (apt)
"acme/scanner"   -> PackageSource::Marketplace  (publisher/name format)
"photis-nadi"    -> PackageSource::FlutterApp   (known desktop app)
```

**Components:**
- `agent-runtime/src/nous.rs` — resolver logic, source detection, unified search
- `SystemPackageDb` — safe wrapper around `apt-cache`/`dpkg-query` (no shell injection)
- Marketplace integration via `LocalRegistry`

**Resolution strategy:** Configurable — `MarketplaceFirst` (default), `SystemFirst`, `OnlySource(...)`, or `SearchAll`.

### aegis — System Security Daemon

*Status: Implemented (`agent-runtime/aegis.rs`, 40 tests) — [ADR-003](adr/adr-003-security-and-trust.md)*

The unified security and threat protection layer. Coordinates threat detection, quarantine, and scanning across all subsystems.

**Implemented:**
- **Threat levels** — Critical, High, Medium, Low, Info with auto-response policies
- **Security events** — 10 event types (IntegrityViolation, SandboxEscape, AnomalousBehavior, etc.)
- **Auto-quarantine** — Critical/High threats trigger automatic agent suspension/termination
- **Scanning** — on-install and on-execute scanning of agents and packages
- **Quarantine management** — quarantine, release, auto-release timeout
- **Event pipeline** — filtering by agent, threat level, resolution status

**Existing primitives it coordinates:**
| Primitive | Current Location | aegis Role |
|-----------|-----------------|------------|
| Landlock sandbox | `agent-runtime/sandbox.rs` | Policy enforcement |
| seccomp-bpf | `agent-runtime/seccomp_profiles.rs` | Syscall filtering |
| IMA/EVM | `agnos-sys/ima.rs` | File integrity |
| TPM 2.0 | `agnos-sys/tpm.rs` | Measured boot |
| dm-verity | `agnos-sys/dmverity.rs` | Partition integrity |
| Audit chain | `agnos-common/audit.rs` | Event logging |
| Anomaly detection | `agent-runtime/learning.rs` | Behavioral analysis |
| Certificate pinning | `agnos-sys/certpin.rs` | TLS trust |

### sigil — Trust System

*Status: Implemented (`agent-runtime/sigil.rs`, 35 tests) — [ADR-003](adr/adr-003-security-and-trust.md)*

The system-wide trust and verification framework. Every binary, package, config, and update is verified through sigil.

**Implemented:**
- **Trust levels** — SystemCore > Verified > Community > Unverified > Revoked
- **Trust policy** — Strict (block unverified), Permissive (warn), AuditOnly (log only)
- **Artifact verification** — `verify_artifact()`, `verify_agent_binary()`, `verify_package()`, `verify_boot_chain()`
- **Ed25519 signing** — `sign_artifact()` with trust store registration
- **Revocation** — RevocationList (revoke by key_id or content_hash), JSON persist
- **Trust store** — cached verification results by content hash

**Foundation components:**
| Component | Location | sigil Role |
|-----------|----------|------------|
| Ed25519 signing | `marketplace/trust.rs` | Core signing primitives |
| Publisher keyring | `marketplace/trust.rs` | Key management |
| Transparency log | `marketplace/transparency.rs` | Publish audit trail |
| Integrity verifier | `integrity.rs` | File hash verification |

### takumi — Package Build System

*Status: Implemented (`agent-runtime/takumi.rs`, 43 tests) — [ADR-004](adr/adr-004-distribution-build-and-installation.md)*

The master craftsman that compiles packages from source into `.ark` binary packages. (Japanese: takumi = master craftsman)

**Implemented:**
- **TOML recipe parser** — BuildRecipe with PackageMetadata, SourceSpec, DependencySpec, BuildSteps, SecurityFlags
- **Security hardening** — PIE, RELRO, FullRelro, Fortify, StackProtector, Bindnow flags with CFLAGS/LDFLAGS generation
- **Build dependency resolution** — topological sort with cycle detection
- **File manifest** — recursive directory walk with SHA-256 per file
- **Build pipeline** — 10 stages from Pending to Complete/Failed
- **.ark package format** — ArkManifest, ArkFileEntry, ArkPackage types

**Recipe example (`openssl.toml`):**
```toml
[package]
name = "openssl"
version = "3.5.2"
groups = ["base", "crypto"]

[source]
url = "https://www.openssl.org/source/openssl-3.5.2.tar.gz"
sha256 = "..."

[depends]
runtime = ["glibc", "zlib"]
build = ["perl", "make"]

[build]
configure = "./config --prefix=/usr shared zlib-dynamic"
make = "make -j$(nproc)"
install = "make DESTDIR=$PKG install"

[security]
hardening = ["pie", "relro", "fortify"]
```

### argonaut — Init System

*Status: Implemented (`agent-runtime/argonaut.rs`, 46 tests) — [ADR-004](adr/adr-004-distribution-build-and-installation.md)*

A single Rust binary that replaces systemd/sysvinit. No shell scripts in the boot path.

**Implemented:**
- **Three boot modes:** Server (headless), Desktop (compositor + shell), Minimal (container)
- **9-stage boot sequence:** MountFilesystems → StartDeviceManager → VerifyRootfs → StartSecurity → StartAgentRuntime → StartLlmGateway → StartCompositor → StartShell → BootComplete
- **Service management:** dependency resolution, state machine, health/ready checks
- **Restart policies:** Always, OnFailure, Never
- **Shutdown:** reverse startup order with configurable timeout

**Target:** <3 seconds from kernel handoff to agent-runtime ready.

### agnova — OS Installer

*Status: Implemented (`agent-runtime/agnova.rs`, 41 tests) — [ADR-004](adr/adr-004-distribution-build-and-installation.md)*

The AGNOS installer. Takes a blank disk and produces a running system.

**Implemented:**
- **4 install modes:** Server, Desktop, Minimal, Custom
- **Disk layout:** GPT partitioning with 512MB ESP + root, optional LUKS2 encryption
- **Bootloader:** systemd-boot and GRUB2 support
- **14-phase install pipeline:** ValidateConfig → PartitionDisk → FormatFilesystems → SetupEncryption → MountFilesystems → InstallBase → InstallPackages → ConfigureSystem → InstallBootloader → CreateUser → SetupSecurity → FirstBootSetup → Cleanup → Complete
- **Security by default:** LUKS, Secure Boot, TPM, dm-verity, strict trust enforcement
- **System generation:** machine-id, hostname, fstab, kernel cmdline
- **Config validation** with error reporting

### Subsystem Interaction

```
User: "ark install acme/scanner"
  |
  v
[ark] parses command, calls nous
  |
  v
[nous] resolves "acme/scanner" -> Marketplace source
  |
  v
[sigil] verifies publisher signature, checks transparency log
  |
  v
[aegis] scans package contents, validates sandbox profile
  |
  v
[ark] presents install plan to user, executes via marketplace installer
  |
  v
[aegis] monitors runtime behavior of installed agent
```

## Kernel Architecture

### Base Kernel

AGNOS uses Linux 6.6 LTS as the base kernel with hardening patches and custom modules.

#### Kernel Configuration

```
# Security
CONFIG_SECURITY=y
CONFIG_SECURITY_SELINUX=y
CONFIG_SECURITY_LANDLOCK=y
CONFIG_SECURITY_SECCOMP=y
CONFIG_SECURITY_SECCOMP_FILTER=y
CONFIG_SECURITY_YAMA=y

# Memory protection
CONFIG_KASAN=y
CONFIG_PAGE_TABLE_ISOLATION=y
CONFIG_RETPOLINE=y

# Namespaces (for agent isolation)
CONFIG_NAMESPACES=y
CONFIG_USER_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
CONFIG_CGROUP_NS=y

# cgroups (for resource limits)
CONFIG_CGROUPS=y
CONFIG_CGROUP_CPUACCT=y
CONFIG_CGROUP_DEVICE=y
CONFIG_CGROUP_PIDS=y
CONFIG_CGROUP_BPF=y

# AGNOS-specific modules
CONFIG_AGNOS_SECURITY=m
CONFIG_AGNOS_AGENT_SUBSYSTEM=m
CONFIG_AGNOS_LLM=m
CONFIG_AGNOS_AUDIT=m
```

### Custom Kernel Modules

- **AGNOS Security Module (ASM)** — LSM hooks for agent filesystem/network access control
- **Agent Kernel Subsystem** — agent process management, resource limits, IPC namespaces
- **LLM Kernel Module** — GPU/NPU memory management, model loading, inference scheduling
- **Audit Module** — tamper-evident, hash-chained, signed audit logging

## User Space Architecture

### Crate Map

```
userland/
  agnos-common/          agnostik — shared types, errors, audit, secrets, telemetry
  agnos-sys/             agnosys — kernel syscall bindings (16 modules)
  agnos-sudo/            shakti — privilege escalation with audit trail
  agent-runtime/         daimon — core daemon, orchestrator, sandbox, IPC (8090)
    marketplace/         mela — package marketplace (trust, registry, flutter)
    nous.rs              nous — package resolver daemon
    ark.rs               ark — unified package manager CLI
  ai-shell/              agnoshi — natural language terminal shell
  llm-gateway/           hoosh — OpenAI-compatible LLM proxy (8088)
  desktop-environment/   aethersafha — Wayland compositor, shell, plugins
```

### daimon — Agent Runtime (port 8090)

**daimon** (Greek: guiding spirit) — the central daemon managing agent lifecycle, sandboxing, and inter-agent communication.

**Key modules:** orchestrator, supervisor, sandbox, registry, IPC (RPC + pub/sub), service manager, lifecycle hooks, package manager, mela marketplace (trust, transparency, local registry, remote client, flutter packaging, sandbox profiles), WASM runtime, network tools, swarm intelligence, learning/anomaly detection, multimodal support, RAG/knowledge base, vector store, memory store, file watcher, resource forecasting, mTLS, integrity verification, MCP server (16 tools), nous resolver, ark package manager.

### hoosh — LLM Gateway (port 8088)

**hoosh** (Persian: intelligence — the actual word for AI in Farsi) — OpenAI-compatible API proxy with provider routing, caching, per-agent rate limiting, hardware acceleration, and certificate pinning.

**Endpoints:** `/v1/chat/completions`, `/v1/models`, `/v1/health`, `/v1/metrics`

### agnoshi (terminal) + vansh (voice)

**agnoshi** (agnos + Japanese shi=knowledge/will) — the terminal AI shell. Natural language command interpreter with 30+ intent types covering: file operations, process management, agent lifecycle, audit viewing, network scanning, service management, journal queries, device management, mela marketplace operations, ark package management, task management (Photis Nadi MCP bridge), ritual tracking, productivity analytics.

**vansh** (Sanskrit vani=voice + sh) — the voice AI shell. TTS/STT conversational interface. Planned.

### aethersafha — Desktop Environment

**aethersafha** (Greek aether=pure sky + Arabic safha=surface) — Wayland compositor with plugin host architecture, XWayland fallback, accessibility (AT-SPI2), high-contrast themes, Flutter theme bridge, shell integration (tray, window management, notifications), security dashboard, and agent HUD.

### agnosys — Kernel Interface

**agnosys** (agnos+sys, a-gnosis=toward knowledge) — Rust bindings for Linux kernel subsystems: Landlock, seccomp, audit, MAC, network namespaces, LUKS, dm-verity, IMA/EVM, TPM 2.0, Secure Boot, certificate pinning, bootloader, journald, udev, FUSE, PAM, system updates.

### agnostik — Shared Types

**agnostik** (agnostic) — shared types library used by all crates: error types, audit chain, secrets management, telemetry/tracing, LLM types, environment profiles, agent manifest.

### shakti — Privilege Escalation

**shakti** (Sanskrit: power/authority) — audited privilege escalation. Every root operation is logged to the audit chain with caller identity, command, and outcome.

## Security Architecture

### Defense in Depth

```
+-----------------------------------------------------------+
|  Network: TLS 1.3, cert pinning (sigil), nftables         |
+-----------------------------------------------------------+
|  Application: Bearer auth, CORS localhost-only, input val  |
+-----------------------------------------------------------+
|  Execution: Landlock + seccomp-bpf (aegis), namespaces     |
+-----------------------------------------------------------+
|  Storage: LUKS encryption, dm-verity, IMA/EVM (aegis)      |
+-----------------------------------------------------------+
|  Trust: Ed25519 signing, transparency log (sigil)          |
+-----------------------------------------------------------+
|  Audit: Hash-chained log, rotation, anomaly detection      |
+-----------------------------------------------------------+
```

### Sandbox Apply Order

1. Encrypted storage (LUKS)
2. MAC policy (Landlock)
3. Syscall filtering (seccomp-bpf)
4. Network isolation (namespaces + nftables)
5. Audit chain activation

### Package Trust Flow (sigil)

```
Publisher signs package with Ed25519 key
  -> Transparency log records publish event (hash-chained)
  -> ark/nous resolves package source (mela or system)
  -> sigil verifies signature against publisher keyring
  -> aegis scans contents
  -> Marketplace installer extracts to sandboxed directory
  -> IMA/EVM records file hashes
  -> Runtime: seccomp + Landlock enforced per sandbox.json
```

## Data Flow

### Agent Action Flow

```
User Request -> agnoshi/vansh -> Agent Runtime (8090) -> Agent Process (sandboxed)
                                         |                       |
                                    Audit Chain             LLM Gateway (8088)
                                    (hash-signed)           (if inference needed)
```

### Package Install Flow (ark + nous)

```
User: "ark install nginx"
  |
  v
[agnsh] parses "ark install nginx" -> ArkInstall intent
  |
  v
[HTTP] POST /v1/ark/install {"packages": ["nginx"]}
  |
  v
[nous] resolve("nginx") -> PackageSource::System
  |
  v
[ark] plan_install -> InstallPlan { SystemInstall("nginx"), requires_root: true }
  |
  v
[agnos-sudo] apt-get install -y nginx (with audit trail)
  |
  v
[aegis] verify installed files, record in audit chain
```

## Technology Stack

### Kernel

| Component | Technology | Purpose |
|-----------|------------|---------|
| Base | Linux 6.6 LTS | Operating system kernel |
| Security | Landlock, seccomp-bpf | Sandboxing |
| Modules | C | Kernel extensions |
| Build | Kbuild | Kernel compilation |

### User Space

| Component | Technology | Purpose |
|-----------|------------|---------|
| daimon (agent-runtime) | Rust (tokio) | Core agent management |
| hoosh (llm-gateway) | Rust (axum) | Model inference proxy |
| agnoshi/vansh (ai-shell) | Rust | Natural language CLI (text + voice) |
| aethersafha (desktop-env) | Rust (Wayland) | GUI environment |
| agnosys (agnos-sys) | Rust | Kernel interface bindings |
| agnostik (agnos-common) | Rust | Shared types library |
| shakti (agnos-sudo) | Rust | Privilege escalation |
| IPC | Unix domain sockets | Inter-process communication |
| Crypto | ed25519-dalek, sha2 | Signing, hashing |
| HTTP | axum, reqwest | API server/client |
| Serialization | serde, serde_json | Data formats |

### Package Management (ark + nous)

| Source | Backend | Format |
|--------|---------|--------|
| System | apt/dpkg | `.deb` (Debian Bookworm base) |
| Marketplace | Local registry | `.agnos-agent` (signed tarball) |
| Flutter Apps | agpkg | `.agnos-agent` (Flutter bundle) |

### Base Distribution

**Alpha (current):** Debian Bookworm slim — pragmatic choice for shipping fast with ML ecosystem compatibility.

**Post-alpha (ADR-004):** LFS-native distribution. ~50 packages built from source via `takumi` recipes, `.ark` binary packages, `ark` as sole package manager. No Debian dependency. AI infrastructure (CUDA, PyTorch, ONNX) shipped as `.ark` packages out of the box.

The architecture is distro-agnostic by design — all AGNOS-specific code uses standard Linux syscalls with no Debian-specific dependencies. The transition from Debian to LFS-native changes only the packaging layer, not the Rust userland.

## Design Decisions

### 1. Linux Kernel Base
Use Linux 6.6 LTS rather than building from scratch. Mature, well-tested, extensive hardware support.

### 2. Rust for User Space
Memory safety, performance comparable to C, strong type system. All 6 userland crates are pure Rust.

### 3. Landlock + seccomp-bpf
Combine unprivileged filesystem sandboxing (Landlock) with syscall filtering (seccomp-bpf). Both upstream in kernel, complementary mechanisms.

### 4. Local-First AI
Prioritize local LLM execution with cloud fallback. Privacy, offline capability, reduced latency.

### 5. Cryptographic Audit Chain
Immutable, hash-chained, signed audit logs. Tamper detection, forensic analysis, compliance.

### 6. Named Subsystems
Major cross-cutting concerns get memorable Greek-inspired names (ark, nous, aegis, sigil) for clear identity, discoverability, and user-facing branding. Internal module names match the subsystem names.

### 7. Distro-Agnostic Base
AGNOS value is in the security model, agent runtime, and LLM gateway — not the base distro. The base is packaging, not architecture. Currently Debian for pragmatic ML compatibility; swappable if a better option emerges.

---

## Related Documentation

- [Development Roadmap](development/roadmap.md)
- [API Explorer](api/explorer.html)
- [Security Guide](security/security-guide.md)
- [ADR Index](adr/README.md)
