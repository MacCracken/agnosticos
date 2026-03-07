# ADR-023: Agnova — OS Installer

**Status:** Accepted
**Date:** 2026-03-07

## Context

AGNOS needs an installer to deploy the OS to bare metal or VMs. Unlike Debian's `d-i` or Arch's `pacstrap`, AGNOS installs from `.ark` packages via `ark`, with full disk encryption and security hardening enabled by default.

## Decision

Create `agnova` (AGNOS + Latin "nova") — a purpose-built installer with three interfaces:

### Install Modes
- **Server**: Base + agent-runtime + llm-gateway. No desktop packages.
- **Desktop**: Base + all server + compositor + shell + fonts + Mesa.
- **Minimal**: Bare minimum for containers. Base only.
- **Custom**: User selects individual packages.

### Default Disk Layout
```
/dev/sdX
  ├── sdX1: 512MB  EFI System Partition (vfat, /boot/efi)
  └── sdX2: rest   Root partition (ext4 or btrfs, /)
              └── LUKS2 encrypted (if enabled)
```

### Security by Default
- LUKS2 full-disk encryption (prompted, recommended)
- dm-verity for rootfs integrity
- Secure Boot key enrollment
- TPM2 measured boot (if hardware present)
- nftables firewall (default deny inbound)
- sigil trust enforcement: strict mode
- aegis security daemon enabled

### Install Pipeline
```
Validate → Partition → Format → Encrypt → Mount
  → Install base .ark packages via ark
  → Configure (hostname, locale, timezone)
  → Install bootloader (systemd-boot or GRUB2)
  → Create user (with agnoshi as default shell)
  → Setup security (LUKS, dm-verity, firewall, sigil keys)
  → First-boot prep (machine-id, aegis baseline)
  → Cleanup → Reboot
```

### First Boot
After reboot, argonaut:
1. Generates machine-specific sigil keys
2. aegis establishes integrity baseline
3. Connects to network (DHCP or configured static)
4. Optional: `ark update` to fetch latest packages
5. Presents agnoshi login prompt (or desktop)

### Interfaces
- **TUI**: ncurses-based for serial/SSH installs
- **CLI**: `agnova --config install.toml` for automated/scripted installs
- **GUI**: Flutter app running on minimal Wayland (future, post-v1.0)

## Consequences

### Positive
- Security enabled by default, not an afterthought
- Automated installs via TOML config (cloud, CI)
- Minimal base install ~200-300MB (vs 800MB+ for Debian minimal)
- ark handles package installation — single code path

### Negative
- Must support diverse hardware (disk controllers, EFI variants)
- LUKS setup adds install time (~30s for key derivation)
- Secure Boot enrollment is complex on some firmware

### Mitigations
- Hardware compatibility tested on common server/desktop platforms
- LUKS key derivation parallelized (argon2id)
- Secure Boot enrollment documented with screenshots

## Related
- ADR-018: LFS-Native Distribution (agnova installs the LFS base)
- ADR-021: Takumi (builds the .ark packages that agnova installs)
- ADR-022: Argonaut (agnova sets up argonaut as init)
- ADR-019: Sigil (agnova enrolls machine keys)
- ADR-020: Aegis (agnova enables aegis for first boot)
