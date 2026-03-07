use std::path::PathBuf;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use agnos_common::{
    FilesystemRule, FsAccess, NetworkAccess, NetworkPolicy, SandboxConfig, SeccompAction,
    SeccompRule,
};

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
pub async fn validate_sandbox_profile_handler(Json(config): Json<SandboxConfig>) -> impl IntoResponse {
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
