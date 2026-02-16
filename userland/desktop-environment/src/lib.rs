mod ai_features;
mod apps;
mod compositor;
mod security_ui;
mod shell;
mod system_tests;

pub use compositor::{
    Compositor, CompositorBackend, CompositorError, ContextType, InputEvent, Rectangle, SurfaceId,
    WaylandBackend, Window, WindowState, Workspace,
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

pub use security_ui::{
    AgentPermission, OverrideRequest, PermissionCategory, PermissionDefinition, PermissionRequest,
    SecurityAlert, SecurityDashboard, SecurityLevel, SecurityUI, SecurityUIError, ThreatLevel,
};
