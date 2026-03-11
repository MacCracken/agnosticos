# AGNOS — AI-Native General Operating System

> **A**rtificial **G**eneral **N**etwork **O**perating **S**ystem

[![License](https://img.shields.io/badge/license-GPLv3-blue)](LICENSE)
[![Kernel](https://img.shields.io/badge/kernel-Linux%206.6%20LTS-orange)](https://kernel.org)
[![Security](https://img.shields.io/badge/security-hardened-red)](docs/security/security-guide.md)
[![Status](https://img.shields.io/badge/status-pre--beta-yellow)](docs/development/roadmap.md)

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

### Agent-Native Kernel Extensions
- **Agent Kernel Module** — Low-level support for agent lifecycle management
- **LLM System Calls** — Native kernel interfaces for model inference
- **Context Switching** — Hardware-accelerated context management for agents
- **Memory Pools** — Isolated memory regions per agent with automatic cleanup

### Security & Auditing
- **Landlock + seccomp-bpf** — Mandatory sandboxing for all agent processes
- **Cryptographic Audit Chain** — Immutable, signed logs of all agent actions
- **RBAC at Kernel Level** — Fine-grained permissions for agent capabilities
- **Supply Chain Security** — Reproducible builds, signed packages, SBOM generation

### Command-First Interface
- **AI Shell (agnsh)** — Natural language command interface with full bash compatibility
- **Agent CLI** — Direct control and monitoring of running agents
- **Audit TTY** — Real-time security event monitoring

### Desktop Environment (Phase 2)
- **Wayland-based compositor** with AI-augmented window management
- **Contextual Workspace** — Workspaces that follow task context, not just applications
- **Ambient Intelligence** — Proactive assistance based on current activity

### Multi-Agent Support
- **Agent Kernel** — Orchestrates multiple agents with conflict resolution
- **Message Bus** — Secure, encrypted inter-agent communication with agent name routing
- **Resource Scheduler** — Fair allocation of GPU/CPU/memory between agents
- **Human-in-the-Loop** — Automatic escalation for sensitive operations

### Networking Toolkit (Phase 6)
Inspired by Kali Linux, AGNOS ships a curated networking toolkit pre-configured for agent-driven analysis. All tool invocations are sandboxed and recorded in the cryptographic audit chain:
- **Reconnaissance** — `nmap`, `masscan`, `netdiscover`, `p0f`
- **Traffic analysis** — `tcpdump`, `wireshark`/`tshark`, `bettercap`, `termshark`
- **Utilities** — `netcat`, `socat`, `mtr`, `iperf3`, `nethogs`
- **DNS** — `dig`, `dnsx`, `dnsrecon`
- **Web layer** — `nikto`, `ffuf`, `gobuster`, `nuclei`
- **Agent integration** — natural language queries like `"scan 192.168.1.0/24 for open ports"`

### LLM Gateway HTTP API
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

# For large LLM workloads, increase the virtual memory limit (default 8GB)
docker run -it --privileged \
  -e AGNOS_ULIMIT_VMEM=16777216 \
  agnostos/agnos:latest

# Or disable the virtual memory limit entirely
docker run -it --privileged \
  -e AGNOS_ULIMIT_VMEM=unlimited \
  agnostos/agnos:latest
```

**Container environment variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `AGNOS_ULIMIT_VMEM` | `8388608` (8GB) | Virtual memory limit in KB (`unlimited` to disable) |
| `AGNOS_ULIMIT_NOFILE` | `4096` | Max open file descriptors |
| `AGNOS_ULIMIT_NPROC` | `256` | Max user processes |
| `AGNOS_LOG_FORMAT` | `json` | Log format (`json` or `text`) |
| `AGNOS_RUNTIME_BIND` | `0.0.0.0` | daimon bind address |
| `AGNOS_GATEWAY_BIND` | `0.0.0.0` | hoosh bind address |
| `RUST_LOG` | `info` | Log level (`debug`, `info`, `warn`, `error`) |

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
AGNOS Userland (17 named subsystems)
├── argonaut — init system (boot sequencing, service management)
├── daimon — agent orchestrator (port 8090)
│   ├── Multi-agent IPC, sandbox, registry
│   ├── Federation, migration, scheduling
│   ├── mela — agent marketplace
│   ├── sigil — trust verification
│   └── aegis — security daemon
├── hoosh — LLM gateway (port 8088, OpenAI-compatible)
├── agnoshi (agnsh) — AI shell
│   ├── Natural language parser + intent classifier
│   └── Approval workflow, aliases, dashboard
├── aethersafha — Wayland compositor + AI features
│   └── Plugin host, XWayland, accessibility
├── ark + nous — package management
├── takumi — package build system (.ark format)
├── agnova — OS installer (4 install modes)
├── agnosys — kernel interface (Landlock, seccomp, LUKS, TPM)
├── agnostik — shared types library
└── shakti — privilege escalation
```

## Development Status

AGNOS is currently in **pre-beta** development. See [docs/development/roadmap.md](docs/development/roadmap.md) for the full roadmap and detailed phase breakdown.

### Current Status: Phases 0-14 complete. 8997+ tests, ~84% coverage, 0 warnings. Beta targeting Q4 2026.

- [x] **Phase 1-4**: Architecture, build system, CI/CD pipeline
- [x] **Phase 5**: Agent Runtime (daimon), AI Shell (agnoshi), LLM Gateway (hoosh), Desktop (aethersafha)
- [x] **Phase 6**: Advanced AI — NPU/GPU acceleration, swarm intelligence, networking toolkit, RAG pipeline, observability, anomaly detection
- [x] **Phase 7**: Ecosystem — Federation (55 tests), distributed scheduling (47 tests), agent migration (54 tests), ratings/reviews (43 tests)
- [x] **Phase 8A-8F**: Distribution — Sigil trust (35 tests), Takumi build (43 tests), Argonaut init (46 tests), Agnova installer (41 tests), Aegis security daemon (40 tests)
- [x] **Phase 8G-8M**: Research — Post-quantum crypto (68 tests), explainability (59 tests), AI safety (77 tests), fine-tuning (73 tests), formal verification (76 tests), novel sandboxing (77 tests), RL optimization (68 tests)
- [x] **Phase 9**: Cloud services (82 tests), human-AI collaboration (87 tests)
- [x] **Phase 10-12**: Platform hardening, integration, and stabilization
- [x] **Phase 13**: Beta polish (infrastructure, hardware recipes, community prep)
- [x] **Phase 14**: Edge OS Profile (fleet management, edge boot mode)
- [x] Test coverage: ~84% (8997+ tests, 0 failures)
- [x] Performance benchmarks — criterion suites for all major components
- [x] Security — 5 CVEs fixed, all CI/CD workflows operational
- [ ] **Bootable ISO** (Phase 13A — self-hosting validation)
- [ ] **Community/docs** (Phase 13C — external documentation and onboarding)
- [ ] **Third-party security audit** (vendor selection in progress)

## Documentation

| Document | Description |
|----------|-------------|
| [docs/development/roadmap.md](docs/development/roadmap.md) | Development roadmap and MVP tasks |
| [architecture.md](docs/architecture.md) | System architecture |
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
