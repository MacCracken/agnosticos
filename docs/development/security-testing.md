# Security Testing Guide

> **Last Updated**: 2026-03-07

This document covers AGNOS security testing methodologies including fuzzing, penetration testing, and vulnerability assessment.

## Fuzzing Infrastructure

AGNOS uses libfuzzer-sys for coverage-guided fuzzing.

### Setup

```bash
# Install fuzzing dependencies
cargo +nightly install cargo-fuzz

# Run a fuzzer
cd fuzz
cargo +nightly fuzz run fuzz_agent_parse
```

### Available Fuzzers

| Fuzzer | Target | Status |
|--------|--------|--------|
| `fuzz_agent_parse` | AgentConfig parsing | ✅ |
| `fuzz_command_split` | Command splitting | ✅ |
| `fuzz_interpreter` | NL input parsing | ✅ |
| `fuzz_llm_inference` | LLM request handling | ✅ |

### Adding New Fuzzers

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

### Corpus

Maintain a corpus of valid inputs:
```
fuzz/corpus/
├── fuzz_agent_parse/
│   ├── valid_config1.json
│   └── valid_config2.json
└── ...
```

## Manual Security Testing

### Network Testing

```bash
# Scan for open ports
nmap -sS -O localhost

# Test SSL/TLS
testssl --starttls smtp localhost
```

### Privilege Escalation

```bash
# Check sudo permissions
sudo -l

# Check capabilities
getcap -r / 2>/dev/null
```

### Sandbox Escape Testing

```bash
# Test Landlock
# Try to access restricted paths from agent context

# Test seccomp
# Try to make disallowed syscalls
```

## Automated Security Tools

### Rust-specific

```bash
# Fuzz with cargo-fuzz
cargo +nightly fuzz run fuzz_agent_parse

# Memory safety
cargo clean
RUSTFLAGS="-Z sanitizer=address" cargo build
ASAN_OPTIONS=detect_leaks=1 ./target/debug/your_test

# Miri for undefined behavior
cargo +nightly miri test
```

### System Scanning

```bash
# Dependency vulnerabilities
cargo audit

# Static analysis
cargo clippy -- -D warnings

# Code complexity
cargo umatrix
```

## Penetration Testing Checklist

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

## Subsystem Security Test Coverage

### aegis (Security Daemon) — 40 tests

The aegis subsystem (`agent-runtime/aegis.rs`) has comprehensive test coverage for:
- Threat level classification and auto-response policies
- Security event pipeline (10 event types: IntegrityViolation, SandboxEscape, AnomalousBehavior, etc.)
- Auto-quarantine of Critical/High threat agents
- On-install and on-execute scanning
- Quarantine management (quarantine, release, auto-release timeout)
- Event filtering by agent, threat level, and resolution status

### sigil (Trust System) — 35 tests

The sigil subsystem (`agent-runtime/sigil.rs`) covers:
- Trust level hierarchy (SystemCore > Verified > Community > Unverified > Revoked)
- Trust policy enforcement (Strict, Permissive, AuditOnly)
- Ed25519 artifact signing and verification
- Revocation list management (by key_id or content_hash)
- Boot chain verification
- Trust store caching by content hash

### Post-Quantum Cryptography (PQC)

PQC readiness is tracked as a future milestone. Current signing uses Ed25519 via `ed25519-dalek`. The sigil trust system is designed to be algorithm-agnostic so PQC signature schemes can be swapped in without architectural changes.

### Formal Verification

Security-critical invariants (sandbox apply order, audit chain integrity, trust level transitions) are verified through property-based tests and exhaustive state-machine tests in the aegis and sigil test suites. Full formal verification (e.g., via Kani or Prusti) is planned for post-alpha.

## Reporting Security Issues

See [SECURITY.md](../../SECURITY.md) for vulnerability disclosure procedures.

## References

- [Rust Fuzz Book](https://rust-fuzz.github.io/book/)
- [OWASP Testing Guide](https://owasp.org/www-project-web-security-testing-guide/)
- [CIS Security](https://www.cisecurity.org/)
