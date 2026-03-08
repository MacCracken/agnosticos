# ADR-005: Desktop Environment

**Status:** Accepted
**Date:** 2026-03-07

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
