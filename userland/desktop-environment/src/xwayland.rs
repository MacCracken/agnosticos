//! XWayland fallback support for the AGNOS compositor.
//!
//! Provides an X11 compatibility layer so that legacy applications (and Flutter
//! apps whose Wayland requirements cannot be satisfied) can run inside the
//! Wayland compositor via XWayland.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;

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
    SetSizeBounds { min: (u32, u32), max: (u32, u32) },
    SetParent(SurfaceId),
    NoChange,
}

// ---------------------------------------------------------------------------
// XWayland manager
// ---------------------------------------------------------------------------

/// Status information returned by [`XWaylandManager::status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XWaylandStatus {
    /// Current lifecycle state.
    pub state: XWaylandState,
    /// PID of the XWayland process, if running.
    pub pid: Option<u32>,
    /// X11 display string (e.g. `:1`).
    pub display: String,
    /// Path to the display socket.
    pub socket_path: PathBuf,
}

/// Trait abstracting process spawning so tests can inject a mock.
pub trait ProcessSpawner: std::fmt::Debug + Send {
    /// Spawn the XWayland binary with the given arguments.
    /// Returns the child process on success.
    fn spawn(&self, program: &str, args: &[&str]) -> std::io::Result<Child>;
}

/// Default spawner that uses `std::process::Command`.
#[derive(Debug)]
pub struct RealProcessSpawner;

impl ProcessSpawner for RealProcessSpawner {
    fn spawn(&self, program: &str, args: &[&str]) -> std::io::Result<Child> {
        std::process::Command::new(program).args(args).spawn()
    }
}

/// Manages XWayland surfaces and their mapping to compositor [`SurfaceId`]s.
pub struct XWaylandManager {
    config: XWaylandConfig,
    state: XWaylandState,
    surfaces: HashMap<u32, SurfaceId>,
    display_number: u32,
    /// The running XWayland child process, if any.
    child: Option<Child>,
    /// PID of the running process (cached for status reporting after the
    /// child handle is consumed on stop).
    pid: Option<u32>,
    /// Socket path for the X11 display.
    socket_path: PathBuf,
    /// Pluggable process spawner (allows test mocking).
    spawner: Box<dyn ProcessSpawner>,
}

impl XWaylandManager {
    /// Create a new manager from the given configuration.
    pub fn new(config: XWaylandConfig) -> Self {
        Self::with_spawner(config, Box::new(RealProcessSpawner))
    }

    /// Create a new manager with a custom [`ProcessSpawner`] (useful for testing).
    pub fn with_spawner(config: XWaylandConfig, spawner: Box<dyn ProcessSpawner>) -> Self {
        let display_number = config.display_number;
        let socket_path = PathBuf::from(format!("/tmp/.X11-unix/X{}", display_number));
        let initial_state = if config.enabled {
            XWaylandState::Disabled // Don't auto-start; caller invokes start()
        } else {
            XWaylandState::Disabled
        };
        Self {
            config,
            state: initial_state,
            surfaces: HashMap::new(),
            display_number,
            child: None,
            pid: None,
            socket_path,
            spawner,
        }
    }

    /// Start the XWayland process.
    ///
    /// Validates the configuration, then spawns `Xwayland :{display} -rootless -noreset`.
    /// On success the state transitions to [`XWaylandState::Running`] and the PID is recorded.
    /// On failure the state transitions to [`XWaylandState::Failed`] with the error message.
    pub fn start(&mut self) -> Result<u32, String> {
        // Validate: must be enabled
        if !self.config.enabled {
            let msg = "XWayland is not enabled in configuration".to_string();
            self.state = XWaylandState::Failed(msg.clone());
            return Err(msg);
        }

        // Validate: display number must be < 1000 (X11 convention)
        if self.display_number >= 1000 {
            let msg = format!(
                "Invalid X11 display number {}: must be less than 1000",
                self.display_number
            );
            self.state = XWaylandState::Failed(msg.clone());
            return Err(msg);
        }

        // Transition to Starting
        self.state = XWaylandState::Starting;

        // Compute socket path
        self.socket_path = PathBuf::from(format!("/tmp/.X11-unix/X{}", self.display_number));

        let display_arg = format!(":{}", self.display_number);
        match self.spawner.spawn("Xwayland", &[&display_arg, "-rootless", "-noreset"]) {
            Ok(child) => {
                let pid = child.id();
                self.child = Some(child);
                self.pid = Some(pid);
                self.state = XWaylandState::Running;
                tracing::info!(
                    pid = pid,
                    display = %display_arg,
                    socket = %self.socket_path.display(),
                    "XWayland process started"
                );
                Ok(pid)
            }
            Err(e) => {
                let msg = format!("Failed to spawn Xwayland: {}", e);
                self.state = XWaylandState::Failed(msg.clone());
                tracing::error!(%msg, "XWayland start failed");
                Err(msg)
            }
        }
    }

    /// Stop the XWayland process by sending SIGTERM.
    ///
    /// Transitions to [`XWaylandState::Stopped`] and clears all tracked surfaces.
    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(ref mut child) = self.child {
            // Send SIGTERM via libc
            let pid = child.id() as libc::pid_t;
            let ret = unsafe { libc::kill(pid, libc::SIGTERM) };
            if ret != 0 {
                let err = std::io::Error::last_os_error();
                // If process already exited, that's fine
                if err.raw_os_error() != Some(libc::ESRCH) {
                    let msg = format!("Failed to send SIGTERM to XWayland (pid {}): {}", pid, err);
                    tracing::warn!(%msg);
                    return Err(msg);
                }
            }
            tracing::info!(pid = pid, "Sent SIGTERM to XWayland process");
        }
        self.child = None;
        self.pid = None;
        self.state = XWaylandState::Stopped;
        self.surfaces.clear();
        Ok(())
    }

    /// Restart the XWayland process (stop then start).
    pub fn restart(&mut self) -> Result<u32, String> {
        self.stop().ok(); // Best-effort stop; ignore errors (process may already be gone)
        self.start()
    }

    /// Return current status information.
    pub fn status(&self) -> XWaylandStatus {
        XWaylandStatus {
            state: self.state.clone(),
            pid: self.pid,
            display: format!(":{}", self.display_number),
            socket_path: self.socket_path.clone(),
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
            X11Property::NetWmName(name) => Some(WindowStateChange::SetTitle(name.clone())),
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
            X11Property::WmTransientFor(parent_x11_id) => self
                .surfaces
                .get(parent_x11_id)
                .copied()
                .map(WindowStateChange::SetParent),
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
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;

    fn enabled_config() -> XWaylandConfig {
        XWaylandConfig {
            enabled: true,
            display_number: 1,
            security_sandbox: true,
        }
    }

    // -----------------------------------------------------------------------
    // Mock process spawner for testing without a real Xwayland binary
    // -----------------------------------------------------------------------

    /// A mock spawner that either succeeds (returning a dummy child) or fails.
    #[derive(Debug, Clone)]
    struct MockSpawner {
        should_fail: Arc<AtomicBool>,
        spawn_count: Arc<AtomicU32>,
    }

    impl MockSpawner {
        fn new(should_fail: bool) -> Self {
            Self {
                should_fail: Arc::new(AtomicBool::new(should_fail)),
                spawn_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn count(&self) -> u32 {
            self.spawn_count.load(Ordering::SeqCst)
        }
    }

    impl ProcessSpawner for MockSpawner {
        fn spawn(&self, _program: &str, _args: &[&str]) -> std::io::Result<Child> {
            self.spawn_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "mock: Xwayland binary not found",
                ));
            }
            // Spawn a harmless process that stays alive briefly so we get a valid Child/PID.
            // `sleep 60` is fine — we kill it in stop().
            std::process::Command::new("sleep").arg("60").spawn()
        }
    }

    fn mock_manager(config: XWaylandConfig, should_fail: bool) -> (XWaylandManager, MockSpawner) {
        let spawner = MockSpawner::new(should_fail);
        let spawner_clone = spawner.clone();
        let mgr = XWaylandManager::with_spawner(config, Box::new(spawner));
        (mgr, spawner_clone)
    }

    // --- config defaults ---

    #[test]
    fn test_config_defaults() {
        let cfg = XWaylandConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.display_number, 0);
        assert!(cfg.security_sandbox);
    }

    // --- state transitions (manual) ---

    #[test]
    fn test_disabled_by_default() {
        let mgr = XWaylandManager::new(XWaylandConfig::default());
        assert!(!mgr.is_enabled());
        assert!(!mgr.is_running());
        assert_eq!(*mgr.state(), XWaylandState::Disabled);
    }

    #[test]
    fn test_enabled_starts_in_disabled_before_start() {
        let mgr = XWaylandManager::new(enabled_config());
        assert!(mgr.is_enabled());
        assert!(!mgr.is_running());
        // Before start() is called, state is Disabled (caller must explicitly start)
        assert_eq!(*mgr.state(), XWaylandState::Disabled);
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
        let change = mgr.translate_property(&X11Property::NetWmState(vec![X11WmState::Fullscreen]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Fullscreen))
        );
    }

    #[test]
    fn test_translate_net_wm_state_maximized() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![X11WmState::Maximized]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Maximized))
        );
    }

    #[test]
    fn test_translate_net_wm_state_hidden() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![X11WmState::Hidden]));
        assert_eq!(
            change,
            Some(WindowStateChange::SetState(WindowState::Minimized))
        );
    }

    #[test]
    fn test_translate_net_wm_state_normal() {
        let mgr = XWaylandManager::new(enabled_config());
        let change = mgr.translate_property(&X11Property::NetWmState(vec![X11WmState::Sticky]));
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

    // -----------------------------------------------------------------------
    // Process management: start / stop / restart / status
    // -----------------------------------------------------------------------

    #[test]
    fn test_start_success() {
        let (mut mgr, spawner) = mock_manager(enabled_config(), false);
        let result = mgr.start();
        assert!(result.is_ok(), "start() should succeed: {:?}", result);
        let pid = result.unwrap();
        assert!(pid > 0);
        assert_eq!(*mgr.state(), XWaylandState::Running);
        assert_eq!(spawner.count(), 1);

        // Clean up spawned process
        mgr.stop().ok();
    }

    #[test]
    fn test_start_disabled_config() {
        let disabled_config = XWaylandConfig {
            enabled: false,
            display_number: 1,
            security_sandbox: true,
        };
        let (mut mgr, spawner) = mock_manager(disabled_config, false);
        let result = mgr.start();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not enabled"));
        assert!(matches!(*mgr.state(), XWaylandState::Failed(_)));
        assert_eq!(spawner.count(), 0); // Should not have attempted spawn
    }

    #[test]
    fn test_start_invalid_display_number() {
        let bad_config = XWaylandConfig {
            enabled: true,
            display_number: 1000,
            security_sandbox: true,
        };
        let (mut mgr, spawner) = mock_manager(bad_config, false);
        let result = mgr.start();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid X11 display number"));
        assert!(matches!(*mgr.state(), XWaylandState::Failed(_)));
        assert_eq!(spawner.count(), 0);
    }

    #[test]
    fn test_start_spawn_failure() {
        let (mut mgr, spawner) = mock_manager(enabled_config(), true);
        let result = mgr.start();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to spawn Xwayland"));
        assert!(matches!(*mgr.state(), XWaylandState::Failed(_)));
        assert_eq!(spawner.count(), 1); // Attempted but failed
    }

    #[test]
    fn test_stop_running_process() {
        let (mut mgr, _) = mock_manager(enabled_config(), false);
        mgr.start().expect("start should succeed");
        assert_eq!(*mgr.state(), XWaylandState::Running);

        // Register a surface so we can verify stop clears them
        mgr.register_surface(42);
        assert_eq!(mgr.surface_count(), 1);

        let result = mgr.stop();
        assert!(result.is_ok());
        assert_eq!(*mgr.state(), XWaylandState::Stopped);
        assert_eq!(mgr.surface_count(), 0);
    }

    #[test]
    fn test_stop_when_not_running() {
        let (mut mgr, _) = mock_manager(enabled_config(), false);
        // stop() without start() should succeed gracefully (no-op)
        let result = mgr.stop();
        assert!(result.is_ok());
        assert_eq!(*mgr.state(), XWaylandState::Stopped);
    }

    #[test]
    fn test_restart() {
        let (mut mgr, spawner) = mock_manager(enabled_config(), false);
        mgr.start().expect("first start should succeed");
        let old_pid = mgr.status().pid.unwrap();

        let result = mgr.restart();
        assert!(result.is_ok());
        let new_pid = result.unwrap();
        assert_eq!(*mgr.state(), XWaylandState::Running);
        assert_eq!(spawner.count(), 2); // spawned twice (start + restart)
        // PIDs should differ (new sleep process)
        assert_ne!(old_pid, new_pid);

        mgr.stop().ok();
    }

    #[test]
    fn test_restart_from_stopped() {
        let (mut mgr, spawner) = mock_manager(enabled_config(), false);
        // restart without prior start
        let result = mgr.restart();
        assert!(result.is_ok());
        assert_eq!(*mgr.state(), XWaylandState::Running);
        assert_eq!(spawner.count(), 1);

        mgr.stop().ok();
    }

    #[test]
    fn test_status_before_start() {
        let (mgr, _) = mock_manager(enabled_config(), false);
        let status = mgr.status();
        assert_eq!(status.state, XWaylandState::Disabled);
        assert_eq!(status.pid, None);
        assert_eq!(status.display, ":1");
        assert_eq!(status.socket_path, PathBuf::from("/tmp/.X11-unix/X1"));
    }

    #[test]
    fn test_status_while_running() {
        let (mut mgr, _) = mock_manager(enabled_config(), false);
        mgr.start().expect("start should succeed");
        let status = mgr.status();
        assert_eq!(status.state, XWaylandState::Running);
        assert!(status.pid.is_some());
        assert!(status.pid.unwrap() > 0);
        assert_eq!(status.display, ":1");

        mgr.stop().ok();
    }

    #[test]
    fn test_status_after_stop() {
        let (mut mgr, _) = mock_manager(enabled_config(), false);
        mgr.start().expect("start should succeed");
        mgr.stop().expect("stop should succeed");
        let status = mgr.status();
        assert_eq!(status.state, XWaylandState::Stopped);
        assert_eq!(status.pid, None);
    }

    #[test]
    fn test_status_after_failure() {
        let (mut mgr, _) = mock_manager(enabled_config(), true);
        let _ = mgr.start();
        let status = mgr.status();
        assert!(matches!(status.state, XWaylandState::Failed(_)));
        assert_eq!(status.pid, None);
    }

    #[test]
    fn test_socket_path_matches_display() {
        let config = XWaylandConfig {
            enabled: true,
            display_number: 5,
            security_sandbox: true,
        };
        let (mgr, _) = mock_manager(config, false);
        let status = mgr.status();
        assert_eq!(status.display, ":5");
        assert_eq!(status.socket_path, PathBuf::from("/tmp/.X11-unix/X5"));
    }

    #[test]
    fn test_start_records_correct_display_args() {
        // Verify display number 0 produces `:0`
        let config = XWaylandConfig {
            enabled: true,
            display_number: 0,
            security_sandbox: true,
        };
        let (mut mgr, _) = mock_manager(config, false);
        mgr.start().expect("start should succeed");
        assert_eq!(mgr.status().display, ":0");
        assert_eq!(
            mgr.status().socket_path,
            PathBuf::from("/tmp/.X11-unix/X0")
        );
        mgr.stop().ok();
    }
}
