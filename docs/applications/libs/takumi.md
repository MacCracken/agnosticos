# Takumi — Build System

- **Repo**: [MacCracken/takumi](https://github.com/MacCracken/takumi)
- **License**: GPL-3.0-only
- **Role**: Package build system — compiles .ark packages from CYML recipes (as of 1.0.0, 2026-06-17)

Parses CYML recipes, resolves build dependencies, topological sort for build order, SHA-256 verification, security hardening flags.

**Consumers**: CI/CD, ark, selfhost pipeline
