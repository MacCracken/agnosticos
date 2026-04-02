# Takumi — Build System

- **Version**: 0.1.0
- **Repo**: [MacCracken/takumi](https://github.com/MacCracken/takumi)
- **License**: GPL-3.0-only
- **Tests**: 57
- **Role**: Package build system — compiles .ark packages from TOML recipes

Parses TOML recipes, resolves build dependencies, topological sort for build order, SHA-256 verification, security hardening flags.

**Consumers**: CI/CD, ark, selfhost pipeline
