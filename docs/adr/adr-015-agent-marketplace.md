# ADR-015: Agent Marketplace Architecture

**Status:** Proposed

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

AGNOS has a mature agent lifecycle: packaging (`agnos install`), manifests, sandboxing,
capability negotiation, and fleet management. However, agent distribution is currently
manual — there is no centralized way to discover, publish, install, or review agents
built by third parties.

For AGNOS to develop an ecosystem, it needs a marketplace. The marketplace must solve:

1. **Discovery** — users need to find agents by capability, category, or keyword
2. **Trust** — agents run inside an OS with significant privileges; provenance and integrity matter
3. **Distribution** — efficient packaging, versioning, and delivery of agent bundles
4. **Quality** — users need signals (ratings, reviews, usage stats) to choose between agents
5. **Monetization** (future) — agent developers may want to charge for premium agents

## Decision

### Package Format

- **Agent bundle**: A signed tarball (`.agnos-agent`) containing:
  - `manifest.toml` — agent metadata, capabilities, dependencies, sandbox requirements
  - `agent.wasm` or native binary (per-arch)
  - `sandbox.toml` — seccomp profile, Landlock rules, network policy
  - `README.md` — user-facing documentation
  - `LICENSE` — required, must be OSI-approved or explicitly proprietary
  - `signature.sig` — Ed25519 signature over the bundle hash
- **Naming**: `publisher/agent-name` (e.g., `acme/web-scanner`, `agnos/file-monitor`)
- **Versioning**: SemVer required. CalVer accepted for first-party agents.

### Registry Service

- **Protocol**: REST API (OpenAPI-specified) served by a central registry
- **Endpoints**:
  - `GET /v1/agents` — search/list with filters (capability, category, publisher)
  - `GET /v1/agents/{publisher}/{name}` — agent metadata
  - `GET /v1/agents/{publisher}/{name}/versions` — version history
  - `GET /v1/agents/{publisher}/{name}/{version}/download` — fetch bundle
  - `POST /v1/agents` — publish a new agent (authenticated)
  - `POST /v1/agents/{publisher}/{name}/reviews` — submit a review
  - `GET /v1/agents/{publisher}/{name}/reviews` — list reviews
- **Self-hostable**: The registry is a standalone Rust binary that can run on any AGNOS
  instance. The default public registry is `registry.agnos.org`.
- **Mirroring**: Enterprises can mirror the public registry behind their firewall.
  `agnos config set registry.mirror https://internal.corp/agnos-registry`

### Trust Model

- **Publisher identity**: Ed25519 keypair. Public key registered with the registry.
  Publishers sign their bundles offline; the registry verifies signatures on upload.
- **Trust levels**:
  - `verified` — publisher identity confirmed (domain verification or GPG cross-sign)
  - `community` — any registered publisher, no verification
  - `agnos-official` — first-party agents maintained by the AGNOS team
- **Transparency log**: All published bundles are recorded in an append-only log
  (similar to Go's sum database). Clients can verify that the bundle they downloaded
  matches the log entry. Prevents registry compromise from silently replacing bundles.
- **Sandbox enforcement**: The marketplace displays the sandbox requirements from the
  manifest. Users see upfront what the agent needs (network, filesystem, syscalls).
  The agent cannot request more capabilities at runtime than declared in the manifest.
- **Revocation**: The registry can yank a version (remains downloadable for existing
  users but hidden from search and install). Full removal requires security justification.

### Client Integration

- **CLI**: `agnos install acme/web-scanner@1.2.0`, `agnos search "pdf parser"`,
  `agnos update`, `agnos publish`
- **Dependency resolution**: Agents can declare dependencies on other agents
  (`requires = ["agnos/llm-tool@^1.0"]`). The package manager resolves versions
  using the existing dependency DAG in `service_manager.rs`.
- **Caching**: Downloaded bundles are cached in `/var/cache/agnos/agents/`. SHA-256
  verified against the transparency log before installation.
- **Rollback**: `agnos rollback acme/web-scanner` reverts to the previous installed version.

### Rating & Review System

- **Ratings**: 1-5 stars, per version.
- **Reviews**: Free-text, attached to a specific version. Markdown supported.
- **Abuse prevention**: One review per user per agent version. Rate limiting on submissions.
- **Automated signals**: Install count, active installs (heartbeat-based), error rate
  (opt-in telemetry), age of latest version.

## Consequences

### What becomes easier
- Third-party developers can distribute agents to all AGNOS users
- Users discover agents by capability rather than manually finding bundles
- Trust is cryptographically enforced, not just policy

### What becomes harder
- AGNOS team must maintain a public registry service
- Publisher key management adds operational burden for agent developers
- Dependency resolution across marketplace agents adds complexity

### Risks
- Supply chain attacks via compromised publisher keys — mitigated by transparency log
  and sandbox enforcement (a compromised agent is still sandboxed)
- Registry availability — mitigated by caching and mirroring support
- Spam/low-quality agents — mitigated by review system and verified publisher tiers

## Alternatives Considered

### Git-based distribution (like Go modules)
Rejected as the sole mechanism: git repos don't provide discovery, ratings, or
pre-built binaries. However, `agnos install github:user/repo` could be supported
as a secondary installation path.

### OCI/Docker registry for agent bundles
Rejected: OCI registries are designed for container images, not agent bundles.
The metadata model doesn't fit (capabilities, sandbox requirements, reviews).
A purpose-built registry is simpler and more expressive.

### Flatpak/Snap-style distribution
Rejected: these are designed for desktop applications with GUI, not headless agents.
The sandbox models don't align with AGNOS's Landlock+seccomp approach.

## References

- Existing package manager: `userland/agent-runtime/src/package_manager.rs`
- Agent manifests: `userland/agnos-common/src/lib.rs` (AgentManifest)
- Capability negotiation: `userland/agent-runtime/src/registry.rs`
- Go sum database design: https://go.dev/ref/mod#checksum-database
