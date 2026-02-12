# AGNOS Desktop Environment

> **Phase**: 4 | **Status**: 🔄 In Progress | **Last Updated**: 2026-02-11

The AGNOS Desktop Environment is an AI-augmented Wayland compositor that provides a secure, intelligent graphical interface for the AGNOS operating system.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    AGNOS Desktop Environment                    │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Compositor │  │ Desktop Shell │  │  AI Desktop Features │  │
│  │  (Wayland)   │  │  Panel/Launch │  │  Context/Suggestions │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Applications│  │ Security UI  │  │     Agent HUD        │  │
│  │ (Terminal,  │  │ Dashboard/   │  │  Real-time Status    │  │
│  │  Manager)   │  │  Permissions │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Wayland Compositor (`compositor.rs`)

The compositor manages windows, workspaces, and input handling.

**Key Features:**
- Multi-workspace support (4 workspaces)
- Agent-aware window management
- Secure mode for sensitive operations
- Window state management (normal, maximized, fullscreen, floating)

**Usage:**
```rust
use compositor::{Compositor, WindowState, ContextType};

let compositor = Compositor::new();

// Create a window
let window_id = compositor.create_window(
    "My App".to_string(),
    "myapp".to_string(),
    false,  // is_agent_window
)?;

// Manage workspaces
compositor.switch_workspace(2)?;
compositor.move_window_to_workspace(window_id, 1)?;

// Enable agent-aware mode
compositor.set_agent_aware_mode(true);
```

### 2. Desktop Shell (`shell.rs`)

The shell provides the traditional desktop experience with AI enhancements.

**Key Components:**
- **Panel**: System status, agent indicators, quick settings
- **Launcher**: App launcher with natural language search
- **Notification System**: Agent notifications, human override requests

**Usage:**
```rust
use shell::{DesktopShell, Notification, NotificationPriority};

let shell = DesktopShell::new();

// Show notification
shell.show_notification(Notification {
    app_name: "My App".to_string(),
    title: "Alert".to_string(),
    body: "Something happened".to_string(),
    priority: NotificationPriority::High,
    requires_action: false,
    is_agent_related: false,
    ..Default::default()
});

// Agent notification
shell.show_agent_notification(
    "Task Complete".to_string(),
    "File processing finished".to_string(),
    false,
);

// Request human override
shell.request_human_override(
    "file-agent".to_string(),
    "Delete /tmp/test.txt".to_string(),
    "Cleaning up temporary files".to_string(),
);
```

### 3. AI Desktop Features (`ai_features.rs`)

Ambient intelligence features that learn from user behavior.

**Key Features:**
- Context detection (development, design, communication)
- Proactive suggestions (window placement, workspace switching)
- Agent HUD for real-time monitoring
- Resource optimization recommendations

**Usage:**
```rust
use ai_features::{AIDesktopFeatures, SuggestionType, ContextEvent, ContextEventType};

let ai = AIDesktopFeatures::new();

// Update context based on user activity
ai.update_context(ContextEvent {
    id: uuid::Uuid::new_v4(),
    event_type: ContextEventType::WindowOpened,
    source: "vscode".to_string(),
    timestamp: chrono::Utc::now(),
    metadata: [("app".to_string(), "vscode".to_string())].into(),
});

// Get proactive suggestions
let suggestions = ai.proactive_suggestions();

// Register agent for HUD
ai.register_agent_hud(agent_id, "File Manager Agent".to_string());
ai.update_agent_hud(agent_id, AgentStatus::Acting, "Copying files".to_string(), 0.75);
```

### 4. Desktop Applications (`apps.rs`)

AGNOS-specific applications with AI integration.

**Available Apps:**
- **Terminal**: AI-integrated command line
- **File Manager**: Agent-assisted file operations
- **Agent Manager**: Manage running agents
- **Audit Viewer**: Security log visualization
- **Model Manager**: LLM model management

**Usage:**
```rust
use apps::{DesktopApplications, AppType};

let apps = DesktopApplications::new();

// Open apps
let terminal = apps.open_terminal()?;
let filemanager = apps.open_file_manager(Some("/home".to_string()))?;
let agent_manager = apps.open_agent_manager()?;

// Access specialized managers
let agent_mgr = apps.get_agent_manager();
agent_mgr.start_agent("File Copier".to_string(), vec!["file:read".to_string()])?;
```

### 5. Security UI (`security_ui.rs`)

Comprehensive security management interface.

**Features:**
- Real-time threat monitoring
- Permission management for agents
- Human override approval
- Emergency kill switch

**Usage:**
```rust
use security_ui::{SecurityUI, SecurityAlert, ThreatLevel, SecurityLevel};

let security = SecurityUI::new();

// Set security level
security.set_security_level(SecurityLevel::Elevated);

// Agent permission management
security.set_agent_permissions(
    agent_id,
    "Code Review Agent".to_string(),
    vec!["file:read".to_string(), "file:write".to_string()],
);

// Permission request
security.request_permission(PermissionRequest {
    id: uuid::Uuid::new_v4(),
    agent_id,
    agent_name: "Code Review Agent".to_string(),
    permission: "network:outbound".to_string(),
    resource: "api.github.com".to_string(),
    reason: "To check for vulnerabilities".to_string(),
    timestamp: chrono::Utc::now(),
    is_granted: false,
});

// Human override
let override_id = security.request_human_override(
    "backup-agent".to_string(),
    "Delete old backups".to_string(),
    "Freeing disk space".to_string(),
);

// Dashboard
let dashboard = security.get_security_dashboard();
println!("Threat level: {:?}", dashboard.threat_level);

// Emergency kill switch
security.emergency_kill_switch();
```

## Command Line Options

```bash
desktop_environment --backend wayland --kiosk --no-ai --secure
```

| Option | Description |
|--------|-------------|
| `--backend` | Display server backend (wayland, x11) |
| `--kiosk` | Start in kiosk mode |
| `--no-ai` | Disable AI features |
| `--secure` | Enable secure mode with elevated security |

## Security Model

### Security Levels

1. **Standard**: Normal operation with default permissions
2. **Elevated**: Additional verification for sensitive operations
3. **Lockdown**: Maximum security, block non-essential operations

### Agent Permissions

Agents require explicit permissions for:
- File system access (read, write, delete)
- Network operations
- Process spawning
- Agent delegation

### Human Override

For high-risk operations, agents must request human approval:
1. Agent submits override request with action and reason
2. User receives notification
3. User approves or denies via Security UI
4. Action proceeds or is blocked

## Integration with Agent Runtime

The desktop environment integrates with the Agent Runtime:

```rust
// Register agent with desktop
let agent_id = agent_runtime.spawn_agent(config)?;
desktop.register_agent_hud(agent_id, config.name);

// Agent notifications
desktop.show_agent_notification(
    format!("Agent {} complete", config.name),
    result_summary,
    false,
);

// Security permissions
desktop.request_permission(agent_id, PermissionType::FileWrite, path);
```

## Workspace Management

Workspaces support context-based organization:

| Workspace | Context Type | Typical Use |
|-----------|--------------|-------------|
| 1 | Development | IDE, terminal, docs |
| 2 | Communication | Email, chat, meetings |
| 3 | Design | Graphics, UI tools |
| 4 | General | Mixed use |

## Building and Running

```bash
# Build
cd userland/desktop-environment
cargo build --release

# Run
./target/release/desktop_environment

# With AI disabled
./target/release/desktop_environment --no-ai

# With secure mode
./target/release/desktop_environment --secure
```

## Future Enhancements

- [ ] Smithay/Wayland backend implementation
- [ ] GPU-accelerated rendering
- [ ] Multi-monitor support
- [ ] Touch/tablet input
- [ ] Custom theme engine
- [ ] Accessibility features
- [ ] Remote desktop sharing
