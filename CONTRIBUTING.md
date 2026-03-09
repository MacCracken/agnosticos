# Contributing to AGNOS

Thank you for your interest in contributing to AGNOS! This document provides guidelines and best practices for contributing to the project.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Getting Started](#getting-started)
3. [Development Environment](#development-environment)
4. [Git Workflow](#git-workflow)
5. [Coding Standards](#coding-standards)
6. [Testing](#testing)
7. [Documentation](#documentation)
8. [Security](#security)
9. [Release Process](#release-process)

## Code of Conduct

This project adheres to a code of conduct. By participating, you are expected to:

- Be respectful and inclusive
- Welcome newcomers
- Accept constructive criticism gracefully
- Focus on what is best for the community
- Show empathy towards others

## Getting Started

### Prerequisites

Before contributing, ensure you have:

- Git 2.30+
- Docker 20.10+ (for containerized builds)
- 50GB+ free disk space
- Basic knowledge of:
  - Linux kernel development
  - Rust, C, or Python (depending on area)
  - Git and GitHub workflow

### First-Time Setup

1. **Fork the repository** on GitHub

2. **Clone your fork**:
   ```bash
   git clone https://github.com/YOUR_USERNAME/agnos.git
   cd agnos
   ```

3. **Add upstream remote**:
   ```bash
   git remote add upstream https://github.com/agnostos/agnos.git
   ```

4. **Set up development environment**:
   ```bash
   # Install build dependencies
   ./scripts/install-build-deps.sh
   
   # Set up git hooks
   ./scripts/setup-git-hooks.sh
   ```

## Development Environment

### Using Docker (Recommended)

```bash
# Build development container
docker build -t agnos-dev -f Dockerfile.dev .

# Run development environment
docker run -it --rm \
  -v $(pwd):/workspace \
  -v agnos-build-cache:/cache \
  agnos-dev

# Inside container
make build
```

### Native Development

```bash
# Install dependencies
sudo ./scripts/install-build-deps.sh

# Build kernel
./scripts/build-kernel.sh

# Build userland
make build-userland

# Run tests
make test
```

## Git Workflow

### Branch Strategy

We use a simplified Git Flow model:

```
main (production-ready)
  ↑
develop (integration branch)
  ↑
feature/* (feature branches)
  ↑
hotfix/* (emergency fixes)
```

### Branch Naming

Use descriptive branch names with prefixes:

| Prefix | Purpose | Example |
|--------|---------|---------|
| `feature/` | New features | `feature/agent-kernel-module` |
| `bugfix/` | Bug fixes | `bugfix/shell-memory-leak` |
| `docs/` | Documentation | `docs/api-reference` |
| `refactor/` | Code refactoring | `refactor/llm-gateway` |
| `security/` | Security fixes | `security/audit-log-integrity` |
| `chore/` | Maintenance | `chore/update-dependencies` |

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style (formatting, no logic change)
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `test`: Tests
- `chore`: Build process, dependencies
- `security`: Security-related changes

**Scopes** (examples):
- `kernel`: Kernel code
- `shell`: AI Shell
- `agent`: Agent runtime
- `desktop`: Desktop environment
- `docs`: Documentation
- `build`: Build system
- `ci`: CI/CD

**Examples**:

```bash
# Good commits
git commit -m "feat(kernel): add Landlock integration for agent sandboxing"
git commit -m "fix(shell): resolve crash on invalid UTF-8 input"
git commit -m "docs(api): add kernel module API reference"
git commit -m "security(agent): prevent privilege escalation in sandbox"

# With body
git commit -m "feat(agent): implement agent lifecycle management

This adds support for creating, suspending, and terminating
agents with proper resource cleanup.

Closes #123"
```

### Pull Request Process

1. **Create a branch** from `develop`:
   ```bash
   git checkout develop
   git pull upstream develop
   git checkout -b feature/your-feature
   ```

2. **Make changes** following coding standards

3. **Commit** with conventional commit messages

4. **Push** to your fork:
   ```bash
   git push origin feature/your-feature
   ```

5. **Create Pull Request** on GitHub:
   - Target: `develop` branch
   - Fill out PR template
   - Link related issues

6. **PR Requirements**:
   - [ ] All tests pass
   - [ ] Code review approved
   - [ ] Documentation updated
   - [ ] Security review (if applicable)
   - [ ] Commit messages follow convention

### Commit Signing

All commits must be signed (GPG or SSH):

```bash
# Generate GPG key
gpg --full-generate-key

# Add to GitHub
gpg --list-secret-keys --keyid-format LONG
# Copy key ID and add to GitHub settings

# Configure git
git config --global user.signingkey YOUR_KEY_ID
git config --global commit.gpgsign true

# Sign commits
git commit -S -m "feat: your message"
```

### Rebasing

Keep your branch up to date:

```bash
# Fetch latest
git fetch upstream

# Rebase your branch
git checkout feature/your-feature
git rebase upstream/develop

# If conflicts, resolve them
git add .
git rebase --continue

# Force push (only for your feature branch!)
git push --force-with-lease origin feature/your-feature
```

## Coding Standards

### General Principles

1. **Security First**: All code must consider security implications
2. **Performance Matters**: Optimize for the critical path
3. **Test Coverage**: New code requires tests
4. **Documentation**: Code must be documented

### Language-Specific Standards

#### Rust (Agent Runtime, System Tools)

```rust
// Use rustfmt for formatting
// Max line length: 100 characters

/// Brief description of function
/// 
/// # Arguments
/// 
/// * `arg1` - Description
/// 
/// # Returns
/// 
/// Description of return value
/// 
/// # Security Considerations
/// 
/// Any security implications
pub fn example_function(arg1: &str) -> Result<(), Error> {
    // Implementation
}

// Use anyhow for error handling
// Use tracing for logging
// Use clap for CLI
```

**Linting**:
```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo audit
```

#### C (Kernel Code)

```c
/*
 * Brief description
 * @param arg1 Description
 * @return Description
 * 
 * Security: Any security considerations
 */
int example_function(const char *arg1)
{
    // Use kernel coding style
    // Checkpatch.pl compliance
    // No floating point in kernel
}
```

**Linting**:
```bash
./scripts/checkpatch.pl --file kernel/module.c
sparse kernel/module.c
```

#### Python (Build Scripts, Tools)

```python
"""Module docstring."""

import os
from typing import Optional


def example_function(arg1: str) -> Optional[int]:
    """
    Brief description.
    
    Args:
        arg1: Description
        
    Returns:
        Description or None
        
    Security:
        Any security considerations
    """
    # Use black for formatting
    # Use ruff for linting
    # Use type hints
```

**Linting**:
```bash
black .
ruff check .
mypy .
bandit -r .
```

### Code Review Checklist

Reviewers should check:

- [ ] **Functionality**: Does it work as intended?
- [ ] **Security**: Are there security implications?
- [ ] **Performance**: Is it efficient?
- [ ] **Testing**: Are there adequate tests?
- [ ] **Documentation**: Is it documented?
- [ ] **Style**: Does it follow conventions?
- [ ] **Error Handling**: Are errors handled properly?
- [ ] **Resource Management**: Are resources freed properly?

## Testing

### Test Structure

```
tests/
├── unit/           # Unit tests
├── integration/    # Integration tests
├── e2e/           # End-to-end tests
├── security/      # Security tests
└── performance/   # Performance benchmarks
```

### Running Tests

```bash
# All tests
make test

# Specific test suite
make test-unit
make test-integration
make test-security

# With coverage
make test-coverage

# Performance benchmarks
make benchmark
```

### Writing Tests

**Rust**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        let result = example_function("test");
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic]
    fn test_panic_case() {
        example_function("");
    }
}
```

**Python**:
```python
import pytest


def test_example():
    """Test example function."""
    result = example_function("test")
    assert result is not None


def test_example_error():
    """Test error handling."""
    with pytest.raises(ValueError):
        example_function("")
```

## Documentation

### Code Documentation

All public APIs must be documented:

```rust
/// Agent Kernel Daemon
/// 
/// The main daemon responsible for managing agent lifecycle
/// and resource allocation.
/// 
/// # Security
/// 
/// This daemon runs with elevated privileges. All agent
/// operations are sandboxed using Landlock and seccomp.
pub struct AgentKernelDaemon {
    // ...
}
```

### User Documentation

- Use clear, simple language
- Include examples
- Add screenshots for UI features
- Keep up to date with code changes

### Documentation Locations

| Type | Location |
|------|----------|
| API docs | `docs/api/` |
| User guides | `docs/user/` |
| Developer docs | `docs/development/` |
| Security docs | `docs/security/` |
| README | Component root |
| Inline | Source code |

## Security

### Security-Focused Development

1. **Never commit secrets**: Use environment variables
2. **Validate all input**: Sanitize user input
3. **Least privilege**: Minimal permissions required
4. **Audit logging**: Log security-relevant events
5. **Defense in depth**: Multiple security layers

### Reporting Security Issues

See [SECURITY.md](SECURITY.md) for vulnerability disclosure.

### Security Review

Security-sensitive changes require:
- Security reviewer approval
- Threat model update
- Security test coverage
- Documentation of security properties

## Release Process

### Versioning

We use [Semantic Versioning](https://semver.org/):

```
MAJOR.MINOR.PATCH

MAJOR - Breaking changes
MINOR - New features (backwards compatible)
PATCH - Bug fixes (backwards compatible)
```

### Versioning Scheme

AGNOS uses **Calendar Versioning (CalVer)** in `YYYY.M.D` format:

- `YYYY` — release year
- `M` — release month
- `D` — release day of the month

Patch releases append `-N` (e.g., `2026.3.5-1`, `2026.3.5-2`).

The canonical version lives in the `VERSION` file at the repository root. Shell scripts, the Makefile, and the Docker entrypoint all read from this file. Cargo workspace version is set in `userland/Cargo.toml` under `[workspace.package]`.

### Release Branches

```
main
  ↑
release/v2026.3.5  ← Release branch
  ↑
develop
```

### Release Checklist

- [ ] Version bumped in `VERSION` file and `userland/Cargo.toml`
- [ ] Changelog updated
- [ ] All tests passing
- [ ] Security review completed
- [ ] Documentation updated
- [ ] Release notes written
- [ ] Tag created and signed
- [ ] Packages built and signed
- [ ] Release published

### Creating a Release

```bash
# Create release branch
git checkout -b release/v2026.3.5

# Update version — edit the VERSION file, then sync Cargo.toml
echo "2026.3.5" > VERSION
# Update version in userland/Cargo.toml [workspace.package] to match

# Update changelog
vim CHANGELOG.md

# Commit
git add .
git commit -m "chore(release): prepare v2026.3.5"

# Create tag
git tag -s v2026.3.5 -m "Release v2026.3.5"

# Push
git push origin release/v2026.3.5
git push origin v2026.3.5
```

## Questions?

- **General questions**: [GitHub Discussions](https://github.com/agnostos/agnos/discussions)
- **Development help**: Matrix channel #agnos-dev:matrix.org
- **Security issues**: security@agnos.io

## License

By contributing, you agree that your contributions will be licensed under the GPL v3.0 License.

---

Thank you for contributing to AGNOS!
