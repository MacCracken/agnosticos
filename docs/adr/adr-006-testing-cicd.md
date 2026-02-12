# ADR-006: Testing Strategy and CI/CD

**Status:** Proposed

**Date:** 2026-02-12

**Authors:** AGNOS Team

## Context

AGNOS requires comprehensive testing for:
- Security validation
- Performance verification
- Regression prevention
- Release quality assurance
- Multi-architecture support

## Decision

We will implement a **multi-layer testing strategy**:

### Testing Levels

1. **Unit Tests** - Individual functions and modules
2. **Integration Tests** - Component interactions
3. **System Tests** - End-to-end workflows
4. **Security Tests** - Vulnerability scanning
5. **Performance Tests** - Benchmarks and load testing

### CI/CD Pipeline

```
┌─────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│  Lint   │ -> │  Build   │ -> │   Test   │ -> │ Security │
└─────────┘    └──────────┘    └──────────┘    └──────────┘
                                                    │
                    ┌──────────┐    ┌──────────┐    v
                    │ Release  │ <- │ Package  │ <- │ Sign   │
                    └──────────┘    └──────────┘    └──────────┘
```

## Consequences

### Positive
- Catch bugs early in development
- Automated security scanning
- Consistent build quality
- Faster release cycles
- Multi-arch validation

### Negative
- CI infrastructure costs
- Longer PR review times
- Maintenance of test suites
- Flaky test management

## Test Categories

| Category | Tools | Coverage Target |
|----------|-------|-----------------|
| Unit | cargo test | 80% |
| Integration | cargo test --test | 70% |
| Security | cargo-audit, trivy, semgrep | 100% checks |
| Lint | clippy, rustfmt | 100% clean |
| Docs | rustdoc | 100% public APIs |

## CI/CD Stages

1. **Check** - Format, lint, clippy
2. **Build** - Debug and release builds
3. **Test** - Unit and integration tests
4. **Security** - Audit, scan, SBOM
5. **Package** - Create installable packages
6. **Sign** - GPG sign packages
7. **Release** - Upload to repository

## References

- [Rust Testing Guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [GitHub Actions](https://docs.github.com/en/actions)
- [Trivy Security Scanner](https://aquasecurity.github.io/trivy/)
- [Semantic Versioning](https://semver.org/)
