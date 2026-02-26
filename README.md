# AGNOS — AI-Native General Operating System

> **A**rtificial **G**eneral **N**etwork **O**perating **S**ystem

[![License](https://img.shields.io/badge/license-GPLv3-blue)](LICENSE)
[![Kernel](https://img.shields.io/badge/kernel-Linux%206.6%20LTS-orange)](https://kernel.org)
[![Security](https://img.shields.io/badge/security-hardened-red)](docs/security/security-guide.md)
[![Status](https://img.shields.io/badge/status-pre--alpha-yellow)](docs/development/roadmap.md)

**AGNOS** is a Linux-based operating system designed from the ground up for AI agents and human-AI collaboration. Built with security-first principles inspired by Arch Linux and enterprise hardened systems, AGNOS provides a sovereign computing environment where AI agents can operate autonomously while remaining fully auditable and controllable by human operators.

## Core Philosophy

1. **Agent-First Architecture** — The OS treats AI agents as first-class citizens, not applications
2. **Security by Design** — Defense in depth with mandatory access controls, sandboxing, and cryptographic audit trails
3. **Human Sovereignty** — Humans maintain ultimate control through comprehensive observability and override mechanisms
4. **Multi-Agent Native** — Built to support single agents, agent teams, and swarm intelligence
5. **Privacy Preserving** — Local-first AI with optional secure cloud offloading

## Quick Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      AGNOS Architecture                         │
├─────────────────────────────────────────────────────────────────┤
│  User Space          │  Agent Runtime        │  Kernel Space    │
│  ┌─────────────────┐ │  ┌─────────────────┐  │  ┌─────────────┐ │
│  │ Desktop (Wayland│ │  │ Agent Kernel    │  │  │ hardened    │ │
│  │ + AI Shell)     │ │  │ ┌─────────────┐ │  │  │ Linux 6.6   │ │
│  ├─────────────────┤ │  │ │ Multi-Agent │ │  │  ├─────────────┤ │
│  │ Applications    │ │  │ │ Orchestrator│ │  │  │ AGNOS       │ │
│  │ ├─ Browser      │ │  │ ├─────────────┤ │  │  │ Security    │ │
│  │ ├─ IDE/Cursor   │ │  │ │ Agent 1     │ │  │  │ Modules     │ │
│  │ └─ Tools        │ │  │ │ Agent 2     │ │  │  ├─────────────┤ │
│  ├─────────────────┤ │  │ │ Agent N...  │ │  │  │ Landlock    │ │
│  │ Human Interface │ │  │ └─────────────┘ │  │  │ seccomp-bpf │ │
│  │ ┌─────────────┐ │ │  │ ┌─────────────┐ │  │  │ Namespaces  │ │
│  │ │ Audit HUD   │ │ │  │ │ LLM Gateway │ │  │  │ eBPF        │ │
│  │ │ Override    │ │ │  │ │ (Local/Cloud│ │  │  │ cgroup v2   │ │
│  │ │ Controls    │ │ │  │ │ /Hybrid)    │ │  │  └─────────────┘ │
│  │ └─────────────┘ │ │  │ └─────────────┘ │  │                  │
│  └─────────────────┘ │  └─────────────────┘  │                  │
└─────────────────────────────────────────────────────────────────┘
```

## Key Features

### 🧠 Agent-Native Kernel Extensions
- **Agent Kernel Module** — Low-level support for agent lifecycle management
- **LLM System Calls** — Native kernel interfaces for model inference
- **Context Switching** — Hardware-accelerated context management for agents
- **Memory Pools** — Isolated memory regions per agent with automatic cleanup

### 🔒 Security & Auditing
- **Landlock + seccomp-bpf** — Mandatory sandboxing for all agent processes
- **Cryptographic Audit Chain** — Immutable, signed logs of all agent actions
- **RBAC at Kernel Level** — Fine-grained permissions for agent capabilities
- **Supply Chain Security** — Reproducible builds, signed packages, SBOM generation

### 🖥️ Command-First Interface
- **AI Shell (agnsh)** — Natural language command interface with full bash compatibility
- **Agent CLI** — Direct control and monitoring of running agents
- **Audit TTY** — Real-time security event monitoring

### 🖼️ Desktop Environment (Phase 2)
- **Wayland-based compositor** with AI-augmented window management
- **Contextual Workspace** — Workspaces that follow task context, not just applications
- **Ambient Intelligence** — Proactive assistance based on current activity

### 🤖 Multi-Agent Support
- **Agent Kernel** — Orchestrates multiple agents with conflict resolution
- **Message Bus** — Secure, encrypted inter-agent communication with agent name routing
- **Resource Scheduler** — Fair allocation of GPU/CPU/memory between agents
- **Human-in-the-Loop** — Automatic escalation for sensitive operations

### 🌐 Networking Toolkit (Phase 6)
Inspired by Kali Linux, AGNOS ships a curated networking toolkit pre-configured for agent-driven analysis. All tool invocations are sandboxed and recorded in the cryptographic audit chain:
- **Reconnaissance** — `nmap`, `masscan`, `netdiscover`, `p0f`
- **Traffic analysis** — `tcpdump`, `wireshark`/`tshark`, `bettercap`, `termshark`
- **Utilities** — `netcat`, `socat`, `mtr`, `iperf3`, `nethogs`
- **DNS** — `dig`, `dnsx`, `dnsrecon`
- **Web layer** — `nikto`, `ffuf`, `gobuster`, `nuclei`
- **Agent integration** — natural language queries like `"scan 192.168.1.0/24 for open ports"`

### 🌐 LLM Gateway HTTP API
- **OpenAI-compatible API** on port 8088 — Drop-in replacement for OpenAI API
- **Multiple provider support** — Local models (Ollama, llama.cpp) + cloud providers
- **Request routing** — Route through agents with `X-Agent-Id` headers
- **Health monitoring** — `/v1/health` endpoint for service discovery

## System Requirements

### Minimum (CLI Mode)
- **CPU**: x86_64 with virtualization support (Intel VT-x / AMD-V)
- **RAM**: 4GB (8GB recommended for local LLMs)
- **Storage**: 20GB SSD
- **Network**: Internet connection for package updates

### Recommended (Desktop + Local LLMs)
- **CPU**: 8+ cores with AVX-512 support
- **GPU**: NVIDIA RTX 4090 / AMD RX 7900 XTX or better
- **RAM**: 32GB+ DDR5
- **Storage**: 100GB NVMe SSD
- **TPM**: 2.0 for secure boot and disk encryption

## Installation

### Method 1: AGNOS Installer (Recommended)

```bash
# Download the latest ISO from releases
curl -LO https://github.com/agnostos/agnos/releases/latest/download/agnos-$(uname -m).iso

# Flash to USB (replace /dev/sdX with your USB device)
sudo dd if=agnos-x86_64.iso of=/dev/sdX bs=4M status=progress

# Boot from USB and follow the installation wizard
# The installer will guide you through disk partitioning,
# encryption setup, and initial agent configuration
```

### Method 2: Build from Source

```bash
# Clone the repository
git clone https://github.com/agnostos/agnos.git
cd agnos

# Install build dependencies
sudo ./scripts/install-build-deps.sh

# Build the system (this will take several hours)
make build

# Create bootable ISO
make iso

# Or install directly to a target disk
make install TARGET=/dev/nvme0n1
```

### Method 3: Container/Docker (Development)

```bash
# Run AGNOS in a container for development
docker run -it --privileged \
  --gpus all \
  -v /dev:/dev \
  -v agnos-data:/var/lib/agnos \
  agnostos/agnos:latest
```

## Quick Start

After installation, AGNOS boots into the AI Shell by default.

```bash
# The AI Shell understands natural language
agnsh> show me system status
agnsh> create a new agent called "code-assistant" with python expertise
agnsh> what agents are currently running?

# Traditional bash is always available
agnsh> bash
[user@agnos ~]$ uname -a
Linux agnos 6.6.0-agnos #1 SMP AGNOS Kernel

# Return to AI Shell
[user@agnos ~]$ exit

# Configure your first agent
agnsh> configure agent

# View security audit log
agnsh> show audit log --last-hour

# Get help
agnsh> help
agnsh> help agents
agnsh> help security
```

## Architecture

### Kernel Space

```
Linux 6.6 LTS (Hardened)
├── AGNOS Security Module
│   ├── Landlock integration
│   ├── seccomp-bpf filters
│   └── Capability namespaces
├── Agent Kernel Subsystem
│   ├── Process isolation
│   ├── Resource quotas
│   └── IPC management
├── LLM Kernel Module
│   ├── Inference acceleration
│   ├── Model memory mapping
│   └── Token streaming
└── Audit Kernel Module
    ├── Event capture
    ├── Chain hashing
    └── Log integrity
```

### User Space

```
AGNOS Userland
├── init system (systemd + AGNOS extensions)
├── Agent Runtime Environment
│   ├── Agent Kernel Daemon
│   ├── LLM Gateway Service
│   ├── Message Bus (D-Bus + custom)
│   └── Resource Scheduler
├── AI Shell (agnsh)
│   ├── Natural language parser
│   ├── Intent classifier
│   └── Command translator
├── Desktop Environment (Phase 2)
│   └── Wayland compositor + AI layer
└── Package Manager (agpkg)
    └── AGNOS-specific packages
```

## Development Status

AGNOS is currently in **pre-alpha** development. See [docs/development/roadmap.md](docs/development/roadmap.md) for the full roadmap and detailed phase breakdown.

### Current Phase: Production Hardening (Phase 5 — 85% Complete)

The core system is implemented. We are now focused on testing, security audits, and release preparation for the **Alpha release targeting Q2 2026**.

- [x] Project scaffolding, architecture, and full documentation
- [x] Build system, toolchain, and CI/CD pipeline
- [x] Agent Runtime — multi-agent orchestration with IPC and sandboxing
- [x] AI Shell (`agnsh`) — natural language command interface
- [x] LLM Gateway — OpenAI-compatible HTTP API (port 8088)
- [x] Desktop Environment — Wayland compositor + AI features
- [x] Security & Compliance — Landlock, seccomp, CIS benchmarks, SBOM
- [x] Release infrastructure — package signing, delta updates, telemetry
- [ ] Test coverage: 60% → 80% (P0 blocker)
- [ ] Integration tests: agent-orchestrator scenarios
- [ ] Third-party security audit
- [ ] Performance benchmarks established

### Planned Phases
- **Phase 6**: Advanced AI — NPU/GPU acceleration, swarm intelligence, networking toolkit
- **Phase 7**: Ecosystem — Agent marketplace, cloud services
- **Phase 8**: Research — Formal verification, novel sandboxing, AI safety

## Documentation

| Document | Description |
|----------|-------------|
| [docs/development/roadmap.md](docs/development/roadmap.md) | Development roadmap and MVP tasks |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | System architecture |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution guidelines |
| [SECURITY.md](SECURITY.md) | Security policies |
| [docs/adr/](docs/adr/) | Architecture Decision Records |
| [docs/api/](docs/api/) | API reference |

## Security

Security is our highest priority. AGNOS implements:

- **Kernel-level MAC** with Landlock and SELinux policies
- **Process isolation** via namespaces and cgroups v2
- **Cryptographic verification** of all system components
- **Supply chain security** with reproducible builds and SBOM generation
- **Comprehensive audit logging** with integrity verification
- **Fuzzing infrastructure** for automated security testing
- **CIS benchmark compliance** validation

### Package Security
- **GPG-signed packages** — All release packages are signed with release keys
- **Delta updates** — Efficient updates with automatic rollback capability
- **SBOM generation** — SPDX and CycloneDX formats for supply chain transparency

See [SECURITY.md](SECURITY.md) for details on reporting vulnerabilities and our security model.

## Community

- **Matrix**: #agnos:matrix.org
- **Discord**: [discord.gg/agnos](https://discord.gg/agnos)
- **Forum**: [discourse.agnos.io](https://discourse.agnos.io)
- **Mastodon**: [@agnos@fosstodon.org](https://fosstodon.org/@agnos)

## Contributing

We welcome contributions! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development environment setup
- Code style and testing requirements
- Git workflow and commit conventions
- Pull request process

## License

AGNOS is licensed under the **GNU General Public License v3.0** (GPLv3).

This ensures that AGNOS and any derivative works remain free and open source, protecting user freedom and preventing proprietary lock-in of AI systems.

See [LICENSE](LICENSE) for full terms.

## Acknowledgments

AGNOS builds upon the work of countless open source projects:

- **Linux Kernel** — The foundation of modern computing
- **Arch Linux** — Inspiration for simplicity and user-centricity
- **NixOS** — Ideas for reproducible system configuration
- **Qubes OS** — Security architecture inspiration
- **Container technologies** — Docker, Podman, systemd-nspawn

Special thanks to the AI safety and open source communities for their guidance on building responsible AI systems.

---

<div align="center">

**AGNOS** — The Operating System for the Age of AI

*Built for agents. Controlled by humans.*

</div>
