mod compositor;
mod shell;
mod ai_features;
mod apps;
mod security_ui;

pub use compositor::{
    Compositor, CompositorError, Window, WindowState, SurfaceId,
    Workspace, ContextType, Rectangle, CompositorBackend,
    InputEvent, WaylandBackend,
};

pub use shell::{
    DesktopShell, ShellError, Notification, NotificationPriority,
    QuickSetting, SystemStatus, NetworkStatus, AppEntry, AppCategory,
    LauncherItem, LauncherAction, NotificationId,
};

pub use ai_features::{
    AIDesktopFeatures, AIFeatureError, AISuggestion, SuggestionType,
    AgentHUDState, AgentStatus, ContextEvent, ContextEventType,
    DesktopContext, ContextType as AIContextType, TimeOfDay, ActivityLevel,
    ResourceMetrics,
};

pub use apps::{
    DesktopApplications, AppError, AppType, AppWindow,
    TerminalApp, FileManagerApp, AgentManagerApp, AuditViewerApp, ModelManagerApp,
    AgentInfo, AuditEntry, AuditFilters, TimeRange, ModelInfo,
};

pub use security_ui::{
    SecurityUI, SecurityUIError, SecurityAlert, ThreatLevel,
    PermissionRequest, PermissionDefinition, PermissionCategory,
    AgentPermission, SecurityDashboard, OverrideRequest, SecurityLevel,
};
