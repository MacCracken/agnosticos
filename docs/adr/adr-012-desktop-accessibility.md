# ADR-012: Desktop Accessibility & Interaction Foundations

**Status:** Accepted

**Date:** 2026-03-06

**Authors:** AGNOS Team

## Context

The AGNOS desktop environment has a Wayland compositor with Dispatch traits, a renderer, AI
features HUD, and security UI. However, it lacks several foundational capabilities expected
of a usable desktop:

1. **No accessibility support** — screen readers, keyboard navigation, and high-contrast themes
   are absent. This is not optional for a real OS; it is a legal and ethical requirement.
2. **No clipboard** — agents and applications cannot copy/paste data. The `wl_data_device`
   protocol is not implemented.
3. **No visual agent ownership** — users cannot tell which agent owns a window or what its
   trust level is. This is a security UX gap.
4. **No popups or tooltips** — `xdg_popup` and `xdg_positioner` are noted as unimplemented.
   Right-click menus and dropdowns do not work.
5. **No touch gestures** — pinch-to-zoom and swipe navigation are expected on modern devices.

## Decision

### Accessibility Foundation (`desktop-environment/accessibility.rs`)

- **Standard**: AT-SPI2 (Assistive Technology Service Provider Interface), the Linux desktop
  accessibility standard. Communicates over D-Bus.
- **Implementation**:
  - `AccessibilityNode` tree mirroring the window/widget hierarchy
  - Each node has: `role` (Window, Button, Text, Menu, etc.), `name`, `description`, `state` (focused, selected, expanded)
  - `AccessibilityBridge` connects the node tree to the AT-SPI2 D-Bus interface
  - Screen readers (Orca) can enumerate windows, read labels, and navigate
- **Keyboard navigation**:
  - All interactive elements reachable via Tab / Shift+Tab
  - `FocusManager` tracks focus chain per window
  - Arrow keys navigate within composite widgets (menus, lists)
  - Escape closes popups/menus, Enter activates focused element
- **Visual accessibility**:
  - High-contrast theme (`ThemeVariant::HighContrast`) with WCAG AA contrast ratios (4.5:1 minimum)
  - Focus indicators: 2px solid outline on focused elements
  - Minimum touch target size: 44x44 CSS pixels equivalent
  - Reduced-motion preference respected (no animations when set)
- **Testing**: Accessibility tree validation tests ensure all UI elements have roles and labels.

### Clipboard Integration (`desktop-environment/clipboard.rs`)

- **Protocol**: `wl_data_device_manager`, `wl_data_device`, `wl_data_source`, `wl_data_offer`
  from the Wayland core protocol.
- **MIME types**: `text/plain;charset=utf-8`, `text/plain`, `text/uri-list`, `image/png`.
  Extensible per data source.
- **Flow**:
  1. Source client creates `wl_data_source` with offered MIME types
  2. User performs copy (Ctrl+C or selection)
  3. Compositor stores the offer reference (not the data — lazy transfer)
  4. Target client requests data via `wl_data_offer.receive(mime_type, fd)`
  5. Source writes data to the fd, compositor mediates
- **Primary selection**: `wp_primary_selection` protocol for middle-click paste.
- **Security**: Clipboard access is logged in the audit trail when crossing agent trust boundaries.
  Agents in different sandbox domains require explicit clipboard permission in their manifest.

### Agent Window Ownership Indicators (`desktop-environment/security_ui.rs`)

- **Visual badge**: Every agent-owned window displays a small badge in the title bar:
  - Color-coded by trust level: green (verified), yellow (unverified), red (restricted)
  - Agent name and sandbox status on hover tooltip
  - Badge is compositor-drawn (not client-side), so agents cannot fake it
- **Window decoration**: Agents with restricted sandbox get a distinct border color.
- **Notification**: When an agent opens a new window, a brief toast shows "Agent X opened a window"
  with the trust level.

### XDG Popup & Positioner (`desktop-environment/wayland.rs`)

- **Protocol**: `xdg_popup` + `xdg_positioner` from `xdg-shell`.
- **Positioner**: Calculates popup placement relative to anchor rect, with constraint adjustment
  (flip, slide, resize) when the popup would extend beyond output bounds.
- **Popup lifecycle**: `create_popup()` -> `configure` event -> client renders -> `popup_done`
  on dismiss (click outside or Escape).
- **Grab**: Popup grabs prevent interaction with other surfaces until dismissed (for menus).
- **Nesting**: Popups can parent other popups (submenu chains). Max depth 8 (prevents abuse).

### Multi-Touch Gesture Support (`desktop-environment/gestures.rs`)

- **Protocol**: `zwp_pointer_gestures_v1` (swipe, pinch, hold).
- **Gestures recognized**:
  - 3-finger swipe left/right: workspace switch
  - 3-finger swipe up: overview / task switcher
  - 2-finger pinch: zoom (forwarded to focused application)
  - 2-finger scroll: standard scroll (already implemented via `wl_pointer.axis`)
- **Gesture detection**: State machine with configurable thresholds (minimum distance, finger count).
- **Accessibility integration**: All gesture actions also available via keyboard shortcuts.
  Gestures are disabled when `reduced-motion` is set.

## Consequences

### What becomes easier
- Users with disabilities can use AGNOS with screen readers and keyboard
- Standard desktop interactions (copy/paste, right-click menus) work
- Users can visually identify agent-owned windows and their trust level
- Touch devices are usable

### What becomes harder
- Compositor code complexity increases significantly (a11y tree, clipboard, popups)
- AT-SPI2 requires D-Bus, adding a runtime dependency
- Accessibility testing requires integration tests with AT-SPI2 introspection

### Risks
- AT-SPI2 bridge performance: large accessibility trees can be slow to traverse.
  Mitigated by lazy node creation (only expand on screen reader request).
- Clipboard data leakage between sandboxed agents. Mitigated by audit logging and
  explicit clipboard permission in agent manifest.
- Popup grab loops (malicious client creates unclosable popup). Mitigated by compositor
  timeout (5s grab without interaction auto-dismisses) and max popup count per client.

## Alternatives Considered

### Custom accessibility protocol instead of AT-SPI2
Rejected: screen readers (Orca, NVDA via bridge) only speak AT-SPI2 on Linux. Inventing
a custom protocol means no screen reader support. Standards compliance is non-negotiable.

### Clipboard via shared memory (no Wayland protocol)
Rejected: bypasses Wayland's security model. The `wl_data_device` protocol exists precisely
to mediate clipboard access through the compositor. Direct shared memory would allow any
client to read clipboard contents without permission.

### Skip touch gestures (keyboard + mouse only)
Rejected: AGNOS targets modern hardware including tablets and touchscreens. Gesture support
is expected UX. The protocol support (`zwp_pointer_gestures_v1`) is straightforward.

## References

- Phase 6.8 roadmap: `docs/development/roadmap.md` (Desktop & Accessibility section)
- AT-SPI2 specification: https://www.freedesktop.org/wiki/Accessibility/AT-SPI2/
- Wayland `wl_data_device` protocol: https://wayland.freedesktop.org/docs/html/apa.html
- WCAG 2.1 AA: https://www.w3.org/WAI/WCAG21/quickref/
- `xdg_popup` protocol: `xdg-shell` stable specification
- Existing Wayland types: `userland/desktop-environment/src/wayland.rs`
- Existing security UI: `userland/desktop-environment/src/security_ui.rs`
