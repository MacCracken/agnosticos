# sandhi

Service-boundary layer (HTTP client + server, HTTP/2, streaming, JSON-RPC, service discovery, TLS policy). सन्धि — *junction, connection, joining*.

**Folded into Cyrius stdlib at v5.7.0** (2026-04-25) as `lib/sandhi.cyr` (vendored byte-identical, 376,037 B / 9,649 lines / 469 fns) per [ADR 0002](https://github.com/MacCracken/sandhi/blob/main/docs/adr/0002-clean-break-fold-at-cyrius-v5-7-0.md). Sandhi repo entered maintenance mode. ADR 0002 planned for later surface patches to land via the Cyrius release cycle rather than sandhi releases; in practice (through sandhi 1.10.0) patches land in sandhi first, each sandhi release regenerates `dist/sandhi.cyr`, and a Cyrius release re-folds it.

- **Repository**: [github.com/MacCracken/sandhi](https://github.com/MacCracken/sandhi)
- **License**: GPL-3.0-only
- **Status**: Stable - See Repo
- **Language**: Cyrius

See the [libs registry](README.md) for the full catalog and dependency context.
