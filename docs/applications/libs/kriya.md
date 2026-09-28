# kriya

Coreutils-equivalent multi-tool — `cp` / `mv` / `rm` / `mkdir` / `echo` / `wc` / `find` / `grep` / … (BusyBox-style dispatcher, Cyrius-native).

- **Repository**: [github.com/MacCracken/kriya](https://github.com/MacCracken/kriya)
- **License**: GPL-3.0-only
- **Status**: Stable — v1.0+
- **Language**: Cyrius

Ships `/bin/<verb>` symlinks dispatched through one binary. No `cat` verb by design — that's [owl](owl.md)'s job. Consumed by agnoshi as the agnos-fs coreutils layer.

See the [binaries registry](../binaries.md) for the full catalog and dependency context.
