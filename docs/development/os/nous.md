# Nous — Package Resolver

- **Version**: 0.1.0
- **Repo**: [MacCracken/nous](https://github.com/MacCracken/nous)
- **License**: GPL-3.0-only
- **Tests**: 57
- **Role**: Intelligence layer for package resolution

Given a package name, determines source (system apt, marketplace, Flutter app, community). Returns resolution plans — does not execute installs. Configurable strategy (MarketplaceFirst, SystemFirst, SearchAll).

**Consumers**: ark
