# Ark — Package Manager

- **Version**: 0.1.0
- **Repo**: [MacCracken/ark](https://github.com/MacCracken/ark)
- **License**: GPL-3.0-only
- **Tests**: 71
- **Role**: Unified package manager CLI for AGNOS

Translates user commands into install plans using nous resolver. Generates InstallPlan instructions — does not directly execute apt/dpkg. Supports system, marketplace, Flutter, and community sources.

**Consumers**: end users, CI/CD, agnoshi, argonaut
**Dependencies**: nous (resolver)
