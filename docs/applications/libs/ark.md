# Ark — Package Manager

- **Repo**: [MacCracken/ark](https://github.com/MacCracken/ark)
- **License**: GPL-3.0-only
- **Role**: Unified package manager CLI for AGNOS

Translates user commands into install plans using the nous resolver. Plan-first: it shows the plan by default and executes only with `--apply`, escalating privileged steps through shakti; since 1.1.0 it also installs `.ark` packages natively. Supports system, marketplace, Flutter, and community sources.

**Consumers**: end users, CI/CD, agnoshi, argonaut
**Dependencies**: sigil, nous, mela, agnostik, sandhi (declared in its `cyrius.cyml`)
