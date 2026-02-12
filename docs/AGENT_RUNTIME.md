# Phase 3: Agent Runtime Implementation

## Overview

Phase 3 implements the **Agent Runtime** - a multi-agent orchestration system for AGNOS. This provides the foundation for running, managing, and coordinating AI agents within the operating system.

## Components Implemented

### 1. Agent Kernel Module (`kernel/modules/agent-subsystem/`)

Provides low-level kernel support for agent process management:

- **Process Type**: Specialized agent processes with unique attributes
- **Syscalls**: 
  - `agnos_agent_create` - Create a new agent process
  - `agnos_agent_terminate` - Terminate an agent
  - Resource quota enforcement
  - Capability restrictions
- **IPC Mechanisms**: Foundation for agent-to-agent and agent-to-kernel communication
- **Resource Scheduler**: GPU allocation, memory limits, CPU prioritization

**Files:**
- `agent_main.c` - Core kernel module with agent process management
- `Kbuild` - Build configuration

### 2. Agent Runtime Daemon (`userland/agent-runtime/`)

The main daemon (`agent-runtime`/`akd`) that manages agent lifecycle:

#### Modules:

**`agent.rs`** - Agent representation and lifecycle
- Agent creation and configuration
- Start/stop/pause/resume operations
- Resource limit enforcement (memory, CPU time)
- Process management with signal handling

**`registry.rs`** - Agent registry
- Central registry for all agents
- Lookup by ID, name, or capability
- Agent discovery and capability advertisement
- Status tracking

**`orchestrator.rs`** - Multi-agent orchestration
- Task distribution and workload balancing
- Priority-based task queues (Critical, High, Normal, Low, Background)
- Auto-assignment of tasks to agents
- Message bus for inter-agent communication
- Task dependency resolution

**`supervisor.rs`** - Health monitoring
- Health check loop with configurable intervals
- Resource limit enforcement
- Automatic failure detection and recovery
- Graceful shutdown coordination

**`ipc.rs`** - Inter-process communication
- Unix domain sockets for agent communication
- Message routing and pub/sub system
- Global message bus

**`sandbox.rs`** - Security isolation
- Landlock filesystem restrictions (when available)
- seccomp-bpf filter support
- Network namespace isolation
- Configurable access levels (None, Localhost, Restricted, Full)

**`resource.rs`** - Resource management
- GPU detection and allocation (NVIDIA via nvidia-smi)
- CPU core allocation
- Memory reservation and tracking
- Multi-GPU support

### 3. LLM Gateway Service (`userland/llm-gateway/`)

Extended LLM gateway with agent support:

#### Modules:

**`main.rs`** - Gateway service
- Unified interface for local and cloud LLMs
- Model loading/unloading
- Inference with streaming support
- Token accounting per agent
- Shared model sessions for multi-agent access

**`providers.rs`** - LLM provider implementations
- Ollama provider (local models)
- llama.cpp provider
- Extensible for OpenAI, Anthropic, Google

**`cache.rs`** - Response caching
- LRU cache with TTL
- Cache statistics
- Configurable cache policies

**`accounting.rs`** - Token accounting
- Per-agent token usage tracking
- Total usage aggregation
- Usage statistics and reporting

### 4. Agent SDK (`userland/agnos-sys/src/agent.rs`)

Rust SDK for building AGNOS agents:

- **`Agent` trait**: Interface all agents must implement
  - `init()` - Initialize the agent
  - `run()` - Main agent loop
  - `handle_message()` - Process incoming messages
  - `shutdown()` - Cleanup before exit

- **`AgentContext`**: Runtime context for agents
  - Agent ID and configuration
  - Message sending capabilities
  - Status management

- **`AgentRuntime`**: Executes agent implementations
- **Helper functions**: LLM inference, audit logging, resource checking
- **`agent_main!` macro**: Simplified entry point for agents

### 5. Example Agent (`userland/examples/file-manager-agent.rs`)

Reference implementation demonstrating:
- Agent trait implementation
- Message handling
- File operations within sandbox constraints
- Proper lifecycle management

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Agent Runtime Daemon                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │   Registry   │  │ Orchestrator │  │   Supervisor     │  │
│  │  (Discovery) │  │ (Scheduling) │  │ (Health Monitor) │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
│           │                │                  │             │
│  ┌────────▼────────┐  ┌─────▼──────┐  ┌───────▼────────┐   │
│  │   Agent Pool    │  │  Task      │  │  Health Checks │   │
│  │                 │  │  Queues    │  │                │   │
│  └─────────────────┘  └────────────┘  └────────────────┘   │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         IPC Layer (Unix Sockets / Message Bus)        │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│   Agent 1    │   │   Agent 2    │   │   Agent N    │
│ (File Mgr)   │   │ (Code Asst)  │   │ (Monitor)    │
└──────────────┘   └──────────────┘   └──────────────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            ▼
              ┌─────────────────────────┐
              │     LLM Gateway         │
              │  (Model Sharing,        │
              │   Token Accounting)     │
              └─────────────────────────┘
                            │
                    ┌───────┴───────┐
                    ▼               ▼
              ┌──────────┐   ┌──────────┐
              │  Local   │   │  Cloud   │
              │  Models  │   │   APIs   │
              └──────────┘   └──────────┘
```

## Configuration

### Agent Runtime Service

Systemd service file: `config/systemd/system/agent-runtime.service`

```ini
[Service]
Type=notify
ExecStart=/usr/bin/agent-runtime daemon
User=agnos
Group=agnos
MemoryMax=512M
CPUQuota=50%
```

### Agent Configuration

Agents are configured via YAML/JSON:

```json
{
  "name": "file-manager",
  "agent_type": "Service",
  "resource_limits": {
    "max_memory": 1073741824,
    "max_cpu_time": 3600000,
    "max_file_descriptors": 1024,
    "max_processes": 64
  },
  "sandbox": {
    "filesystem_rules": [
      {"path": "/tmp", "access": "ReadWrite"}
    ],
    "network_access": "LocalhostOnly",
    "isolate_network": true
  },
  "permissions": ["FileRead", "FileWrite", "NetworkAccess"]
}
```

## Usage

### Starting the Agent Runtime

```bash
# Start the daemon
agent-runtime daemon

# Or via systemd
systemctl start agent-runtime
```

### Managing Agents

```bash
# Start a new agent
agent-runtime start --config /etc/agnos/agents/file-manager.json

# Stop an agent
agent-runtime stop <agent-id>

# List all agents
agent-runtime list

# Get agent status
agent-runtime status <agent-id>

# Send message to agent
agent-runtime send <agent-id> '{"action": "list_files"}'
```

### LLM Gateway

```bash
# Start the gateway
llm-gateway daemon

# Load a model
llm-gateway load llama2:7b

# Run inference
llm-gateway infer --model llama2:7b --prompt "Hello, world!"

# Show token usage
llm-gateway stats
```

## Security Features

1. **Sandboxing**
   - Landlock filesystem restrictions
   - seccomp-bpf syscall filtering
   - Network namespace isolation
   - Resource limits enforcement

2. **Capability System**
   - Fine-grained permissions
   - Capability delegation
   - Revocation support

3. **Audit Trail**
   - All agent actions logged
   - Resource usage tracking
   - Token accounting

## Performance Targets

Per TODO.md Phase 3 KPIs:

| Metric | Target | Status |
|--------|--------|--------|
| Agent spawn time | <500ms | 🔄 In Progress |
| Task distribution latency | <100ms | 🔄 In Progress |
| Multi-agent throughput | 100+ agents | 🔄 In Progress |
| LLM inference latency | Provider dependent | 🔄 In Progress |

## Future Enhancements

1. **Advanced Orchestration**
   - Consensus mechanisms for distributed agents
   - Conflict resolution strategies
   - Dynamic load balancing

2. **Security**
   - Code signing for agents
   - Runtime verification
   - Supply chain validation

3. **Performance**
   - Hardware-accelerated inference (NPU/GPU)
   - Distributed agent computing
   - Swarm intelligence protocols

## Integration with Other Phases

- **Phase 2 (AI Shell)**: Agents can be invoked from the shell
- **Phase 4 (Desktop)**: Agent status visible in GUI
- **Phase 5 (Production)**: Security audits and hardening

## Files Modified/Created

### Kernel Module
- `kernel/modules/agent-subsystem/agent_main.c`
- `kernel/modules/agent-subsystem/Kbuild`

### Userland
- `userland/agent-runtime/src/main.rs` (complete rewrite)
- `userland/agent-runtime/src/agent.rs` (new)
- `userland/agent-runtime/src/registry.rs` (new)
- `userland/agent-runtime/src/orchestrator.rs` (new)
- `userland/agent-runtime/src/supervisor.rs` (new)
- `userland/agent-runtime/src/ipc.rs` (new)
- `userland/agent-runtime/src/sandbox.rs` (new)
- `userland/agent-runtime/src/resource.rs` (new)
- `userland/agent-runtime/Cargo.toml` (updated)

- `userland/llm-gateway/src/main.rs` (complete rewrite)
- `userland/llm-gateway/src/providers.rs` (new)
- `userland/llm-gateway/src/cache.rs` (new)
- `userland/llm-gateway/src/accounting.rs` (new)
- `userland/llm-gateway/Cargo.toml` (updated)

- `userland/agnos-sys/src/agent.rs` (new)
- `userland/agnos-sys/src/lib.rs` (updated)
- `userland/agnos-sys/Cargo.toml` (updated)

- `userland/agnos-common/src/lib.rs` (updated exports)

- `userland/Cargo.toml` (updated dependencies)
- `userland/examples/file-manager-agent.rs` (new)

## Testing

Run the following to verify the implementation:

```bash
# Build all components
make build-userland

# Run unit tests
cargo test --package agent_runtime
cargo test --package llm_gateway
cargo test --package agnos-sys

# Check formatting and linting
make format-check
make lint
```

## References

- TODO.md Phase 3 specifications
- AGNOS Architecture Document
- Agent SDK Documentation (to be written)

---

**Status**: Core implementation complete 🎉
**Next Steps**: Testing, refinement, and integration with Phase 4 (Desktop)
