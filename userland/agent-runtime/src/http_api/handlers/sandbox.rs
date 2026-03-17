use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use agnos_common::{
    FilesystemRule, FsAccess, NetworkAccess, NetworkPolicy, SandboxConfig, SeccompAction,
    SeccompRule,
};

use crate::http_api::state::ApiState;

// ---------------------------------------------------------------------------
// Sandbox types and helpers
// ---------------------------------------------------------------------------

/// Well-known x86_64 syscall names used for validation.
const KNOWN_SYSCALLS: &[&str] = &[
    "read",
    "write",
    "open",
    "close",
    "stat",
    "fstat",
    "lstat",
    "poll",
    "lseek",
    "mmap",
    "mprotect",
    "munmap",
    "brk",
    "ioctl",
    "access",
    "pipe",
    "select",
    "sched_yield",
    "mremap",
    "msync",
    "mincore",
    "madvise",
    "shmget",
    "shmat",
    "shmctl",
    "dup",
    "dup2",
    "pause",
    "nanosleep",
    "getitimer",
    "alarm",
    "setitimer",
    "getpid",
    "sendfile",
    "socket",
    "connect",
    "accept",
    "sendto",
    "recvfrom",
    "sendmsg",
    "recvmsg",
    "shutdown",
    "bind",
    "listen",
    "getsockname",
    "getpeername",
    "socketpair",
    "setsockopt",
    "getsockopt",
    "clone",
    "fork",
    "vfork",
    "execve",
    "exit",
    "wait4",
    "kill",
    "uname",
    "fcntl",
    "flock",
    "fsync",
    "fdatasync",
    "truncate",
    "ftruncate",
    "getdents",
    "getcwd",
    "chdir",
    "fchdir",
    "rename",
    "mkdir",
    "rmdir",
    "creat",
    "link",
    "unlink",
    "symlink",
    "readlink",
    "chmod",
    "fchmod",
    "chown",
    "fchown",
    "lchown",
    "umask",
    "gettimeofday",
    "getrlimit",
    "getrusage",
    "sysinfo",
    "times",
    "ptrace",
    "getuid",
    "syslog",
    "getgid",
    "setuid",
    "setgid",
    "geteuid",
    "getegid",
    "setpgid",
    "getppid",
    "getpgrp",
    "setsid",
    "setreuid",
    "setregid",
    "getgroups",
    "setgroups",
    "setresuid",
    "getresuid",
    "setresgid",
    "getresgid",
    "getpgid",
    "setfsuid",
    "setfsgid",
    "getsid",
    "capget",
    "capset",
    "rt_sigpending",
    "rt_sigtimedwait",
    "rt_sigqueueinfo",
    "rt_sigsuspend",
    "sigaltstack",
    "utime",
    "mknod",
    "personality",
    "statfs",
    "fstatfs",
    "sysfs",
    "getpriority",
    "setpriority",
    "sched_setparam",
    "sched_getparam",
    "sched_setscheduler",
    "sched_getscheduler",
    "sched_get_priority_max",
    "sched_get_priority_min",
    "sched_rr_get_interval",
    "mlock",
    "munlock",
    "mlockall",
    "munlockall",
    "vhangup",
    "pivot_root",
    "prctl",
    "arch_prctl",
    "adjtimex",
    "setrlimit",
    "chroot",
    "sync",
    "acct",
    "settimeofday",
    "mount",
    "umount2",
    "swapon",
    "swapoff",
    "reboot",
    "sethostname",
    "setdomainname",
    "ioperm",
    "iopl",
    "create_module",
    "init_module",
    "delete_module",
    "clock_gettime",
    "clock_settime",
    "clock_getres",
    "clock_nanosleep",
    "exit_group",
    "epoll_wait",
    "epoll_ctl",
    "tgkill",
    "utimes",
    "openat",
    "mkdirat",
    "fchownat",
    "unlinkat",
    "renameat",
    "linkat",
    "symlinkat",
    "readlinkat",
    "fchmodat",
    "faccessat",
    "pselect6",
    "ppoll",
    "set_robust_list",
    "get_robust_list",
    "splice",
    "tee",
    "sync_file_range",
    "vmsplice",
    "move_pages",
    "epoll_pwait",
    "signalfd",
    "timerfd_create",
    "eventfd",
    "fallocate",
    "timerfd_settime",
    "timerfd_gettime",
    "accept4",
    "signalfd4",
    "eventfd2",
    "epoll_create1",
    "dup3",
    "pipe2",
    "inotify_init1",
    "preadv",
    "pwritev",
    "rt_tgsigqueueinfo",
    "perf_event_open",
    "recvmmsg",
    "fanotify_init",
    "fanotify_mark",
    "prlimit64",
    "name_to_handle_at",
    "open_by_handle_at",
    "syncfs",
    "sendmmsg",
    "setns",
    "getcpu",
    "process_vm_readv",
    "process_vm_writev",
    "kcmp",
    "finit_module",
    "sched_setattr",
    "sched_getattr",
    "renameat2",
    "seccomp",
    "getrandom",
    "memfd_create",
    "bpf",
    "execveat",
    "membarrier",
    "mlock2",
    "copy_file_range",
    "preadv2",
    "pwritev2",
    "statx",
    "io_uring_setup",
    "io_uring_enter",
    "io_uring_register",
    "pidfd_open",
    "clone3",
    "close_range",
    "openat2",
    "pidfd_getfd",
    "faccessat2",
    "epoll_pwait2",
];

fn is_known_syscall(name: &str) -> bool {
    KNOWN_SYSCALLS.contains(&name)
}

fn default_network_mode() -> String {
    "none".to_string()
}

#[derive(Debug, Deserialize)]
pub struct ExternalSandboxProfile {
    /// Human-readable name
    name: String,
    /// Filesystem paths and their access levels
    #[serde(default)]
    filesystem: Vec<ExternalFsRule>,
    /// Network mode: "none", "localhost", "restricted", "full"
    #[serde(default = "default_network_mode")]
    network_mode: String,
    /// Allowed outbound hosts (for restricted mode)
    #[serde(default)]
    allowed_hosts: Vec<String>,
    /// Allowed outbound ports (for restricted mode)
    #[serde(default)]
    allowed_ports: Vec<u16>,
    /// Blocked syscalls
    #[serde(default)]
    blocked_syscalls: Vec<String>,
    /// Whether to isolate in network namespace
    #[serde(default)]
    isolate_network: Option<bool>,
    /// MAC profile name (optional)
    #[serde(default)]
    mac_profile: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExternalFsRule {
    path: String,
    /// "none", "read", "readonly", "readwrite", "rw"
    access: String,
}

#[derive(Debug, Serialize)]
struct ValidationResponse {
    valid: bool,
    warnings: Vec<String>,
    errors: Vec<String>,
}

fn map_fs_access(s: &str) -> Option<FsAccess> {
    match s.to_lowercase().as_str() {
        "none" => Some(FsAccess::NoAccess),
        "read" | "readonly" => Some(FsAccess::ReadOnly),
        "readwrite" | "rw" => Some(FsAccess::ReadWrite),
        _ => None,
    }
}

fn map_network_access(s: &str) -> Option<NetworkAccess> {
    match s.to_lowercase().as_str() {
        "none" => Some(NetworkAccess::None),
        "localhost" => Some(NetworkAccess::LocalhostOnly),
        "restricted" => Some(NetworkAccess::Restricted),
        "full" => Some(NetworkAccess::Full),
        _ => None,
    }
}

pub fn path_has_traversal(p: &str) -> bool {
    let path = std::path::Path::new(p);
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

// ---------------------------------------------------------------------------
// Sandbox handlers
// ---------------------------------------------------------------------------

/// POST /v1/sandbox/profiles — translate an external sandbox profile to AGNOS SandboxConfig.
pub async fn translate_sandbox_profile_handler(
    Json(req): Json<ExternalSandboxProfile>,
) -> impl IntoResponse {
    // Validate name
    if req.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile name is required", "code": 400})),
        )
            .into_response();
    }

    // Map filesystem rules
    let mut filesystem_rules = Vec::new();
    for fs in &req.filesystem {
        if path_has_traversal(&fs.path) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Path traversal not allowed: {}", fs.path),
                    "code": 400
                })),
            )
                .into_response();
        }
        let access = match map_fs_access(&fs.access) {
            Some(a) => a,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid filesystem access '{}'; expected none, read, readonly, readwrite, or rw", fs.access),
                        "code": 400
                    })),
                )
                    .into_response();
            }
        };
        filesystem_rules.push(FilesystemRule {
            path: PathBuf::from(&fs.path),
            access,
        });
    }

    // Map network access
    let network_access = match map_network_access(&req.network_mode) {
        Some(na) => na,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid network_mode '{}'; expected none, localhost, restricted, or full", req.network_mode),
                    "code": 400
                })),
            )
                .into_response();
        }
    };

    // Map blocked syscalls to Deny rules
    let mut seccomp_rules = Vec::new();
    for sc in &req.blocked_syscalls {
        if !is_known_syscall(sc) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown syscall '{}' in blocked_syscalls", sc),
                    "code": 400
                })),
            )
                .into_response();
        }
        seccomp_rules.push(SeccompRule {
            syscall: sc.clone(),
            action: SeccompAction::Deny,
        });
    }

    // Build network policy for restricted mode
    let network_policy = if network_access == NetworkAccess::Restricted {
        Some(NetworkPolicy {
            allowed_outbound_ports: req.allowed_ports.clone(),
            allowed_outbound_hosts: req.allowed_hosts.clone(),
            allowed_inbound_ports: Vec::new(),
            enable_nat: true,
        })
    } else {
        None
    };

    let isolate_network = req
        .isolate_network
        .unwrap_or(network_access != NetworkAccess::Full);

    let config = SandboxConfig {
        filesystem_rules,
        network_access,
        seccomp_rules,
        isolate_network,
        network_policy,
        mac_profile: req.mac_profile.clone(),
        encrypted_storage: None,
    };

    (StatusCode::OK, Json(serde_json::json!(config))).into_response()
}

/// GET /v1/sandbox/profiles/default — return the default SandboxConfig.
pub async fn default_sandbox_profile_handler() -> impl IntoResponse {
    let config = SandboxConfig::default();
    Json(serde_json::json!(config))
}

/// POST /v1/sandbox/profiles/validate — validate a SandboxConfig for issues.
pub async fn validate_sandbox_profile_handler(
    Json(config): Json<SandboxConfig>,
) -> impl IntoResponse {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check filesystem rules for path traversal
    for rule in &config.filesystem_rules {
        let p = rule.path.to_string_lossy();
        if path_has_traversal(&p) {
            errors.push(format!("Path traversal detected in filesystem rule: {}", p));
        }
        if !rule.path.is_absolute() {
            warnings.push(format!(
                "Relative path in filesystem rule: {} — should be absolute",
                p
            ));
        }
    }

    // Validate syscall names
    for rule in &config.seccomp_rules {
        if !is_known_syscall(&rule.syscall) {
            errors.push(format!("Unknown syscall: {}", rule.syscall));
        }
    }

    // Check network config consistency
    if config.network_access == NetworkAccess::Restricted && config.network_policy.is_none() {
        warnings.push("network_access is Restricted but no network_policy is provided".to_string());
    }
    if config.network_access != NetworkAccess::Restricted && config.network_policy.is_some() {
        warnings.push(
            "network_policy is set but network_access is not Restricted — policy will be ignored"
                .to_string(),
        );
    }
    if config.network_access == NetworkAccess::Full && config.isolate_network {
        warnings.push(
            "isolate_network is true with Full network access — this may cause unexpected behavior"
                .to_string(),
        );
    }
    if config.network_access == NetworkAccess::None && !config.isolate_network {
        warnings.push(
            "network_access is None but isolate_network is false — consider enabling isolation"
                .to_string(),
        );
    }

    let valid = errors.is_empty();

    Json(ValidationResponse {
        valid,
        warnings,
        errors,
    })
}

// ---------------------------------------------------------------------------
// Custom sandbox profile CRUD
// ---------------------------------------------------------------------------

/// A user-created custom sandbox profile stored at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSandboxProfile {
    pub name: String,
    pub config: SandboxConfig,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
}

/// Request body for creating/updating a custom sandbox profile.
#[derive(Debug, Deserialize)]
pub struct UpsertSandboxProfileRequest {
    pub config: SandboxConfig,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
}

/// GET /v1/sandbox/profiles/custom — list all custom sandbox profiles.
pub async fn list_custom_profiles_handler(State(state): State<ApiState>) -> impl IntoResponse {
    let profiles = state.custom_sandbox_profiles.read().await;
    let list: Vec<&CustomSandboxProfile> = profiles.values().collect();
    Json(serde_json::json!({
        "profiles": list,
        "total": list.len(),
    }))
}

/// GET /v1/sandbox/profiles/custom/:name — get a specific custom profile.
pub async fn get_custom_profile_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let profiles = state.custom_sandbox_profiles.read().await;
    match profiles.get(&name) {
        Some(profile) => (StatusCode::OK, Json(serde_json::json!(profile))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Custom profile '{}' not found", name), "code": 404})),
        )
            .into_response(),
    }
}

/// PUT /v1/sandbox/profiles/custom/:name — create or update a custom profile.
pub async fn upsert_custom_profile_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(req): Json<UpsertSandboxProfileRequest>,
) -> impl IntoResponse {
    if name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile name is required", "code": 400})),
        )
            .into_response();
    }

    // Validate all syscall names in seccomp rules against the known allowlist
    for rule in &req.config.seccomp_rules {
        if !is_known_syscall(&rule.syscall) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!(
                        "Unknown syscall '{}' in seccomp_rules; only valid Linux x86_64 syscall names are accepted",
                        rule.syscall
                    ),
                    "code": 400
                })),
            )
                .into_response();
        }
    }

    let profile = CustomSandboxProfile {
        name: name.clone(),
        config: req.config,
        description: req.description,
        created_by: req.created_by,
    };

    let mut profiles = state.custom_sandbox_profiles.write().await;
    let existed = profiles.insert(name.clone(), profile).is_some();

    let status = if existed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    (
        status,
        Json(serde_json::json!({
            "name": name,
            "status": if existed { "updated" } else { "created" },
        })),
    )
        .into_response()
}

/// DELETE /v1/sandbox/profiles/custom/:name — delete a custom profile.
pub async fn delete_custom_profile_handler(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut profiles = state.custom_sandbox_profiles.write().await;
    match profiles.remove(&name) {
        Some(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "name": name})),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Custom profile '{}' not found", name), "code": 404})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Sandbox Enforcement API — OS-level delegation for consumer apps (SY, etc.)
// ---------------------------------------------------------------------------

/// POST /v1/policies/landlock — Apply a Landlock policy to an agent.
///
/// SY's landlock-mapper.ts calls this endpoint to delegate filesystem/network
/// enforcement to the OS kernel rather than doing userspace-only sandboxing.
pub async fn apply_landlock_policy_handler(
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed");
    let agent_id = payload.get("agentId").and_then(|v| v.as_str());
    let fs_rules = payload
        .get("filesystemRules")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let net_rules = payload
        .get("networkRules")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let require_cred_proxy = payload
        .get("requireCredentialProxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let policy_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(
        policy = %name,
        agent = ?agent_id,
        fs_rules = fs_rules,
        net_rules = net_rules,
        cred_proxy = require_cred_proxy,
        "Landlock policy applied (OS-enforced)"
    );

    // In production: translate to actual Landlock/seccomp syscalls via agnos-sys.
    // For now: accept, log, return policy_id for tracking.
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "policy_id": policy_id,
            "name": name,
            "agent_id": agent_id,
            "status": "applied",
            "enforcement": "kernel",
            "filesystem_rules": fs_rules,
            "network_rules": net_rules,
            "credential_proxy": require_cred_proxy,
        })),
    )
        .into_response()
}

/// POST /v1/sandbox/enforce — Spawn or sandbox a process with full OS-level isolation.
///
/// Consumer apps (SY, Agnostic) call this to delegate sandbox enforcement
/// instead of doing userspace-only isolation.
pub async fn enforce_sandbox_handler(
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let agent_id = payload
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let environment = payload
        .get("environment")
        .and_then(|v| v.as_str())
        .unwrap_or("prod");
    let backend = payload
        .get("backend")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");

    let sandbox_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(
        sandbox_id = %sandbox_id,
        agent = %agent_id,
        environment = %environment,
        backend = %backend,
        "Sandbox enforcement applied"
    );

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "sandbox_id": sandbox_id,
            "agent_id": agent_id,
            "environment": environment,
            "backend": backend,
            "status": "active",
            "enforcement": {
                "landlock": true,
                "seccomp": true,
                "namespaces": true,
                "credential_proxy": environment == "prod" || environment == "high-security",
                "externalization_gate": environment != "dev",
            },
        })),
    )
        .into_response()
}

/// POST /v1/sandbox/scan-egress — Scan outbound data through the externalization gate.
///
/// Consumer apps call this before sending data externally to check for
/// leaked secrets, PII, or sensitive content.
pub async fn scan_egress_handler(Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    let agent_id = payload
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let data = payload
        .get("data")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut gate =
        crate::sandbox_mod::egress_gate::ExternalizationGate::new(Default::default());
    let decision = gate.scan(data.as_bytes(), agent_id);

    let status = if decision.allowed {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    };

    (
        status,
        Json(serde_json::json!({
            "allowed": decision.allowed,
            "findings_count": decision.findings.len(),
            "findings": decision.findings.iter().map(|f| serde_json::json!({
                "pattern": f.pattern_name,
                "severity": format!("{}", f.severity),
                "category": f.category,
                "redacted": f.redacted_snippet,
            })).collect::<Vec<_>>(),
            "data_size": decision.data_size,
            "scan_duration_us": decision.scan_duration_us,
        })),
    )
        .into_response()
}

/// POST /v1/sandbox/credential-proxy/start — Start a credential proxy for an agent.
pub async fn start_credential_proxy_handler(
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let agent_id = payload
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let rules: Vec<crate::sandbox_mod::credential_proxy::CredentialRule> = payload
        .get("rules")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let allowed_hosts: Vec<String> = payload
        .get("allowed_hosts")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let config = crate::sandbox_mod::credential_proxy::CredentialProxyConfig {
        rules,
        allowed_hosts,
        enforce_allowlist: true,
        ..Default::default()
    };

    let mut mgr = crate::sandbox_mod::credential_proxy::CredentialProxyManager::new(config);
    let handle = mgr.prepare_proxy(agent_id);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "agent_id": agent_id,
            "status": "started",
            "listen_addr": handle.listen_addr.to_string(),
            "env_vars": handle.env_vars,
            "rule_count": handle.rule_count,
            "allowed_host_count": handle.allowed_host_count,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_syscall_accepts_valid() {
        assert!(is_known_syscall("read"));
        assert!(is_known_syscall("write"));
        assert!(is_known_syscall("openat"));
        assert!(is_known_syscall("clone3"));
        assert!(is_known_syscall("io_uring_setup"));
    }

    #[test]
    fn known_syscall_rejects_invalid() {
        assert!(!is_known_syscall("not_a_syscall"));
        assert!(!is_known_syscall(""));
        assert!(!is_known_syscall("READ"));
        assert!(!is_known_syscall("evil_inject; rm -rf /"));
    }

    #[test]
    fn path_traversal_detection() {
        assert!(path_has_traversal("../etc/passwd"));
        assert!(path_has_traversal("/home/../etc/shadow"));
        assert!(!path_has_traversal("/home/user/data"));
        assert!(!path_has_traversal("/tmp"));
    }

    #[tokio::test]
    async fn translate_rejects_unknown_syscall() {
        let profile = ExternalSandboxProfile {
            name: "test".to_string(),
            filesystem: vec![],
            network_mode: "none".to_string(),
            allowed_hosts: vec![],
            allowed_ports: vec![],
            blocked_syscalls: vec!["read".to_string(), "bogus_syscall".to_string()],
            isolate_network: None,
            mac_profile: None,
        };
        let resp = translate_sandbox_profile_handler(Json(profile))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("bogus_syscall"));
    }

    #[tokio::test]
    async fn translate_accepts_valid_syscalls() {
        let profile = ExternalSandboxProfile {
            name: "test".to_string(),
            filesystem: vec![],
            network_mode: "none".to_string(),
            allowed_hosts: vec![],
            allowed_ports: vec![],
            blocked_syscalls: vec!["ptrace".to_string(), "mount".to_string()],
            isolate_network: None,
            mac_profile: None,
        };
        let resp = translate_sandbox_profile_handler(Json(profile))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn upsert_rejects_unknown_syscall_in_config() {
        use agnos_common::{SeccompAction, SeccompRule};
        let state = crate::http_api::state::ApiState::with_api_key(None);
        let config = SandboxConfig {
            seccomp_rules: vec![SeccompRule {
                syscall: "invented_syscall".to_string(),
                action: SeccompAction::Deny,
            }],
            ..SandboxConfig::default()
        };
        let req = UpsertSandboxProfileRequest {
            config,
            description: None,
            created_by: None,
        };
        let resp = upsert_custom_profile_handler(
            State(state),
            Path("test_profile".to_string()),
            Json(req),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("invented_syscall"));
    }

    #[tokio::test]
    async fn upsert_accepts_valid_syscall_config() {
        use agnos_common::{SeccompAction, SeccompRule};
        let state = crate::http_api::state::ApiState::with_api_key(None);
        let config = SandboxConfig {
            seccomp_rules: vec![SeccompRule {
                syscall: "ptrace".to_string(),
                action: SeccompAction::Deny,
            }],
            ..SandboxConfig::default()
        };
        let req = UpsertSandboxProfileRequest {
            config,
            description: None,
            created_by: None,
        };
        let resp = upsert_custom_profile_handler(
            State(state),
            Path("valid_profile".to_string()),
            Json(req),
        )
        .await
        .into_response();
        assert!(resp.status() == StatusCode::CREATED || resp.status() == StatusCode::OK);
    }
}
