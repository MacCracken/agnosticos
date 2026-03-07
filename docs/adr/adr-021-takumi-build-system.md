# ADR-021: Takumi — Package Build System

**Status:** Accepted
**Date:** 2026-03-07

## Context

ADR-018 defines AGNOS's path to an LFS-native distribution with `.ark` packages built from source. This requires a build system that:
1. Defines reproducible build recipes for ~50 base system packages
2. Resolves build dependency ordering (glibc before coreutils before bash...)
3. Applies security hardening flags consistently (PIE, RELRO, FORTIFY)
4. Produces signed `.ark` packages with complete file manifests
5. Integrates with sigil for package signing and ark for installation

## Decision

Create `takumi` (Japanese: master craftsman) — a TOML-based package build system.

### Recipe Format

Recipes are TOML files organized by tier:
```
build/recipes/
  base/         # Tier 0-1: toolchain + core utils
  security/     # Tier 2: crypto, auth, capabilities
  languages/    # Tier 3: python, rust, perl
  libraries/    # Tier 4: zlib, readline, ncurses
  services/     # Tier 5: eudev, dbus, nftables
  ai/           # Tier 6: CUDA, ONNX, PyTorch
  desktop/      # Tier 7: Wayland, Mesa, libinput
```

Each recipe specifies source URL + hash, dependencies, build commands, and security flags.

### .ark Package Format

```
package-name-1.0.0-1.ark
  ├── MANIFEST       (JSON: name, version, arch, depends, sizes, hashes)
  ├── SIGNATURE      (Ed25519 signature of MANIFEST via sigil)
  ├── FILES          (JSON: path, sha256, size, type for every installed file)
  └── data.tar.zst   (zstd-compressed file tree)
```

### Build Pipeline

```
Recipe (.toml)
  → Download source (verify SHA-256)
  → Extract to build dir
  → Apply patches (if any)
  → Configure (with security CFLAGS/LDFLAGS)
  → Build (make -j)
  → Test (make check, optional)
  → Install to fake root ($PKG)
  → Scan installed files → FILES manifest
  → Create MANIFEST
  → Sign with sigil → SIGNATURE
  → Pack data.tar.zst
  → Output: package-version-release.ark
```

### Security Hardening

Every package built by takumi gets security flags by default:
- `-fPIE -pie` (Position Independent Executable)
- `-Wl,-z,relro,-z,now` (Full RELRO)
- `-D_FORTIFY_SOURCE=2` (Buffer overflow detection)
- `-fstack-protector-strong` (Stack smashing protection)

Recipes can add additional flags or disable specific ones if needed.

### Dependency Resolution

Takumi resolves build order via topological sort:
- Tier 0 packages (glibc, gcc) have no AGNOS dependencies (cross-compiled)
- Each tier builds on the previous
- Circular dependencies are detected and rejected
- Diamond dependencies are handled correctly

## Consequences

### Positive
- Every AGNOS binary built with consistent security hardening
- Reproducible builds from source (recipe + source hash = deterministic)
- Complete file manifest enables integrity verification
- Signed packages integrate with sigil trust chain
- TOML recipes are human-readable and version-controllable

### Negative
- Building from source is slow (hours for full base system)
- Recipe maintenance burden (~50 packages to keep updated)
- Cross-compilation for Tier 0 is complex

### Mitigations
- CI farm builds packages; users download pre-built .ark files
- Automated CVE monitoring triggers recipe version bumps
- Tier 0 cross-toolchain built once, cached for months

## Related
- ADR-018: LFS-Native Distribution (takumi implements the build system)
- ADR-019: Sigil — Trust Verification (takumi signs packages via sigil)
- ADR-020: Aegis — Security Daemon (aegis scans .ark packages built by takumi)
