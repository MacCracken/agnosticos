# Testing Guide

> **Last Updated**: 2026-03-11

AGNOS uses a multi-layer testing strategy to ensure security, reliability, and performance.

## Current Test Status

- **Total tests**: 8,997+ across all crates
- **Coverage**: ~84% (cargo-tarpaulin)
- **Compiler warnings**: 0
- **Benchmark suites**: 3 Criterion suites (agent-runtime, llm-gateway, desktop-environment)
- **Known flaky**: `cd` tests in ai-shell (process cwd race)

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

### Fuzzing Infrastructure

AGNOS uses libfuzzer-sys for coverage-guided fuzzing.

```bash
# Install fuzzing dependencies
cargo +nightly install cargo-fuzz

# Run a fuzzer
cd fuzz
cargo +nightly fuzz run fuzz_agent_parse
```

#### Available Fuzzers

| Fuzzer | Target | Status |
|--------|--------|--------|
| `fuzz_agent_parse` | AgentConfig parsing | Active |
| `fuzz_command_split` | Command splitting | Active |
| `fuzz_interpreter` | NL input parsing | Active |
| `fuzz_llm_inference` | LLM request handling | Active |

#### Adding New Fuzzers

```rust
// fuzz/my_fuzzer.rs
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Your fuzzing logic here
});
```

Add to `fuzz/Cargo.toml`:
```toml
[[bin]]
name = "my_fuzzer"
path = "my_fuzzer.rs"
```

Maintain a corpus of valid inputs under `fuzz/corpus/<fuzzer_name>/`.

### Manual Security Testing

```bash
# Network: scan for open ports
nmap -sS -O localhost

# Privilege escalation: check capabilities
getcap -r / 2>/dev/null

# Sandbox escape: test Landlock and seccomp from agent context
```

### Automated Security Tools

```bash
# Fuzz with cargo-fuzz
cargo +nightly fuzz run fuzz_agent_parse

# Memory safety (AddressSanitizer)
cargo clean
RUSTFLAGS="-Z sanitizer=address" cargo build
ASAN_OPTIONS=detect_leaks=1 ./target/debug/your_test

# Undefined behavior (Miri)
cargo +nightly miri test

# Dependency vulnerabilities
cargo audit

# Static analysis
cargo clippy -- -D warnings
```

### SAST / DAST / Compliance

- **cargo-audit**: Dependency vulnerability checks
- **semgrep**: Pattern-based security scanning
- **clippy**: Rust linting with security warnings
- **License check**: Verify GPL compliance
- **SBOM**: Software Bill of Materials

### Penetration Testing Checklist

- [ ] Network reconnaissance
- [ ] Service enumeration
- [ ] Authentication testing
- [ ] Authorization testing
- [ ] Input validation testing
- [ ] Crypto implementation review
- [ ] Memory safety
- [ ] Race conditions
- [ ] Information disclosure
- [ ] Denial of service

### Subsystem Security Test Coverage

#### aegis (Security Daemon) — 55 tests

The aegis subsystem (`agent-runtime/aegis.rs`) covers:
- Threat level classification and auto-response policies
- Security event pipeline (10 event types: IntegrityViolation, SandboxEscape, AnomalousBehavior, etc.)
- Auto-quarantine of Critical/High threat agents
- On-install and on-execute scanning
- Quarantine management (quarantine, release, auto-release timeout)
- Event filtering by agent, threat level, and resolution status

#### sigil (Trust System) — 46 tests

The sigil subsystem (`agent-runtime/sigil.rs`) covers:
- Trust level hierarchy (SystemCore > Verified > Community > Unverified > Revoked)
- Trust policy enforcement (Strict, Permissive, AuditOnly)
- Ed25519 artifact signing and verification
- Revocation list management (by key_id or content_hash)
- Boot chain verification
- Trust store caching by content hash

#### Post-Quantum Cryptography (PQC)

PQC readiness is tracked as a future milestone. Current signing uses Ed25519 via `ed25519-dalek`. The sigil trust system is designed to be algorithm-agnostic so PQC signature schemes can be swapped in without architectural changes.

#### Formal Verification

Security-critical invariants (sandbox apply order, audit chain integrity, trust level transitions) are verified through property-based tests and exhaustive state-machine tests in the aegis and sigil test suites. Full formal verification (e.g., via Kani or Prusti) is planned for post-alpha.

### Reporting Security Issues

See [SECURITY.md](../../SECURITY.md) for vulnerability disclosure procedures.

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
