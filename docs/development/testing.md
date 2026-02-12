# Testing Guide

AGNOS uses a multi-layer testing strategy to ensure security, reliability, and performance.

## Test Categories

### 1. Unit Tests

Unit tests verify individual functions and modules.

```bash
cd userland
cargo test --lib
```

**Coverage Targets:**
- Core logic: 90%+
- Security functions: 95%+
- Public APIs: 100%

### 2. Integration Tests

Integration tests verify component interactions.

```bash
cd userland
cargo test --test '*'
```

**Test Areas:**
- Agent lifecycle
- IPC communication
- LLM gateway providers
- Security permission system

### 3. System Tests

System tests verify end-to-end workflows.

```bash
# Run system tests
make test-system

# Run in VM
make test-vm
```

### 4. Security Tests

Security tests validate the security model.

```bash
# Run cargo audit
cargo audit

# Run Trivy scan
trivy fs .

# Run security unit tests
cargo test security
```

### 5. Performance Tests

Performance tests ensure system responsiveness.

```bash
# Run benchmarks
cargo bench

# Load testing
make test-load
```

## Test Organization

```
tests/
├── unit/              # Unit tests (alongside source)
├── integration/       # Integration tests
├── system/           # System/end-to-end tests
├── security/         # Security-specific tests
└── benchmarks/       # Performance benchmarks
```

## Writing Tests

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let runtime = AgentRuntime::new();
        let config = AgentConfig {
            name: "test-agent".to_string(),
            capabilities: vec!["file:read".to_string()],
        };
        let agent_id = runtime.spawn_agent(config).unwrap();
        assert!(!agent_id.is_nil());
    }

    #[test]
    fn test_permission_check() {
        let security = SecurityUI::new();
        let agent_id = Uuid::new_v4();
        security.set_agent_permissions(agent_id, "Test".to_string(), vec!["file:read".to_string()]);
        
        assert!(security.has_permission(agent_id, "file:read"));
        assert!(!security.has_permission(agent_id, "file:delete"));
    }
}
```

### Integration Test Example

```rust
// tests/agent_integration.rs
use agnos::agent::{AgentRuntime, AgentConfig};
use agnos::security::SecurityUI;

#[tokio::test]
async fn test_agent_security_integration() {
    let runtime = AgentRuntime::new();
    let security = SecurityUI::new();
    
    let config = AgentConfig {
        name: "test".to_string(),
        capabilities: vec!["file:read".to_string()],
    };
    
    let agent_id = runtime.spawn_agent(config).unwrap();
    security.set_agent_permissions(agent_id, "Test".to_string(), vec!["file:read".to_string()]);
    
    // Verify agent can perform allowed actions
    assert!(runtime.execute_action(agent_id, "file:read", "/etc/passwd").await.is_ok());
    
    // Verify agent cannot perform disallowed actions
    assert!(runtime.execute_action(agent_id, "file:delete", "/etc/passwd").await.is_err());
}
```

### Security Test Example

```rust
// tests/security/sandbox_tests.rs
use agnos::security::{Sandbox, FilesystemRule};

#[test]
fn test_landlock_sandbox() {
    let sandbox = Sandbox::new();
    sandbox.apply_landlock(&[
        FilesystemRule {
            path: "/tmp".to_string(),
            access: Access::ReadWrite,
        },
    ]).unwrap();
    
    // Should be able to read/write /tmp
    assert!(std::fs::write("/tmp/test.txt", "data").is_ok());
    
    // Should NOT be able to read /etc
    assert!(std::fs::read_to_string("/etc/passwd").is_err());
}
```

## Continuous Integration

Tests run automatically on:
- Every pull request
- Every push to main/develop
- Nightly builds

See `.github/workflows/ci.yml` for CI configuration.

## Code Coverage

We aim for minimum 80% code coverage.

```bash
# Generate coverage report
cargo install cargo-tarpaulin
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

## Security Testing

### SAST (Static Application Security Testing)

- **cargo-audit**: Checks dependencies for vulnerabilities
- **semgrep**: Pattern-based security scanning
- **clippy**: Rust linting with security warnings

### DAST (Dynamic Application Security Testing)

- **Fuzzing**: Automated input fuzzing
- **Penetration tests**: Manual security testing

### Compliance

- **License check**: Verify GPL compliance
- **SBOM**: Software Bill of Materials

## Test Checklist

Before submitting PR:

- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Security tests pass
- [ ] Code coverage >= 80%
- [ ] No clippy warnings
- [ ] Code formatted with rustfmt
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

## Debugging Tests

```bash
# Run single test with output
cargo test test_name -- --nocapture

# Run with debugger
cargo test test_name -- --exact

# Run with backtrace
RUST_BACKTRACE=1 cargo test
```
