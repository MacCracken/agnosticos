# Aegis — Security Daemon

- **Repo**: [MacCracken/aegis](https://github.com/MacCracken/aegis)
- **License**: GPL-3.0-only
- **Role**: System security daemon

Security policy enforcement, agent trust management, quarantine, compliance checking.

**Consumers** (per aegis's README): daimon (security-policy enforcement), argonaut (boot hardening). Neither declares `[deps.aegis]` (2026-09-28): argonaut starts aegis as a boot service (`argonaut/src/services.cyr:520`), and daimon's source does not reference it.
