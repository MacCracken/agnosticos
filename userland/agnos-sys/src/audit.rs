//! Linux Audit Subsystem Interface
//!
//! Provides safe Rust bindings for the Linux audit subsystem (netlink socket)
//! and the AGNOS kernel audit module (`/proc/agnos/audit`).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::path::Path;

// Linux netlink/audit constants
#[cfg(target_os = "linux")]
const NETLINK_AUDIT: libc::c_int = 9;

// Audit message types
#[cfg(target_os = "linux")]
const AUDIT_GET: u16 = 1000;
#[cfg(target_os = "linux")]
const AUDIT_SET: u16 = 1001;
#[cfg(target_os = "linux")]
const AUDIT_ADD_RULE: u16 = 1011;
#[cfg(target_os = "linux")]
const AUDIT_DEL_RULE: u16 = 1012;
#[cfg(target_os = "linux")]
const AUDIT_USER: u16 = 1005;

// Custom AGNOS audit syscall number
#[cfg(target_os = "linux")]
const SYS_AGNOS_AUDIT_LOG: libc::c_long = 520;

// Netlink message header size
#[cfg(target_os = "linux")]
const NLMSG_HDRLEN: usize = 16;

/// Handle wrapping a netlink audit socket file descriptor.
#[derive(Debug)]
pub struct AuditHandle {
    /// The netlink socket fd (-1 if using proc-only mode)
    fd: i32,
    /// Configuration used to open this handle
    config: AuditConfig,
}

/// Configuration for opening an audit connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Use the netlink audit socket (AF_NETLINK, NETLINK_AUDIT)
    pub use_netlink: bool,
    /// Use the AGNOS /proc interface
    pub use_agnos_proc: bool,
    /// Path to the AGNOS proc audit file
    pub proc_path: String,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            use_netlink: true,
            use_agnos_proc: false,
            proc_path: "/proc/agnos/audit".to_string(),
        }
    }
}

/// Current audit subsystem status (from AUDIT_GET).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStatus {
    /// Whether auditing is enabled (1) or disabled (0)
    pub enabled: u32,
    /// Failure action: 0=silent, 1=printk, 2=panic
    pub failure_action: u32,
    /// PID of the audit daemon (0 if none)
    pub pid: u32,
    /// Maximum number of outstanding audit messages
    pub backlog_limit: u32,
    /// Number of audit messages lost
    pub lost: u32,
    /// Current backlog count
    pub backlog: u32,
}

impl Default for AuditStatus {
    fn default() -> Self {
        Self {
            enabled: 0,
            failure_action: 1,
            pid: 0,
            backlog_limit: 8192,
            lost: 0,
            backlog: 0,
        }
    }
}

/// Type of audit rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditRuleType {
    /// Watch file access (equivalent to auditctl -w)
    FileWatch,
    /// Watch syscall invocations (equivalent to auditctl -a)
    SyscallWatch,
}

/// An audit rule to add or delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRule {
    /// Type of rule
    pub rule_type: AuditRuleType,
    /// Path to watch (for FileWatch rules)
    pub path: Option<String>,
    /// Syscall number (for SyscallWatch rules)
    pub syscall: Option<u32>,
    /// Key string for filtering audit logs
    pub key: String,
}

impl AuditRule {
    /// Create a file watch rule.
    pub fn file_watch(path: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            rule_type: AuditRuleType::FileWatch,
            path: Some(path.into()),
            syscall: None,
            key: key.into(),
        }
    }

    /// Create a syscall watch rule.
    pub fn syscall_watch(syscall: u32, key: impl Into<String>) -> Self {
        Self {
            rule_type: AuditRuleType::SyscallWatch,
            path: None,
            syscall: Some(syscall),
            key: key.into(),
        }
    }

    /// Validate that the rule is well-formed.
    pub fn validate(&self) -> Result<()> {
        if self.key.is_empty() {
            return Err(SysError::InvalidArgument("Audit rule key cannot be empty".into()));
        }
        if self.key.len() > 256 {
            return Err(SysError::InvalidArgument("Audit rule key too long (max 256)".into()));
        }
        match self.rule_type {
            AuditRuleType::FileWatch => {
                if self.path.is_none() {
                    return Err(SysError::InvalidArgument(
                        "FileWatch rule requires a path".into(),
                    ));
                }
                let path = self.path.as_ref().unwrap();
                if path.is_empty() {
                    return Err(SysError::InvalidArgument(
                        "FileWatch path cannot be empty".into(),
                    ));
                }
                if !path.starts_with('/') {
                    return Err(SysError::InvalidArgument(
                        "FileWatch path must be absolute".into(),
                    ));
                }
            }
            AuditRuleType::SyscallWatch => {
                if self.syscall.is_none() {
                    return Err(SysError::InvalidArgument(
                        "SyscallWatch rule requires a syscall number".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A raw audit entry from `/proc/agnos/audit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAuditEntry {
    /// Sequence number
    pub sequence: u64,
    /// Timestamp in nanoseconds since epoch
    pub timestamp_ns: u64,
    /// Type of audit action
    pub action_type: String,
    /// Result code (0 = success)
    pub result: i32,
    /// SHA-256 hash of this entry
    pub hash: String,
    /// SHA-256 hash of the previous entry (empty for first)
    pub prev_hash: String,
    /// Raw payload data
    pub payload: String,
}

/// Open an audit connection based on the given configuration.
///
/// If `use_netlink` is true, opens an `AF_NETLINK` socket with `NETLINK_AUDIT` protocol.
/// Requires `CAP_AUDIT_CONTROL` or root.
///
/// # Errors
/// Returns `SysError::PermissionDenied` if the process lacks capabilities.
/// Returns `SysError::NotSupported` on non-Linux.
pub fn open_audit(config: &AuditConfig) -> Result<AuditHandle> {
    #[cfg(target_os = "linux")]
    {
        let fd = if config.use_netlink {
            let fd = unsafe {
                libc::socket(
                    libc::AF_NETLINK,
                    libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                    NETLINK_AUDIT,
                )
            };
            if fd < 0 {
                let err = std::io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(libc::EPERM) | Some(libc::EACCES) => Err(SysError::PermissionDenied),
                    Some(libc::EPROTONOSUPPORT) => Err(SysError::NotSupported),
                    _ => Err(SysError::Unknown(format!("socket(NETLINK_AUDIT) failed: {}", err))),
                };
            }

            // Bind the socket
            let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
            addr.nl_family = libc::AF_NETLINK as u16;
            addr.nl_pid = unsafe { libc::getpid() } as u32;
            addr.nl_groups = 0;

            let ret = unsafe {
                libc::bind(
                    fd,
                    &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
                )
            };

            if ret < 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(fd); }
                return Err(SysError::Unknown(format!("bind(NETLINK_AUDIT) failed: {}", err)));
            }

            tracing::debug!("Opened netlink audit socket (fd={})", fd);
            fd
        } else {
            -1
        };

        // Verify proc path if configured
        if config.use_agnos_proc && !Path::new(&config.proc_path).exists() {
            if fd >= 0 {
                unsafe { libc::close(fd); }
            }
            tracing::warn!("AGNOS proc audit path does not exist: {}", config.proc_path);
        }

        Ok(AuditHandle {
            fd,
            config: config.clone(),
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Err(SysError::NotSupported)
    }
}

/// Send an audit user message via the netlink socket.
///
/// Sends an `AUDIT_USER` type message with the given event type string and message payload.
pub fn send_audit_event(handle: &AuditHandle, event_type: &str, message: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if handle.fd < 0 {
            return Err(SysError::InvalidArgument(
                "Audit handle has no netlink socket".into(),
            ));
        }

        if event_type.is_empty() {
            return Err(SysError::InvalidArgument("Event type cannot be empty".into()));
        }
        if message.len() > 8192 {
            return Err(SysError::InvalidArgument("Audit message too large (max 8192)".into()));
        }

        let payload = format!("op={} {}", event_type, message);
        let payload_bytes = payload.as_bytes();
        let total_len = NLMSG_HDRLEN + payload_bytes.len();

        // Build netlink message header + payload
        let mut buf = vec![0u8; total_len];

        // nlmsghdr: len (u32), type (u16), flags (u16), seq (u32), pid (u32)
        let len_bytes = (total_len as u32).to_ne_bytes();
        buf[0..4].copy_from_slice(&len_bytes);
        let type_bytes = AUDIT_USER.to_ne_bytes();
        buf[4..6].copy_from_slice(&type_bytes);
        // flags = NLM_F_REQUEST (1)
        buf[6..8].copy_from_slice(&1u16.to_ne_bytes());
        // seq = 1
        buf[8..12].copy_from_slice(&1u32.to_ne_bytes());
        // pid
        let pid = unsafe { libc::getpid() } as u32;
        buf[12..16].copy_from_slice(&pid.to_ne_bytes());
        // payload
        buf[NLMSG_HDRLEN..].copy_from_slice(payload_bytes);

        let ret = unsafe {
            libc::send(handle.fd, buf.as_ptr() as *const libc::c_void, total_len, 0)
        };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("send(audit event) failed: {}", err)));
        }

        tracing::debug!("Sent audit event: op={} ({} bytes)", event_type, payload_bytes.len());
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (handle, event_type, message);
        Err(SysError::NotSupported)
    }
}

/// Query the current audit subsystem status via AUDIT_GET.
pub fn get_audit_status(handle: &AuditHandle) -> Result<AuditStatus> {
    #[cfg(target_os = "linux")]
    {
        if handle.fd < 0 {
            return Err(SysError::InvalidArgument(
                "Audit handle has no netlink socket".into(),
            ));
        }

        // Build AUDIT_GET request
        let total_len = NLMSG_HDRLEN;
        let mut buf = vec![0u8; total_len];
        buf[0..4].copy_from_slice(&(total_len as u32).to_ne_bytes());
        buf[4..6].copy_from_slice(&AUDIT_GET.to_ne_bytes());
        buf[6..8].copy_from_slice(&1u16.to_ne_bytes()); // NLM_F_REQUEST
        buf[8..12].copy_from_slice(&1u32.to_ne_bytes()); // seq
        let pid = unsafe { libc::getpid() } as u32;
        buf[12..16].copy_from_slice(&pid.to_ne_bytes());

        let ret = unsafe {
            libc::send(handle.fd, buf.as_ptr() as *const libc::c_void, total_len, 0)
        };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("send(AUDIT_GET) failed: {}", err)));
        }

        // Read response
        let mut recv_buf = vec![0u8; 4096];
        let n = unsafe {
            libc::recv(
                handle.fd,
                recv_buf.as_mut_ptr() as *mut libc::c_void,
                recv_buf.len(),
                0,
            )
        };

        if n < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("recv(AUDIT_GET) failed: {}", err)));
        }

        if (n as usize) < NLMSG_HDRLEN + 24 {
            // Minimum: header + audit_status struct fields we care about
            return Err(SysError::Unknown(
                "AUDIT_GET response too short".into(),
            ));
        }

        // Parse audit_status from the payload after nlmsghdr.
        // struct audit_status layout (simplified, first 6 u32 fields):
        //   u32 mask, u32 enabled, u32 failure, u32 pid, u32 rate_limit, u32 backlog_limit, u32 lost, u32 backlog
        let payload = &recv_buf[NLMSG_HDRLEN..n as usize];
        let read_u32 = |offset: usize| -> u32 {
            if offset + 4 <= payload.len() {
                u32::from_ne_bytes([
                    payload[offset],
                    payload[offset + 1],
                    payload[offset + 2],
                    payload[offset + 3],
                ])
            } else {
                0
            }
        };

        Ok(AuditStatus {
            enabled: read_u32(4),         // offset 4
            failure_action: read_u32(8),  // offset 8
            pid: read_u32(12),            // offset 12
            backlog_limit: read_u32(20),  // offset 20
            lost: read_u32(24),           // offset 24
            backlog: read_u32(28),        // offset 28
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = handle;
        Err(SysError::NotSupported)
    }
}

/// Enable or disable the audit subsystem via AUDIT_SET.
pub fn set_audit_enabled(handle: &AuditHandle, enabled: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if handle.fd < 0 {
            return Err(SysError::InvalidArgument(
                "Audit handle has no netlink socket".into(),
            ));
        }

        // Build AUDIT_SET message with audit_status payload
        // We set mask=1 (AUDIT_STATUS_ENABLED) and enabled=0/1
        let payload_len = 32; // enough for the audit_status fields
        let total_len = NLMSG_HDRLEN + payload_len;
        let mut buf = vec![0u8; total_len];

        // nlmsghdr
        buf[0..4].copy_from_slice(&(total_len as u32).to_ne_bytes());
        buf[4..6].copy_from_slice(&AUDIT_SET.to_ne_bytes());
        buf[6..8].copy_from_slice(&1u16.to_ne_bytes()); // NLM_F_REQUEST
        buf[8..12].copy_from_slice(&1u32.to_ne_bytes());
        let pid = unsafe { libc::getpid() } as u32;
        buf[12..16].copy_from_slice(&pid.to_ne_bytes());

        // audit_status payload
        // mask = 1 (AUDIT_STATUS_ENABLED)
        buf[NLMSG_HDRLEN..NLMSG_HDRLEN + 4].copy_from_slice(&1u32.to_ne_bytes());
        // enabled
        let val: u32 = if enabled { 1 } else { 0 };
        buf[NLMSG_HDRLEN + 4..NLMSG_HDRLEN + 8].copy_from_slice(&val.to_ne_bytes());

        let ret = unsafe {
            libc::send(handle.fd, buf.as_ptr() as *const libc::c_void, total_len, 0)
        };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("send(AUDIT_SET) failed: {}", err)));
        }

        tracing::info!("Audit subsystem {}", if enabled { "enabled" } else { "disabled" });
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (handle, enabled);
        Err(SysError::NotSupported)
    }
}

/// Add an audit rule via AUDIT_ADD_RULE.
pub fn add_audit_rule(handle: &AuditHandle, rule: &AuditRule) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        rule.validate()?;
        send_rule_message(handle, rule, AUDIT_ADD_RULE)?;
        tracing::debug!("Added audit rule: key={}", rule.key);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (handle, rule);
        Err(SysError::NotSupported)
    }
}

/// Delete an audit rule via AUDIT_DEL_RULE.
pub fn delete_audit_rule(handle: &AuditHandle, rule: &AuditRule) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        rule.validate()?;
        send_rule_message(handle, rule, AUDIT_DEL_RULE)?;
        tracing::debug!("Deleted audit rule: key={}", rule.key);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (handle, rule);
        Err(SysError::NotSupported)
    }
}

/// Send an AUDIT_ADD_RULE or AUDIT_DEL_RULE message.
#[cfg(target_os = "linux")]
fn send_rule_message(handle: &AuditHandle, rule: &AuditRule, msg_type: u16) -> Result<()> {
    if handle.fd < 0 {
        return Err(SysError::InvalidArgument(
            "Audit handle has no netlink socket".into(),
        ));
    }

    // Serialize the rule as a simple text payload (the kernel audit interface
    // accepts both structured audit_rule_data and text-based specifications).
    let rule_text = match rule.rule_type {
        AuditRuleType::FileWatch => {
            format!(
                "watch={} key={}",
                rule.path.as_deref().unwrap_or(""),
                rule.key
            )
        }
        AuditRuleType::SyscallWatch => {
            format!(
                "syscall={} key={}",
                rule.syscall.unwrap_or(0),
                rule.key
            )
        }
    };

    let payload = rule_text.as_bytes();
    let total_len = NLMSG_HDRLEN + payload.len();
    let mut buf = vec![0u8; total_len];

    buf[0..4].copy_from_slice(&(total_len as u32).to_ne_bytes());
    buf[4..6].copy_from_slice(&msg_type.to_ne_bytes());
    buf[6..8].copy_from_slice(&1u16.to_ne_bytes());
    buf[8..12].copy_from_slice(&1u32.to_ne_bytes());
    let pid = unsafe { libc::getpid() } as u32;
    buf[12..16].copy_from_slice(&pid.to_ne_bytes());
    buf[NLMSG_HDRLEN..].copy_from_slice(payload);

    let ret = unsafe {
        libc::send(handle.fd, buf.as_ptr() as *const libc::c_void, total_len, 0)
    };
    if ret < 0 {
        let err = std::io::Error::last_os_error();
        return Err(SysError::Unknown(format!(
            "send(audit rule msg_type={}) failed: {}",
            msg_type, err
        )));
    }

    Ok(())
}

/// Read audit events from the AGNOS `/proc/agnos/audit` interface.
///
/// Each line in the proc file is a JSON-encoded `RawAuditEntry`.
/// Returns an empty vec if the file does not exist.
pub fn read_agnos_audit_events(proc_path: &str) -> Result<Vec<RawAuditEntry>> {
    let path = Path::new(proc_path);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|e| SysError::Unknown(format!("Failed to read {}: {}", proc_path, e)))?;

    let mut entries = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<RawAuditEntry>(trimmed) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                tracing::warn!("Skipping malformed audit entry: {}", e);
            }
        }
    }

    Ok(entries)
}

/// Log an audit event via the AGNOS custom syscall (SYS_AGNOS_AUDIT_LOG = 520).
///
/// This is the fast path for kernel-level audit logging from userspace.
pub fn agnos_audit_log_syscall(action: &str, data: &str, result: i32) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if action.is_empty() {
            return Err(SysError::InvalidArgument("Audit action cannot be empty".into()));
        }
        if action.len() > 256 {
            return Err(SysError::InvalidArgument("Audit action too long (max 256)".into()));
        }
        if data.len() > 4096 {
            return Err(SysError::InvalidArgument("Audit data too long (max 4096)".into()));
        }

        let action_cstr = std::ffi::CString::new(action)
            .map_err(|_| SysError::InvalidArgument("Action contains null byte".into()))?;
        let data_cstr = std::ffi::CString::new(data)
            .map_err(|_| SysError::InvalidArgument("Data contains null byte".into()))?;

        let ret = unsafe {
            libc::syscall(
                SYS_AGNOS_AUDIT_LOG,
                action_cstr.as_ptr(),
                data_cstr.as_ptr(),
                result as libc::c_int,
            )
        };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return match err.raw_os_error() {
                Some(libc::ENOSYS) => Err(SysError::NotSupported),
                Some(libc::EPERM) => Err(SysError::PermissionDenied),
                _ => Err(SysError::Unknown(format!("SYS_AGNOS_AUDIT_LOG failed: {}", err))),
            };
        }

        tracing::debug!("Logged audit syscall: action={}, result={}", action, result);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (action, data, result);
        Err(SysError::NotSupported)
    }
}

/// Close an audit handle, releasing the netlink socket.
pub fn close_audit(handle: AuditHandle) {
    #[cfg(target_os = "linux")]
    {
        if handle.fd >= 0 {
            unsafe { libc::close(handle.fd); }
            tracing::debug!("Closed audit handle (fd={})", handle.fd);
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = handle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_config_default() {
        let config = AuditConfig::default();
        assert!(config.use_netlink);
        assert!(!config.use_agnos_proc);
        assert_eq!(config.proc_path, "/proc/agnos/audit");
    }

    #[test]
    fn test_audit_status_default() {
        let status = AuditStatus::default();
        assert_eq!(status.enabled, 0);
        assert_eq!(status.failure_action, 1);
        assert_eq!(status.backlog_limit, 8192);
    }

    #[test]
    fn test_audit_rule_file_watch() {
        let rule = AuditRule::file_watch("/etc/passwd", "passwd_watch");
        assert_eq!(rule.rule_type, AuditRuleType::FileWatch);
        assert_eq!(rule.path.as_deref(), Some("/etc/passwd"));
        assert!(rule.syscall.is_none());
        assert_eq!(rule.key, "passwd_watch");
    }

    #[test]
    fn test_audit_rule_syscall_watch() {
        let rule = AuditRule::syscall_watch(59, "execve_watch");
        assert_eq!(rule.rule_type, AuditRuleType::SyscallWatch);
        assert!(rule.path.is_none());
        assert_eq!(rule.syscall, Some(59));
        assert_eq!(rule.key, "execve_watch");
    }

    #[test]
    fn test_audit_rule_validate_file_watch_ok() {
        let rule = AuditRule::file_watch("/etc/shadow", "shadow");
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_audit_rule_validate_file_watch_no_path() {
        let rule = AuditRule {
            rule_type: AuditRuleType::FileWatch,
            path: None,
            syscall: None,
            key: "test".to_string(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_audit_rule_validate_file_watch_relative_path() {
        let rule = AuditRule {
            rule_type: AuditRuleType::FileWatch,
            path: Some("relative/path".to_string()),
            syscall: None,
            key: "test".to_string(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_audit_rule_validate_syscall_watch_ok() {
        let rule = AuditRule::syscall_watch(1, "write_watch");
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_audit_rule_validate_syscall_watch_no_syscall() {
        let rule = AuditRule {
            rule_type: AuditRuleType::SyscallWatch,
            path: None,
            syscall: None,
            key: "test".to_string(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_audit_rule_validate_empty_key() {
        let rule = AuditRule {
            rule_type: AuditRuleType::FileWatch,
            path: Some("/etc/passwd".to_string()),
            syscall: None,
            key: String::new(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_audit_rule_validate_key_too_long() {
        let rule = AuditRule {
            rule_type: AuditRuleType::FileWatch,
            path: Some("/etc/passwd".to_string()),
            syscall: None,
            key: "x".repeat(257),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_read_agnos_audit_events_nonexistent() {
        let entries = read_agnos_audit_events("/tmp/nonexistent_agnos_audit_test").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_agnos_audit_events_from_file() {
        let dir = std::env::temp_dir().join("agnos_audit_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit_events.json");

        let entry = RawAuditEntry {
            sequence: 1,
            timestamp_ns: 1000000,
            action_type: "sandbox_applied".to_string(),
            result: 0,
            hash: "abc123".to_string(),
            prev_hash: "".to_string(),
            payload: "agent_id=test-1".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        std::fs::write(&path, format!("{}\n", json)).unwrap();

        let entries = read_agnos_audit_events(path.to_str().unwrap()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].sequence, 1);
        assert_eq!(entries[0].action_type, "sandbox_applied");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_agnos_audit_log_syscall_validation() {
        let result = agnos_audit_log_syscall("", "data", 0);
        assert!(result.is_err());

        let result = agnos_audit_log_syscall(&"x".repeat(257), "data", 0);
        assert!(result.is_err());

        let result = agnos_audit_log_syscall("test", &"x".repeat(4097), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_raw_audit_entry_serialization() {
        let entry = RawAuditEntry {
            sequence: 42,
            timestamp_ns: 1709500000000000000,
            action_type: "test_event".to_string(),
            result: 0,
            hash: "deadbeef".to_string(),
            prev_hash: "cafebabe".to_string(),
            payload: "key=value".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: RawAuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sequence, 42);
        assert_eq!(deserialized.action_type, "test_event");
    }

    #[test]
    #[ignore = "Requires CAP_AUDIT_CONTROL (root)"]
    fn test_open_audit_netlink() {
        let config = AuditConfig::default();
        let handle = open_audit(&config).unwrap();
        assert!(handle.fd >= 0);
        close_audit(handle);
    }

    #[test]
    #[ignore = "Requires CAP_AUDIT_CONTROL (root)"]
    fn test_get_audit_status() {
        let config = AuditConfig::default();
        let handle = open_audit(&config).unwrap();
        let status = get_audit_status(&handle).unwrap();
        // Just verify we got a response
        assert!(status.backlog_limit > 0);
        close_audit(handle);
    }
}
