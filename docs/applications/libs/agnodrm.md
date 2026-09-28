# agnodrm

Device / DRM model — udev enumeration + DRM/KMS device access, with error + util support (Cyrius-native).

- **Repository**: [github.com/MacCracken/agnodrm](https://github.com/MacCracken/agnodrm)
- **License**: GPL-3.0-only
- **Status**: Stable — v1.0+
- **Language**: Cyrius

**Was `agnosys`** (the kernel-interface lib), decomposed 2026-06-19 after it out-grew its remit. Its subsystems moved to their proper homes: trust → [sigil](sigil.md), security / MAC / audit → [kavach](kavach.md), PAM → [aegis](aegis.md), structured logging → [sakshi](sakshi.md), and the per-arch **syscall layer into the Cyrius language itself** — the language became the syscall provider. What remains in agnodrm: the device/DRM model (udev + DRM/KMS), the error/util core, and a deferred Linux-eccentric group (bootloader / update / netns / fuse / journald) parked post-v1.

See the [libs registry](README.md) for the full catalog and dependency context.
