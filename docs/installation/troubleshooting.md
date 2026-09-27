# AGNOS Troubleshooting

> Last Updated: 2026-09-27
>
> **Pre-Beta note:** AGNOS is not yet a running end-user OS. This guide covers what is testable today — the sovereign boot pipeline, the Cyrius toolchain, per-subsystem builds, and the QEMU kernel boot test. Full system-level troubleshooting (services, networking, desktop) arrives with the **ISO Stage-4 cut** and the `agnova` installer.

---

## Table of Contents

1. [Cyrius Toolchain](#cyrius-toolchain)
2. [Sovereign Boot Pipeline](#sovereign-boot-pipeline)
3. [Component Verification](#component-verification)
4. [QEMU Kernel Boot Test](#qemu-kernel-boot-test)
5. [Per-Subsystem Builds](#per-subsystem-builds)
6. [Getting Help](#getting-help)

---

## Cyrius Toolchain

### `cyrius: command not found`

```sh
which cyrius
ls ~/.cyrius/bin/
```

Install options:
- Release install: `curl -sSf https://raw.githubusercontent.com/MacCracken/cyrius/main/scripts/install.sh | sh` (see the `cyrius` repo's `README.md` § Install)
- Source build: `cd /home/macro/Repos/cyrius && sh bootstrap/bootstrap.sh`

### Toolchain version mismatch

Every Cyrius project pins a version in its `cyrius.cyml` manifest via the `cyrius = "<version>"` field (single source of truth — there is no separate `.cyrius-toolchain` file):

```sh
grep '^cyrius' /home/macro/Repos/agnosticos/scripts/cyrius.cyml
cyrius --version
```

If they disagree, rebuild the toolchain from source (`cd /home/macro/Repos/cyrius && sh bootstrap/bootstrap.sh`) or install the matching release tarball.

### `stdout` or `println` doesn't print

You are calling raw `cycc` (the self-hosting compiler) instead of `cyrius build`. Raw `cycc` does not auto-prepend stdlib includes. Always use:

```sh
cyrius build <source.cyr> <output>
```

### `main()` runs but the program exits 0 with no output

Cyrius executes top-level code, not `fn main()` automatically. Programs must call `main()` at the top level — from a **bare** statement, exiting via `SYS_EXIT`:

```cyrius
fn main(): i64 { ... return 0; }
fn _entry(): i64 { var r = main(); syscall(SYS_EXIT, r); return 0; }
_entry();                 # bare top-level call
```

The older `var exit_code = main(); syscall(60, exit_code);` form is wrong on agnos: agnos numbers its own syscalls, `exit` is `0` there and `60` is a different call, so the literal only exits on Linux — `SYS_EXIT` resolves correctly on both. (The `var … = main()` shape also used to hide argv on agnos, because `main` ran before the init stack was captured; cyrius 6.1.32 fixed that.)

Study working programs in `/home/macro/Repos/cyrius/programs/*.cyr` (65+ examples).

---

## Sovereign Boot Pipeline

### `cyrius build src/boot.cyr build/boot` fails

```sh
cd /home/macro/Repos/agnosticos/scripts
cyrius build src/boot.cyr build/boot
```

Common causes:

| Error | Cause | Fix |
|-------|-------|-----|
| Missing stdlib symbols | `cyrius.cyml` deps stale | `cd scripts && cyrius deps` |
| Include not found | Local source file path wrong | Paths in `src/boot.cyr` are relative to `scripts/` |
| Linker error on `syscall` | Toolchain mismatch | Check `cyrius = "<version>"` in `scripts/cyrius.cyml` matches installed `cyrius --version` |

### `./build/boot --help` prints nothing

The binary built but wasn't linked with stdlib. Re-run via `cyrius build` (not raw `cycc`).

### `make boot` fails

```sh
make boot
```

Runs `cyrius build` under the hood. If it fails, `cd scripts && cyrius build src/boot.cyr build/boot` directly to see the raw error.

---

## Component Verification

### `make iso-check` reports a missing build artifact

```
  MISS   kybernet      vX.Y.Z  [no build artifact]
```

The sibling repo hasn't been built (iso-check checks that each artifact is present and prints the repo's `VERSION`; it does not check freshness). Fix:

```sh
cd /home/macro/Repos/kybernet
cyrius build src/main.cyr build/kybernet
cd /home/macro/Repos/agnosticos
make iso-check
```

### `make iso-check` reports missing repo

A sibling repo path in `scripts/src/boot.cyr` or the Makefile points to a repo that isn't cloned locally. Check `docs/architecture.md` for the canonical repo list.

### Version drift

The version each sibling reports is read straight from that repo's `VERSION` file — iso-check does not look inside the build artifact. If a sibling was upgraded but its artifact predates the upgrade, rebuild it from source; iso-check cannot tell.

---

## QEMU Kernel Boot Test

### `make boot-test` fails: `Error loading uncompressed kernel without PVH ELF Note`

Expected since agnos **1.30.0**. `boot.cyr` boots with QEMU's `-kernel`, the pre-1.30.0 multiboot path; the kernel is now an ELF64 image entered only through gnoboot's sovereign handoff (`RDI = &boot_info`), which `-kernel` cannot provide (checked 2026-09-27 against agnos 1.57.10). Smoke-boot it through gnoboot + OVMF instead (a timed, non-interactive run that asserts gnoboot's hand-off banner):

```sh
# needs OVMF (edk2-ovmf), parted and mtools, and a built gnoboot:
#   CYRIUS_TARGET_EFI=1 cyrius build src/main.cyr build/BOOTX64.EFI
cd /home/macro/Repos/gnoboot && sh tests/ovmf_smoke.sh   # AGNOS_KERNEL defaults to ../agnos/build/agnos
```

The kernel's own QEMU gates are the agnos repo's smoke battery (`scripts/smoke/`, driven by `scripts/sweep.sh`).

### `make boot-test` hangs on black screen

`boot.cyr` already wires QEMU's serial console to stdio (`-serial stdio -display none`); there is no `--serial` flag — `boot` rejects it as an unknown option. To exit QEMU: `Ctrl-a x`.

### `make boot-test` reports "kernel not found"

The AGNOS kernel isn't built. Fix:

```sh
cd /home/macro/Repos/agnos
sh scripts/build.sh
cd /home/macro/Repos/agnosticos
make boot-test
```

### Kernel boots but panics on first syscall

Most common cause: `agnos` and its `cyrius` toolchain pin are out of sync. Rebuild the kernel against the pinned toolchain from current sources. (Since agnos 1.56.51, `scripts/build.sh` refuses to build on pin drift; `AGNOS_ALLOW_PIN_DRIFT=1` is the deliberate override.)

### QEMU version too old

```sh
qemu-system-x86_64 --version  # requires ≥ 7.0
```

Upgrade via your host package manager.

---

## Per-Subsystem Builds

Every subsystem builds standalone. Pattern:

```sh
cd /home/macro/Repos/<name>
cyrius build src/main.cyr build/<name>
./build/<name> --help
```

### Build fails with "dep not resolved"

```sh
cd /home/macro/Repos/<name>
cat cyrius.cyml  # check [deps.*] blocks
cyrius deps      # resolve
```

If a dep points to a sibling repo that isn't cloned, clone it from `MacCracken/<name>`.

### Tests fail after rebase

Each repo has its own test suite. Run from that repo:

```sh
cd /home/macro/Repos/<name>
cyrius test
```

Don't mix test runs across repos — each one pins its own toolchain and deps.

---

## Getting Help

- **Issue tracker**: https://github.com/MacCracken/agnosticos/issues
- **Per-subsystem issues**: file against the specific repo (`MacCracken/<name>/issues`)
- **Security issues**: see [/SECURITY.md](/SECURITY.md) for private disclosure
- **Roadmap**: tracked in the project's development docs
- **ISO Stage-4 cut** (the base→server blocker): the ISO work that closes the "is AGNOS installable?" gap — plan in [`iso-stage4-plan.md`](../development/iso-stage4-plan.md)

---

## Historical Note

Prior versions of this document covered `systemctl`, `agent-runtime`, `llm-gateway`, `journalctl`, and desktop-environment issues. Those refer to a Rust-era AGNOS built on a Debian base that was retired 2026-04-01 (monolith extraction) / 2026-04-04 (Cyrius pivot). That content returns — against the sovereign stack — when `agnova` and the Stage-4 ISO ship.
