//! Security system interface
//!
//! Provides safe Rust bindings for security-related syscalls.

use crate::error::{Result, SysError};
use std::path::PathBuf;

// Linux Landlock ABI syscall numbers (x86_64, available since kernel 5.13)
#[cfg(target_os = "linux")]
const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
#[cfg(target_os = "linux")]
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
#[cfg(target_os = "linux")]
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;

// Landlock ABI constants
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
#[cfg(target_os = "linux")]
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
#[cfg(target_os = "linux")]
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

/// Landlock ruleset attribute (ABI v1)
#[cfg(target_os = "linux")]
#[repr(C)]
struct LandlockRulesetAttr {
    handled_access_fs: u64,
}

/// Landlock path-beneath attribute
#[cfg(target_os = "linux")]
#[repr(C)]
struct LandlockPathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

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

/// Apply Landlock filesystem restrictions to the calling process.
///
/// This uses the Landlock ABI v1+ syscalls (kernel 5.13+) to restrict filesystem
/// access for the calling process. If Landlock is not supported by the kernel,
/// logs a warning and returns Ok (graceful degradation).
///
/// # Errors
/// Returns an error if the Landlock syscalls fail for reasons other than
/// kernel incompatibility (e.g., ENOMEM, EINVAL).
pub fn apply_landlock(rules: &[FilesystemRule]) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::RawFd;

        if rules.is_empty() {
            return Ok(());
        }

        // Determine the full set of access rights we want to handle
        let handled_access = LANDLOCK_ACCESS_FS_READ_FILE
            | LANDLOCK_ACCESS_FS_READ_DIR
            | LANDLOCK_ACCESS_FS_WRITE_FILE;

        let attr = LandlockRulesetAttr {
            handled_access_fs: handled_access,
        };

        // Create a Landlock ruleset
        let ruleset_fd: RawFd = unsafe {
            libc::syscall(
                SYS_LANDLOCK_CREATE_RULESET,
                &attr as *const LandlockRulesetAttr,
                std::mem::size_of::<LandlockRulesetAttr>(),
                0u32,
            ) as RawFd
        };

        if ruleset_fd < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ENOSYS) || err.raw_os_error() == Some(libc::EOPNOTSUPP) {
                tracing::warn!("Landlock not supported by kernel, skipping filesystem restrictions");
                return Ok(());
            }
            return Err(SysError::Unknown(format!("landlock_create_ruleset failed: {}", err)));
        }

        // Add rules for each path
        for rule in rules {
            let allowed_access = match rule.access {
                FsAccess::NoAccess => 0u64,
                FsAccess::ReadOnly => LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR,
                FsAccess::ReadWrite => {
                    LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR | LANDLOCK_ACCESS_FS_WRITE_FILE
                }
            };

            if allowed_access == 0 {
                continue; // NoAccess means don't add a rule (default deny)
            }

            // Open the path to get a file descriptor
            let path_fd: RawFd = unsafe {
                let c_path = std::ffi::CString::new(
                    rule.path.as_os_str().as_encoded_bytes()
                ).map_err(|_| SysError::InvalidArgument("Path contains null byte".into()))?;
                libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC)
            };

            if path_fd < 0 {
                let err = std::io::Error::last_os_error();
                tracing::warn!("Cannot open path {:?} for Landlock rule: {}", rule.path, err);
                unsafe { libc::close(ruleset_fd); }
                return Err(SysError::Unknown(format!(
                    "Cannot open path {:?} for Landlock: {}", rule.path, err
                )));
            }

            let path_beneath = LandlockPathBeneathAttr {
                allowed_access,
                parent_fd: path_fd,
            };

            let ret = unsafe {
                libc::syscall(
                    SYS_LANDLOCK_ADD_RULE,
                    ruleset_fd,
                    LANDLOCK_RULE_PATH_BENEATH,
                    &path_beneath as *const LandlockPathBeneathAttr,
                    0u32,
                )
            };

            unsafe { libc::close(path_fd); }

            if ret < 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(ruleset_fd); }
                return Err(SysError::Unknown(format!(
                    "landlock_add_rule failed for {:?}: {}", rule.path, err
                )));
            }
        }

        // Enforce the ruleset on the calling process
        // First, set no_new_privs (required by Landlock)
        let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(ruleset_fd); }
            return Err(SysError::Unknown(format!("PR_SET_NO_NEW_PRIVS failed: {}", err)));
        }

        let ret = unsafe {
            libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0u32)
        };

        unsafe { libc::close(ruleset_fd); }

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("landlock_restrict_self failed: {}", err)));
        }

        tracing::debug!("Applied {} Landlock rules", rules.len());
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = rules;
        tracing::warn!("Landlock is only available on Linux");
    }

    Ok(())
}

/// Load a seccomp-BPF filter into the calling process.
///
/// The filter must be a valid sequence of BPF `sock_filter` instructions
/// (8 bytes each). Use `create_basic_seccomp_filter()` to generate one.
///
/// This sets `PR_SET_NO_NEW_PRIVS` (required) and then installs the filter
/// via `PR_SET_SECCOMP` with `SECCOMP_MODE_FILTER`.
///
/// # Errors
/// Returns an error if the filter is empty, malformed (not a multiple of 8 bytes),
/// or if the kernel rejects the filter.
pub fn load_seccomp(filter: &[u8]) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if filter.is_empty() {
            return Err(SysError::InvalidArgument("Empty filter".into()));
        }

        if filter.len() % 8 != 0 {
            return Err(SysError::InvalidArgument(
                format!("Filter size {} is not a multiple of 8 (sock_filter size)", filter.len()),
            ));
        }

        let num_instructions = filter.len() / 8;
        tracing::debug!("Loading seccomp filter ({} instructions, {} bytes)", num_instructions, filter.len());

        // Require no_new_privs before installing seccomp filter
        let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("PR_SET_NO_NEW_PRIVS failed: {}", err)));
        }

        // Build sock_fprog struct
        #[repr(C)]
        struct SockFprog {
            len: libc::c_ushort,
            filter: *const u8,
        }

        let prog = SockFprog {
            len: num_instructions as libc::c_ushort,
            filter: filter.as_ptr(),
        };

        let ret = unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                2, // SECCOMP_MODE_FILTER
                &prog as *const SockFprog,
                0,
                0,
            )
        };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!("PR_SET_SECCOMP failed: {}", err)));
        }

        tracing::debug!("Seccomp filter installed successfully");
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = filter;
        tracing::warn!("Seccomp is only available on Linux");
    }

    Ok(())
}

/// BPF sock_filter instruction (8 bytes each, matching kernel struct sock_filter).
#[repr(C)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

impl SockFilter {
    const fn new(code: u16, jt: u8, jf: u8, k: u32) -> Self {
        Self { code, jt, jf, k }
    }

    fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..2].copy_from_slice(&self.code.to_ne_bytes());
        bytes[2] = self.jt;
        bytes[3] = self.jf;
        bytes[4..8].copy_from_slice(&self.k.to_ne_bytes());
        bytes
    }
}

// BPF instruction constants
const BPF_LD_W_ABS: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const BPF_JMP_JEQ_K: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const BPF_RET_K: u16 = 0x06; // BPF_RET | BPF_K

// Seccomp return values
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;

/// Create a basic seccomp filter that allows safe syscalls and kills on dangerous ones.
///
/// The filter uses proper BPF sock_filter encoding (8 bytes per instruction):
/// `u16 code, u8 jt, u8 jf, u32 k`
///
/// Allowed: read, write, exit, exit_group, rt_sigreturn, mmap, mprotect, brk,
///          close, fstat, munmap, sigaltstack, arch_prctl, gettid, futex,
///          set_tid_address, set_robust_list, rseq, getrandom, clock_gettime.
/// All other syscalls: KILL_PROCESS.
pub fn create_basic_seccomp_filter() -> Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        // Allowlisted syscalls (x86_64 numbers)
        let allow_syscalls: &[u32] = &[
            0,   // read
            1,   // write
            3,   // close
            5,   // fstat
            9,   // mmap
            10,  // mprotect
            11,  // munmap
            12,  // brk
            15,  // rt_sigreturn
            60,  // exit
            131, // sigaltstack
            158, // arch_prctl
            186, // gettid
            202, // futex
            218, // set_tid_address
            231, // exit_group
            273, // set_robust_list
            318, // getrandom
            228, // clock_gettime
            334, // rseq
        ];

        let num_allowed = allow_syscalls.len();
        // Total instructions: 1 (load) + 2*num_allowed (jeq + allow per syscall) + 1 (default kill)
        let mut instructions: Vec<SockFilter> = Vec::with_capacity(2 + 2 * num_allowed);

        // Instruction 0: Load syscall number from seccomp_data.nr (offset 0)
        instructions.push(SockFilter::new(BPF_LD_W_ABS, 0, 0, 0));

        // For each allowed syscall: JEQ → ALLOW, else fall through
        for (i, &nr) in allow_syscalls.iter().enumerate() {
            let remaining = (num_allowed - i - 1) as u8;
            // jt = jump to ALLOW (skip remaining comparisons + default kill)
            // jf = 0 (fall through to next comparison)
            let jt = remaining * 2 + 1; // skip remaining (jeq+ret) pairs + the final kill
            instructions.push(SockFilter::new(BPF_JMP_JEQ_K, jt, 0, nr));
        }

        // Default: KILL_PROCESS
        instructions.push(SockFilter::new(BPF_RET_K, 0, 0, SECCOMP_RET_KILL_PROCESS));

        // ALLOW return (target of all successful JEQ jumps)
        instructions.push(SockFilter::new(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW));

        // Serialize to bytes
        let mut filter = Vec::with_capacity(instructions.len() * 8);
        for insn in &instructions {
            filter.extend_from_slice(&insn.to_bytes());
        }

        Ok(filter)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(vec![])
    }
}

/// Map common namespace/unshare errno values to descriptive SysError variants.
#[cfg(target_os = "linux")]
fn map_namespace_error(operation: &str) -> SysError {
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EPERM) => SysError::PermissionDenied,
        Some(libc::ENOMEM) => SysError::Unknown(format!("{}: out of memory", operation)),
        Some(libc::EINVAL) => SysError::InvalidArgument(format!("{}: invalid flags", operation)),
        Some(libc::ENOSPC) => SysError::Unknown(format!(
            "{}: namespace limit reached (see /proc/sys/user/max_*_namespaces)", operation
        )),
        Some(libc::EUSERS) => SysError::Unknown(format!(
            "{}: nesting limit for user namespaces exceeded", operation
        )),
        _ => SysError::Unknown(format!("{}: {}", operation, err)),
    }
}

/// Enter a new network namespace.
///
/// Requires `CAP_SYS_ADMIN` in the current user namespace, or an unprivileged
/// user namespace must be created first.
///
/// # Safety considerations
/// This calls `libc::unshare(CLONE_NEWNET)` which is safe from Rust's memory
/// safety perspective. The operation is kernel-mediated and validated.
pub fn enter_network_namespace() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let ret = unsafe { libc::unshare(libc::CLONE_NEWNET) };
        if ret != 0 {
            return Err(map_namespace_error("enter_network_namespace"));
        }

        tracing::debug!("Entered new network namespace");
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing::warn!("Network namespaces are only available on Linux");
    }

    Ok(())
}

/// Create new namespace(s) with specified flags.
///
/// Requires appropriate capabilities depending on flags:
/// - `NETWORK`: `CAP_SYS_ADMIN` (or user namespace)
/// - `MOUNT`: `CAP_SYS_ADMIN` (or user namespace)
/// - `PID`: `CAP_SYS_ADMIN` (or user namespace)
/// - `USER`: unprivileged (but subject to nesting limits)
///
/// # Safety considerations
/// This calls `libc::unshare()` which is safe from Rust's memory safety
/// perspective. The operation is kernel-mediated and validated.
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

        let ret = unsafe { libc::unshare(sflags) };
        if ret != 0 {
            return Err(map_namespace_error("create_namespace"));
        }

        tracing::debug!("Created namespace with flags: {:?}", flags);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = flags;
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
    fn test_load_seccomp_empty() {
        let result = load_seccomp(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_seccomp_invalid_size() {
        // Not a multiple of 8 bytes (sock_filter size)
        let filter: &[u8] = &[0x06, 0x00, 0x00, 0x7f];
        let result = load_seccomp(filter);
        assert!(result.is_err());
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

    #[test]
    fn test_namespace_flags_default() {
        let flags = NamespaceFlags::default();
        assert!(flags.is_empty());
        assert!(!flags.contains(NamespaceFlags::NETWORK));
        assert!(!flags.contains(NamespaceFlags::MOUNT));
        assert!(!flags.contains(NamespaceFlags::PID));
        assert!(!flags.contains(NamespaceFlags::USER));
    }

    #[test]
    fn test_namespace_flags_all_combinations() {
        let all = NamespaceFlags::NETWORK | NamespaceFlags::MOUNT | NamespaceFlags::PID | NamespaceFlags::USER;
        assert!(all.contains(NamespaceFlags::NETWORK));
        assert!(all.contains(NamespaceFlags::MOUNT));
        assert!(all.contains(NamespaceFlags::PID));
        assert!(all.contains(NamespaceFlags::USER));
    }

    #[test]
    fn test_namespace_flags_individual_values() {
        assert_eq!(NamespaceFlags::NETWORK.bits(), 1);
        assert_eq!(NamespaceFlags::MOUNT.bits(), 2);
        assert_eq!(NamespaceFlags::PID.bits(), 4);
        assert_eq!(NamespaceFlags::USER.bits(), 8);
    }

    #[test]
    fn test_namespace_flags_debug() {
        let flags = NamespaceFlags::NETWORK | NamespaceFlags::PID;
        let dbg = format!("{:?}", flags);
        assert!(dbg.contains("NETWORK"));
        assert!(dbg.contains("PID"));
    }

    #[test]
    fn test_namespace_flags_clone_eq() {
        let a = NamespaceFlags::MOUNT;
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_fs_access_debug() {
        assert_eq!(format!("{:?}", FsAccess::NoAccess), "NoAccess");
        assert_eq!(format!("{:?}", FsAccess::ReadOnly), "ReadOnly");
        assert_eq!(format!("{:?}", FsAccess::ReadWrite), "ReadWrite");
    }

    #[test]
    fn test_fs_access_clone_eq() {
        let a = FsAccess::ReadWrite;
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(a, FsAccess::NoAccess);
    }

    #[test]
    fn test_filesystem_rule_new() {
        let rule = FilesystemRule::new("/tmp", FsAccess::ReadOnly);
        assert_eq!(rule.path, PathBuf::from("/tmp"));
        assert_eq!(rule.access, FsAccess::ReadOnly);
    }

    #[test]
    fn test_filesystem_rule_new_pathbuf() {
        let rule = FilesystemRule::new(PathBuf::from("/var/log"), FsAccess::ReadWrite);
        assert_eq!(rule.path, PathBuf::from("/var/log"));
        assert_eq!(rule.access, FsAccess::ReadWrite);
    }

    #[test]
    fn test_apply_landlock_empty_rules() {
        let result = apply_landlock(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_basic_seccomp_filter_structure() {
        let filter = create_basic_seccomp_filter().unwrap();
        // Must be a multiple of 8 (sock_filter size)
        assert_eq!(filter.len() % 8, 0);
        // At least: 1 load + some jeq + 1 kill + 1 allow = minimum 4 instructions = 32 bytes
        assert!(filter.len() >= 32);
    }

    #[test]
    fn test_load_seccomp_seven_bytes() {
        // 7 bytes is not a multiple of 8
        let result = load_seccomp(&[0; 7]);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not a multiple of 8"));
    }

    #[test]
    fn test_sock_filter_to_bytes() {
        let sf = SockFilter::new(0x20, 1, 2, 0xDEAD);
        let bytes = sf.to_bytes();
        assert_eq!(bytes.len(), 8);
        // Verify code field (first 2 bytes in native endian)
        assert_eq!(u16::from_ne_bytes([bytes[0], bytes[1]]), 0x20);
        // Verify jt and jf
        assert_eq!(bytes[2], 1);
        assert_eq!(bytes[3], 2);
        // Verify k field (last 4 bytes in native endian)
        assert_eq!(u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 0xDEAD);
    }

    #[test]
    fn test_sock_filter_const_new() {
        let sf = SockFilter::new(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW);
        assert_eq!(sf.code, BPF_RET_K);
        assert_eq!(sf.jt, 0);
        assert_eq!(sf.jf, 0);
        assert_eq!(sf.k, SECCOMP_RET_ALLOW);
    }
}
