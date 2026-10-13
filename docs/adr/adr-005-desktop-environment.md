# ADR-005: Desktop Environment

**Status:** Accepted — ⚠ **Superseded (display protocol) 2026-07-06** (Wayland refused; see the note below)
**Date:** 2026-03-07

> **Display-protocol supersession (2026-07-06).** The *Wayland Compositor
> (aethersafha)* decision below — Wayland as the display protocol, built on
> `smithay`, with XWayland for legacy X11 — is **superseded**. AGNOS will not
> port Wayland; aethersafha speaks a **native, first-principles display protocol**
> (Wayland refused, not ported). Foreign Wayland/X11 clients, if ever wanted,
> arrive only through the firewalled `mehman` swallow lane — never the native
> path. Canonical rationale: [`../design-patterns.md`](../design-patterns.md)
> §"Sovereign substitution at the protocol layer — Wayland refused"; per-repo
> decision: `aethersafha/docs/adr/0001-native-display-protocol.md`; cross-repo
> seam: `dhancha/docs/development/sovereign-desktop.md`. The rest of this ADR
> (accessibility, plugins, agent-window ownership, capture/recording, gestures)
> remains in force; preserved as historical record.
>
> **Update 2026-08-05 — the superseding path has SHIPPED; it is no longer
> theoretical.** ⭐ Iron-proven 2026-08-03 on archaemenid at `smp: cpus online: 4`:
> aethersafha composited **two real client windows** — setu's `present_probe` and
> `crab`'s dual-pane file manager — 278 frames, keys delivered to the client, clean
> Esc quit. Scope: the CPU blit path; the GPU composite path is iron-proven
> separately and only for a single opaque surface. Detail:
> `aethersafha/docs/development/planning/desktop.md`.
>
> ⛔ **The transport premise has moved again.** TCP on loopback:7700 — the transport
> the native protocol first rode on agnos — is **retired as the wrong primitive** for
> local display IPC (operator ruling 2026-08-03). The retirement is architectural,
> not empirical: it is *not* a claim that it never worked. The replacement is a
> kernel channel band (`chan_*`, syscall `#97`), which by rule gets no codename;
> agnos 1.56.40 is that cycle — **open and not burned**. The band's early bites *have*
> landed in the kernel (`#97 chan_op` answers `CH_CAPS` only; every other op is
> `BADOP`), but **no consumer has cut over** — setu and aethersafha still ride the TCP
> transport. Design + migration: `agnos/docs/development/planning/ipc.md`
> §9. Live versions and burn status:
> [`../development/state.md`](../development/state.md).

## Context

AGNOS provides a desktop environment for human-AI collaboration. The compositor must support modern GPU acceleration, fine-grained security, AI-augmented window management, and extensibility through plugins.

## Decisions

### Wayland Compositor (aethersafha)

Wayland is the display protocol. The compositor is built on the `smithay` crate with:

- Custom protocols for agent window management (`agnos_agent_surface_v1`)
- Security extensions for screenshot/access control (`zwp_security_context_v1`)
- XWayland support for legacy X11 applications
- AI context protocol for workspace management

**Alternative rejected:** X11 (any client can access any other's windows — fundamentally insecure).

### Accessibility

AT-SPI2 (Linux desktop standard) over D-Bus:

- `AccessibilityNode` tree mirroring window/widget hierarchy with roles, names, states
- Keyboard navigation: Tab/Shift+Tab for all interactive elements, arrow keys within composites
- High-contrast theme (WCAG AA, 4.5:1 minimum contrast ratio)
- Focus indicators (2px solid outline), minimum 44x44 touch targets
- Reduced-motion preference respected

### Desktop Plugins

Plugins run as separate sandboxed processes (crash isolation), communicating via UDS:

| Type | Capability | Example |
|------|-----------|---------|
| Theme | Color palette, fonts, icons | Dark/light, high-contrast |
| Panel widget | Render in panel region | Clock, system monitor |
| Window decorator | Custom title bars | Tiling indicators |
| Input method | Keystroke interception | CJK input, emoji picker |
| Overlay | Transparent layer above windows | Screen annotation |
| Notification handler | Custom notification behavior | Do-not-disturb |

**Security boundaries:**
- Plugins cannot read other windows' contents
- Overlays have compositor-drawn borders (prevents UI spoofing)
- Input methods only see keystrokes for the focused window when active
- Theme plugins have no capabilities beyond reading their own files
- 16ms per-frame timeout per plugin; compositor uses last good buffer on miss

Plugins are distributed via the agent marketplace with `category = "desktop-plugin"`.

**Alternative rejected:** In-process plugins (crash takes down compositor).

### Agent Window Ownership

Compositor-drawn visual indicators (cannot be faked by agents):

- Color-coded trust badge in title bar (green=verified, yellow=unverified, red=restricted)
- Agent name and sandbox status on hover
- Restricted-sandbox agents get distinct border color
- Toast notification when an agent opens a new window

### Screen Capture and Recording

Built-in compositor feature (not a plugin) providing screenshot and recording capabilities:

- **Capture targets**: Full screen, per-window (by surface ID), arbitrary region
- **Formats**: PNG (self-contained encoder), BMP, raw ARGB8888 — no external image crate dependency
- **Security controls**:
  - Secure mode (`set_secure_mode(true)`) blocks all captures globally
  - Per-agent permission grants with allowed target kinds (full_screen, window, region)
  - Time-based permission expiry
  - Per-agent rate limiting (configurable captures/minute)
  - All captures audit-logged
- **Recording**: Frame-by-frame with poll-based streaming (agents fetch frames via sequence numbers)
  - Configurable frame interval, max frames, max duration
  - One active recording per agent enforced
  - Ring buffer retains last 100 frames to bound memory
- **REST API**: Exposed through daimon (port 8090) at `/v1/screen/*`

**Alternative rejected:** Wayland `wlr-screencopy-unstable-v1` protocol (insufficient security controls — any Wayland client could request captures without compositor-enforced per-agent permissions).

**Alternative rejected:** Plugin-based capture (plugin cannot access compositor internals needed for efficient framebuffer reads).

### Clipboard, Popups, and Gestures

- **Clipboard** — `wl_data_device` protocol with lazy transfer, primary selection, audit logging across trust boundaries
- **Popups** — `xdg_popup` + `xdg_positioner` with constraint adjustment, max depth 8
- **Touch gestures** — 3-finger swipe (workspace switch, overview), 2-finger pinch (zoom), all actions available via keyboard

## Consequences

### Positive
- Security-first desktop (per-application sandboxing, compositor-enforced trust indicators)
- Accessible to users with disabilities (AT-SPI2, keyboard nav, high contrast)
- Extensible via plugins without recompiling the compositor
- Standard Wayland compatibility for existing applications

### Negative
- XWayland needed for legacy apps (additional attack surface)
- AT-SPI2 requires D-Bus runtime dependency
- Plugin protocol must remain stable (breaking changes affect all plugins)
- Compositor complexity is significant
