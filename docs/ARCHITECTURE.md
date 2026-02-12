# AGNOS System Architecture

This document provides a detailed technical architecture of AGNOS (AI-Native General Operating System).

## Table of Contents

1. [System Overview](#system-overview)
2. [Kernel Architecture](#kernel-architecture)
3. [User Space Architecture](#user-space-architecture)
4. [Security Architecture](#security-architecture)
5. [Data Flow](#data-flow)
6. [Technology Stack](#technology-stack)
7. [Design Decisions](#design-decisions)

## System Overview

AGNOS is a specialized Linux distribution designed for AI agent execution with human oversight. The architecture consists of three main layers:

```
┌─────────────────────────────────────────────────────────────────┐
│                      AGNOS Architecture                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │                    User Space Layer                        │ │
│  │  ┌─────────────┐ ┌─────────────┐ ┌──────────────────────┐ │ │
│  │  │   Desktop   │ │  AI Shell   │ │  Agent Applications  │ │ │
│  │  │Environment  │ │   (agnsh)   │ │                      │ │ │
│  │  └──────┬──────┘ └──────┬──────┘ └──────────┬───────────┘ │ │
│  │         └─────────────────┴───────────────────┘            │ │
│  │                           │                                │ │
│  │                    ┌──────┴──────┐                         │ │
│  │                    │ Agent Runtime│                        │ │
│  │                    │ Environment  │                        │ │
│  │                    │ ┌──────────┐ │                        │ │
│  │                    │ │ Agent    │ │                        │ │
│  │                    │ │ Kernel   │ │                        │ │
│  │                    │ │ Daemon   │ │                        │ │
│  │                    │ ├──────────┤ │                        │ │
│  │                    │ │ LLM      │ │                        │ │
│  │                    │ │ Gateway  │ │                        │ │
│  │                    │ ├──────────┤ │                        │ │
│  │                    │ │ Message  │ │                        │ │
│  │                    │ │ Bus      │ │                        │ │
│  │                    │ └──────────┘ │                        │ │
│  │                    └──────┬───────┘                        │ │
│  └───────────────────────────┼────────────────────────────────┘ │
│                              │                                  │
├──────────────────────────────┼──────────────────────────────────┤
│                              │                                  │
│  ┌───────────────────────────┼────────────────────────────────┐ │
│  │                  Kernel Space Layer                        │ │
│  │  ┌────────────────────────┴──────────────────────────────┐ │ │
│  │  │              Linux 6.6 LTS (Hardened)                  │ │ │
│  │  │  ┌─────────────┐ ┌─────────────┐ ┌──────────────────┐ │ │ │
│  │  │  │   AGNOS     │ │   Agent     │ │     LLM          │ │ │ │
│  │  │  │   Security  │ │   Kernel    │ │   Kernel         │ │ │ │
│  │  │  │   Module    │ │   Subsystem │ │   Module         │ │ │ │
│  │  │  └─────────────┘ └─────────────┘ └──────────────────┘ │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  │                            │                                │ │
│  │  ┌─────────────────────────┴─────────────────────────────┐ │ │
│  │  │              Hardware Abstraction Layer                │ │ │
│  │  │         (CPU, GPU, NPU, Memory, Storage, I/O)          │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Kernel Architecture

### Base Kernel

AGNOS uses Linux 6.6 LTS as the base kernel with extensive hardening patches and custom modules.

#### Kernel Configuration

Key kernel configuration options:

```
# Security
CONFIG_SECURITY=y
CONFIG_SECURITY_SELINUX=y
CONFIG_SECURITY_LANDLOCK=y
CONFIG_SECURITY_SECCOMP=y
CONFIG_SECURITY_SECCOMP_FILTER=y
CONFIG_SECURITY_YAMA=y

# Memory protection
CONFIG_KASAN=y
CONFIG_KASAN_INLINE=y
CONFIG_KMSAN=y
CONFIG_KCSAN=y
CONFIG_PAGE_TABLE_ISOLATION=y
CONFIG_RETPOLINE=y

# Namespaces
CONFIG_NAMESPACES=y
CONFIG_USER_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
CONFIG_IPC_NS=y
CONFIG_UTS_NS=y
CONFIG_CGROUP_NS=y
CONFIG_TIME_NS=y

# cgroups
CONFIG_CGROUPS=y
CONFIG_CGROUP_CPUACCT=y
CONFIG_CGROUP_DEVICE=y
CONFIG_CGROUP_FREEZER=y
CONFIG_CGROUP_SCHED=y
CONFIG_CGROUP_PIDS=y
CONFIG_CGROUP_RDMA=y
CONFIG_CGROUP_BPF=y
CONFIG_CGROUP_MISC=y

# AGNOS-specific
CONFIG_AGNOS_SECURITY=m
CONFIG_AGNOS_AGENT_SUBSYSTEM=m
CONFIG_AGNOS_LLM=m
CONFIG_AGNOS_AUDIT=m
```

### AGNOS Security Module

The AGNOS Security Module (ASM) is a loadable kernel module that provides additional security features.

```c
// kernel/security/agnos/asm.c

/**
 * AGNOS Security Module
 * 
 * Provides:
 * - Enhanced Landlock integration
 * - Custom seccomp-bpf policies
 * - Agent capability management
 * - Security event auditing
 */

#include <linux/lsm_hooks.h>
#include <linux/security.h>
#include <linux/agnos_security.h>

static int agnos_file_open(struct file *file, const struct cred *cred)
{
    struct agnos_agent_ctx *ctx = agnos_get_current_agent();
    
    if (ctx && !agnos_agent_may_access(ctx, file->f_path.dentry)) {
        agnos_audit_log(ctx, AGNOS_AUDIT_FILE_ACCESS, file, -EACCES);
        return -EACCES;
    }
    
    return 0;
}

static int agnos_socket_create(int family, int type, int protocol, int kern)
{
    struct agnos_agent_ctx *ctx = agnos_get_current_agent();
    
    if (ctx && !agnos_agent_may_network(ctx, family, type)) {
        agnos_audit_log(ctx, AGNOS_AUDIT_NETWORK, NULL, -EACCES);
        return -EACCES;
    }
    
    return 0;
}

static struct security_hook_list agnos_hooks[] = {
    LSM_HOOK_INIT(file_open, agnos_file_open),
    LSM_HOOK_INIT(socket_create, agnos_socket_create),
    // ... more hooks
};
```

### Agent Kernel Subsystem

The Agent Kernel Subsystem provides low-level support for agent processes.

#### Agent Process Type

```c
// kernel/agnos/agent/agent_process.c

/**
 * Agent Process Structure
 * 
 * Extends standard task_struct with agent-specific fields
 */
struct agent_process {
    struct task_struct *task;
    
    // Agent identification
    uuid_t agent_id;
    char agent_name[AGNOS_AGENT_NAME_MAX];
    
    // Security context
    struct landlock_ruleset *ruleset;
    struct seccomp_filter *seccomp_filter;
    u64 capabilities;
    
    // Resource limits
    struct agent_limits {
        u64 max_memory;
        u64 max_cpu_time;
        u32 max_file_descriptors;
        u32 max_processes;
    } limits;
    
    // Current usage
    struct agent_usage {
        u64 memory_used;
        u64 cpu_time_used;
        u32 file_descriptors_used;
    } usage;
    
    // Audit context
    struct audit_context *audit_ctx;
    
    // IPC
    struct agent_namespace *ipc_ns;
};

// System calls
SYSCALL_DEFINE2(agnos_agent_create,
                const struct agnos_agent_config __user *, config,
                u32 __user *, agent_id);

SYSCALL_DEFINE1(agnos_agent_terminate,
                u32, agent_id);

SYSCALL_DEFINE3(agnos_agent_set_limits,
                u32, agent_id,
                const struct agent_limits __user *, limits);
```

### LLM Kernel Module

The LLM Kernel Module provides hardware-accelerated inference capabilities.

```c
// kernel/agnos/llm/llm_module.c

/**
 * LLM Kernel Module
 * 
 * Provides:
 * - GPU/NPU memory management
 * - Model memory mapping
 * - Token streaming
 * - Inference scheduling
 */

// Memory region for model weights
struct llm_model_region {
    void *vaddr;
    phys_addr_t paddr;
    size_t size;
    struct agnos_agent *owner;
    struct list_head list;
};

// Inference request
struct llm_inference_req {
    u32 agent_id;
    void *input_tokens;
    size_t input_len;
    void *output_buffer;
    size_t output_buffer_len;
    u32 max_new_tokens;
    struct completion completion;
};

// System calls
SYSCALL_DEFINE2(agnos_llm_load_model,
                const char __user *, model_path,
                u32 __user *, model_id);

SYSCALL_DEFINE2(agnos_llm_inference,
                u32, model_id,
                struct llm_inference_req __user *, req);

SYSCALL_DEFINE1(agnos_llm_unload_model,
                u32, model_id);
```

### Audit Kernel Module

```c
// kernel/agnos/audit/audit_module.c

/**
 * AGNOS Audit Module
 * 
 * Provides tamper-evident audit logging
 */

struct audit_entry {
    u64 sequence;
    u64 timestamp;
    uuid_t agent_id;
    uuid_t user_id;
    u32 action_type;
    u32 result;
    u8 hash[SHA256_DIGEST_SIZE];
    u8 prev_hash[SHA256_DIGEST_SIZE];
    u8 signature[HMAC_SHA256_SIZE];
    char payload[];  // Variable length
};

// Write audit entry with cryptographic signing
int agnos_audit_log(struct audit_context *ctx, u32 action, void *data, int result)
{
    struct audit_entry *entry;
    u8 hash[SHA256_DIGEST_SIZE];
    
    entry = kzalloc(sizeof(*entry) + payload_len, GFP_KERNEL);
    
    entry->sequence = atomic64_inc_return(&audit_seq);
    entry->timestamp = ktime_get_real_ns();
    entry->action_type = action;
    entry->result = result;
    
    // Copy previous hash for chain
    memcpy(entry->prev_hash, ctx->last_hash, SHA256_DIGEST_SIZE);
    
    // Calculate hash of entry
    agnos_hash_entry(entry, hash);
    memcpy(entry->hash, hash, SHA256_DIGEST_SIZE);
    
    // Sign with kernel key
    agnos_sign_entry(entry, ctx->signing_key);
    
    // Store in append-only buffer
    agnos_audit_store(entry);
    
    // Update context
    memcpy(ctx->last_hash, hash, SHA256_DIGEST_SIZE);
    
    return 0;
}
```

## User Space Architecture

### Agent Runtime Environment

The Agent Runtime Environment (ARE) manages the lifecycle of agents and provides necessary services.

```rust
// userland/agent-runtime/src/daemon.rs

//! Agent Kernel Daemon
//! 
//! Central daemon for managing agent processes

use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct AgentKernelDaemon {
    /// Running agents
    agents: RwLock<HashMap<Uuid, AgentHandle>>,
    
    /// Resource scheduler
    scheduler: ResourceScheduler,
    
    /// Message bus
    message_bus: MessageBus,
    
    /// LLM gateway client
    llm_gateway: LlmGatewayClient,
    
    /// Security policy engine
    policy_engine: PolicyEngine,
    
    /// Audit client
    audit: AuditClient,
}

impl AgentKernelDaemon {
    /// Create a new agent
    pub async fn create_agent(
        &self,
        config: AgentConfig,
    ) -> Result<Uuid, AgentError> {
        // Validate configuration
        self.policy_engine.validate(&config)?;
        
        // Allocate resources
        let resources = self.scheduler.allocate(&config.resource_request).await?;
        
        // Create kernel agent process
        let agent_id = unsafe {
            agnos_sys::agent_create(&config.into())?
        };
        
        // Set up sandbox
        self.setup_sandbox(agent_id, &config.sandbox)?;
        
        // Start agent process
        let handle = AgentHandle::new(agent_id, resources, config);
        
        // Store in registry
        self.agents.write().await.insert(agent_id, handle);
        
        // Log creation
        self.audit.log(AuditEvent::AgentCreated {
            agent_id,
            config: config.sanitized(),
        }).await?;
        
        Ok(agent_id)
    }
    
    /// Terminate an agent
    pub async fn terminate_agent(
        &self,
        agent_id: Uuid,
        force: bool,
    ) -> Result<(), AgentError> {
        // Get agent handle
        let mut agents = self.agents.write().await;
        let handle = agents.remove(&agent_id)
            .ok_or(AgentError::NotFound)?;
        
        // Release resources
        self.scheduler.release(handle.resources).await?;
        
        // Terminate kernel process
        unsafe {
            agnos_sys::agent_terminate(agent_id.as_u128(), force)?;
        }
        
        // Log termination
        self.audit.log(AuditEvent::AgentTerminated {
            agent_id,
            reason: if force { "forced" } else { "graceful" },
        }).await?;
        
        Ok(())
    }
}
```

### LLM Gateway Service

```rust
// userland/llm-gateway/src/service.rs

//! LLM Gateway Service
//! 
//! Manages LLM inference requests from agents

pub struct LlmGatewayService {
    /// Local model manager
    local_models: ModelManager,
    
    /// Cloud API clients
    cloud_clients: HashMap<Provider, Box<dyn CloudClient>>,
    
    /// Request queue
    request_queue: PriorityQueue<InferenceRequest>,
    
    /// Token usage tracker
    usage_tracker: UsageTracker,
    
    /// Routing strategy
    router: Router,
}

impl LlmGatewayService {
    /// Process inference request
    pub async fn inference(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, GatewayError> {
        // Check permissions
        self.verify_permissions(&request)?;
        
        // Determine routing
        let route = self.router.select(&request)?;
        
        match route {
            Route::Local(model_id) => {
                // Use local model via kernel module
                let response = self.local_inference(model_id, &request).await?;
                Ok(response)
            }
            Route::Cloud(provider) => {
                // Use cloud API
                let response = self.cloud_inference(provider, &request).await?;
                
                // Track usage
                self.usage_tracker.record(&request.agent_id, &response).await?;
                
                Ok(response)
            }
            Route::Hybrid => {
                // Use both for comparison
                self.hybrid_inference(&request).await
            }
        }
    }
    
    async fn local_inference(
        &self,
        model_id: ModelId,
        request: &InferenceRequest,
    ) -> Result<InferenceResponse, GatewayError> {
        // Prepare request for kernel module
        let kernel_req = KernelInferenceRequest {
            model_id: model_id.0,
            input_tokens: &request.input_tokens,
            max_tokens: request.max_tokens,
        };
        
        // Call kernel module
        let result = unsafe {
            agnos_sys::llm_inference(&kernel_req)?
        };
        
        Ok(InferenceResponse {
            output_tokens: result.output,
            usage: result.usage,
            provider: Provider::Local,
        })
    }
}
```

### AI Shell (agnsh)

```rust
// userland/ai-shell/src/shell.rs

//! AI Shell - Natural Language Command Interface

pub struct AiShell {
    /// Shell configuration
    config: ShellConfig,
    
    /// Natural language parser
    nl_parser: NlParser,
    
    /// Intent classifier
    classifier: IntentClassifier,
    
    /// Command translator
    translator: CommandTranslator,
    
    /// LLM client
    llm: LlmClient,
    
    /// Session history
    history: History,
    
    /// Current working directory
    cwd: PathBuf,
    
    /// Environment variables
    env: HashMap<String, String>,
}

impl AiShell {
    /// Main shell loop
    pub async fn run(&mut self) -> Result<(), ShellError> {
        loop {
            // Display prompt
            print!("{} ", self.prompt());
            io::stdout().flush()?;
            
            // Read input
            let input = self.read_line().await?;
            
            if input.trim().is_empty() {
                continue;
            }
            
            // Parse and execute
            match self.process_input(&input).await {
                Ok(result) => {
                    if let Some(output) = result {
                        println!("{}", output);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            }
            
            // Add to history
            self.history.add(&input).await?;
        }
    }
    
    /// Process user input
    async fn process_input(&self, input: &str) -> Result<Option<String>, ShellError> {
        // Check for special commands
        if input.starts_with("bash") {
            // Switch to bash mode
            return self.run_bash(&input[4..]).await;
        }
        
        // Classify intent
        let intent = self.classifier.classify(input).await?;
        
        match intent.category {
            IntentCategory::ShellCommand => {
                // Direct shell command
                self.execute_shell(input).await
            }
            IntentCategory::NaturalLanguage => {
                // Translate to command
                let command = self.translator.translate(input).await?;
                self.execute_shell(&command).await
            }
            IntentCategory::Question => {
                // Answer using LLM
                let answer = self.llm.ask(input).await?;
                Ok(Some(answer))
            }
            IntentCategory::AgentCommand => {
                // Agent management
                self.handle_agent_command(input).await
            }
        }
    }
    
    async fn execute_shell(&self, command: &str) -> Result<Option<String>, ShellError> {
        // Execute using system shell
        let output = tokio::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.cwd)
            .envs(&self.env)
            .output()
            .await?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(if stdout.is_empty() { None } else { Some(stdout.to_string()) })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ShellError::CommandFailed(stderr.to_string()))
        }
    }
}
```

## Security Architecture

### Defense in Depth

```
┌─────────────────────────────────────────────────────────────┐
│                    Network Layer                             │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────────────┐  │
│  │   TLS 1.3   │ │ Domain      │ │   Firewall           │  │
│  │             │ │ Whitelist   │ │   (iptables/nftables)│  │
│  └─────────────┘ └─────────────┘ └──────────────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────┴─────────────────────────────────┐
│                 Application Layer                            │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────────────┐  │
│  │     RBAC    │ │   Input     │ │   Authentication     │  │
│  │             │ │ Validation  │ │   (JWT/API Keys)     │  │
│  └─────────────┘ └─────────────┘ └──────────────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────┴─────────────────────────────────┐
│                  Execution Layer                             │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────────────┐  │
│  │   Landlock  │ │   seccomp   │ │   Encryption         │  │
│  │   Sandbox   │ │   -bpf      │ │   (AES-256-GCM)      │  │
│  └─────────────┘ └─────────────┘ └──────────────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────┴─────────────────────────────────┐
│                   Audit Layer                                │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────────────┐  │
│  │   Event     │ │   Chain     │ │   Integrity          │  │
│  │   Capture   │ │   Hashing   │ │   Verification       │  │
│  └─────────────┘ └─────────────┘ └──────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Sandbox Implementation

```rust
// userland/agent-runtime/src/sandbox.rs

//! Sandboxing implementation using Landlock and seccomp

pub struct Sandbox {
    /// Landlock ruleset
    ruleset: LandlockRuleset,
    
    /// seccomp filter
    seccomp_filter: SeccompFilter,
    
    /// Network namespace
    net_ns: NetworkNamespace,
    
    /// PID namespace
    pid_ns: PidNamespace,
    
    /// Mount namespace
    mount_ns: MountNamespace,
}

impl Sandbox {
    /// Create sandbox from configuration
    pub fn create(config: &SandboxConfig) -> Result<Self, SandboxError> {
        // Create Landlock ruleset
        let mut ruleset = LandlockRuleset::new()?;
        
        // Add filesystem rules
        for rule in &config.filesystem_rules {
            match rule.access {
                FsAccess::ReadOnly => {
                    ruleset.add_rule(
                        rule.path.clone(),
                        LandlockAccess::READ_FILE | LandlockAccess::READ_DIR,
                    )?;
                }
                FsAccess::ReadWrite => {
                    ruleset.add_rule(
                        rule.path.clone(),
                        LandlockAccess::all(),
                    )?;
                }
                FsAccess::NoAccess => {
                    // Explicitly deny
                }
            }
        }
        
        // Create seccomp filter
        let mut seccomp = SeccompFilter::new(Action::Errno(libc::EPERM));
        
        // Allow basic syscalls
        seccomp.allow_syscall(libc::SYS_read);
        seccomp.allow_syscall(libc::SYS_write);
        seccomp.allow_syscall(libc::SYS_exit);
        seccomp.allow_syscall(libc::SYS_exit_group);
        
        // Add custom rules
        for rule in &config.seccomp_rules {
            match rule.action {
                SeccompAction::Allow => seccomp.allow_syscall(rule.syscall),
                SeccompAction::Deny => seccomp.deny_syscall(rule.syscall),
                SeccompAction::Trap => seccomp.trap_syscall(rule.syscall),
            }
        }
        
        // Create namespaces
        let net_ns = if config.isolate_network {
            NetworkNamespace::new()?
        } else {
            NetworkNamespace::current()
        };
        
        let pid_ns = PidNamespace::new()?;
        let mount_ns = MountNamespace::new()?;
        
        Ok(Sandbox {
            ruleset,
            seccomp_filter: seccomp,
            net_ns,
            pid_ns,
            mount_ns,
        })
    }
    
    /// Apply sandbox to current process
    pub fn apply(&self) -> Result<(), SandboxError> {
        // Apply Landlock
        self.ruleset.restrict_self()?;
        
        // Apply seccomp
        self.seccomp_filter.load()?;
        
        // Namespaces are already applied at process creation
        
        Ok(())
    }
}
```

## Data Flow

### Agent Action Flow

```
┌─────────┐     ┌──────────────┐     ┌─────────────────┐
│  User   │────▶│   AI Shell   │────▶│ Agent Runtime   │
│ Request │     │   (agnsh)    │     │   Daemon        │
└─────────┘     └──────────────┘     └────────┬────────┘
                                              │
                                              ▼
┌─────────┐     ┌──────────────┐     ┌─────────────────┐
│  Audit  │◀────│   Kernel     │◀────│  Agent Process  │
│   Log   │     │   Module     │     │   (Sandboxed)   │
└─────────┘     └──────────────┘     └────────┬────────┘
                                              │
                                              ▼
                                       ┌─────────────────┐
                                       │  LLM Gateway    │
                                       │  (if needed)    │
                                       └─────────────────┘
```

### Audit Flow

```
Security Event
      │
      ▼
┌─────────────────┐
│ Kernel Hook     │
│ (LSM/Probe)     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Audit Module    │
│ - Capture event │
│ - Add metadata  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Hash Chain      │
│ - SHA256 hash   │
│ - Link previous │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Sign Entry      │
│ - HMAC-SHA256   │
│ - Kernel key    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Store           │
│ - Append-only   │
│ - Immutable     │
└─────────────────┘
```

## Technology Stack

### Kernel

| Component | Technology | Purpose |
|-----------|------------|---------|
| Base | Linux 6.6 LTS | Operating system kernel |
| Security | Landlock, seccomp-bpf | Sandboxing |
| Modules | C, Rust | Kernel extensions |
| Build | Kbuild, LLVM | Kernel compilation |

### User Space

| Component | Technology | Purpose |
|-----------|------------|---------|
| Agent Runtime | Rust | Core agent management |
| LLM Gateway | Rust, Python | Model inference |
| AI Shell | Rust | Natural language CLI |
| Desktop | Wayland, Rust | GUI environment |
| Message Bus | D-Bus, gRPC | Inter-process communication |
| Database | SQLite | Local data storage |
| Crypto | Ring, libsodium | Cryptographic operations |

### Build System

| Component | Technology | Purpose |
|-----------|------------|---------|
| Build Tool | Make, Cargo | Build orchestration |
| Packages | Custom (agpkg) | Package management |
| CI/CD | GitHub Actions | Continuous integration |
| Testing | pytest, cargo test | Test execution |

## Design Decisions

### 1. Linux Kernel Base

**Decision**: Use Linux kernel rather than building from scratch

**Rationale**:
- Mature, well-tested codebase
- Extensive hardware support
- Large developer community
- Proven security track record

**Trade-offs**:
- Must work within kernel constraints
- Patching overhead
- Complexity of kernel development

### 2. Rust for User Space

**Decision**: Use Rust for agent runtime and critical components

**Rationale**:
- Memory safety guarantees
- Performance comparable to C/C++
- Strong type system
- Growing ecosystem

**Trade-offs**:
- Learning curve for contributors
- Compilation time
- Some libraries less mature than C equivalents

### 3. Landlock + seccomp-bpf

**Decision**: Combine Landlock and seccomp-bpf for sandboxing

**Rationale**:
- Landlock: Unprivileged filesystem sandboxing
- seccomp-bpf: System call filtering
- Both upstream in kernel
- Complementary security mechanisms

**Trade-offs**:
- Requires recent kernel (5.13+ for Landlock)
- Complexity of managing both systems
- Performance overhead

### 4. Local-First AI

**Decision**: Prioritize local LLM execution with cloud fallback

**Rationale**:
- Privacy preservation
- Works offline
- Reduced latency
- No vendor lock-in

**Trade-offs**:
- Higher hardware requirements
- Limited to smaller models locally
- More complex resource management

### 5. Cryptographic Audit Chain

**Decision**: Implement immutable, signed audit logs

**Rationale**:
- Tamper detection
- Compliance requirements
- Forensic analysis capability
- Trust in audit data

**Trade-offs**:
- Storage overhead
- Performance impact of signing
- Key management complexity

---

## Related Documentation

- [API Reference](api/)
- [Security Model](security/security-model.md)
- [Agent Development Guide](development/agent-development.md)
- [Kernel Development Guide](development/kernel-development.md)

---

*Last Updated: 2026-02-11*
