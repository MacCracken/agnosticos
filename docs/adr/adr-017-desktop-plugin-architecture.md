# ADR-017: Desktop Plugin Architecture

**Status:** Proposed

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

The AGNOS desktop environment currently has a fixed set of modules: compositor, renderer,
AI features HUD, security UI, shell, and Wayland protocol handlers. Adding new desktop
functionality (themes, widgets, window management behaviors, input methods) requires
modifying the desktop-environment crate directly.

For an OS that emphasizes extensibility through agents, the desktop should follow the same
principle. Desktop plugins allow:

1. **Customization** — users can install themes, panel widgets, and window behaviors
2. **Integration** — third-party agents can extend the desktop UI (status indicators,
   notification handlers, custom overlays)
3. **Experimentation** — new desktop features can be prototyped as plugins before
   promotion to core

## Decision

### Plugin Model

- **Execution**: Plugins run as separate processes communicating with the compositor via
  a plugin protocol over Unix domain sockets. Plugins do NOT run inside the compositor
  process (crash isolation).
- **Sandboxing**: Desktop plugins are agents — they get the same Landlock+seccomp sandbox,
  manifest, and capability system. A theme plugin needs only filesystem read access; a
  panel widget needs Wayland surface creation permission.
- **Lifecycle**: Managed by the agent-runtime's service manager. Plugins declare
  `type = "desktop-plugin"` in their manifest and are started/stopped with the desktop.

### Plugin Types

| Type | Capability | Example |
|------|-----------|---------|
| **Theme** | Override color palette, font, icon set | Dark/light variants, high-contrast |
| **Panel widget** | Render into a reserved panel region | Clock, system monitor, weather |
| **Window decorator** | Custom title bar rendering | macOS-style, tiling indicators |
| **Input method** | Intercept keystrokes, produce composed input | CJK input, emoji picker |
| **Overlay** | Render transparent surface above all windows | Screen annotation, magnifier |
| **Notification handler** | Custom notification rendering/behavior | Do-not-disturb, grouping |

### Plugin Protocol

- **Transport**: Unix domain socket at `/run/agnos/desktop-plugins/{plugin_id}.sock`
- **Messages**: JSON-encoded, newline-delimited (consistent with agent IPC)
- **Core messages**:
  - `PluginInit { plugin_type, version, capabilities }` — handshake
  - `SurfaceGrant { surface_id, region }` — compositor grants a drawable region
  - `RenderRequest { surface_id }` — compositor requests a frame
  - `RenderResponse { surface_id, buffer_fd, format, size }` — plugin provides pixels
  - `InputEvent { event }` — forwarded input events (for input method plugins)
  - `ThemeUpdate { palette }` — theme plugin provides color/font overrides
  - `PluginShutdown` — graceful teardown
- **Buffer sharing**: Plugins render to shared memory (`wl_shm`-style) and pass the fd
  to the compositor. The compositor blits the plugin's buffer into the final frame.

### Discovery & Installation

- Desktop plugins are distributed via the agent marketplace (ADR-015) with
  `category = "desktop-plugin"` in their manifest.
- `agnos install acme/weather-widget` installs a panel widget.
- Desktop settings UI shows installed plugins with enable/disable toggles.
- Active plugins are listed in `fleet.toml` under a `[desktop-plugins]` section.

### Security Boundaries

- Plugins cannot read other windows' contents (no screenshot capability unless explicitly
  granted by the user via approval dialog).
- Overlay plugins are visually distinguished (compositor adds a subtle border) so users
  can tell the overlay from real UI — prevents UI spoofing.
- Input method plugins see keystrokes only for the focused window and only when the
  input method is active. They cannot log keystrokes silently.
- Theme plugins have no process capabilities beyond reading their own theme files.

## Consequences

### What becomes easier
- Desktop customization without recompiling the compositor
- Third-party desktop integrations (agent status widgets, media controls)
- A/B testing new desktop features as plugins before promotion

### What becomes harder
- Plugin protocol must be stable (breaking changes affect all plugins)
- Buffer sharing and compositor blitting adds rendering complexity
- Security review surface increases (each plugin type has different threat model)

### Risks
- Plugin rendering latency: slow plugins could delay frame composition.
  Mitigated by timeout (16ms budget per plugin per frame; if exceeded,
  compositor uses the last good buffer).
- UI spoofing via overlay plugins: mitigated by visual indicators and
  approval requirement for overlay capability.
- Input method keylogging: mitigated by audit logging all keystrokes
  forwarded to input method plugins and requiring explicit user activation.

## Alternatives Considered

### In-process plugins (shared library / WASM)
Rejected: a crashing plugin would take down the compositor. Process isolation is
worth the IPC overhead. WASM could be reconsidered for performance-critical plugins
(theme rendering) if IPC latency proves problematic.

### D-Bus for plugin protocol
Rejected: D-Bus adds latency and doesn't support efficient buffer sharing (fd passing).
Direct UDS with `SCM_RIGHTS` for fd passing is faster and already used for agent IPC.

### No plugin system (all features in core)
Rejected: defeats AGNOS's agent-centric philosophy. The desktop should be as extensible
as the rest of the system.

## References

- Phase 7 roadmap: `docs/development/roadmap.md` (Marketplace / Plugin architecture)
- Existing Wayland protocol: `userland/desktop-environment/src/wayland.rs`
- Agent IPC: `userland/agent-runtime/src/ipc.rs`
- ADR-015: Agent Marketplace (distribution channel for plugins)
- `wl_shm` protocol: Wayland core specification
