//! XWayland fallback support for the AGNOS compositor.
//!
//! Provides an X11 compatibility layer so that legacy applications (and Flutter
//! apps whose Wayland requirements cannot be satisfied) can run inside the
//! Wayland compositor via XWayland.

use std::collections::HashMap;

use uuid::Uuid;

use crate::compositor::{SurfaceId, WindowState};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the XWayland compatibility layer.
#[derive(Debug, Clone)]
pub struct XWaylandConfig {
    /// Whether XWayland is enabled at all.
    pub enabled: bool,
    /// X11 display number (e.g. `0` for `:0`).
    pub display_number: u32,
    /// Whether to sandbox XWayland with the compositor's security policy.
    pub security_sandbox: bool,
}

impl Default for XWaylandConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            display_number: 0,
            security_sandbox: true,
        }
    }
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// Lifecycle state of the XWayland process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XWaylandState {
    Disabled,
    Starting,
    Running,
    Failed(String),
    Stopped,
}

// ---------------------------------------------------------------------------
// X11 property types
// ---------------------------------------------------------------------------

/// Subset of X11 window-manager properties that the compositor cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X11Property {
    /// `_NET_WM_NAME` — the window title.
    NetWmName(String),
    /// `_NET_WM_STATE` — state atoms (maximized, fullscreen, etc.).
    NetWmState(Vec<X11WmState>),
    /// `WM_HINTS` — icon, urgency, etc. (opaque for now).
    WmHints,
    /// `WM_NORMAL_HINTS` — size constraints.
    WmNormalHints {
        min_w: u32,
        min_h: u32,
        max_w: u32,
        max_h: u32,
    },
    /// `WM_TRANSIENT_FOR` — parent window.
    WmTransientFor(u32),
}

/// Individual `_NET_WM_STATE` atoms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X11WmState {
    Maximized,
    Fullscreen,
    Hidden,
    Above,
    Below,
    Modal,
    Sticky,
}

// ---------------------------------------------------------------------------
// Compositor-side state change
// ---------------------------------------------------------------------------

/// A translated window state change that the compositor can apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowStateChange {
    SetTitle(String),
    SetState(WindowState),
    SetSizeBounds {
        min: (u32, u32),
        max: (u32, u32),
    },
    SetParent(SurfaceId),
    NoChange,
}

// ---------------------------------------------------------------------------
// XWayland manager
// ---------------------------------------------------------------------------

/// Manages XWayland surfaces and their mapping to compositor [`SurfaceId`]s.
pub struct XWaylandManager {
    config: XWaylandConfig,
    state: XWaylandState,
    surfaces: HashMap<u32, SurfaceId>,
    display_number: u32,
}

impl XWaylandManager {
    /// Create a new manager from the given configuration.
    pub fn new(config: XWaylandConfig) -> Self {
        let display_number = config.display_number;
        let initial_state = if config.enabled {
            XWaylandState::Starting
        } else {
            XWaylandState::Disabled
        };
        Self {
            config,
            state: initial_state,
            surfaces: HashMap::new(),
            display_number,
        }
    }

    /// Whether XWayland is enabled in the configuration.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Whether the XWayland process is currently running.
    pub fn is_running(&self) -> bool {
        self.state == XWaylandState::Running
    }

    /// Current lifecycle state.
    pub fn state(&self) -> &XWaylandState {
        &self.state
    }

    /// Register a new X11 window, creating a compositor surface for it.
    /// Returns the newly assigned [`SurfaceId`].
    pub fn register_surface(&mut self, x11_window_id: u32) -> SurfaceId {
        let surface_id = Uuid::new_v4();
        self.surfaces.insert(x11_window_id, surface_id);
        surface_id
    }

    /// Remove the mapping for an X11 window. Returns the compositor
    /// [`SurfaceId`] if it was registered.
    pub fn unregister_surface(&mut self, x11_window_id: u32) -> Option<SurfaceId> {
        self.surfaces.remove(&x11_window_id)
    }

    /// Look up the compositor surface for an X11 window.
    pub fn get_surface(&self, x11_window_id: u32) -> Option<SurfaceId> {
        self.surfaces.get(&x11_window_id).copied()
    }

    /// Translate an X11 property change into a compositor-level
    /// [`WindowStateChange`].
    pub fn translate_property(&self, property: &X11Property) -> Option<WindowStateChange> {
        match property {
            X11Property::NetWmName(name) => {
                Some(WindowStateChange::SetTitle(name.clone()))
            }
            X11Property::NetWmState(states) => {
                if states.contains(&X11WmState::Fullscreen) {
                    Some(WindowStateChange::SetState(WindowState::Fullscreen))
                } else if states.contains(&X11WmState::Maximized) {
                    Some(WindowStateChange::SetState(WindowState::Maximized))
                } else if states.contains(&X11WmState::Hidden) {
                    Some(WindowStateChange::SetState(WindowState::Minimized))
                } else {
                    Some(WindowStateChange::SetState(WindowState::Normal))
                }
            }
            X11Property::WmHints => {
                // WM_HINTS is opaque; nothing to translate yet
                Some(WindowStateChange::NoChange)
            }
            X11Property::WmNormalHints {
                min_w,
                min_h,
                max_w,
                max_h,
            } => Some(WindowStateChange::SetSizeBounds {
                min: (*min_w, *min_h),
                max: (*max_w, *max_h),
            }),
            X11Property::WmTransientFor(parent_x11_id) => {
                self.surfaces
                    .get(parent_x11_id)
                    .copied()
                    .map(WindowStateChange::SetParent)
            }
        }
    }

    /// Number of tracked X11 surfaces.
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Transition to the Running state (called after the XWayland process is up).
    pub fn set_running(&mut self) {
        if self.config.enabled {
            self.state = XWaylandState::Running;
        }
    }

    /// Transition to the Failed state.
    pub fn set_failed(&mut self, reason: String) {
        self.state = XWaylandState::Failed(reason);
    }

    /// Transition to the Stopped state.
    pub fn set_stopped(&mut self) {
        self.state = XWaylandState::Stopped;
        self.surfaces.clear();
    }

    /// The X11 display number.
    pub fn display_number(&self) -> u32 {
        self.display_number
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> XWaylandConfig {
        XWaylandConfig {
            enabled: true,
            display_number: 1,
            security_sandbox: true,
        }
    }

    // --- config defaults ---

    #[test]
    fn test_config_defaults() {
        let cfg = XWaylandConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.display_number, 0);
        assert!(cfg.security_sandbox);
    }

    // --- state transitions ---

    #[test]
    fn test_disabled_by_default() {
        let mgr = XWaylandManager::new(XWaylandConfig::default());
        assert!(!mgr.is_enabled());
        assert!(!mgr.is_running());
        assert_eq!(*mgr.state(), XWaylandState::Disabled);
    }

    #[test]
    fn test_enabled_starts_in_starting_state() {
        let mgr = XWaylandManager::new(enabled_config());
        assert!(mgr.is_enabled());
        assert!(!mgr.is_running());
        assert_eq!(*mgr.state(), XWaylandState::Starting);
    }

    #[test]
    fn test_transition_to_running() {
        let mut mgr = XWaylandManager::new(enabled_config());
        mgr.set_running();
        assert!(mgr.is_running());
        assert_eq!(*mgr.state(), XWaylandState::Running);
    }

    #[test]
    fn test_transition_to_failed() {
        let mut mgr = XWaylandManager::new(enabled_config());
        mgr.set_failed("socket error".to_string());
        assert!(!mgr.is_running());
        assert_eq!(
            *mgr.state(),
            XWaylandState::Failed("socket error".to_string())
        );
    }

    #[test]
    fn test_transition_to_stopped() {
        let mut mgr = XWaylandManager::new(enabled_config());
        mgr.set_running();
        mgr.register_surface(100);
        assert_eq!(mgr.surface_count(), 1);
        mgr.set_stopped();
        assert_eq!(*mgr.state(), XWaylandState::Stopped);
        assert_eq!(mgr.surface_count(), 0);
    }

    // --- surface registration ---

    #[test]
    fn test_register_surface() {
        let mut mgr = XWaylandManager::new(enabled_config());
        let sid = mgr.register_surface(42);
        assert_eq!(mgr.get_surface(42), Some(sid));
        assert_eq!(mgr.surface_count(), 1);
    }

    #[test]
    fn test_unregister_surface() {
        let mut mgr = XWaylandManager::new(enabled_config());
        let sid = mgr.register_surface(42);
        let removed = mgr.unregister_surface(42);
        assert_eq!(removed, Some(sid));
        assert_eq!(mgr.get_surface(42), None);
        assert_eq!(mgr.surface_count(), 0);
    }

    #[test]
    fn test_unregister_nonexistent() {
        let mut mgr = XWaylandManager::new(enabled_config());
        assert_eq!(mgr.unregister_surface(999), None);
    }

    #[test]
    fn test_get_surface_nonexistent() {
        let mgr = XWaylandManager::new(enabled_config());
        assert_eq!(mgr.get_surface(999), None);
    }

    // --- property translation ---

    #[test]
    fn test_translate_net_wm_name() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmName("Firefox".to_string()));
        assert_eq!(
            change,
            Some(WindowStateChange::SetTitle("Firefox".to_string()))
        );
    }

    #[test]
    fn test_translate_net_wm_state_fullscreen() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![
            X11WmState::Fullscreen,
        ]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Fullscreen))
        );
    }

    #[test]
    fn test_translate_net_wm_state_maximized() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![
            X11WmState::Maximized,
        ]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Maximized))
        );
    }

    #[test]
    fn test_translate_net_wm_state_hidden() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![
            X11WmState::Hidden,
        ]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Minimized))
        );
    }

    #[test]
    fn test_translate_net_wm_state_normal() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![
            X11WmState::Sticky,
        ]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Normal))
        );
    }

    #[test]
    fn test_translate_wm_hints_no_change() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::WmHints);
        assert_eq!(change, Some(WindowStateChange::NoChange));
    }

    #[test]
    fn test_translate_wm_normal_hints() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::WmNormalHints {
            min_w: 200,
            min_h: 100,
            max_w: 1920,
            max_h: 1080,
        });
        assert_eq!(
            change,
            Some(WindowStateChange::SetSizeBounds {
                min: (200, 100),
                max: (1920, 1080),
            })
        );
    }

    #[test]
    fn test_translate_wm_transient_for_known_parent() {
        let mut mgr = XWaylandManager::new(enabled_config());
        let parent_sid = mgr.register_surface(10);
        let change = mgr.translate_property(&X11Property::WmTransientFor(10));
        assert_eq!(change, Some(WindowStateChange::SetParent(parent_sid)));
    }

    #[test]
    fn test_translate_wm_transient_for_unknown_parent() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::WmTransientFor(999));
        assert_eq!(change, None);
    }

    // --- display number ---

    #[test]
    fn test_display_number() {
        let mgr = XWaylandManager::new(enabled_config());
        assert_eq!(mgr.display_number(), 1);
    }
}
