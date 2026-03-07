# Agent Development Guide

> **Last Updated**: 2026-03-07

This guide walks you through creating custom agents for AGNOS.

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Agent Architecture](#agent-architecture)
4. [Creating Your First Agent](#creating-your-first-agent)
5. [Agent Configuration](#agent-configuration)
6. [Permissions & Security](#permissions--security)
7. [Inter-Agent Communication](#inter-agent-communication)
8. [Using LLM Capabilities](#using-llm-capabilities)
9. [Testing Agents](#testing-agents)
10. [Best Practices](#best-practices)

---

## Overview

Agents in AGNOS are autonomous processes that can:
- Perform tasks on behalf of users
- Communicate with other agents
- Access system resources (within permissions)
- Use LLM capabilities for reasoning

### Agent Types

| Type | Description |
|------|-------------|
| `Service` | Long-running background agent |
| `Task` | One-time task execution |
| `Interactive` | User-facing agent with UI |

---

## Quick Start

Create a simple agent in 5 minutes:

```bash
# 1. Create new agent project
cargo new --lib my-agent
cd my-agent

# 2. Add dependencies
cargo add agnos-sys agnos-common tokio

# 3. Implement the agent
```

---

## Agent Architecture

### Core Components

```
Agent
├── AgentContext     # Runtime context & state
├── AgentConfig      # Configuration & permissions  
├── Message Handler  # Inter-agent communication
└── Resources       # GPU, memory, storage access
```

### Lifecycle

```
┌─────────┐
│ Created │──> init() 
└─────────┘
      │
      v
┌─────────┐
│Starting │──> set_status(Running)
└─────────┘
      │
      v
┌─────────┐
│ Running │──> run() loop
└─────────┘
      │
      v
┌─────────┐
│Stopping │──> shutdown()
└─────────┘
      │
      v
┌─────────┐
│ Stopped │
└─────────┘
```

---

## Creating Your First Agent

### 1. Define Your Agent

```rust
use agnos_sys::agent::{Agent, AgentContext, AgentRuntime};
use agnos_common::{AgentConfig, AgentType, Permission};
use anyhow::Result;
use async_trait::async_trait;

pub struct MyAgent {
    name: String,
}

impl MyAgent {
    pub fn new() -> Result<Self> {
        Ok(Self {
            name: "my-agent".to_string(),
        })
    }
}

#[async_trait]
impl Agent for MyAgent {
    async fn init(&mut self, ctx: &AgentContext) -> Result<()> {
        println!("Initializing agent: {}", self.name);
        Ok(())
    }

    async fn run(&mut self, ctx: &AgentContext) -> Result<()> {
        println!("Agent {} is running!", self.name);
        
        // Your agent logic here
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }

    async fn handle_message(&mut self, ctx: &AgentContext, message: agnos_common::Message) -> Result<()> {
        println!("Received: {:?}", message.payload);
        Ok(())
    }

    async fn shutdown(&mut self, ctx: &AgentContext) -> Result<()> {
        println!("Shutting down agent: {}", self.name);
        Ok(())
    }
}
```

### 2. Configure Agent

```rust
fn main() -> Result<()> {
    let config = AgentConfig {
        name: "my-agent".to_string(),
        agent_type: AgentType::Service,
        permissions: vec![
            Permission::FileRead,
            Permission::FileWrite,
            Permission::NetworkOutgoing,
        ],
        resource_limits: ResourceLimits {
            max_memory: 512 * 1024 * 1024, // 512MB
            max_cpu_percent: 50,
            ..Default::default()
        },
        ..Default::default()
    };

    Ok(())
}
```

### 3. Register and Run

```rust
use agnos_sys::agent::agent_main;

agent_main!(MyAgent);
```

---

## Agent Configuration

### Configuration File

Save as `/etc/agnos/agents/my-agent.yaml`:

```yaml
name: my-agent
type: service

permissions:
  - file:read
  - file:write
  - network:outgoing

resources:
  max_memory: 512MB
  max_cpu: 50%
  gpu_required: false

environment:
  RUST_LOG: info
  MODEL_PATH: /var/lib/agnos/models

autostart: true
restart_policy: on-failure
```

### Resource Limits

```rust
use agnos_common::{ResourceLimits, AgentConfig};

let config = AgentConfig {
    resource_limits: ResourceLimits {
        max_memory: 1024 * 1024 * 1024,  // 1GB
        max_file_descriptors: 100,
        max_processes: 10,
        max_cpu_percent: 80,
        max_gpu_memory: Some(2 * 1024 * 1024 * 1024), // 2GB
    },
    ..Default::default()
};
```

---

## Permissions & Security

### Permission Categories

| Category | Permissions |
|----------|-------------|
| File | `file:read`, `file:write`, `file:delete` |
| Network | `network:incoming`, `network:outgoing` |
| Process | `process:spawn`, `process:kill` |
| System | `system:info`, `system:config` |
| LLM | `llm:inference`, `llm:load_model` |

### Requesting Permissions

```rust
async fn request_permission(ctx: &AgentContext, permission: Permission) -> Result<bool> {
    // Permission requests go to Security UI for human approval
    // For now, check if we have it
    ctx.config.permissions.contains(&permission)
}
```

### Sandbox Configuration

```rust
use agnos_common::SandboxConfig;

let sandbox = SandboxConfig {
    enable_landlock: true,
    enable_seccomp: true,
    isolate_network: true,
    network_access: NetworkAccess::Restricted,
    allowed_hosts: vec!["api.example.com".to_string()],
    allowed_paths: vec!["/home/user/data".to_string()],
};
```

---

## Inter-Agent Communication

### Sending Messages

```rust
async fn send_to_agent(ctx: &AgentContext, target: &str, payload: serde_json::Value) -> Result<()> {
    ctx.send_message(target, payload).await
}
```

### Receiving Messages

```rust
async fn handle_message(&mut self, ctx: &AgentContext, message: agnos_common::Message) -> Result<()> {
    match message.message_type {
        MessageType::Command => {
            // Handle command
        }
        MessageType::Event => {
            // Handle event
        }
        _ => {}
    }
    Ok(())
}
```

### Message Types

- **Command**: Request another agent to perform action
- **Response**: Reply to a command
- **Event**: Notify about something that happened
- **Heartbeat**: Keepalive between agents

---

## Using LLM Capabilities

### Simple Inference

```rust
use agnos_sys::agent::helpers::llm_inference;

async fn ask_llm(prompt: &str) -> Result<String> {
    let response = llm_inference(prompt, None).await?;
    Ok(response)
}
```

### Advanced Usage

```rust
use agnos_common::llm::{InferenceRequest, InferenceRequestBuilder};

async fn advanced_inference() -> Result<String> {
    let request = InferenceRequest::builder()
        .prompt("Explain this code:")
        .model("llama2")
        .max_tokens(512)
        .temperature(0.7)
        .build();
    
    // Send to LLM gateway via IPC
    // ...
    Ok(response)
}
```

---

## Testing Agents

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_initialization() {
        let agent = MyAgent::new().unwrap();
        assert_eq!(agent.name, "my-agent");
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_agent_lifecycle() {
    let config = AgentConfig {
        name: "test-agent".to_string(),
        ..Default::default()
    };

    let (runtime, _rx) = AgentRuntime::new(config);
    let agent = MyAgent::new().unwrap();
    
    // Test initialization
    // runtime.run(agent).await;
}
```

---

## Best Practices

### 1. Error Handling

```rust
async fn safe_operation() -> Result<T> {
    // Always handle errors gracefully
    // Log errors for debugging
    // Return meaningful error messages
}
```

### 2. Resource Management

```rust
impl Drop for MyAgent {
    fn drop(&mut self) {
        // Clean up resources
        // Close connections
        // Save state
    }
}
```

### 3. Logging

```rust
use tracing::{info, error, debug};

info!("Agent started");
debug!("Processing request: {:?}", request);
error!("Failed to connect: {}", err);
```

### 4. Security

- Request minimum required permissions
- Never log sensitive data
- Validate all inputs
- Use sandbox for file operations

### 5. Performance

- Use async/await for I/O
- Cache frequently accessed data
- Set appropriate resource limits
- Monitor memory usage

---

## Example: File Manager Agent

See `userland/examples/file-manager-agent.rs` for a complete example.

Key features:
- List/read/write files
- Permission checking
- Audit logging
- Error handling

---

## Marketplace Packaging

Agents can be distributed via the mela marketplace in `.agnos-agent` format (signed tarball + metadata).

### Package Structure

```
my-agent.agnos-agent
├── manifest.toml        # Name, version, permissions, sandbox profile
├── binary               # Compiled agent binary
├── sandbox.json         # Landlock + seccomp rules
└── signature.ed25519    # Ed25519 signature (verified by sigil)
```

### Installing via ark

```bash
# Install from marketplace
ark install publisher/my-agent

# Install from local file
ark install ./my-agent.agnos-agent

# List installed agents
ark list --agents
```

## MCP Tools

The agent runtime exposes 16 MCP (Model Context Protocol) tools that agents can invoke for system interaction. These are accessible via the `/v1/mcp/tools` API endpoint.

Key MCP tool categories:
- **File operations** — read, write, list files within sandbox
- **Agent management** — spawn, stop, query other agents
- **System info** — hardware, resource usage, health
- **Knowledge** — RAG query, knowledge search, indexing

## Import Paths

When importing AGNOS crates in your agent code, use the actual crate names:

```rust
use agent_runtime::prelude::*;   // Agent runtime types
use agnos_common::*;              // Shared types, errors, audit
use agnos_sys::*;                 // Kernel interface bindings
```

**Note:** In `Cargo.toml`, crate names use hyphens (`agent-runtime`, `agnos-common`, `agnos-sys`) but in Rust `use` statements they use underscores.

---

## Next Steps

- Read [AGENT_RUNTIME.md](../AGENT_RUNTIME.md) for detailed architecture
- Check [Security Guide](../security/security-guide.md) for security best practices
- Explore [API Reference](../api/README.md)
- Join community at #agnos:matrix.org
