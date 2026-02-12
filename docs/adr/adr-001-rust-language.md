# ADR-001: Rust as Primary Implementation Language

**Status:** Accepted

**Date:** 2026-02-01

**Authors:** AGNOS Team

## Context

AGNOS requires a systems programming language that provides:
- Memory safety without garbage collection
- High performance for real-time agent operations
- Strong type system for reliability
- Concurrency support for multi-agent orchestration
- Cross-platform compilation support

## Decision

We will use **Rust** as the primary implementation language for:
1. User space components (agent runtime, desktop environment)
2. System utilities and tools
3. Kernel module bindings

## Consequences

### Positive
- Memory safety guarantees prevent common vulnerabilities
- Zero-cost abstractions enable high performance
- Rich ecosystem with crates for async, networking, security
- Built-in testing and documentation tools
- Strong community and tooling (cargo, clippy, rustfmt)

### Negative
- Steeper learning curve for contributors
- Longer compile times compared to C
- Smaller pool of developers compared to C/C++
- Some kernel modules still require C

## Alternatives Considered

### C
**Rejected:** While C is traditional for OS development, it lacks memory safety guarantees which are critical for an AI-native OS handling untrusted agent code.

### C++
**Rejected:** C++ provides some safety features but still allows unsafe operations. Rust's ownership model provides stronger guarantees.

### Go
**Rejected:** Go's garbage collector introduces unpredictable latency, unsuitable for real-time agent operations.

### Zig
**Considered:** Zig is promising but ecosystem is immature compared to Rust.

## References

- [Rust Security Features](https://www.rust-lang.org/tools/security)
- [Linux Kernel Rust Support](https://rust-for-linux.com/)
- [Rust in Operating Systems](https://os.phil-opp.com/)
