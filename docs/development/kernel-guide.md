# AGNOS Kernel Development Guide

> **Kernel Version:** 6.6 LTS | **Last Updated:** 2026-03-10

This guide covers building the AGNOS kernel, writing custom kernel modules, and contributing to the kernel layer.

---

## Overview

AGNOS uses Linux 6.6 LTS as its kernel, with:
- Custom `agnos_defconfig` for x86_64
- AGNOS-specific kernel modules in `kernel/modules/`
- Patches in `kernel/6.6-lts/patches/`
- Security hardening (Landlock, seccomp, IMA, dm-verity, Secure Boot)

The kernel interface is exposed to userspace via **agnosys** (`userland/agnos-sys/`), which provides safe Rust wrappers for syscalls, LSM hooks, and hardware interfaces.

---

## Directory Structure

```
kernel/
├── 6.6-lts/
│   ├── configs/
│   │   └── agnos_defconfig      # AGNOS kernel configuration
│   └── patches/                 # Version-specific patches
├── 6.x-stable/                  # Tracking latest stable
├── 7.0-devel/                   # Development branch
├── config/
│   └── agnos_defconfig          # Shared base config
├── patches/                     # Cross-version patches
├── modules/                     # AGNOS custom kernel modules (C)
├── build/                       # Build scratch area
└── sources/                     # Downloaded kernel tarballs
```

---

## Building the Kernel

### Prerequisites

```bash
# Debian/Ubuntu
sudo apt install build-essential bc kmod flex bison libssl-dev \
  libelf-dev dwarves pahole

# Or use the full dependency installer
./scripts/install-build-deps.sh
```

### Build Commands

```bash
# Build the default (6.6-lts) kernel
./scripts/build-kernel.sh

# Build a specific version
./scripts/build-kernel.sh -v 6.6-lts
./scripts/build-kernel.sh -v 6.x-stable
./scripts/build-kernel.sh -v 7.0-devel

# Verbose output
./scripts/build-kernel.sh -v 6.6-lts -V

# Clean rebuild
./scripts/build-kernel.sh -v 6.6-lts --clean
```

### What the Build Script Does

1. Sets up the build environment
2. Copies kernel source (rsync-optimized)
3. Applies AGNOS patches from `kernel/$VERSION/patches/`
4. Loads config from `kernel/$VERSION/configs/agnos_defconfig`
5. Merges additional config fragments
6. Builds kernel image + modules (`make -j$(nproc)`)
7. Builds AGNOS custom modules from `kernel/modules/`
8. Packages as tarball with checksums

Output: `build/kernel/linux-VERSION-agnos.tar.gz`

---

## Kernel Configuration

### AGNOS Defaults

The `agnos_defconfig` enables:

**Security:**
- `CONFIG_SECURITY_LANDLOCK=y` — filesystem sandboxing
- `CONFIG_SECCOMP=y`, `CONFIG_SECCOMP_FILTER=y` — syscall filtering
- `CONFIG_IMA=y` — Integrity Measurement Architecture
- `CONFIG_DM_VERITY=y` — verified block devices
- `CONFIG_EFI_SECURE_BOOT=y` — UEFI Secure Boot
- `CONFIG_SECURITY_LOCKDOWN_LSM=y` — kernel lockdown
- `CONFIG_HARDENED_USERCOPY=y` — hardened memory copies

**Filesystems:**
- ext4, btrfs, xfs, vfat, squashfs, overlayfs, FUSE
- `CONFIG_FUSE_FS=m` — for user-space filesystems

**Networking:**
- `CONFIG_NET_NS=y` — network namespaces (agent isolation)
- `CONFIG_NETFILTER=y` — nftables firewall
- `CONFIG_BRIDGE=m` — network bridging

**Drivers:**
- `CONFIG_DRM=m` — Direct Rendering Manager
- `CONFIG_DRM_NOUVEAU=m` — NVIDIA open-source
- `CONFIG_DRM_AMDGPU=m` — AMD GPU
- `CONFIG_DRM_I915=m` — Intel GPU
- `CONFIG_SND_HDA_INTEL=m` — Intel HD Audio
- `CONFIG_USB_XHCI_HCD=y` — USB 3.x
- `CONFIG_THUNDERBOLT=m` — Thunderbolt/USB4

### Customizing the Config

```bash
# Start from AGNOS defaults
cp kernel/config/agnos_defconfig kernel/build/.config

# Interactive configuration
cd kernel/build
make menuconfig    # ncurses TUI
make nconfig       # newer ncurses TUI
make xconfig       # Qt GUI (requires qt5)

# Save back to AGNOS config
cp .config ../../kernel/config/agnos_defconfig
```

---

## Writing Kernel Modules

### Module Template

Create a new file in `kernel/modules/`:

```c
/*
 * kernel/modules/agnos_example.c
 *
 * Example AGNOS kernel module.
 * Demonstrates the pattern for custom kernel extensions.
 */
#include <linux/init.h>
#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/fs.h>

MODULE_LICENSE("GPL");
MODULE_AUTHOR("AGNOS Project");
MODULE_DESCRIPTION("Example AGNOS kernel module");
MODULE_VERSION("1.0");

/* Module parameters */
static int debug_level = 0;
module_param(debug_level, int, 0644);
MODULE_PARM_DESC(debug_level, "Debug verbosity (0=off, 1=info, 2=trace)");

static int __init agnos_example_init(void)
{
    pr_info("agnos_example: loaded (debug_level=%d)\n", debug_level);
    return 0;
}

static void __exit agnos_example_exit(void)
{
    pr_info("agnos_example: unloaded\n");
}

module_init(agnos_example_init);
module_exit(agnos_example_exit);
```

### Building a Module

**Standalone (for development):**

```bash
# Create a build directory
mkdir -p /tmp/mymodule
cp kernel/modules/agnos_example.c /tmp/mymodule/

# Create Makefile
cat > /tmp/mymodule/Makefile << 'EOF'
obj-m += agnos_example.o
KDIR ?= /lib/modules/$(shell uname -r)/build
all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules
clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean
EOF

# Build
cd /tmp/mymodule && make
```

**As part of AGNOS build:**

1. Place your `.c` file in `kernel/modules/`
2. Run `./scripts/build-kernel.sh` — it automatically builds all modules in that directory
3. Modules are included in the kernel package tarball

### Loading and Testing

```bash
# Load the module
sudo insmod agnos_example.ko debug_level=1

# Check it loaded
lsmod | grep agnos_example
dmesg | tail -5

# Module info
modinfo agnos_example.ko

# Unload
sudo rmmod agnos_example
```

### Self-Hosting Validation

The self-hosting validation script tests kernel module compilation:

```bash
# Validate kernel module build capability
./scripts/selfhost-validate.sh --phase kernel

# This will:
# 1. Check kernel headers exist
# 2. Compile a test module (agnos_test.ko)
# 3. Verify modinfo works
# 4. Build each module in kernel/modules/
```

---

## Kernel-Userspace Interface (agnosys)

AGNOS custom kernel features are exposed to userspace via the **agnosys** crate (`userland/agnos-sys/`). This provides safe Rust wrappers organized into 16 modules:

| Module | Kernel Feature | Description |
|--------|---------------|-------------|
| `audit` | Audit subsystem | Cryptographic audit log |
| `mac` | LSM hooks | Mandatory Access Control |
| `netns` | Network namespaces | Per-agent network isolation |
| `dmverity` | dm-verity | Block device integrity verification |
| `luks` | dm-crypt/LUKS2 | Encrypted storage |
| `ima` | IMA | Integrity Measurement Architecture |
| `tpm` | TPM 2.0 | Trusted Platform Module |
| `secureboot` | EFI Secure Boot | Boot integrity chain |
| `certpin` | x509/SPKI | Certificate pinning verification |
| `bootloader` | EFI variables | Bootloader management |
| `journald` | Journal API | Structured logging |
| `udev` | udev/netlink | Device event monitoring |
| `fuse` | FUSE | User-space filesystem support |
| `pam` | PAM | Pluggable authentication |
| `update` | System update | Atomic update mechanism |
| `llm` | Hardware detect | GPU/accelerator detection for LLM inference |

### Adding a New Kernel Interface

1. **Write the kernel module** in `kernel/modules/`
2. **Create the Rust wrapper** in `userland/agnos-sys/src/`
3. **Add tests** — agnosys has 750+ tests
4. **Wire it up** — add `pub mod mymodule;` in `agnos-sys/src/lib.rs`

Example Rust wrapper pattern:

```rust
//! myfeature — Rust wrapper for agnos_myfeature kernel module

use std::fs;
use std::io;
use std::path::Path;

/// Read a value from the kernel module's sysfs interface.
pub fn read_status() -> io::Result<String> {
    fs::read_to_string("/sys/module/agnos_myfeature/parameters/status")
        .map(|s| s.trim().to_string())
}

/// Write a command to the kernel module.
pub fn send_command(cmd: &str) -> io::Result<()> {
    fs::write("/sys/module/agnos_myfeature/parameters/command", cmd)
}
```

---

## Patches

### Applying Patches

Patches live in `kernel/6.6-lts/patches/` and are applied in alphabetical order during the build:

```
kernel/6.6-lts/patches/
├── 0001-agnos-security-hardening.patch
├── 0002-agnos-custom-syscalls.patch
└── 0003-agnos-driver-quirks.patch
```

### Creating a Patch

```bash
# In the kernel source tree
cd kernel/build/linux-6.6.72

# Make your changes
vim drivers/char/agnos_feature.c

# Create patch
git diff > ../../../kernel/6.6-lts/patches/0004-my-feature.patch

# Or for a commit:
git format-patch -1 HEAD -o ../../../kernel/6.6-lts/patches/
```

### Patch Naming Convention

```
NNNN-agnos-category-description.patch
```

- `NNNN` — 4-digit sequence number
- `agnos-` — always prefix with agnos
- `category` — `security`, `driver`, `config`, `fix`, `perf`
- `description` — brief description in kebab-case

---

## Testing

### Automated Testing

```bash
# Full kernel build + module test
./scripts/build-kernel.sh -v 6.6-lts

# Self-hosting kernel validation
./scripts/selfhost-validate.sh --phase kernel

# QEMU boot test (builds ISO, boots it, runs smoke tests)
make qemu-boot-test
```

### QEMU Development Cycle

For fast iteration on kernel changes:

```bash
# Build kernel only (no full ISO rebuild)
./scripts/build-kernel.sh -v 6.6-lts

# Boot with custom kernel in QEMU
qemu-system-x86_64 \
  -kernel build/kernel/vmlinuz \
  -initrd build/kernel/initramfs.img \
  -append "root=/dev/sda1 console=ttyS0" \
  -serial stdio \
  -m 2048 \
  -nographic
```

### agnosys Tests

```bash
# Run all kernel interface tests
cargo test -p agnos-sys --lib

# Run specific module tests
cargo test -p agnos-sys --lib audit
cargo test -p agnos-sys --lib netns
```

---

## Security Considerations

### Module Signing

In production AGNOS builds with Secure Boot enabled:
- All kernel modules must be signed with the AGNOS module signing key
- Unsigned modules are rejected at load time
- Use `CONFIG_MODULE_SIG_FORCE=y` in production configs

### Coding Standards

- All modules must use `MODULE_LICENSE("GPL")` (for full kernel API access)
- No `printk` in hot paths — use `pr_debug()` gated by debug parameters
- Validate all userspace inputs via `copy_from_user()` / `copy_to_user()`
- Use `kzalloc()` (not `kmalloc()`) to prevent information leaks
- Check return values from all kernel API calls
- Never sleep with locks held

### Review Checklist

Before submitting a kernel module:

- [ ] Compiles with `W=1` (extra warnings) clean
- [ ] Passes `scripts/checkpatch.pl` (kernel coding style)
- [ ] Module loads and unloads cleanly (no memory leaks)
- [ ] `modinfo` shows correct metadata
- [ ] Corresponding agnosys Rust wrapper exists (if applicable)
- [ ] Self-hosting validation passes with the new module

---

*See also: [ARCHITECTURE.md](/docs/ARCHITECTURE.md) for system overview, [Security Guide](/docs/security/security-guide.md) for hardening details.*
