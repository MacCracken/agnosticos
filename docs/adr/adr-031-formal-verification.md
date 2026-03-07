# ADR-031: Formal Verification Framework

**Status:** Accepted

**Date:** 2026-03-07

**Authors:** AGNOS Team

## Context

AGNOS has security-critical components: sandboxing, trust verification, audit chains,
state machines, privilege escalation. Bugs in these components have outsized impact —
a sandbox escape or broken audit chain undermines the entire security model.

Traditional testing catches bugs but cannot prove their absence. Formal verification
provides stronger guarantees by proving properties hold for all possible inputs, not
just tested ones.

## Decision

### Property-Based Verification

Rather than full theorem proving (which requires specialized tools and expertise),
AGNOS adopts a practical approach:

1. **Property specification**: Security invariants expressed as typed `Property` structs
   with categories (Invariant, Precondition, Postcondition, Safety, Liveness, Refinement)
2. **Property checking**: `PropertyChecker` verifies properties via model checking,
   property testing, static analysis, or runtime monitoring
3. **State machine verification**: Dedicated verifiers for reachability, deadlock freedom,
   determinism, and unreachable state detection
4. **Refinement checking**: Verify concrete implementations refine abstract specifications
   via trace containment

### Built-in AGNOS Properties

~15 pre-defined security properties for core AGNOS components:
- Audit chain integrity (hash chain never broken)
- Sandbox isolation (no file access outside allowed paths)
- Trust hierarchy ordering (SystemCore > Verified > ... > Revoked)
- Service state machine validity (no invalid transitions)
- Privilege escalation requires approval
- Secret zeroing on drop
- Rate limiter atomicity
- IPC message ordering (FIFO per channel)

### Runtime Monitoring

`InvariantMonitor` continuously checks invariants at runtime, catching violations
that static analysis might miss (configuration-dependent behavior, race conditions).

### Verification Reports

`VerificationReport` provides coverage metrics per component: how many properties
are verified vs. failed vs. unverified, enabling systematic security assurance.

## Consequences

### What becomes easier
- Systematic identification and tracking of security properties
- Confidence that critical invariants hold (beyond test coverage)
- Regression detection when changes violate established properties
- Compliance evidence (verified properties as audit artifacts)

### What becomes harder
- Writing formal properties requires understanding both the spec and the code
- False sense of security if properties are incomplete
- Property checking adds CI time (mitigated by selective verification)

## References

- ADR-005: Security Model and Human Override
- ADR-019: Sigil Trust System (trust hierarchy properties)
- ADR-022: Argonaut Init System (state machine properties)
- TLA+ specification language: https://lamport.azurewebsites.net/tla/tla.html
- Property-based testing: https://hypothesis.works/
