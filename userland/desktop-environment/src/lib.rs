// Desktop environment library: many public types define the API surface for the
// Wayland compositor and are not yet exercised by the binary. These will be
// consumed by desktop integration, plugins, and tests as the compositor matures.
#![allow(dead_code, unused_mut)]

pub mod accessibility;
mod ai_features;
mod apps;
mod compositor;
pub mod gestures;
pub mod plugin_host;
pub mod renderer;
mod security_ui;
mod shell;
pub mod shell_integration;
mod system_tests;
pub mod theme_bridge;
pub mod wayland;
pub mod xwayland;

pub use compositor::{
    Compositor, CompositorBackend, CompositorError, ContextType, InputAction, InputEvent,
    Rectangle, SurfaceId, WaylandBackend, Window, WindowState, Workspace,
};

pub use shell::{
    AppCategory, AppEntry, DesktopShell, LauncherAction, LauncherItem, NetworkStatus, Notification,
    NotificationId, NotificationPriority, QuickSetting, ShellError, SystemStatus,
};

pub use ai_features::{
    AIDesktopFeatures, AIFeatureError, AISuggestion, ActivityLevel, AgentHUDState, AgentStatus,
    ContextEvent, ContextEventType, ContextType as AIContextType, DesktopContext, ResourceMetrics,
    SuggestionType, TimeOfDay,
};

pub use apps::{
    AgentInfo, AgentManagerApp, AppError, AppType, AppWindow, AuditEntry, AuditFilters,
    AuditViewerApp, DesktopApplications, FileManagerApp, ModelInfo, ModelManagerApp, TerminalApp,
    TimeRange,
};

pub use renderer::{
    DamageTracker, DecorationHit, DesktopRenderer, Framebuffer, Layer, Pixel, ResizeEdge,
    SceneGraph, SceneSurface,
};

pub use security_ui::{
    AgentPermission, OverrideRequest, PermissionCategory, PermissionDefinition, PermissionRequest,
    SecurityAlert, SecurityDashboard, SecurityLevel, SecurityUI, SecurityUIError, ThreatLevel,
};

pub use plugin_host::{
    PluginCapability, PluginHost, PluginHostError, PluginInfo, PluginMessage, PluginSandboxProfile,
    PluginState, PluginType,
};

pub use accessibility::{
    AccessibilityRole, AccessibilityState, AccessibilityTree, AccessibleAction, AccessibleNode,
    HighContrastTheme, KeyboardNavConfig,
};

pub use shell_integration::{
    ExternalNotification, NotificationBridge, ShellIntegrationError, ShellIntegrationManager,
    SystemTrayItem, TrayAction, TrayMenuItem, Urgency, WindowManagementRequest,
    WindowManagementResult,
};

pub use theme_bridge::{
    color_hex_to_u32, color_u32_to_hex, FlutterThemeData, PlatformChannelMessage, ThemeBridge,
    ThemeOverrides,
};
