# API Reference

AGNOS provides multiple APIs for interacting with the system.

## Core APIs

### Agent Runtime API

The Agent Runtime API allows spawning and managing AI agents.

#### Spawn Agent

```rust
use agnos::agent::{AgentRuntime, AgentConfig};

let runtime = AgentRuntime::new();
let config = AgentConfig {
    name: "file-manager".to_string(),
    capabilities: vec![
        "file:read".to_string(),
        "file:write".to_string(),
    ],
    max_memory_mb: 512,
    timeout_seconds: 300,
};

let agent_id = runtime.spawn_agent(config)?;
```

#### Stop Agent

```rust
runtime.stop_agent(agent_id)?;
```

#### List Agents

```rust
let agents = runtime.list_agents();
for agent in agents {
    println!("{}: {}", agent.id, agent.name);
}
```

### Security API

The Security API manages permissions and access control.

#### Set Permissions

```rust
use agnos::security::SecurityUI;

let security = SecurityUI::new();
security.set_agent_permissions(
    agent_id,
    "file-agent".to_string(),
    vec!["file:read".to_string(), "file:write".to_string()],
)?;
```

#### Check Permission

```rust
if security.has_permission(agent_id, "file:write") {
    // Allow operation
}
```

#### Request Human Override

```rust
let request_id = security.request_human_override(
    "backup-agent".to_string(),
    "Delete /tmp/old_backups".to_string(),
    "Freeing disk space".to_string(),
)?;
```

### LLM Gateway API

The LLM Gateway API provides unified access to language models.

#### Send Prompt

```rust
use agnos::llm::{LlmGateway, Provider};

let gateway = LlmGateway::new();
gateway.set_provider(Provider::Ollama {
    base_url: "http://localhost:11434".to_string(),
    model: "llama2".to_string(),
});

let response = gateway.complete("What is 2+2?").await?;
println!("{}", response.text);
```

#### Streaming Response

```rust
let mut stream = gateway.complete_stream("Tell me a story").await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk.text);
}
```

### Desktop API

The Desktop API controls the graphical environment.

#### Create Window

```rust
use agnos::desktop::Compositor;

let compositor = Compositor::new();
let window_id = compositor.create_window(
    "My App".to_string(),
    "myapp".to_string(),
    false,
)?;
```

#### Manage Workspaces

```rust
compositor.switch_workspace(2)?;
compositor.move_window_to_workspace(window_id, 1)?;
```

#### AI Features

```rust
use agnos::desktop::AIDesktopFeatures;

let ai = AIDesktopFeatures::new();
ai.register_agent_hud(agent_id, "File Manager".to_string());
ai.update_agent_hud(agent_id, AgentStatus::Acting, "Copying files".to_string(), 0.75);
```

## gRPC API

The system services expose gRPC APIs for remote control.

### Agent Service

```protobuf
service AgentService {
    rpc SpawnAgent(SpawnRequest) returns (SpawnResponse);
    rpc StopAgent(StopRequest) returns (StopResponse);
    rpc ListAgents(ListRequest) returns (ListResponse);
    rpc GetAgentStatus(StatusRequest) returns (StatusResponse);
}
```

### LLM Service

```protobuf
service LlmService {
    rpc Complete(CompleteRequest) returns (CompleteResponse);
    rpc CompleteStream(CompleteRequest) returns (stream CompleteResponse);
    rpc GetModels(ModelsRequest) returns (ModelsResponse);
}
```

### Security Service

```protobuf
service SecurityService {
    rpc SetPermissions(PermissionsRequest) returns (PermissionsResponse);
    rpc CheckPermission(CheckRequest) returns (CheckResponse);
    rpc RequestOverride(OverrideRequest) returns (OverrideResponse);
}
```

## REST API

REST endpoints for web integration.

### Authentication

```bash
POST /api/v1/auth/login
Content-Type: application/json

{
    "username": "admin",
    "password": "..."
}
```

### Agents

```bash
# List agents
GET /api/v1/agents

# Spawn agent
POST /api/v1/agents
Content-Type: application/json

{
    "name": "file-manager",
    "capabilities": ["file:read"]
}

# Stop agent
DELETE /api/v1/agents/{id}
```

### LLM

```bash
# Complete prompt
POST /api/v1/llm/complete
Content-Type: application/json

{
    "provider": "ollama",
    "model": "llama2",
    "prompt": "What is AGNOS?"
}
```

## IPC API

Inter-process communication for local agents.

### Message Types

```rust
pub enum Message {
    Task {
        id: Uuid,
        description: String,
        priority: Priority,
    },
    Result {
        task_id: Uuid,
        success: bool,
        output: String,
    },
    Status {
        agent_id: Uuid,
        state: AgentState,
    },
}
```

### Message Bus

```rust
use agnos::ipc::MessageBus;

let bus = MessageBus::new("agnos-agent-bus");

// Subscribe to messages
bus.subscribe(|msg| {
    match msg {
        Message::Task { id, description, .. } => {
            println!("New task: {} - {}", id, description);
        }
        _ => {}
    }
});

// Send message
bus.send(Message::Result {
    task_id: Uuid::new_v4(),
    success: true,
    output: "Task completed".to_string(),
})?;
```

## Shell API

The AI Shell provides a natural language interface.

### Execute Command

```rust
use agnos::shell::{AIShell, Command};

let shell = AIShell::new();
let result = shell.execute("list files in /home").await?;
println!("{}", result.output);
```

### Batch Commands

```rust
let commands = vec![
    "cd /home/user",
    "ls -la",
    "cat README.md",
];

for cmd in commands {
    shell.execute(cmd).await?;
}
```

## Event API

Subscribe to system events.

```rust
use agnos::events::{EventStream, EventType};

let events = EventStream::new();
events.subscribe(EventType::AgentSpawned, |event| {
    println!("Agent spawned: {:?}", event.agent_id);
});

events.subscribe(EventType::PermissionDenied, |event| {
    println!("Permission denied: {:?}", event);
});
```

## Error Handling

All APIs use standardized error types.

```rust
pub enum AGNOSError {
    NotFound(String),
    PermissionDenied(String),
    InvalidInput(String),
    Timeout(String),
    Internal(String),
}
```

## Rate Limiting

APIs implement rate limiting to prevent abuse.

| API | Rate Limit |
|-----|------------|
| LLM Complete | 100 req/min |
| Agent Spawn | 10 req/min |
| File Operations | 1000 req/min |

## Versioning

APIs follow semantic versioning.

- **v1** (Current): Initial API release
- Changes are backward compatible within major versions
- Deprecated endpoints are marked and supported for 6 months

## SDKs

Official SDKs available:

- **Rust**: Built-in (`agnos` crate)
- **Python**: `pip install agnos-sdk`
- **JavaScript**: `npm install @agnos/sdk`

## Examples

See `/examples` directory for complete working examples:

- `file-manager-agent.rs` - File management agent
- `code-assistant-agent.rs` - Code review agent
- `system-monitor-agent.rs` - System monitoring agent
