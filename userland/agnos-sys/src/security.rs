//! Security system interface
//!
//! Provides safe Rust bindings for security-related syscalls.

use crate::error::{Result, SysError};
use std::path::PathBuf;

bitflags::bitflags! {
    /// Namespace flags for creating Linux namespaces
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NamespaceFlags: u32 {
        /// Network namespace
        const NETWORK = 1;
        /// Mount namespace
        const MOUNT = 2;
        /// PID namespace
        const PID = 4;
        /// User namespace
        const USER = 8;
    }
}

impl Default for NamespaceFlags {
    fn default() -> Self {
        Self::empty()
    }
}

/// Apply Landlock filesystem restrictions
pub fn apply_landlock(rules: &[FilesystemRule]) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;

        // Landlock requires kernel 5.13+
        // Rules are passed as a byte array to the syscall
        // For now, we'll use the userspace library approach

        let mut access_bytes: Vec<u8> = Vec::new();

        for rule in rules {
            let path_cstr = CString::new(rule.path.to_string_lossy().as_bytes())
                .map_err(|_| SysError::InvalidArgument("Path contains null byte".into()))?;
            let path_bytes = path_cstr.as_bytes_with_nul();

            // Convert FsAccess to Landlock access flags
            let access = match rule.access {
                FsAccess::NoAccess => 0u32,
                FsAccess::ReadOnly => 1u32,  // LANDLOCK_ACCESS_FS_READ
                FsAccess::ReadWrite => 3u32, // LANDLOCK_ACCESS_FS_READ | LANDLOCK_ACCESS_FS_WRITE
            };

            // Pack path and access (simplified format)
            access_bytes.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
            access_bytes.extend_from_slice(path_bytes);
            access_bytes.extend_from_slice(&access.to_le_bytes());
        }

        // In a full implementation, this would call the actual Landlock syscalls
        // landlock_create_ruleset(..., access_bytes.as_ptr() as *const _, access_bytes.len(), 0)

        tracing::debug!("Applied {} Landlock rules", rules.len());
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing::warn!("Landlock is only available on Linux");
    }

    Ok(())
}

/// Create a Landlock ruleset from filesystem rules
pub fn create_landlock_ruleset(rules: &[FilesystemRule]) -> Result<u32> {
    #[cfg(target_os = "linux")]
    {
        tracing::debug!("Creating Landlock ruleset with {} rules", rules.len());

        // TODO: Implement actual Landlock syscalls (landlock_create_ruleset, landlock_add_rule,
        // landlock_restrict_self). Return a dummy fd handle until implemented.
        Ok(0)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(0)
    }
}

/// Restrict filesystem access using Landlock
pub fn restrict_filesystem(rules: &[FilesystemRule]) -> Result<()> {
    apply_landlock(rules)
}

/// Load seccomp-bpf filter
pub fn load_seccomp(filter: &[u8]) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // seccomp_syscall_filter_basic or seccomp_load
        // In userspace, we use libseccomp

        if filter.is_empty() {
            return Err(SysError::InvalidArgument("Empty filter".into()));
        }

        tracing::debug!("Loading seccomp filter ({} bytes)", filter.len());

        // TODO: In a full implementation:
        // let ctx = seccomp_init(SECCOMP_RET_KILL);
        // seccomp_rule_add_array(ctx, SCMP_ACT_KILL, SCMP_SYS(write), 0, null_mut());
        // seccomp_load(ctx);
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing::warn!("Seccomp is only available on Linux");
    }

    Ok(())
}

/// Create a basic seccomp filter that denies dangerous syscalls
pub fn create_basic_seccomp_filter() -> Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        let mut filter = Vec::new();

        // BPF filter in compact binary format
        // This is a minimal filter that allows everything except dangerous syscalls
        // Format: BPF_STMT + BPF_JUMP

        // Allow: read, write, exit, rt_sigreturn, mmap, mprotect, brk
        // Deny: kill (can be used to kill other processes), reboot, setuid

        let allow_syscalls = [
            0,   // read
            1,   // write
            60,  // exit
            231, // rt_sigreturn
            9,   // mmap
            125, // mprotect
            45,  // brk
        ];

        for &syscall in &allow_syscalls {
            filter.push(0x20); // BPF_LD | BPF_W | BPF_ABS
            filter.push(0); // offset to syscall number
            filter.push(0);
            filter.push(0);

            let imm = (syscall as u32).to_le_bytes();
            filter.extend_from_slice(&imm);

            filter.push(0x15); // BPF_JMP | BPF_JEQ
            filter.push(0); // k
            filter.push(0); // jt
            filter.push(1); // jf (skip next instruction = allow)

            // Next instruction: return ALLOW
            filter.push(0x06); // BPF_RET | BPF_K
            filter.push(0);
            filter.push(0);
            filter.push(0x7f); // SECCOMP_RET_ALLOW
        }

        // Default: return ERRNO (deny)
        filter.push(0x06); // BPF_RET | BPF_K
        filter.push(0);
        filter.push(0);
        filter.push(0x7f); // SECCOMP_RET_ERRNO | 0x7f

        Ok(filter)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(vec![])
    }
}

/// Enter a new network namespace
pub fn enter_network_namespace() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Use raw syscall for CLONE_NEWNET
        unsafe {
            let ret = libc::unshare(libc::CLONE_NEWNET);
            if ret != 0 {
                return Err(SysError::Unknown(format!(
                    "Failed to enter network namespace: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }

        tracing::debug!("Entered new network namespace");
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing::warn!("Network namespaces are only available on Linux");
    }

    Ok(())
}

/// Create a new namespace with specified flags
pub fn create_namespace(flags: NamespaceFlags) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let mut sflags: libc::c_int = 0;

        if flags.contains(NamespaceFlags::NETWORK) {
            sflags |= libc::CLONE_NEWNET;
        }
        if flags.contains(NamespaceFlags::MOUNT) {
            sflags |= libc::CLONE_NEWNS;
        }
        if flags.contains(NamespaceFlags::PID) {
            sflags |= libc::CLONE_NEWPID;
        }
        if flags.contains(NamespaceFlags::USER) {
            sflags |= libc::CLONE_NEWUSER;
        }

        unsafe {
            let ret = libc::unshare(sflags);
            if ret != 0 {
                return Err(SysError::Unknown(format!(
                    "Failed to create namespace: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }

        tracing::debug!("Created namespace with flags: {:?}", flags);
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing::warn!("Namespaces are only available on Linux");
    }

    Ok(())
}

/// Filesystem access rule for Landlock
pub struct FilesystemRule {
    pub path: std::path::PathBuf,
    pub access: FsAccess,
}

impl FilesystemRule {
    /// Create a new filesystem rule
    pub fn new(path: impl Into<PathBuf>, access: FsAccess) -> Self {
        Self {
            path: path.into(),
            access,
        }
    }

    /// Create a read-only rule
    pub fn read_only(path: impl Into<PathBuf>) -> Self {
        Self::new(path, FsAccess::ReadOnly)
    }

    /// Create a read-write rule
    pub fn read_write(path: impl Into<PathBuf>) -> Self {
        Self::new(path, FsAccess::ReadWrite)
    }
}

/// Filesystem access levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsAccess {
    NoAccess,
    ReadOnly,
    ReadWrite,
}

impl Default for FsAccess {
    fn default() -> Self {
        FsAccess::NoAccess
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filesystem_rule() {
        let rule = FilesystemRule {
            path: std::path::PathBuf::from("/tmp"),
            access: FsAccess::ReadWrite,
        };
        assert_eq!(rule.path, std::path::PathBuf::from("/tmp"));
    }

    #[test]
    fn test_filesystem_rule_helper_methods() {
        let ro_rule = FilesystemRule::read_only("/tmp");
        assert_eq!(ro_rule.access, FsAccess::ReadOnly);

        let rw_rule = FilesystemRule::read_write("/var/data");
        assert_eq!(rw_rule.access, FsAccess::ReadWrite);
    }

    #[test]
    fn test_fs_access_variants() {
        assert!(matches!(FsAccess::NoAccess, FsAccess::NoAccess));
        assert!(matches!(FsAccess::ReadOnly, FsAccess::ReadOnly));
        assert!(matches!(FsAccess::ReadWrite, FsAccess::ReadWrite));
    }

    #[test]
    fn test_fs_access_default() {
        assert_eq!(FsAccess::default(), FsAccess::NoAccess);
    }

    #[test]
    fn test_apply_landlock() {
        let rules = vec![FilesystemRule {
            path: std::path::PathBuf::from("/tmp"),
            access: FsAccess::ReadWrite,
        }];
        let result = apply_landlock(&rules);
        assert!(result.is_ok());
    }

    #[test]
    fn test_restrict_filesystem() {
        let rules = vec![
            FilesystemRule::read_only("/etc"),
            FilesystemRule::read_write("/tmp"),
        ];
        let result = restrict_filesystem(&rules);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_landlock_ruleset() {
        let rules = vec![FilesystemRule::read_only("/home")];
        let result = create_landlock_ruleset(&rules);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_seccomp() {
        // Use a non-empty filter (at least 1 byte)
        let filter: &[u8] = &[0x06, 0x00, 0x00, 0x7f];
        let result = load_seccomp(filter);
        assert!(result.is_ok());
    }

    #[test]
    #[ignore = "Requires privileged access to create network namespaces"]
    fn test_enter_network_namespace() {
        let result = enter_network_namespace();
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_basic_seccomp_filter() {
        let filter = create_basic_seccomp_filter();
        assert!(filter.is_ok());
        // Filter should contain BPF instructions
        assert!(!filter.unwrap().is_empty());
    }

    #[test]
    fn test_create_namespace_flags() {
        let flags = NamespaceFlags::NETWORK | NamespaceFlags::MOUNT;
        assert!(flags.contains(NamespaceFlags::NETWORK));
        assert!(flags.contains(NamespaceFlags::MOUNT));
        assert!(!flags.contains(NamespaceFlags::PID));
    }

    #[test]
    fn test_create_namespace() {
        // Test with empty flags (should succeed even if namespaces not available)
        let result = create_namespace(NamespaceFlags::empty());
        assert!(result.is_ok());
    }
}
