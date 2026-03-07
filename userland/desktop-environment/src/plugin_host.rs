//! Desktop Plugin Host
//!
//! Manages desktop plugin lifecycle, communication, and sandboxing.
//! Plugins run as separate processes communicating over Unix domain sockets.
//! Per ADR-017, plugins are crash-isolated from the compositor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during plugin host operations.
#[derive(Debug, Error)]
pub enum PluginHostError {
    #[error("plugin not found: {0}")]
    PluginNotFound(Uuid),
    #[error("max plugins reached: {max}")]
    MaxPluginsReached { max: usize },
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: PluginState, to: PluginState },
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// The type of desktop plugin, determining its role and default capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginType {
    /// Visual theme provider (colors, fonts, decorations).
    Theme,
    /// Panel widget (clock, system tray item, etc.).
    PanelWidget,
    /// Custom window decoration renderer.
    WindowDecorator,
    /// Input method editor (IME) for text input.
    InputMethod,
    /// Overlay displayed above all windows.
    Overlay,
    /// Notification handler/renderer.
    Notification,
    /// Full desktop application running as a plugin.
    DesktopApp,
}

/// The lifecycle state of a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginState {
    /// Plugin process is being spawned and initialized.
    Starting,
    /// Plugin is active and responding to messages.
    Running,
    /// Plugin is temporarily suspended (e.g., window not visible).
    Suspended,
    /// Plugin process crashed or exited unexpectedly.
    Crashed,
    /// Plugin has been gracefully stopped.
    Stopped,
}

/// Capabilities that a plugin may request or be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCapability {
    /// Read access to allowed filesystem paths.
    FilesystemRead,
    /// Permission to create and manage Wayland surfaces.
    WaylandSurface,
    /// Receive input events (keyboard, pointer).
    InputEvents,
    /// Outbound network access.
    NetworkAccess,
    /// Render overlays above other surfaces.
    Overlay,
}

/// Information about a registered plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique plugin instance identifier.
    pub id: Uuid,
    /// Human-readable plugin name.
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// The type/role of this plugin.
    pub plugin_type: PluginType,
    /// Current lifecycle state.
    pub state: PluginState,
    /// Granted capabilities.
    pub capabilities: Vec<PluginCapability>,
    /// Path to the Unix domain socket for IPC.
    pub socket_path: PathBuf,
    /// Timestamp when the plugin was started.
    pub started_at: DateTime<Utc>,
    /// Number of times this plugin has been restarted after crashes.
    pub restart_count: u32,
    /// Timestamp of the last heartbeat received.
    pub last_heartbeat: Option<DateTime<Utc>>,
}

impl PluginInfo {
    /// Returns `true` if the plugin is running and has sent a heartbeat
    /// within the last 30 seconds.
    pub fn is_healthy(&self) -> bool {
        if self.state != PluginState::Running {
            return false;
        }
        match self.last_heartbeat {
            Some(hb) => {
                let elapsed = Utc::now().signed_duration_since(hb);
                elapsed.num_seconds() < 30
            }
            None => false,
        }
    }

    /// Returns how long this plugin has been running since `started_at`.
    pub fn uptime(&self) -> Duration {
        let elapsed = Utc::now().signed_duration_since(self.started_at);
        elapsed.to_std().unwrap_or(Duration::ZERO)
    }
}

/// JSON-RPC style messages exchanged between the compositor and plugins
/// over Unix domain sockets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginMessage {
    /// Sent by the plugin during initialization.
    Init {
        plugin_type: PluginType,
        version: String,
        capabilities: Vec<PluginCapability>,
    },
    /// Compositor grants a surface region to the plugin.
    SurfaceGrant {
        surface_id: u64,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    /// Compositor requests the plugin to render its content.
    RenderRequest {
        surface_id: u64,
    },
    /// Plugin responds with rendered content metadata.
    RenderResponse {
        surface_id: u64,
        format: String,
        width: u32,
        height: u32,
    },
    /// Input event forwarded to the plugin.
    InputEvent {
        event_type: String,
        data: String,
    },
    /// Theme palette update broadcast to all plugins.
    ThemeUpdate {
        palette: HashMap<String, String>,
    },
    /// Periodic liveness check.
    Heartbeat,
    /// Graceful shutdown request.
    Shutdown,
}

impl PluginMessage {
    /// Serialize this message to a JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("PluginMessage serialization should not fail")
    }

    /// Deserialize a message from a JSON string.
    pub fn from_json(s: &str) -> Result<Self, PluginHostError> {
        Ok(serde_json::from_str(s)?)
    }
}

/// Sandbox constraints applied to a plugin process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSandboxProfile {
    /// Filesystem paths the plugin may read.
    pub allowed_paths: Vec<PathBuf>,
    /// Syscall names permitted in the seccomp filter.
    pub allowed_syscalls: Vec<String>,
    /// Whether outbound network access is allowed.
    pub network_allowed: bool,
    /// Maximum resident memory in bytes.
    pub max_memory_bytes: u64,
    /// Maximum CPU usage as a percentage (0.0 - 100.0).
    pub max_cpu_percent: f32,
}

/// The plugin host manages all registered plugins, their lifecycle, and
/// provides default sandbox profiles based on plugin type.
pub struct PluginHost {
    plugins: HashMap<Uuid, PluginInfo>,
    max_plugins: usize,
    render_timeout_ms: u64,
    heartbeat_interval_secs: u64,
}

impl PluginHost {
    /// Create a new plugin host that allows up to `max_plugins` concurrent plugins.
    pub fn new(max_plugins: usize) -> Self {
        Self {
            plugins: HashMap::new(),
            max_plugins,
            render_timeout_ms: 16, // ~60 fps
            heartbeat_interval_secs: 10,
        }
    }

    /// Register a new plugin and return its assigned ID.
    pub fn register_plugin(
        &mut self,
        name: String,
        version: String,
        plugin_type: PluginType,
        capabilities: Vec<PluginCapability>,
    ) -> Result<Uuid, PluginHostError> {
        if self.plugins.len() >= self.max_plugins {
            return Err(PluginHostError::MaxPluginsReached {
                max: self.max_plugins,
            });
        }

        let id = Uuid::new_v4();
        let socket_path = PathBuf::from(format!("/run/agnos/plugins/{}.sock", id));
        let now = Utc::now();

        let info = PluginInfo {
            id,
            name,
            version,
            plugin_type,
            state: PluginState::Starting,
            capabilities,
            socket_path,
            started_at: now,
            restart_count: 0,
            last_heartbeat: None,
        };

        self.plugins.insert(id, info);
        Ok(id)
    }

    /// Unregister a plugin and return its info.
    pub fn unregister_plugin(&mut self, id: Uuid) -> Result<PluginInfo, PluginHostError> {
        self.plugins
            .remove(&id)
            .ok_or(PluginHostError::PluginNotFound(id))
    }

    /// Get a reference to a plugin's info.
    pub fn get_plugin(&self, id: Uuid) -> Option<&PluginInfo> {
        self.plugins.get(&id)
    }

    /// List all registered plugins.
    pub fn list_plugins(&self) -> Vec<&PluginInfo> {
        self.plugins.values().collect()
    }

    /// List plugins of a specific type.
    pub fn list_by_type(&self, plugin_type: PluginType) -> Vec<&PluginInfo> {
        self.plugins
            .values()
            .filter(|p| p.plugin_type == plugin_type)
            .collect()
    }

    /// List plugins in a specific state.
    pub fn list_by_state(&self, state: PluginState) -> Vec<&PluginInfo> {
        self.plugins
            .values()
            .filter(|p| p.state == state)
            .collect()
    }

    /// Update the lifecycle state of a plugin.
    pub fn update_state(
        &mut self,
        id: Uuid,
        state: PluginState,
    ) -> Result<(), PluginHostError> {
        let plugin = self
            .plugins
            .get_mut(&id)
            .ok_or(PluginHostError::PluginNotFound(id))?;

        // Validate state transitions
        let valid = matches!(
            (plugin.state, state),
            (PluginState::Starting, PluginState::Running)
                | (PluginState::Starting, PluginState::Crashed)
                | (PluginState::Starting, PluginState::Stopped)
                | (PluginState::Running, PluginState::Suspended)
                | (PluginState::Running, PluginState::Crashed)
                | (PluginState::Running, PluginState::Stopped)
                | (PluginState::Suspended, PluginState::Running)
                | (PluginState::Suspended, PluginState::Crashed)
                | (PluginState::Suspended, PluginState::Stopped)
                | (PluginState::Crashed, PluginState::Starting)
                | (PluginState::Stopped, PluginState::Starting)
        );

        if !valid {
            return Err(PluginHostError::InvalidStateTransition {
                from: plugin.state,
                to: state,
            });
        }

        if state == PluginState::Starting && plugin.state == PluginState::Crashed {
            plugin.restart_count += 1;
        }

        plugin.state = state;
        Ok(())
    }

    /// Record a heartbeat for a plugin, updating its last_heartbeat timestamp.
    pub fn record_heartbeat(&mut self, id: Uuid) -> Result<(), PluginHostError> {
        let plugin = self
            .plugins
            .get_mut(&id)
            .ok_or(PluginHostError::PluginNotFound(id))?;
        plugin.last_heartbeat = Some(Utc::now());
        Ok(())
    }

    /// Check plugin health and return IDs of plugins that have missed heartbeats.
    /// A plugin is considered unhealthy if it is in the Running state but has not
    /// sent a heartbeat within `heartbeat_interval_secs * 3`.
    pub fn check_health(&self) -> Vec<Uuid> {
        let timeout = chrono::Duration::seconds((self.heartbeat_interval_secs * 3) as i64);
        let now = Utc::now();

        self.plugins
            .values()
            .filter(|p| p.state == PluginState::Running)
            .filter(|p| match p.last_heartbeat {
                Some(hb) => now.signed_duration_since(hb) > timeout,
                None => true, // Never sent a heartbeat while running
            })
            .map(|p| p.id)
            .collect()
    }

    /// Return a default sandbox profile appropriate for the given plugin type.
    pub fn sandbox_profile_for(&self, plugin_type: PluginType) -> PluginSandboxProfile {
        let base_syscalls = vec![
            "read".to_string(),
            "write".to_string(),
            "close".to_string(),
            "mmap".to_string(),
            "munmap".to_string(),
            "brk".to_string(),
            "exit_group".to_string(),
        ];

        match plugin_type {
            PluginType::Theme => PluginSandboxProfile {
                allowed_paths: vec![PathBuf::from("/usr/share/themes")],
                allowed_syscalls: base_syscalls,
                network_allowed: false,
                max_memory_bytes: 32 * 1024 * 1024, // 32 MB
                max_cpu_percent: 5.0,
            },
            PluginType::PanelWidget => PluginSandboxProfile {
                allowed_paths: vec![
                    PathBuf::from("/usr/share/icons"),
                    PathBuf::from("/tmp"),
                ],
                allowed_syscalls: base_syscalls,
                network_allowed: false,
                max_memory_bytes: 64 * 1024 * 1024, // 64 MB
                max_cpu_percent: 10.0,
            },
            PluginType::WindowDecorator => PluginSandboxProfile {
                allowed_paths: vec![PathBuf::from("/usr/share/themes")],
                allowed_syscalls: base_syscalls,
                network_allowed: false,
                max_memory_bytes: 32 * 1024 * 1024,
                max_cpu_percent: 5.0,
            },
            PluginType::InputMethod => PluginSandboxProfile {
                allowed_paths: vec![
                    PathBuf::from("/usr/share/locale"),
                    PathBuf::from("/usr/share/ibus"),
                ],
                allowed_syscalls: base_syscalls,
                network_allowed: false,
                max_memory_bytes: 128 * 1024 * 1024, // 128 MB
                max_cpu_percent: 15.0,
            },
            PluginType::Overlay => PluginSandboxProfile {
                allowed_paths: vec![PathBuf::from("/tmp")],
                allowed_syscalls: base_syscalls,
                network_allowed: false,
                max_memory_bytes: 64 * 1024 * 1024,
                max_cpu_percent: 10.0,
            },
            PluginType::Notification => PluginSandboxProfile {
                allowed_paths: vec![
                    PathBuf::from("/usr/share/icons"),
                    PathBuf::from("/usr/share/sounds"),
                ],
                allowed_syscalls: base_syscalls,
                network_allowed: false,
                max_memory_bytes: 32 * 1024 * 1024,
                max_cpu_percent: 5.0,
            },
            PluginType::DesktopApp => PluginSandboxProfile {
                allowed_paths: vec![
                    PathBuf::from("/tmp"),
                    PathBuf::from("/home"),
                ],
                allowed_syscalls: {
                    let mut syscalls = base_syscalls;
                    syscalls.extend_from_slice(&[
                        "openat".to_string(),
                        "stat".to_string(),
                        "fstat".to_string(),
                        "getdents64".to_string(),
                        "socket".to_string(),
                        "connect".to_string(),
                    ]);
                    syscalls
                },
                network_allowed: true,
                max_memory_bytes: 512 * 1024 * 1024, // 512 MB
                max_cpu_percent: 50.0,
            },
        }
    }

    /// Return the number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Return `true` if no plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_host() -> PluginHost {
        PluginHost::new(10)
    }

    fn register_test_plugin(host: &mut PluginHost) -> Uuid {
        host.register_plugin(
            "test-plugin".to_string(),
            "1.0.0".to_string(),
            PluginType::PanelWidget,
            vec![PluginCapability::WaylandSurface],
        )
        .unwrap()
    }

    // --- Registration and lookup ---

    #[test]
    fn test_register_and_get_plugin() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        let plugin = host.get_plugin(id).unwrap();
        assert_eq!(plugin.name, "test-plugin");
        assert_eq!(plugin.version, "1.0.0");
        assert_eq!(plugin.plugin_type, PluginType::PanelWidget);
        assert_eq!(plugin.state, PluginState::Starting);
        assert_eq!(plugin.capabilities, vec![PluginCapability::WaylandSurface]);
        assert_eq!(plugin.restart_count, 0);
        assert!(plugin.last_heartbeat.is_none());
    }

    #[test]
    fn test_register_sets_socket_path() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        let plugin = host.get_plugin(id).unwrap();
        assert!(plugin.socket_path.to_str().unwrap().contains(&id.to_string()));
        assert!(plugin.socket_path.to_str().unwrap().starts_with("/run/agnos/plugins/"));
    }

    #[test]
    fn test_get_nonexistent_plugin() {
        let host = make_host();
        assert!(host.get_plugin(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_unregister_plugin() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        assert_eq!(host.len(), 1);
        let info = host.unregister_plugin(id).unwrap();
        assert_eq!(info.id, id);
        assert_eq!(host.len(), 0);
        assert!(host.get_plugin(id).is_none());
    }

    #[test]
    fn test_unregister_nonexistent() {
        let mut host = make_host();
        let result = host.unregister_plugin(Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginHostError::PluginNotFound(_)));
    }

    // --- Max plugins limit ---

    #[test]
    fn test_max_plugins_enforcement() {
        let mut host = PluginHost::new(2);
        register_test_plugin(&mut host);
        register_test_plugin(&mut host);
        let result = host.register_plugin(
            "third".to_string(),
            "1.0.0".to_string(),
            PluginType::Theme,
            vec![],
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PluginHostError::MaxPluginsReached { max: 2 }
        ));
    }

    // --- State transitions ---

    #[test]
    fn test_state_transition_starting_to_running() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        assert!(host.update_state(id, PluginState::Running).is_ok());
        assert_eq!(host.get_plugin(id).unwrap().state, PluginState::Running);
    }

    #[test]
    fn test_state_transition_invalid() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        // Starting -> Suspended is not valid
        let result = host.update_state(id, PluginState::Suspended);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PluginHostError::InvalidStateTransition {
                from: PluginState::Starting,
                to: PluginState::Suspended,
            }
        ));
    }

    #[test]
    fn test_state_transition_crash_restart_increments_count() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        host.update_state(id, PluginState::Running).unwrap();
        host.update_state(id, PluginState::Crashed).unwrap();
        assert_eq!(host.get_plugin(id).unwrap().restart_count, 0);
        host.update_state(id, PluginState::Starting).unwrap();
        assert_eq!(host.get_plugin(id).unwrap().restart_count, 1);
    }

    #[test]
    fn test_update_state_nonexistent() {
        let mut host = make_host();
        let result = host.update_state(Uuid::new_v4(), PluginState::Running);
        assert!(result.is_err());
    }

    // --- Heartbeat and health ---

    #[test]
    fn test_record_heartbeat() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        host.update_state(id, PluginState::Running).unwrap();
        assert!(host.get_plugin(id).unwrap().last_heartbeat.is_none());
        host.record_heartbeat(id).unwrap();
        assert!(host.get_plugin(id).unwrap().last_heartbeat.is_some());
    }

    #[test]
    fn test_record_heartbeat_nonexistent() {
        let mut host = make_host();
        assert!(host.record_heartbeat(Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_check_health_no_heartbeat() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        host.update_state(id, PluginState::Running).unwrap();
        // No heartbeat recorded, so the plugin should be flagged
        let unhealthy = host.check_health();
        assert!(unhealthy.contains(&id));
    }

    #[test]
    fn test_check_health_recent_heartbeat() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        host.update_state(id, PluginState::Running).unwrap();
        host.record_heartbeat(id).unwrap();
        // Just recorded, should be healthy
        let unhealthy = host.check_health();
        assert!(!unhealthy.contains(&id));
    }

    // --- PluginInfo methods ---

    #[test]
    fn test_plugin_info_is_healthy() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        // Starting state is not healthy
        assert!(!host.get_plugin(id).unwrap().is_healthy());
        host.update_state(id, PluginState::Running).unwrap();
        // Running but no heartbeat
        assert!(!host.get_plugin(id).unwrap().is_healthy());
        host.record_heartbeat(id).unwrap();
        // Running with recent heartbeat
        assert!(host.get_plugin(id).unwrap().is_healthy());
    }

    #[test]
    fn test_plugin_info_uptime() {
        let mut host = make_host();
        let id = register_test_plugin(&mut host);
        let uptime = host.get_plugin(id).unwrap().uptime();
        // Should be very small (just created)
        assert!(uptime.as_secs() < 2);
    }

    // --- Listing and filtering ---

    #[test]
    fn test_list_plugins() {
        let mut host = make_host();
        assert!(host.list_plugins().is_empty());
        register_test_plugin(&mut host);
        register_test_plugin(&mut host);
        assert_eq!(host.list_plugins().len(), 2);
    }

    #[test]
    fn test_list_by_type() {
        let mut host = make_host();
        register_test_plugin(&mut host); // PanelWidget
        host.register_plugin(
            "theme".to_string(),
            "1.0.0".to_string(),
            PluginType::Theme,
            vec![],
        )
        .unwrap();
        assert_eq!(host.list_by_type(PluginType::PanelWidget).len(), 1);
        assert_eq!(host.list_by_type(PluginType::Theme).len(), 1);
        assert_eq!(host.list_by_type(PluginType::Overlay).len(), 0);
    }

    #[test]
    fn test_list_by_state() {
        let mut host = make_host();
        let id1 = register_test_plugin(&mut host);
        let _id2 = register_test_plugin(&mut host);
        assert_eq!(host.list_by_state(PluginState::Starting).len(), 2);
        host.update_state(id1, PluginState::Running).unwrap();
        assert_eq!(host.list_by_state(PluginState::Starting).len(), 1);
        assert_eq!(host.list_by_state(PluginState::Running).len(), 1);
    }

    // --- len / is_empty ---

    #[test]
    fn test_len_and_is_empty() {
        let mut host = make_host();
        assert!(host.is_empty());
        assert_eq!(host.len(), 0);
        register_test_plugin(&mut host);
        assert!(!host.is_empty());
        assert_eq!(host.len(), 1);
    }

    // --- Sandbox profiles ---

    #[test]
    fn test_sandbox_profile_theme() {
        let host = make_host();
        let profile = host.sandbox_profile_for(PluginType::Theme);
        assert!(!profile.network_allowed);
        assert_eq!(profile.max_memory_bytes, 32 * 1024 * 1024);
        assert!(profile.max_cpu_percent <= 5.0);
        assert!(profile.allowed_paths.contains(&PathBuf::from("/usr/share/themes")));
    }

    #[test]
    fn test_sandbox_profile_desktop_app() {
        let host = make_host();
        let profile = host.sandbox_profile_for(PluginType::DesktopApp);
        assert!(profile.network_allowed);
        assert_eq!(profile.max_memory_bytes, 512 * 1024 * 1024);
        assert!(profile.max_cpu_percent <= 50.0);
        assert!(profile.allowed_syscalls.contains(&"socket".to_string()));
        assert!(profile.allowed_syscalls.contains(&"connect".to_string()));
    }

    #[test]
    fn test_sandbox_profile_input_method() {
        let host = make_host();
        let profile = host.sandbox_profile_for(PluginType::InputMethod);
        assert!(!profile.network_allowed);
        assert_eq!(profile.max_memory_bytes, 128 * 1024 * 1024);
        assert!(profile.allowed_paths.contains(&PathBuf::from("/usr/share/locale")));
    }

    // --- Message serialization ---

    #[test]
    fn test_message_heartbeat_roundtrip() {
        let msg = PluginMessage::Heartbeat;
        let json = msg.to_json();
        let parsed = PluginMessage::from_json(&json).unwrap();
        assert_eq!(parsed, PluginMessage::Heartbeat);
    }

    #[test]
    fn test_message_init_roundtrip() {
        let msg = PluginMessage::Init {
            plugin_type: PluginType::PanelWidget,
            version: "2.0.0".to_string(),
            capabilities: vec![PluginCapability::WaylandSurface, PluginCapability::InputEvents],
        };
        let json = msg.to_json();
        let parsed = PluginMessage::from_json(&json).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_message_surface_grant_roundtrip() {
        let msg = PluginMessage::SurfaceGrant {
            surface_id: 42,
            x: 100,
            y: 200,
            width: 800,
            height: 600,
        };
        let json = msg.to_json();
        let parsed = PluginMessage::from_json(&json).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_message_theme_update_roundtrip() {
        let mut palette = HashMap::new();
        palette.insert("bg".to_string(), "#1e1e2e".to_string());
        palette.insert("fg".to_string(), "#cdd6f4".to_string());
        let msg = PluginMessage::ThemeUpdate { palette };
        let json = msg.to_json();
        let parsed = PluginMessage::from_json(&json).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_message_shutdown_roundtrip() {
        let msg = PluginMessage::Shutdown;
        let json = msg.to_json();
        let parsed = PluginMessage::from_json(&json).unwrap();
        assert_eq!(parsed, PluginMessage::Shutdown);
    }

    #[test]
    fn test_message_from_invalid_json() {
        let result = PluginMessage::from_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_message_render_request_response_roundtrip() {
        let req = PluginMessage::RenderRequest { surface_id: 7 };
        let resp = PluginMessage::RenderResponse {
            surface_id: 7,
            format: "argb8888".to_string(),
            width: 1920,
            height: 1080,
        };
        assert_eq!(
            PluginMessage::from_json(&req.to_json()).unwrap(),
            req
        );
        assert_eq!(
            PluginMessage::from_json(&resp.to_json()).unwrap(),
            resp
        );
    }

    #[test]
    fn test_message_input_event_roundtrip() {
        let msg = PluginMessage::InputEvent {
            event_type: "key_press".to_string(),
            data: "{\"key\":\"Enter\"}".to_string(),
        };
        let json = msg.to_json();
        let parsed = PluginMessage::from_json(&json).unwrap();
        assert_eq!(parsed, msg);
    }
}
