# AGNOS Development Roadmap

> **Status**: Pre-Alpha | **Last Updated**: 2026-02-11

This document tracks the development roadmap for AGNOS (AI-Native General Operating System). Tasks are organized by phase, with each phase representing a major milestone toward MVP.

## Phase Overview

| Phase | Name | Status | Goal |
|-------|------|--------|------|
| **0** | Foundation | 🔄 In Progress | Project setup and tooling |
| **1** | Core OS | 📋 Planned | Bootable hardened Linux base |
| **2** | AI Shell | 📋 Planned | Natural language CLI |
| **3** | Agent Runtime | 📋 Planned | Multi-agent orchestration |
| **4** | Desktop | 📋 Planned | GUI with AI integration |
| **5** | Production | 📋 Planned | Security audits and release |

---

## Phase 0: Foundation 🔄

**Goal**: Establish project infrastructure, build system, and documentation

**Duration**: 4-6 weeks

### Infrastructure
- [x] Create project repository structure
- [x] Write initial README.md with project vision
- [x] Write TODO.md with phased development plan
- [x] Create CONTRIBUTING.md with git best practices
- [x] Create SECURITY.md with vulnerability disclosure policy
- [ ] Set up CI/CD pipeline (GitHub Actions)
  - [ ] Build verification on PR
  - [ ] Security scanning (bandit, trivy, semgrep)
  - [ ] License compliance check
  - [ ] Code quality gates (linting, formatting)
- [ ] Create development container/Dockerfile
- [ ] Set up package signing infrastructure

### Build System
- [ ] Create Makefile with standard targets
  - [ ] `make deps` — Install build dependencies
  - [ ] `make build` — Build kernel and userland
  - [ ] `make iso` — Create bootable ISO
  - [ ] `make install` — Install to target device
  - [ ] `make test` — Run test suite
  - [ ] `make clean` — Clean build artifacts
- [ ] Create `scripts/` directory with helper scripts
  - [ ] `scripts/install-build-deps.sh` — Install build dependencies
  - [ ] `scripts/build-kernel.sh` — Build hardened kernel
  - [ ] `scripts/build-initramfs.sh` — Build initramfs
  - [ ] `scripts/create-iso.sh` — Create bootable ISO
  - [ ] `scripts/run-tests.sh` — Execute test suite

### Documentation
- [x] README.md — Project overview and quick start
- [x] TODO.md — This roadmap document
- [ ] ARCHITECTURE.md — Detailed system architecture
- [ ] BUILD.md — Build instructions and toolchain
- [ ] PHASES.md — Detailed phase planning
- [ ] AGENTS.md — Agent system design
- [ ] API.md — Kernel and user-space APIs
- [ ] Create `docs/` directory structure
  - [ ] `docs/security/` — Security documentation
  - [ ] `docs/development/` — Developer guides
  - [ ] `docs/user/` — User documentation
  - [ ] `docs/api/` — API reference

### Toolchain Setup
- [ ] Define base toolchain requirements
  - [ ] GCC/Clang version requirements
  - [ ] Rust toolchain for agent runtime
  - [ ] Python for build scripts
  - [ ] Go for system utilities
- [ ] Create toolchain container image
- [ ] Set up cross-compilation support
  - [ ] x86_64
  - [ ] ARM64 (Raspberry Pi, Apple Silicon)
  - [ ] RISC-V (experimental)

### Package Management
- [ ] Design package format (agpkg)
- [ ] Create package build system
- [ ] Set up package repository structure
- [ ] Implement package signing

---

## Phase 1: Core OS 📋

**Goal**: Create a bootable, hardened Linux base system

**Duration**: 8-10 weeks

### Kernel Hardening
- [ ] Select Linux 6.6 LTS as base
- [ ] Apply security patches
  - [ ] grsecurity (if available) or mainline hardening
  - [ ] Kernel Self Protection Project (KSPP) configs
  - [ ] Sign all kernel modules
- [ ] Configure kernel with security-focused options
  - [ ] Disable unnecessary drivers and features
  - [ ] Enable kernel lockdown mode
  - [ ] Enable memory safety features (KASLR, KPTI, etc.)
  - [ ] Enable Landlock LSM
  - [ ] Configure seccomp-bpf
- [ ] Build and test kernel
- [ ] Create kernel package

### Init System
- [ ] Extend systemd with AGNOS features
- [ ] Create AGNOS-specific targets
  - [ ] `agnos-cli.target` — Command-line only
  - [ ] `agnos-desktop.target` — Full desktop
  - [ ] `agnos-recovery.target` — Recovery mode
- [ ] Implement early boot security
  - [ ] Secure boot support
  - [ ] TPM integration
  - [ ] Disk encryption (LUKS)

### Base Userland
- [ ] Create minimal base system
  - [ ] Coreutils replacement or hardening
  - [ ] Shell (bash + agnsh)
  - [ ] Essential utilities
- [ ] Implement filesystem layout
  - [ ] `/agnos/` — AGNOS-specific directories
  - [ ] `/agnos/agents/` — Agent data and configs
  - [ ] `/agnos/audit/` — Audit logs
  - [ ] `/agnos/models/` — Local LLM models
- [ ] Set up user management
  - [ ] Root account hardening
  - [ ] User account creation
  - [ ] Role-based access control

### Boot Process
- [ ] Create bootloader configuration
  - [ ] systemd-boot or GRUB2
  - [ ] Encrypted boot partition option
  - [ ] Recovery boot entries
- [ ] Build initramfs
  - [ ] Early userspace initialization
  - [ ] Disk unlock prompt
  - [ ] Integrity verification
- [ ] Create installation scripts
  - [ ] Disk partitioning
  - [ ] Filesystem creation (ext4/btrfs)
  - [ ] System installation

### Package Repository
- [ ] Set up package build farm
- [ ] Build essential packages
  - [ ] kernel-agnos
  - [ ] systemd-agnos
  - [ ] base-system
  - [ ] security-tools
- [ ] Create package index
- [ ] Set up repository hosting

### Testing
- [ ] Create VM test harness
  - [ ] QEMU/KVM automation
  - [ ] Boot testing
  - [ ] Integration tests
- [ ] Hardware compatibility tests
  - [ ] Common laptop models
  - [ ] Desktop configurations
  - [ ] Virtual machines

---

## Phase 2: AI Shell 📋

**Goal**: Implement natural language command interface

**Duration**: 6-8 weeks

### Core Shell (agnsh)
- [ ] Design shell architecture
- [ ] Implement parser
  - [ ] Natural language understanding
  - [ ] Intent classification
  - [ ] Command mapping
- [ ] Build command translator
  - [ ] NL to bash translation
  - [ ] Context awareness
  - [ ] Error handling
- [ ] Create bash compatibility layer
  - [ ] Full bash syntax support
  - [ ] Script execution
  - [ ] Environment management

### LLM Integration
- [ ] Create LLM Gateway service
  - [ ] Local model support (Ollama, llama.cpp)
  - [ ] Cloud API support (OpenAI, Anthropic)
  - [ ] Hybrid mode with automatic routing
- [ ] Implement model management
  - [ ] Download and cache models
  - [ ] Model versioning
  - [ ] Resource allocation
- [ ] Build prompt engineering system
  - [ ] System prompt templates
  - [ ] Context management
  - [ ] Response streaming

### Shell Features
- [ ] Implement command suggestions
- [ ] Add command history with context
- [ ] Create help system
  - [ ] Natural language help
  - [ ] Command reference
  - [ ] Examples
- [ ] Build configuration system
  - [ ] User preferences
  - [ ] Shell customization
  - [ ] Theme support

### System Integration
- [ ] Integrate with systemd
- [ ] Add session management
- [ ] Implement TTY switching
- [ ] Create login shell

---

## Phase 3: Agent Runtime 📋

**Goal**: Build multi-agent orchestration system

**Duration**: 10-12 weeks

### Agent Kernel Module
- [ ] Design kernel interface
- [ ] Implement agent process type
  - [ ] Specialized process attributes
  - [ ] Resource quotas
  - [ ] Capability restrictions
- [ ] Build IPC mechanisms
  - [ ] Agent-to-agent messaging
  - [ ] Agent-to-kernel communication
  - [ ] Secure shared memory
- [ ] Create resource scheduler
  - [ ] GPU allocation
  - [ ] Memory limits
  - [ ] CPU prioritization

### Agent Runtime Daemon
- [ ] Build agent kernel daemon (akd)
- [ ] Implement agent lifecycle
  - [ ] Creation
  - [ ] Suspension/Resumption
  - [ ] Termination
  - [ ] Migration
- [ ] Create agent configuration system
  - [ ] YAML/JSON config files
  - [ ] Dynamic reconfiguration
  - [ ] Template system
- [ ] Build agent monitoring
  - [ ] Health checks
  - [ ] Resource usage
  - [ ] Performance metrics

### Multi-Agent Orchestrator
- [ ] Design orchestration architecture
  - [ ] Central vs. distributed
  - [ ] Consensus mechanisms
  - [ ] Conflict resolution
- [ ] Implement agent registry
  - [ ] Discovery
  - [ ] Capabilities advertisement
  - [ ] Status tracking
- [ ] Build task distribution
  - [ ] Workload balancing
  - [ ] Priority management
  - [ ] Dependency resolution
- [ ] Create agent communication bus
  - [ ] Message routing
  - [ ] Pub/sub system
  - [ ] Request/reply patterns

### Security & Isolation
- [ ] Implement sandboxing
  - [ ] Landlock integration
  - [ ] seccomp-bpf policies
  - [ ] Namespace isolation
- [ ] Build capability system
  - [ ] Fine-grained permissions
  - [ ] Capability delegation
  - [ ] Revocation
- [ ] Create agent attestation
  - [ ] Code signing
  - [ ] Runtime verification
  - [ ] Supply chain validation

### LLM Gateway Service
- [ ] Extend LLM gateway for agents
- [ ] Implement model sharing
  - [ ] Multi-agent model access
  - [ ] Context isolation
  - [ ] Token accounting
- [ ] Build fallback chains
- [ ] Add cost tracking (for cloud APIs)

### Agent SDK
- [ ] Create agent development kit
  - [ ] Rust SDK
  - [ ] Python SDK
  - [ ] JavaScript/TypeScript SDK
- [ ] Build standard agent templates
  - [ ] File manager agent
  - [ ] Code assistant agent
  - [ ] System monitoring agent
- [ ] Create agent marketplace structure

---

## Phase 4: Desktop Environment 📋

**Goal**: Build AI-augmented graphical interface

**Duration**: 8-10 weeks

### Wayland Compositor
- [ ] Select or build compositor base
  - [ ] Option A: Extend wlroots-based compositor
  - [ ] Option B: Build custom
- [ ] Implement AGNOS-specific features
  - [ ] Agent-aware window management
  - [ ] Contextual workspace switching
  - [ ] AI-augmented compositor
- [ ] Add security features
  - [ ] Screen lock with biometrics
  - [ ] Secure clipboard
  - [ ] Screenshot/access control

### Desktop Shell
- [ ] Build panel/top bar
  - [ ] System status
  - [ ] Agent status indicators
  - [ ] Quick settings
- [ ] Create application launcher
  - [ ] Traditional app menu
  - [ ] Natural language search
  - [ ] AI-powered suggestions
- [ ] Implement notification system
  - [ ] Agent notifications
  - [ ] Human override requests
  - [ ] Security alerts

### AI Desktop Features
- [ ] Build contextual workspace
  - [ ] Project-based workspaces
  - [ ] Automatic context detection
  - [ ] Window grouping by task
- [ ] Implement ambient intelligence
  - [ ] Proactive suggestions
  - [ ] Smart window placement
  - [ ] Focus assistance
- [ ] Create agent visualization
  - [ ] Running agents HUD
  - [ ] Agent interaction interface
  - [ ] Real-time activity monitoring

### Applications
- [ ] Adapt essential applications
  - [ ] Terminal with AI integration
  - [ ] File manager with agent assistance
  - [ ] Text editor (VSCode/Cursor integration)
  - [ ] Web browser (Firefox/Brave with AI extensions)
- [ ] Build AGNOS-specific apps
  - [ ] Agent manager GUI
  - [ ] Audit log viewer
  - [ ] System configuration
  - [ ] Model manager

### Security UI
- [ ] Create security dashboard
  - [ ] Real-time threat monitoring
  - [ ] Agent permission manager
  - [ ] Audit log visualization
- [ ] Build human override interface
  - [ ] Action approval dialogs
  - [ ] Emergency kill switch
  - [ ] Privilege escalation prompts

---

## Phase 5: Production 📋

**Goal**: Security audits, certifications, and release

**Duration**: 6-8 weeks

### Security Hardening
- [ ] Conduct security audit
  - [ ] Code review
  - [ ] Penetration testing
  - [ ] Fuzzing
- [ ] Implement findings
- [ ] Create security hardening guide
- [ ] Build automated security scanning

### Certifications
- [ ] Pursue security certifications
  - [ ] FIPS 140-2 (if applicable)
  - [ ] Common Criteria (target EAL4+)
  - [ ] CIS benchmarks compliance
- [ ] Create compliance documentation
- [ ] Build compliance checking tools

### Documentation
- [ ] Complete user documentation
  - [ ] Installation guide
  - [ ] User manual
  - [ ] Troubleshooting guide
- [ ] Complete developer documentation
  - [ ] Kernel development guide
  - [ ] Agent development guide
  - [ ] API documentation
- [ ] Create video tutorials
- [ ] Build example configurations

### Release Infrastructure
- [ ] Set up release automation
- [ ] Create update system
  - [ ] Delta updates
  - [ ] Rollback capability
  - [ ] Signed updates
- [ ] Build telemetry (opt-in)
  - [ ] Crash reporting
  - [ ] Usage statistics
  - [ ] Performance metrics
- [ ] Create support infrastructure
  - [ ] Issue tracking
  - [ ] Community forums
  - [ ] Commercial support (optional)

### Enterprise Features
- [ ] Centralized management
  - [ ] Fleet management
  - [ ] Policy enforcement
  - [ ] Remote monitoring
- [ ] Integration features
  - [ ] Active Directory/LDAP
  - [ ] SIEM integration
  - [ ] Audit log forwarding

---

## Backlog / Future Phases

### Phase 6: Advanced AI
- [ ] Hardware-accelerated inference
  - [ ] NPU support
  - [ ] GPU optimization
  - [ ] Custom silicon (long-term)
- [ ] Distributed agent computing
- [ ] Swarm intelligence protocols
- [ ] Advanced agent learning

### Phase 7: Ecosystem
- [ ] Third-party agent marketplace
- [ ] Plugin architecture
- [ ] Integration marketplace
- [ ] Cloud services (optional)

### Phase 8: Research
- [ ] Formal verification of critical components
- [ ] Novel security architectures
- [ ] AI safety mechanisms
- [ ] Human-AI collaboration research

---

## Sprint Planning

### Sprint Structure
- **Duration**: 2 weeks
- **Review**: Friday of week 2
- **Retrospective**: Monday after sprint
- **Planning**: Tuesday after retrospective

### Current Sprint: Sprint 0

**Dates**: 2026-02-11 to 2026-02-25

**Goals**:
1. Complete Phase 0 documentation
2. Set up CI/CD pipeline
3. Create initial build system scaffolding
4. Design package format

**Tasks**:
- [x] Create README.md
- [x] Create TODO.md
- [x] Create CONTRIBUTING.md
- [x] Create SECURITY.md
- [ ] Set up GitHub Actions
- [ ] Create Makefile
- [ ] Design agpkg format
- [ ] Create docs/ structure

---

## Metrics & Success Criteria

### Phase Completion Criteria

Each phase must meet these criteria before advancing:

1. **Functionality**: All major features implemented and tested
2. **Documentation**: User and developer docs complete
3. **Tests**: >80% code coverage, all integration tests passing
4. **Security**: Security review completed, no critical vulnerabilities
5. **Performance**: Meets defined benchmarks
6. **Stability**: <1% crash rate in testing

### Key Performance Indicators

| Metric | Target |
|--------|--------|
| Boot time | <10 seconds to login |
| Agent spawn time | <500ms |
| Shell response time | <100ms |
| Memory overhead | <2GB for base system |
| Update size | <100MB for minor updates |

---

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

To pick up a task:
1. Check the [GitHub Issues](https://github.com/agnostos/agnos/issues)
2. Comment on an issue to claim it
3. Create a feature branch
4. Submit a PR referencing the issue

---

*Last Updated: 2026-02-11*
