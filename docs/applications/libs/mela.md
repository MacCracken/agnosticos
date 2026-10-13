# Mela — App Marketplace

- **Repo**: [MacCracken/mela](https://github.com/MacCracken/mela)
- **License**: GPL-3.0-only
- **Role**: Agent marketplace — local registry, remote client, ratings, trust, transparency

Package installation/removal, Flutter app packaging, sandbox profiles, publisher verification, dependency resolution ordering.

**Consumers**: ark (declares `[deps.mela]`, for `.ark` fetch). daimon (discovery) and nous are intended consumers that do not declare it — nous keeps a stub seam where mela's endpoint would plug in (`nous/src/registry.cyr:160`, `src/updates.cyr:9`).
