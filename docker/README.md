# docker/ — AGNOS Container Images

Shippable AGNOS container images. Test-sweep harnesses do **not** live here — see
[What is NOT here](#what-is-not-here--qemu-in-a-container) at the bottom.

## Images

| Path | Image | Status | What it is |
|---|---|---|---|
| `Dockerfile.thin` | `agnos-thin` | **base** | `FROM scratch` + `/mirshi` + **iam** (identity) + **ark** (package mgr) — no shell. **~430 KB, 3 layers.** The minimal agnos container: know what it is, install onto it. `CMD` = `/bin/iam`. |
| `Dockerfile.shell` | `agnos-shell` | **usable env** | `FROM agnos-thin` **+ agnsh** (the interactive shell). **~468 KB, 4 layers** — just the agnsh delta over the base. `run` / `&` / `>` / `\|` under mirshi. `CMD` = `/bin/agnsh`. |
| `Dockerfile.dev` | `agnos-dev` | **0.1.0 target** | `FROM agnos-shell` **+ kriya · owl · kii · bnrmr · cmdrs · cyim · sit · shu · hapi · yo · dig · whirl · darshini · anuenue** — the full dev userland (coreutils · cat · image · banner · prompt · editor · VCS · monitor · dotfiles · net-tools · HTTP · file-list · rainbow) **plus the flagship servers agora (telnet BBS) + descent (MUD)**. **~2.68 MB compressed, 5 layers** (agora/descent are ~14 MB apiece but ~99% zero-filled BSS heap-reservation → only +487 KB compressed; ~82 MB uncompressed on disk). `CMD` = `/bin/agnsh`. Net users (yo/dig/whirl clients; agora/descent servers) need mirshi's net band — pass `--net`/`--net-allow` (egress default-deny) or `--net-listen-any` to serve. All 16 delta tools verified `--agnos`-built + mirshi-run 2026-07-09 (agora serves telnet on :2323, descent on :4000). |
| `mirshi-fanout/` | `agnos-mirshi-fanout` | internal | The **native** container path: the real agnos-target userland ELF run on the host kernel via [mirshi](https://github.com/MacCracken/mirshi) (the AGNOS↔Linux syscall shim) — **no VM**. The Linux-swallow + cloud-deployability surface. Plan: [`../docs/development/planning/mirshi.md`](../docs/development/planning/mirshi.md). |
| `archive-pre-cyrius/` | — | **reference only** | 17 Rust-era Dockerfiles (Ubuntu + rustc). Historical; **do not resurrect** — AGNOS builds with `cycc` now, not Rust. |

### Build & run the stack

The images are a **clean layered stack** — each is the one below it + one delta
layer: `agnos-thin` (base) → `agnos-shell` (+agnsh) → `agnos-dev` (+CLI tools).
`build-dev.sh` compiles `mirshi` (Linux-target) + the agnos tools (agnos-target,
via cyrius) and docker-builds each tier `FROM` its parent. Stock Docker's default
seccomp blocks `ptrace`, so add the caps to run:

```sh
docker/build-dev.sh                  # build the whole stack: thin -> shell -> dev
PROFILE=thin  docker/build-dev.sh    # just the base (agnos-thin)
PROFILE=shell docker/build-dev.sh    # base + shell (agnos-thin, agnos-shell)

# interactive agnos shell (agnos-shell or agnos-dev; default CMD = /bin/agnsh):
docker run --rm -it --cap-add=SYS_PTRACE --security-opt seccomp=unconfined agnos-shell
#   at the [ASSIST] > prompt:  run /bin/iam  ·  run /bin/ark status  ·  exit
# the thin base (identity + package mgr, no shell):
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp=unconfined agnos-thin /bin/ark list
```

**`agnsh` is the interactive shell** (in `agnos-shell` and up). Its `run` / `&` /
`>` / `|` land on mirshi's exec band (`execwait#37` / `spawn_path#43` /
`exec_redirect#62` / `symlink#63`), complete as of **mirshi 1.10.2**. Proven:
`docker run agnos-shell` drops into agnsh, and `run /bin/<tool>` executes it in
the `FROM scratch` container — no VM.

## The installer is the ISO, not a Docker image

AGNOS installs from the **ISO `.img` / `.iso`** cut by `scripts/src/iso.cyr`
(gnoboot + agnos kernel + ext4 rootfs — see
[`../docs/development/iso-stage4-plan.md`](../docs/development/iso-stage4-plan.md); `iso.cyr` is
planned and not yet written), not from a container. `agnova` (the native installer) is the server-stage
successor. So there is **no `Dockerfile.installer`** — that role is the ISO.

## Later (deferred)

- **edge** image — thin / edge deployment.
- **aarch64** dev image — Pi / Apple-Silicon host, behind the x86 line.
- **stiva** image — the OCI runtime ([stiva](https://github.com/MacCracken/stiva)) is
  Cyrius-native now (the Rust crate survives only as its `rust-old/` parity oracle); a
  `stiva`-served container story is still post-0.1.0.

## What is NOT here — QEMU-in-a-container

Running the agnos kernel under QEMU *inside* a Docker container
("VM-in-a-container") is a **dead approach**. Kernel / net / SMP validation lives
on **QEMU-direct + iron**; the container test surface is **mirshi**
(userland-native, above). The old `net-sweep` / `sched-sweep` / `descent-sweep`
harnesses were retired 2026-07-07.

**This includes the dev image.** `Dockerfile.dev` deliberately ships **no
`qemu`/`ovmf`** — you build the kernel + ecosystem and assemble media inside it,
then run the QEMU smoke battery **QEMU-direct on the host** (parsa/dev box), not
in the container.
