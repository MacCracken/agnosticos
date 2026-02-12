# Getting Started with AGNOS Development

Welcome to AGNOS development! This guide will help you get started with building and contributing to the AI-Native General Operating System.

## Prerequisites

Before you begin, ensure you have:

- **OS**: Linux (Ubuntu 24.04+, Fedora 40+, or Arch Linux recommended)
- **Disk Space**: At least 50GB free
- **RAM**: 8GB minimum (16GB recommended)
- **CPU**: x86_64 with virtualization support
- **Git**: Version 2.30 or later
- **Docker**: Optional but recommended (20.10+)

## Quick Start

### 1. Clone the Repository

```bash
git clone https://github.com/agnostos/agnos.git
cd agnos
```

### 2. Set Up Development Environment

Choose one of the following methods:

#### Option A: Native Development

```bash
# Install build dependencies (requires sudo)
./scripts/install-build-deps.sh

# Verify environment
make check
```

#### Option B: Docker Development (Recommended)

```bash
# Build and enter development container
make docker-dev

# Inside container, you have all tools pre-installed
root@agnos-dev:/workspace# make build
```

### 3. Build AGNOS

```bash
# Full build (kernel + userland)
make build

# Or build components separately
make build-kernel      # Build Linux kernel
make build-userland    # Build Rust userland
make build-initramfs   # Build initial ramdisk

# Create bootable ISO
make iso
```

### 4. Run Tests

```bash
# Run all tests
make test

# Run specific test suites
make test-unit         # Unit tests
make test-integration  # Integration tests
make test-security     # Security tests

# Run tests with coverage
make test-coverage
```

### 5. Code Quality

```bash
# Format code
make format

# Run linters
make lint

# Run security scans
make security-scan
```

## Project Structure

```
agnos/
├── .github/              # GitHub configuration
│   ├── workflows/        # CI/CD workflows
│   ├── ISSUE_TEMPLATE/   # Issue templates
│   └── pull_request_template.md
├── config/               # System configuration
├── docs/                 # Documentation
│   ├── ARCHITECTURE.md   # System architecture
│   └── PHASES.md         # Development phases
├── kernel/               # Linux kernel source
├── scripts/              # Build and utility scripts
├── userland/             # User space components
│   ├── agent-runtime/    # Agent runtime daemon
│   ├── ai-shell/         # AI Shell (agnsh)
│   ├── desktop/          # Desktop environment
│   └── llm-gateway/      # LLM Gateway service
├── CHANGELOG.md          # Release notes
├── CODE_OF_CONDUCT.md    # Community guidelines
├── CONTRIBUTING.md       # Contribution guidelines
├── Dockerfile.dev        # Development container
├── LICENSE               # GPL v3.0 license
├── Makefile              # Build automation
├── README.md             # Project overview
├── SECURITY.md           # Security policies
└── TODO.md               # Development roadmap
```

## Development Workflow

### 1. Create a Feature Branch

```bash
git checkout develop
git pull upstream develop
git checkout -b feature/your-feature-name
```

### 2. Make Changes

- Write code following our [coding standards](../CONTRIBUTING.md#coding-standards)
- Add tests for new functionality
- Update documentation as needed

### 3. Commit Changes

We use [Conventional Commits](https://www.conventionalcommits.org/):

```bash
git add .
git commit -m "feat(kernel): add Landlock integration

This adds Landlock sandboxing support for agent processes.

Closes #123"
```

### 4. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub targeting the `develop` branch.

## Development Areas

### Kernel Development

Working on kernel modules:

```bash
cd kernel/
make menuconfig    # Configure kernel
make -j$(nproc)    # Build kernel
make modules_install
```

See [Kernel Development Guide](kernel-development.md) for details.

### Userland Development

Working on Rust components:

```bash
# Agent Runtime
cd userland/agent-runtime
cargo build
cargo test
cargo run

# AI Shell
cd userland/ai-shell
cargo build --release
./target/release/agnsh

# LLM Gateway
cd userland/llm-gateway
cargo run
```

### Documentation

Building documentation:

```bash
# Build API docs
make docs

# Serve locally
make docs-serve
```

## Testing in VM

### Using QEMU

```bash
# Build ISO first
make iso

# Run in QEMU
qemu-system-x86_64 \
  -m 4G \
  -smp 4 \
  -enable-kvm \
  -cdrom dist/agnos-0.1.0-alpha-x86_64.iso
```

### Using VirtualBox/VMware

1. Build ISO: `make iso`
2. Create new VM in VirtualBox
3. Attach ISO as boot device
4. Start VM

## Debugging

### Kernel Debugging

```bash
# Enable debug symbols
make kernel-debug

# Run with GDB
qemu-system-x86_64 ... -s -S
# In another terminal:
gdb vmlinux
(gdb) target remote :1234
```

### Userland Debugging

```bash
# Rust debugging
cd userland/agent-runtime
cargo build
gdb ./target/debug/agent-runtime-daemon

# Or use rust-gdb
cargo run -- gdb
```

## Common Issues

### Build Failures

```bash
# Clean and rebuild
make clean
make build

# Check dependencies
make check
```

### Permission Errors

```bash
# Ensure you're in docker group (for Docker builds)
sudo usermod -aG docker $USER
# Log out and back in
```

### Out of Disk Space

```bash
# Clean build artifacts
make clean

# Remove Docker images
docker system prune -a
```

## Contributing

1. **Read** [CONTRIBUTING.md](../CONTRIBUTING.md)
2. **Check** [TODO.md](../TODO.md) for open tasks
3. **Claim** an issue by commenting on it
4. **Follow** our git workflow
5. **Submit** a Pull Request

### Good First Issues

Look for issues labeled:
- `good first issue`
- `help wanted`
- `documentation`

## Getting Help

- **Documentation**: Check the `docs/` directory
- **Issues**: Search [GitHub Issues](https://github.com/agnostos/agnos/issues)
- **Discussions**: Join [GitHub Discussions](https://github.com/agnostos/agnos/discussions)
- **Matrix**: #agnos-dev:matrix.org
- **Discord**: [discord.gg/agnos](https://discord.gg/agnos)

## Resources

### Documentation
- [System Architecture](../ARCHITECTURE.md)
- [Development Roadmap](../PHASES.md)
- [API Reference](api/)
- [Security Model](../security/security-model.md)

### External Resources
- [Linux Kernel Documentation](https://www.kernel.org/doc/html/latest/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [Landlock Documentation](https://docs.kernel.org/userspace-api/landlock.html)

## Next Steps

1. **Set up** your development environment
2. **Build** AGNOS
3. **Run** the test suite
4. **Pick** a task from TODO.md
5. **Join** the community

Welcome to the AGNOS team!

---

*For detailed information about specific components, see the relevant documentation in the `docs/` directory.*
