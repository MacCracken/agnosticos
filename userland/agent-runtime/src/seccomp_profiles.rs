//! Pre-compiled seccomp profiles for common language runtimes
//!
//! Provides curated syscall allowlists for Python, Node.js, Shell, and WASM
//! runtimes, plus a custom profile for advanced users.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tracing::info;

/// Pre-defined seccomp profile for a language runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SeccompProfile {
    /// Python interpreter (~45 syscalls)
    Python,
    /// Node.js / Bun runtime (~40 syscalls)
    Node,
    /// Shell (bash/sh) execution (~25 syscalls)
    #[default]
    Shell,
    /// WebAssembly (Wasmtime/Wasmer) (~20 syscalls)
    Wasm,
    /// Custom allowlist of syscall names
    Custom(Vec<String>),
}

/// Base syscalls required by any running process.
fn base_syscalls() -> Vec<&'static str> {
    vec![
        // Process lifecycle
        "exit",
        "exit_group",
        "getpid",
        "getppid",
        "gettid",
        // Memory management
        "brk",
        "mmap",
        "munmap",
        "mprotect",
        "madvise",
        "mremap",
        // Basic I/O
        "read",
        "write",
        "close",
        "openat",
        "fstat",
        "newfstatat",
        "lseek",
        // Signals
        "rt_sigaction",
        "rt_sigprocmask",
        "rt_sigreturn",
        // Time
        "clock_gettime",
        "clock_nanosleep",
        "nanosleep",
        // File descriptors
        "dup",
        "dup2",
        "dup3",
        "fcntl",
        "ioctl",
        // Polling
        "poll",
        "ppoll",
        "epoll_create1",
        "epoll_ctl",
        "epoll_wait",
        // Misc
        "futex",
        "set_robust_list",
        "get_robust_list",
        "getrandom",
        "arch_prctl",
        "set_tid_address",
        "rseq",
    ]
}

/// Syscalls for Python interpreter.
fn python_syscalls() -> Vec<&'static str> {
    let mut syscalls = base_syscalls();
    syscalls.extend_from_slice(&[
        // File operations (importlib, os module)
        "access",
        "readlink",
        "readlinkat",
        "getcwd",
        "getdents64",
        "statx",
        "faccessat2",
        // Memory (ctypes, numpy)
        "pread64",
        "pwrite64",
        // Process info
        "getuid",
        "getgid",
        "geteuid",
        "getegid",
        "getgroups",
        "uname",
        "sysinfo",
        // Pipe/socket for subprocess
        "pipe2",
        "socket",
        "connect",
        "sendto",
        "recvfrom",
        "setsockopt",
        "getsockopt",
        "bind",
        "listen",
        "accept4",
        "shutdown",
        // Threading
        "clone3",
        "clone",
        "wait4",
        // Shared libs
        "prlimit64",
        "sched_getaffinity",
        "sched_yield",
        // Signal
        "tgkill",
        "sigaltstack",
    ]);
    syscalls
}

/// Syscalls for Node.js / Bun runtime.
fn node_syscalls() -> Vec<&'static str> {
    let mut syscalls = base_syscalls();
    syscalls.extend_from_slice(&[
        // File operations
        "access",
        "readlink",
        "readlinkat",
        "getcwd",
        "getdents64",
        "statx",
        "faccessat2",
        "rename",
        "renameat2",
        "unlinkat",
        "mkdirat",
        // Network (http, net modules)
        "socket",
        "connect",
        "sendto",
        "recvfrom",
        "setsockopt",
        "getsockopt",
        "getsockname",
        "getpeername",
        "bind",
        "listen",
        "accept4",
        "shutdown",
        // Process
        "getuid",
        "getgid",
        "geteuid",
        "getegid",
        "uname",
        "pipe2",
        "pread64",
        "pwrite64",
        "eventfd2",
        // Threading (worker_threads, libuv)
        "clone3",
        "clone",
        "wait4",
        "sched_getaffinity",
        "sched_yield",
        // V8 JIT
        "prlimit64",
        "sigaltstack",
        "tgkill",
    ]);
    syscalls
}

/// Syscalls for shell (bash/sh) execution — minimal.
fn shell_syscalls() -> Vec<&'static str> {
    let mut syscalls = base_syscalls();
    syscalls.extend_from_slice(&[
        // File operations
        "access",
        "getcwd",
        "getdents64",
        "readlink",
        "faccessat2",
        // Process spawn
        "execve",
        "clone",
        "wait4",
        "pipe2",
        // Identity
        "getuid",
        "getgid",
        "geteuid",
        "getegid",
        "uname",
        // Signal handling
        "sigaltstack",
        "tgkill",
    ]);
    syscalls
}

/// Syscalls for WASM runtime (Wasmtime) — most restricted.
fn wasm_syscalls() -> Vec<&'static str> {
    let mut syscalls = base_syscalls();
    syscalls.extend_from_slice(&[
        // WASI filesystem
        "pread64",
        "pwrite64",
        "getdents64",
        "faccessat2",
        // Threading (Wasmtime thread pool)
        "clone3",
        "sched_yield",
        // Signals
        "sigaltstack",
    ]);
    syscalls
}

/// Get the allowed syscall set for a given profile.
pub fn allowed_syscalls(profile: &SeccompProfile) -> HashSet<String> {
    match profile {
        SeccompProfile::Python => python_syscalls().into_iter().map(String::from).collect(),
        SeccompProfile::Node => node_syscalls().into_iter().map(String::from).collect(),
        SeccompProfile::Shell => shell_syscalls().into_iter().map(String::from).collect(),
        SeccompProfile::Wasm => wasm_syscalls().into_iter().map(String::from).collect(),
        SeccompProfile::Custom(list) => list.iter().cloned().collect(),
    }
}

/// Build a BPF-compatible filter description from a seccomp profile.
///
/// Returns the list of allowed syscall names.  The actual BPF compilation
/// is delegated to `agnos-sys::security::create_basic_seccomp_filter()`
/// since BPF bytecode generation requires kernel-specific constants.
///
/// In a production build this would emit raw BPF instructions; for now we
/// produce a structured representation that `sandbox.rs` can use.
#[derive(Debug, Clone)]
pub struct BpfFilterSpec {
    /// Profile name for logging.
    pub profile_name: String,
    /// Allowed syscall names.
    pub allowed: HashSet<String>,
    /// Default action for non-allowed syscalls: "kill" or "trap".
    pub default_action: String,
}

/// Build a BPF filter specification from a profile.
pub fn build_seccomp_filter(profile: &SeccompProfile) -> BpfFilterSpec {
    let allowed = allowed_syscalls(profile);
    let profile_name = match profile {
        SeccompProfile::Python => "python".to_string(),
        SeccompProfile::Node => "node".to_string(),
        SeccompProfile::Shell => "shell".to_string(),
        SeccompProfile::Wasm => "wasm".to_string(),
        SeccompProfile::Custom(_) => "custom".to_string(),
    };

    info!(
        "Built seccomp filter for '{}' profile: {} allowed syscalls",
        profile_name,
        allowed.len()
    );

    BpfFilterSpec {
        profile_name,
        allowed,
        default_action: "kill".to_string(),
    }
}

/// Validate that a profile's syscall list is sane (non-empty, contains
/// essential syscalls).
pub fn validate_profile(profile: &SeccompProfile) -> Result<(), String> {
    let allowed = allowed_syscalls(profile);

    if allowed.is_empty() {
        return Err("Profile has no allowed syscalls".to_string());
    }

    // Every profile must allow these essential syscalls
    let essential = ["exit", "exit_group", "read", "write", "mmap", "brk"];
    for &syscall in &essential {
        if !allowed.contains(syscall) {
            return Err(format!("Profile missing essential syscall '{}'", syscall));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_syscalls_not_empty() {
        let base = base_syscalls();
        assert!(base.len() > 10);
        assert!(base.contains(&"read"));
        assert!(base.contains(&"write"));
        assert!(base.contains(&"exit"));
    }

    #[test]
    fn test_python_profile() {
        let allowed = allowed_syscalls(&SeccompProfile::Python);
        assert!(allowed.len() >= 45);
        // Python-specific
        assert!(allowed.contains("clone3"));
        assert!(allowed.contains("socket"));
        assert!(allowed.contains("getuid"));
        // Base
        assert!(allowed.contains("read"));
        assert!(allowed.contains("mmap"));
    }

    #[test]
    fn test_node_profile() {
        let allowed = allowed_syscalls(&SeccompProfile::Node);
        assert!(allowed.len() >= 40);
        // Node-specific
        assert!(allowed.contains("socket"));
        assert!(allowed.contains("eventfd2"));
        assert!(allowed.contains("accept4"));
    }

    #[test]
    fn test_shell_profile() {
        let allowed = allowed_syscalls(&SeccompProfile::Shell);
        assert!(allowed.len() >= 25);
        // Shell-specific
        assert!(allowed.contains("execve"));
        assert!(allowed.contains("wait4"));
        assert!(allowed.contains("pipe2"));
    }

    #[test]
    fn test_wasm_profile() {
        let allowed = allowed_syscalls(&SeccompProfile::Wasm);
        // WASM should be the most restricted
        assert!(allowed.len() >= 20);
        // Should NOT have execve or socket
        assert!(!allowed.contains("execve"));
        assert!(!allowed.contains("socket"));
    }

    #[test]
    fn test_custom_profile() {
        let custom = SeccompProfile::Custom(vec![
            "read".to_string(),
            "write".to_string(),
            "exit".to_string(),
        ]);
        let allowed = allowed_syscalls(&custom);
        assert_eq!(allowed.len(), 3);
    }

    #[test]
    fn test_build_seccomp_filter() {
        let filter = build_seccomp_filter(&SeccompProfile::Python);
        assert_eq!(filter.profile_name, "python");
        assert_eq!(filter.default_action, "kill");
        assert!(!filter.allowed.is_empty());
    }

    #[test]
    fn test_validate_profile_builtin() {
        assert!(validate_profile(&SeccompProfile::Python).is_ok());
        assert!(validate_profile(&SeccompProfile::Node).is_ok());
        assert!(validate_profile(&SeccompProfile::Shell).is_ok());
        assert!(validate_profile(&SeccompProfile::Wasm).is_ok());
    }

    #[test]
    fn test_validate_profile_custom_valid() {
        let custom =
            SeccompProfile::Custom(base_syscalls().into_iter().map(String::from).collect());
        assert!(validate_profile(&custom).is_ok());
    }

    #[test]
    fn test_validate_profile_custom_empty() {
        let custom = SeccompProfile::Custom(vec![]);
        assert!(validate_profile(&custom).is_err());
    }

    #[test]
    fn test_validate_profile_custom_missing_essential() {
        let custom = SeccompProfile::Custom(vec!["getpid".to_string()]);
        let result = validate_profile(&custom);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("essential syscall"));
    }

    #[test]
    fn test_profile_default() {
        assert_eq!(SeccompProfile::default(), SeccompProfile::Shell);
    }

    #[test]
    fn test_wasm_more_restrictive_than_python() {
        let wasm = allowed_syscalls(&SeccompProfile::Wasm);
        let python = allowed_syscalls(&SeccompProfile::Python);
        assert!(wasm.len() < python.len());
    }

    #[test]
    fn test_profile_serialization() {
        let profile = SeccompProfile::Python;
        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: SeccompProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SeccompProfile::Python);

        let custom = SeccompProfile::Custom(vec!["read".to_string()]);
        let json = serde_json::to_string(&custom).unwrap();
        let deserialized: SeccompProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, custom);
    }

    #[test]
    fn test_no_duplicate_syscalls() {
        // Ensure no profile has duplicates (the HashSet ensures uniqueness,
        // but we want to verify the source lists are clean)
        for profile in [
            SeccompProfile::Python,
            SeccompProfile::Node,
            SeccompProfile::Shell,
            SeccompProfile::Wasm,
        ] {
            let allowed = allowed_syscalls(&profile);
            // If there were duplicates in the Vec, the HashSet would be smaller
            let vec_len = match &profile {
                SeccompProfile::Python => python_syscalls().len(),
                SeccompProfile::Node => node_syscalls().len(),
                SeccompProfile::Shell => shell_syscalls().len(),
                SeccompProfile::Wasm => wasm_syscalls().len(),
                _ => 0,
            };
            assert_eq!(
                allowed.len(),
                vec_len,
                "Profile {:?} has duplicate syscalls ({} unique vs {} total)",
                profile,
                allowed.len(),
                vec_len
            );
        }
    }

    #[test]
    fn test_build_seccomp_filter_node() {
        let filter = build_seccomp_filter(&SeccompProfile::Node);
        assert_eq!(filter.profile_name, "node");
        assert_eq!(filter.default_action, "kill");
    }

    #[test]
    fn test_build_seccomp_filter_shell() {
        let filter = build_seccomp_filter(&SeccompProfile::Shell);
        assert_eq!(filter.profile_name, "shell");
    }

    #[test]
    fn test_build_seccomp_filter_wasm() {
        let filter = build_seccomp_filter(&SeccompProfile::Wasm);
        assert_eq!(filter.profile_name, "wasm");
    }

    #[test]
    fn test_build_seccomp_filter_custom() {
        let custom = SeccompProfile::Custom(vec!["read".to_string(), "write".to_string()]);
        let filter = build_seccomp_filter(&custom);
        assert_eq!(filter.profile_name, "custom");
    }

    // ==================================================================
    // New coverage: validate all builtin profiles contain base syscalls,
    // custom profile with all essentials, BpfFilterSpec fields,
    // profile equality, serialization roundtrip for all variants
    // ==================================================================

    #[test]
    fn test_all_profiles_contain_base_syscalls() {
        let base = base_syscalls();
        for profile in [
            SeccompProfile::Python,
            SeccompProfile::Node,
            SeccompProfile::Shell,
            SeccompProfile::Wasm,
        ] {
            let allowed = allowed_syscalls(&profile);
            for syscall in &base {
                assert!(
                    allowed.contains(*syscall),
                    "Profile {:?} missing base syscall '{}'",
                    profile,
                    syscall
                );
            }
        }
    }

    #[test]
    fn test_validate_custom_with_all_essentials() {
        let essentials: Vec<String> = vec!["exit", "exit_group", "read", "write", "mmap", "brk"]
            .into_iter()
            .map(String::from)
            .collect();
        let custom = SeccompProfile::Custom(essentials);
        assert!(validate_profile(&custom).is_ok());
    }

    #[test]
    fn test_validate_custom_missing_exit() {
        let custom = SeccompProfile::Custom(vec![
            "exit_group".to_string(),
            "read".to_string(),
            "write".to_string(),
            "mmap".to_string(),
            "brk".to_string(),
        ]);
        let result = validate_profile(&custom);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exit"));
    }

    #[test]
    fn test_validate_custom_missing_mmap() {
        let custom = SeccompProfile::Custom(vec![
            "exit".to_string(),
            "exit_group".to_string(),
            "read".to_string(),
            "write".to_string(),
            "brk".to_string(),
        ]);
        let result = validate_profile(&custom);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mmap"));
    }

    #[test]
    fn test_bpf_filter_spec_fields() {
        let filter = build_seccomp_filter(&SeccompProfile::Python);
        assert_eq!(filter.profile_name, "python");
        assert_eq!(filter.default_action, "kill");
        assert!(filter.allowed.contains("read"));
        assert!(filter.allowed.contains("write"));
        assert!(filter.allowed.contains("socket")); // Python-specific
    }

    #[test]
    fn test_bpf_filter_spec_debug() {
        let filter = build_seccomp_filter(&SeccompProfile::Shell);
        let dbg = format!("{:?}", filter);
        assert!(dbg.contains("shell"));
        assert!(dbg.contains("kill"));
    }

    #[test]
    fn test_bpf_filter_spec_clone() {
        let filter = build_seccomp_filter(&SeccompProfile::Node);
        let cloned = filter.clone();
        assert_eq!(cloned.profile_name, filter.profile_name);
        assert_eq!(cloned.allowed.len(), filter.allowed.len());
        assert_eq!(cloned.default_action, filter.default_action);
    }

    #[test]
    fn test_profile_equality() {
        assert_eq!(SeccompProfile::Python, SeccompProfile::Python);
        assert_ne!(SeccompProfile::Python, SeccompProfile::Node);
        assert_ne!(SeccompProfile::Shell, SeccompProfile::Wasm);
    }

    #[test]
    fn test_profile_serialization_all_variants() {
        for profile in [
            SeccompProfile::Python,
            SeccompProfile::Node,
            SeccompProfile::Shell,
            SeccompProfile::Wasm,
        ] {
            let json = serde_json::to_string(&profile).unwrap();
            let deser: SeccompProfile = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, profile);
        }
    }

    #[test]
    fn test_custom_profile_serialization_with_many_syscalls() {
        let custom =
            SeccompProfile::Custom(base_syscalls().into_iter().map(String::from).collect());
        let json = serde_json::to_string(&custom).unwrap();
        let deser: SeccompProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, custom);
    }

    #[test]
    fn test_shell_profile_has_execve() {
        let allowed = allowed_syscalls(&SeccompProfile::Shell);
        assert!(
            allowed.contains("execve"),
            "Shell profile must allow execve"
        );
    }

    #[test]
    fn test_wasm_profile_no_execve_no_socket() {
        let allowed = allowed_syscalls(&SeccompProfile::Wasm);
        assert!(!allowed.contains("execve"), "WASM should not allow execve");
        assert!(!allowed.contains("socket"), "WASM should not allow socket");
    }

    #[test]
    fn test_python_profile_has_threading() {
        let allowed = allowed_syscalls(&SeccompProfile::Python);
        assert!(allowed.contains("clone3"));
        assert!(allowed.contains("clone"));
    }

    #[test]
    fn test_node_profile_has_eventfd2() {
        let allowed = allowed_syscalls(&SeccompProfile::Node);
        assert!(
            allowed.contains("eventfd2"),
            "Node profile needs eventfd2 for libuv"
        );
    }
}
