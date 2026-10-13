# sit

Cyrius-native version control — a git replacement (binary `sit`).

- **Repository**: [github.com/MacCracken/sit](https://github.com/MacCracken/sit)
- **License**: GPL-3.0-only
- **Status**: Stable — v1.0+
- **Language**: Cyrius

Keeps git's object model (content-addressed DAG, distributed, SHA-integrity) and drops the accretion (submodules, shell hooks, LFS, the porcelain/plumbing split — one CLI for humans and scripts). Built on already-sovereign deps: [sankoch](sankoch.md) (compression), [sigil](sigil.md) (crypto), [patra](patra.md) (storage). See the article *Memory Should Be Sovereign Too*.

See the [binaries registry](../binaries.md) for the full catalog and dependency context.
