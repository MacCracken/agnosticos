# aethersafha -- AGNOS Desktop Environment

> **Last Updated**: 2026-03-07 | **Version**: 2026.3.7

**aethersafha** (Greek *aether* + Arabic *safha* = surface) is the AI-augmented Wayland compositor and desktop environment for AGNOS.

## Architecture

```
userland/desktop-environment/
  src/
    compositor.rs      Wayland compositor core, Dispatch traits, workspaces
    renderer.rs        Software framebuffer renderer, scene graph, damage tracking
    wayland.rs         Wayland protocol bridge (feature-gated)
    accessibility.rs   AccessibilityTree, AT-SPI2 bridge, high-contrast themes
    plugin_host.rs     Plugin lifecycle, IPC, sandboxing
    xwayland.rs        XWaylandManager, X11 property translation
    shell_integration.rs  System tray, window mgmt, notification bridge
    theme_bridge.rs    AGNOS -> Flutter ThemeData, platform channels
    security_ui.rs     Security dashboard, permission UI, kill switch
    ai_features.rs     Context detection, proactive suggestions, Agent HUD
    shell.rs           Panel, launcher, notification system
    apps.rs            Built-in apps (terminal, file manager, agent/audit/model managers)
    gestures.rs        Multi-touch gesture recognition
```

## Core Compositor

The compositor (`compositor.rs`) implements Wayland `Dispatch` traits and manages surfaces, windows, workspaces, and input routing.

- **Window states**: Normal, Maximized, Fullscreen, Floating
- **Workspaces**: 4 context-aware (Development, Communication, Design, General)
- **Input**: Keyboard, pointer, and touch forwarding via `InputEvent` / `InputAction`
- **Backend abstraction**: `CompositorBackend` / `WaylandBackend` traits

## Renderer

Software ARGB8888 framebuffer renderer (`renderer.rs`) targeting DRM/KMS scanout or Wayland SHM submission.

- **Scene graph**: `SceneGraph` with `SceneSurface` nodes, layer ordering, opacity
- **Damage tracking**: `DamageTracker` for incremental repaints
- **Decorations**: Server-side window decorations with `DecorationHit` / `ResizeEdge`
- **High-contrast**: Integrates `HighContrastTheme` from the accessibility module
- **Criterion benchmarks**: `compositor_benchmarks.rs` -- render frame and scene graph operations at 1/5/10/20 surfaces

## Wayland Protocol Support

The `wayland.rs` module bridges the internal compositor to the Wayland wire protocol.

**Core protocols:**

| Protocol | Purpose |
|---|---|
| `wl_compositor` | Surface factory |
| `wl_surface` | Client surface lifecycle |
| `wl_shm` | Shared-memory pixel buffers (ARGB8888, XRGB8888) |
| `wl_seat` | Input device grouping (keyboard, pointer) |
| `wl_output` | Monitor geometry and mode advertisement |
| `xdg_wm_base` | XDG Shell window management |
| `xdg_surface` | Window surface role |
| `xdg_toplevel` | Top-level window semantics |

**Extension protocols:**

`wl_data_device` (clipboard), `zwp_text_input` (IME), `zxdg_decoration` (SSD/CSD negotiation), `wp_viewporter` (viewport scaling), `wp_fractional_scale` (HiDPI)

**Feature gate:** The `wayland` Cargo feature enables real `wayland-server` integration (depends on `wayland-server 0.31`, `wayland-protocols 0.31`, `wayland-protocols-wlr 0.2`). Without it, a compile-time stub is provided so the crate builds and tests run on any platform.

## Accessibility

The accessibility module (`accessibility.rs`) provides AT-SPI2 bridge foundations.

- **AccessibilityTree**: Hierarchical tree of `AccessibleNode` elements with semantic `AccessibilityRole` (Window, Button, TextInput, Menu, Slider, etc.)
- **AccessibilityState**: Dynamic state per node -- focused, selected, expanded, checked, disabled, hidden, value, description
- **AccessibleAction**: Click, Focus, and other programmatic actions
- **Keyboard navigation**: Tab/Shift-Tab traversal via `KeyboardNavConfig`
- **Screen reader**: Announcement queue for live region changes
- **High-contrast themes**: `HighContrastTheme` definitions consumed by the renderer and theme bridge

## Plugin Host

The plugin host (`plugin_host.rs`, per ADR-005) manages third-party desktop plugins as crash-isolated processes communicating over Unix domain sockets.

**Plugin types (`PluginType`):**

| Type | Description |
|---|---|
| `Theme` | Visual theme provider (colors, fonts, decorations) |
| `PanelWidget` | Panel/tray widgets (clock, status items) |
| `WindowDecorator` | Custom window decoration renderer |
| `InputMethod` | Input method editor (IME) |
| `Overlay` | Always-on-top overlay |
| `Notification` | Notification handler/renderer |
| `DesktopApp` | Full application running as a plugin |

**Lifecycle states:** Starting, Running, Suspended, Crashed, Stopped

**Capabilities (`PluginCapability`):** FilesystemRead, WaylandSurface, InputEvents, NetworkAccess, Overlay -- each granted individually per sandbox profile (`PluginSandboxProfile`).

## XWayland

The XWayland module (`xwayland.rs`) provides X11 compatibility for legacy applications.

- **XWaylandManager**: Process lifecycle (Disabled, Starting, Running, Failed, Stopped)
- **Surface mapping**: X11 window ID to compositor `SurfaceId` translation
- **Property translation**: `_NET_WM_NAME`, `_NET_WM_STATE`, `WM_HINTS`, `WM_NORMAL_HINTS`, `WM_TRANSIENT_FOR` mapped to internal types
- **Security**: Opt-in sandboxed via `XWaylandConfig::security_sandbox` (enabled by default)
- **Display**: Configurable X11 display number (default `:0`)

## Shell Integration

The shell integration layer (`shell_integration.rs`) bridges external application APIs to the compositor.

- **System tray**: `SystemTrayItem` with context menus (`TrayMenuItem`, `TrayAction`)
- **Window management**: `WindowManagementRequest` / `WindowManagementResult` for external window control
- **Notification bridge**: `NotificationBridge` converts `ExternalNotification` (with `Urgency` levels) to compositor notifications

## Theme Bridge

The theme bridge (`theme_bridge.rs`) synchronizes AGNOS appearance with Flutter applications.

- **FlutterThemeData**: Serializable representation of Flutter's `ThemeData` (brightness, Material 3, color scheme, font scale)
- **PlatformChannelMessage**: Flutter platform channel JSON protocol (channel, method, args)
- **ThemeOverrides**: Optional per-app overrides layered on top of the base theme
- **High-contrast flow**: `HighContrastTheme` -> `FlutterThemeData` -> platform channel -> Flutter UI

## Security UI

The security UI (`security_ui.rs`) provides real-time security management.

- **Security levels**: Standard, Elevated, Lockdown
- **Threat dashboard**: `SecurityDashboard` with `ThreatLevel` indicator
- **Permission management**: Per-agent `PermissionRequest` with category-based `PermissionDefinition`
- **Human override**: `OverrideRequest` workflow (request -> notify -> approve/deny)
- **Emergency kill switch**: Immediately terminates all agent activity

## Gestures

Multi-touch gesture recognition (`gestures.rs`) for touchscreen and trackpad input.

- Tap, double-tap, long-press, swipe (4-directional), pinch-to-zoom, rotation

## Tests

**Total: 1394 tests**

| Area | Count |
|---|---|
| Wayland protocol | 63 + 49 (stub + feature-gated) |
| Plugin host | 31 |
| XWayland | 20 |
| Shell integration | 26 |
| Theme bridge | 18 |
| Compositor, renderer, security, a11y, AI, shell, apps, gestures | remainder |

## Benchmarks

Criterion benchmark suite (`benches/compositor_benchmarks.rs`):

- **Renderer**: Frame rendering at 1, 5, 10, 20 surfaces (1920x1080)
- **Scene graph**: Surface insertion, removal, layer sorting, hit testing

Run with:

```bash
cd userland/desktop-environment
cargo bench
```

## Building

```bash
# Without Wayland (stub mode, runs anywhere)
cargo build -p desktop_environment --release

# With real Wayland server integration
cargo build -p desktop_environment --release --features wayland
```

## Dependencies

| Crate | Role |
|---|---|
| `wayland-server 0.31` | Wayland protocol server (optional) |
| `wayland-protocols 0.31` | XDG Shell, extensions (optional) |
| `wayland-protocols-wlr 0.2` | wlroots protocols (optional) |
| `agnos-common`, `agnos-sys` | Shared types, system bindings |
| `tokio` | Async runtime |
| `serde`, `serde_json` | Serialization |
| `chrono`, `uuid` | Timestamps, identifiers |
| `clap` | CLI argument parsing |
| `tracing` | Structured logging |
| `criterion` | Benchmarks (dev) |
