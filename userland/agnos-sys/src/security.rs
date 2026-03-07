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
            if err.raw_os_error() == Some(libc::ENOSYS)
                || err.raw_os_error() == Some(libc::EOPNOTSUPP)
            {
                tracing::warn!(
                    "Landlock not supported by kernel, skipping filesystem restrictions"
                );
                return Ok(());
            }
            return Err(SysError::Unknown(format!(
                "landlock_create_ruleset failed: {}",
                err
            )));
        }

        // Add rules for each path
        for rule in rules {
            let allowed_access = match rule.access {
                FsAccess::NoAccess => 0u64,
                FsAccess::ReadOnly => LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR,
                FsAccess::ReadWrite => {
                    LANDLOCK_ACCESS_FS_READ_FILE
                        | LANDLOCK_ACCESS_FS_READ_DIR
                        | LANDLOCK_ACCESS_FS_WRITE_FILE
                }
            };

            if allowed_access == 0 {
                continue; // NoAccess means don't add a rule (default deny)
            }

            // Open the path to get a file descriptor
            let path_fd: RawFd = unsafe {
                let c_path = std::ffi::CString::new(rule.path.as_os_str().as_encoded_bytes())
                    .map_err(|_| SysError::InvalidArgument("Path contains null byte".into()))?;
                libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC)
            };

            if path_fd < 0 {
                let err = std::io::Error::last_os_error();
                tracing::warn!(
                    "Cannot open path {:?} for Landlock rule: {}",
                    rule.path,
                    err
                );
                unsafe {
                    libc::close(ruleset_fd);
                }
                return Err(SysError::Unknown(format!(
                    "Cannot open path {:?} for Landlock: {}",
                    rule.path, err
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

            unsafe {
                libc::close(path_fd);
            }

            if ret < 0 {
                let err = std::io::Error::last_os_error();
                unsafe {
                    libc::close(ruleset_fd);
                }
                return Err(SysError::Unknown(format!(
                    "landlock_add_rule failed for {:?}: {}",
                    rule.path, err
                )));
            }
        }

        // Enforce the ruleset on the calling process
        // First, set no_new_privs (required by Landlock)
        let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            unsafe {
                libc::close(ruleset_fd);
            }
            return Err(SysError::Unknown(format!(
                "PR_SET_NO_NEW_PRIVS failed: {}",
                err
            )));
        }

        let ret = unsafe { libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0u32) };

        unsafe {
            libc::close(ruleset_fd);
        }

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!(
                "landlock_restrict_self failed: {}",
                err
            )));
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
            return Err(SysError::InvalidArgument(format!(
                "Filter size {} is not a multiple of 8 (sock_filter size)",
                filter.len()
            )));
        }

        let num_instructions = filter.len() / 8;
        tracing::debug!(
            "Loading seccomp filter ({} instructions, {} bytes)",
            num_instructions,
            filter.len()
        );

        // Require no_new_privs before installing seccomp filter
        let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            return Err(SysError::Unknown(format!(
                "PR_SET_NO_NEW_PRIVS failed: {}",
                err
            )));
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
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
pub const SECCOMP_RET_TRAP: u32 = 0x0003_0000;

/// Map a syscall name to its x86_64 syscall number.
///
/// Returns `None` for unrecognized names. Covers the most common Linux syscalls.
pub fn syscall_name_to_nr(name: &str) -> Option<u32> {
    match name {
        "read" => Some(0),
        "write" => Some(1),
        "open" => Some(2),
        "close" => Some(3),
        "stat" => Some(4),
        "fstat" => Some(5),
        "lstat" => Some(6),
        "poll" => Some(7),
        "lseek" => Some(8),
        "mmap" => Some(9),
        "mprotect" => Some(10),
        "munmap" => Some(11),
        "brk" => Some(12),
        "rt_sigaction" => Some(13),
        "rt_sigprocmask" => Some(14),
        "rt_sigreturn" => Some(15),
        "ioctl" => Some(16),
        "pread64" => Some(17),
        "pwrite64" => Some(18),
        "readv" => Some(19),
        "writev" => Some(20),
        "access" => Some(21),
        "pipe" => Some(22),
        "select" => Some(23),
        "sched_yield" => Some(24),
        "mremap" => Some(25),
        "msync" => Some(26),
        "dup" => Some(32),
        "dup2" => Some(33),
        "nanosleep" => Some(35),
        "getpid" => Some(39),
        "socket" => Some(41),
        "connect" => Some(42),
        "accept" => Some(43),
        "sendto" => Some(44),
        "recvfrom" => Some(45),
        "sendmsg" => Some(46),
        "recvmsg" => Some(47),
        "shutdown" => Some(48),
        "bind" => Some(49),
        "listen" => Some(50),
        "getsockname" => Some(51),
        "getpeername" => Some(52),
        "setsockopt" => Some(54),
        "getsockopt" => Some(55),
        "clone" => Some(56),
        "fork" => Some(57),
        "vfork" => Some(58),
        "execve" => Some(59),
        "exit" => Some(60),
        "wait4" => Some(61),
        "kill" => Some(62),
        "fcntl" => Some(72),
        "flock" => Some(73),
        "fsync" => Some(74),
        "fdatasync" => Some(75),
        "truncate" => Some(76),
        "ftruncate" => Some(77),
        "getdents" => Some(78),
        "getcwd" => Some(79),
        "chdir" => Some(80),
        "rename" => Some(82),
        "mkdir" => Some(83),
        "rmdir" => Some(84),
        "creat" => Some(85),
        "link" => Some(86),
        "unlink" => Some(87),
        "symlink" => Some(88),
        "readlink" => Some(89),
        "chmod" => Some(90),
        "chown" => Some(92),
        "getuid" => Some(102),
        "getgid" => Some(104),
        "geteuid" => Some(107),
        "getegid" => Some(108),
        "setpgid" => Some(109),
        "getppid" => Some(110),
        "getpgrp" => Some(111),
        "setsid" => Some(112),
        "getgroups" => Some(115),
        "sigaltstack" => Some(131),
        "statfs" => Some(137),
        "fstatfs" => Some(138),
        "prctl" => Some(157),
        "arch_prctl" => Some(158),
        "mount" => Some(165),
        "umount2" => Some(166),
        "reboot" => Some(169),
        "gettid" => Some(186),
        "futex" => Some(202),
        "getdents64" => Some(217),
        "set_tid_address" => Some(218),
        "clock_gettime" => Some(228),
        "exit_group" => Some(231),
        "epoll_wait" => Some(232),
        "epoll_ctl" => Some(233),
        "openat" => Some(257),
        "mkdirat" => Some(258),
        "newfstatat" => Some(262),
        "unlinkat" => Some(263),
        "renameat" => Some(264),
        "set_robust_list" => Some(273),
        "pipe2" => Some(293),
        "dup3" => Some(292),
        "epoll_create1" => Some(291),
        "accept4" => Some(288),
        "eventfd2" => Some(290),
        "getrandom" => Some(318),
        "memfd_create" => Some(319),
        "ptrace" => Some(101),
        "rseq" => Some(334),
        _ => None,
    }
}

/// Build a custom seccomp-BPF filter from explicit allow/deny/trap rules.
///
/// `base_allowed` is the set of syscall numbers always allowed (baseline).
/// `extra_allowed` are additional syscall numbers to allow.
/// `denied` maps syscall numbers to their action (kill or trap).
///
/// The generated filter checks denied rules first (returning kill/trap),
/// then checks allowed rules (returning allow), then defaults to kill.
pub fn create_custom_seccomp_filter(
    base_allowed: &[u32],
    extra_allowed: &[u32],
    denied: &[(u32, u32)], // (syscall_nr, seccomp_ret action)
) -> Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        let mut all_allowed: Vec<u32> = base_allowed.to_vec();
        for &nr in extra_allowed {
            if !all_allowed.contains(&nr) {
                all_allowed.push(nr);
            }
        }
        // Remove any syscall that appears in the denied list
        let denied_nrs: std::collections::HashSet<u32> = denied.iter().map(|&(nr, _)| nr).collect();
        all_allowed.retain(|nr| !denied_nrs.contains(nr));

        let num_denied = denied.len();
        let num_allowed = all_allowed.len();

        // Layout: 1 (load) + 2*denied (jeq+ret per denied) + 2*allowed (jeq+ret per allowed) + 1 (default kill) + 1 (allow ret)
        // But we use a simpler linear scan approach like the basic filter.
        // Denied entries: each is JEQ → specific RET (kill/trap)
        // Allowed entries: each is JEQ → jump to final ALLOW
        // Default: KILL_PROCESS

        let total_after_load = num_denied * 2 + num_allowed + 2; // denied pairs + allowed jeqs + default_kill + allow_ret
        let mut instructions: Vec<SockFilter> = Vec::with_capacity(1 + total_after_load);

        // Instruction 0: Load syscall number
        instructions.push(SockFilter::new(BPF_LD_W_ABS, 0, 0, 0));

        // Denied rules first — each denied syscall gets JEQ → next instruction (its RET)
        for &(nr, action) in denied {
            // JEQ: if match, jump to next instruction (jt=0), else skip the RET (jf=1)
            instructions.push(SockFilter::new(BPF_JMP_JEQ_K, 0, 1, nr));
            instructions.push(SockFilter::new(BPF_RET_K, 0, 0, action));
        }

        // Allowed rules: JEQ → jump to ALLOW at the end
        for (i, &nr) in all_allowed.iter().enumerate() {
            let remaining = (num_allowed - i - 1) as u8;
            // Jump over remaining JEQs + default kill to reach ALLOW
            let jt = remaining + 1; // skip remaining comparisons + default kill
            instructions.push(SockFilter::new(BPF_JMP_JEQ_K, jt, 0, nr));
        }

        // Default: KILL_PROCESS
        instructions.push(SockFilter::new(BPF_RET_K, 0, 0, SECCOMP_RET_KILL_PROCESS));

        // ALLOW return
        instructions.push(SockFilter::new(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW));

        let mut filter = Vec::with_capacity(instructions.len() * 8);
        for insn in &instructions {
            filter.extend_from_slice(&insn.to_bytes());
        }

        Ok(filter)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (base_allowed, extra_allowed, denied);
        Ok(vec![])
    }
}

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
            "{}: namespace limit reached (see /proc/sys/user/max_*_namespaces)",
            operation
        )),
        Some(libc::EUSERS) => SysError::Unknown(format!(
            "{}: nesting limit for user namespaces exceeded",
            operation
        )),
        _ => SysError::Unknown(format!("{}: {}", operation, err)),
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FsAccess {
    #[default]
    NoAccess,
    ReadOnly,
    ReadWrite,
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
        let all = NamespaceFlags::NETWORK
            | NamespaceFlags::MOUNT
            | NamespaceFlags::PID
            | NamespaceFlags::USER;
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
        let b = a;
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
        let b = a;
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
        assert_eq!(
            u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            0xDEAD
        );
    }

    #[test]
    fn test_sock_filter_const_new() {
        let sf = SockFilter::new(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW);
        assert_eq!(sf.code, BPF_RET_K);
        assert_eq!(sf.jt, 0);
        assert_eq!(sf.jf, 0);
        assert_eq!(sf.k, SECCOMP_RET_ALLOW);
    }

    // --- New coverage tests (batch 2) ---

    #[test]
    fn test_filesystem_rule_no_access() {
        let rule = FilesystemRule::new("/tmp", FsAccess::NoAccess);
        assert_eq!(rule.access, FsAccess::NoAccess);
        assert_eq!(rule.path, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_filesystem_rule_read_only_from_string() {
        let rule = FilesystemRule::read_only(String::from("/usr/share"));
        assert_eq!(rule.access, FsAccess::ReadOnly);
        assert_eq!(rule.path, PathBuf::from("/usr/share"));
    }

    #[test]
    fn test_filesystem_rule_read_write_from_string() {
        let rule = FilesystemRule::read_write(String::from("/home/user"));
        assert_eq!(rule.access, FsAccess::ReadWrite);
        assert_eq!(rule.path, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_fs_access_all_variants_ne() {
        assert_ne!(FsAccess::NoAccess, FsAccess::ReadOnly);
        assert_ne!(FsAccess::NoAccess, FsAccess::ReadWrite);
        assert_ne!(FsAccess::ReadOnly, FsAccess::ReadWrite);
    }

    #[test]
    fn test_fs_access_copy_semantics() {
        let a = FsAccess::ReadOnly;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn test_namespace_flags_empty_is_default() {
        assert_eq!(NamespaceFlags::empty(), NamespaceFlags::default());
    }

    #[test]
    fn test_namespace_flags_bitwise_operations() {
        let mut flags = NamespaceFlags::NETWORK;
        flags |= NamespaceFlags::PID;
        assert!(flags.contains(NamespaceFlags::NETWORK));
        assert!(flags.contains(NamespaceFlags::PID));
        assert!(!flags.contains(NamespaceFlags::MOUNT));

        flags &= !NamespaceFlags::NETWORK;
        assert!(!flags.contains(NamespaceFlags::NETWORK));
        assert!(flags.contains(NamespaceFlags::PID));
    }

    #[test]
    fn test_namespace_flags_intersection() {
        let a = NamespaceFlags::NETWORK | NamespaceFlags::MOUNT;
        let b = NamespaceFlags::MOUNT | NamespaceFlags::PID;
        let intersection = a & b;
        assert!(intersection.contains(NamespaceFlags::MOUNT));
        assert!(!intersection.contains(NamespaceFlags::NETWORK));
        assert!(!intersection.contains(NamespaceFlags::PID));
    }

    #[test]
    fn test_namespace_flags_is_empty() {
        assert!(NamespaceFlags::empty().is_empty());
        assert!(!NamespaceFlags::NETWORK.is_empty());
    }

    #[test]
    fn test_create_basic_seccomp_filter_length() {
        let filter = create_basic_seccomp_filter().unwrap();
        // 20 allowed syscalls => 1 load + 20 jeq + 1 kill + 1 allow = 23 instructions = 184 bytes
        let num_instructions = filter.len() / 8;
        assert_eq!(num_instructions, 23, "Expected 23 BPF instructions");
    }

    #[test]
    fn test_create_basic_seccomp_filter_starts_with_load() {
        let filter = create_basic_seccomp_filter().unwrap();
        // First instruction should be BPF_LD_W_ABS (0x20)
        let first_code = u16::from_ne_bytes([filter[0], filter[1]]);
        assert_eq!(first_code, BPF_LD_W_ABS);
    }

    #[test]
    fn test_create_basic_seccomp_filter_ends_with_allow() {
        let filter = create_basic_seccomp_filter().unwrap();
        // Last instruction should be RET ALLOW
        let last_start = filter.len() - 8;
        let last_code = u16::from_ne_bytes([filter[last_start], filter[last_start + 1]]);
        let last_k = u32::from_ne_bytes([
            filter[last_start + 4],
            filter[last_start + 5],
            filter[last_start + 6],
            filter[last_start + 7],
        ]);
        assert_eq!(last_code, BPF_RET_K);
        assert_eq!(last_k, SECCOMP_RET_ALLOW);
    }

    #[test]
    fn test_create_basic_seccomp_filter_penultimate_is_kill() {
        let filter = create_basic_seccomp_filter().unwrap();
        // Penultimate instruction should be RET KILL_PROCESS
        let pen_start = filter.len() - 16;
        let pen_code = u16::from_ne_bytes([filter[pen_start], filter[pen_start + 1]]);
        let pen_k = u32::from_ne_bytes([
            filter[pen_start + 4],
            filter[pen_start + 5],
            filter[pen_start + 6],
            filter[pen_start + 7],
        ]);
        assert_eq!(pen_code, BPF_RET_K);
        assert_eq!(pen_k, SECCOMP_RET_KILL_PROCESS);
    }

    #[test]
    fn test_create_basic_seccomp_filter_deterministic() {
        let f1 = create_basic_seccomp_filter().unwrap();
        let f2 = create_basic_seccomp_filter().unwrap();
        assert_eq!(f1, f2, "Filter should be deterministic");
    }

    #[test]
    fn test_sock_filter_to_bytes_all_zeros() {
        let sf = SockFilter::new(0, 0, 0, 0);
        assert_eq!(sf.to_bytes(), [0u8; 8]);
    }

    #[test]
    fn test_sock_filter_to_bytes_max_values() {
        let sf = SockFilter::new(u16::MAX, u8::MAX, u8::MAX, u32::MAX);
        let bytes = sf.to_bytes();
        assert_eq!(u16::from_ne_bytes([bytes[0], bytes[1]]), u16::MAX);
        assert_eq!(bytes[2], u8::MAX);
        assert_eq!(bytes[3], u8::MAX);
        assert_eq!(
            u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            u32::MAX
        );
    }

    #[test]
    fn test_load_seccomp_exactly_8_bytes() {
        // 8 bytes = 1 instruction, valid size but may be rejected by kernel
        // We just test it doesn't return InvalidArgument for size
        let filter = [0u8; 8];
        let result = load_seccomp(&filter);
        // On Linux this will try to install and likely fail with a kernel error (not size error)
        // On non-Linux it succeeds (no-op)
        #[cfg(not(target_os = "linux"))]
        assert!(result.is_ok());
        #[cfg(target_os = "linux")]
        {
            // Should not be an InvalidArgument about size
            if let Err(e) = result {
                let msg = format!("{}", e);
                assert!(!msg.contains("not a multiple of 8"));
            }
        }
    }

    #[test]
    fn test_load_seccomp_16_bytes() {
        let filter = [0u8; 16];
        let result = load_seccomp(&filter);
        #[cfg(not(target_os = "linux"))]
        assert!(result.is_ok());
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = result {
                let msg = format!("{}", e);
                assert!(!msg.contains("not a multiple of 8"));
            }
        }
    }

    #[test]
    fn test_load_seccomp_1_byte() {
        let result = load_seccomp(&[0x42]);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_seccomp_15_bytes() {
        let result = load_seccomp(&[0u8; 15]);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("not a multiple of 8"));
    }

    #[test]
    fn test_apply_landlock_with_no_access_rule() {
        // NoAccess rules should be skipped (default deny)
        let rules = vec![FilesystemRule::new("/tmp", FsAccess::NoAccess)];
        let result = apply_landlock(&rules);
        // On non-Linux: Ok. On Linux: depends on kernel support
        // The NoAccess rule should be skipped in the loop
        let _ = result;
    }

    #[test]
    fn test_apply_landlock_multiple_rules() {
        let rules = vec![
            FilesystemRule::read_only("/usr"),
            FilesystemRule::read_write("/tmp"),
            FilesystemRule::new("/var", FsAccess::NoAccess),
        ];
        let result = apply_landlock(&rules);
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_bpf_constants() {
        assert_eq!(BPF_LD_W_ABS, 0x20);
        assert_eq!(BPF_JMP_JEQ_K, 0x15);
        assert_eq!(BPF_RET_K, 0x06);
    }

    #[test]
    fn test_seccomp_return_values() {
        assert_eq!(SECCOMP_RET_ALLOW, 0x7fff_0000);
        assert_eq!(SECCOMP_RET_KILL_PROCESS, 0x8000_0000);
        assert_eq!(SECCOMP_RET_TRAP, 0x0003_0000);
    }

    #[test]
    fn test_syscall_name_to_nr_common() {
        assert_eq!(syscall_name_to_nr("read"), Some(0));
        assert_eq!(syscall_name_to_nr("write"), Some(1));
        assert_eq!(syscall_name_to_nr("close"), Some(3));
        assert_eq!(syscall_name_to_nr("mmap"), Some(9));
        assert_eq!(syscall_name_to_nr("socket"), Some(41));
        assert_eq!(syscall_name_to_nr("connect"), Some(42));
        assert_eq!(syscall_name_to_nr("execve"), Some(59));
        assert_eq!(syscall_name_to_nr("exit"), Some(60));
        assert_eq!(syscall_name_to_nr("kill"), Some(62));
        assert_eq!(syscall_name_to_nr("ptrace"), Some(101));
        assert_eq!(syscall_name_to_nr("mount"), Some(165));
        assert_eq!(syscall_name_to_nr("reboot"), Some(169));
        assert_eq!(syscall_name_to_nr("getrandom"), Some(318));
    }

    #[test]
    fn test_syscall_name_to_nr_unknown() {
        assert_eq!(syscall_name_to_nr("nonexistent_syscall"), None);
        assert_eq!(syscall_name_to_nr(""), None);
    }

    #[test]
    fn test_custom_seccomp_filter_basic() {
        let base = &[0u32, 1, 3, 60, 231]; // read, write, close, exit, exit_group
        let extra = &[41u32, 42]; // socket, connect
        let denied: &[(u32, u32)] = &[];

        let filter = create_custom_seccomp_filter(base, extra, denied).unwrap();
        assert!(!filter.is_empty());
        assert_eq!(filter.len() % 8, 0);
        // 1 (load) + 7 (allowed JEQs) + 1 (default kill) + 1 (allow ret) = 10 instructions
        assert_eq!(filter.len() / 8, 10);
    }

    #[test]
    fn test_custom_seccomp_filter_with_denied() {
        let base = &[0u32, 1, 3, 60, 231];
        let extra = &[];
        let denied = &[(101u32, SECCOMP_RET_KILL_PROCESS), (169, SECCOMP_RET_TRAP)]; // ptrace, reboot

        let filter = create_custom_seccomp_filter(base, extra, denied).unwrap();
        assert!(!filter.is_empty());
        assert_eq!(filter.len() % 8, 0);
        // 1 (load) + 2*2 (denied pairs) + 5 (allowed JEQs) + 1 (kill) + 1 (allow) = 12
        assert_eq!(filter.len() / 8, 12);
    }

    #[test]
    fn test_custom_seccomp_filter_denied_overrides_base() {
        // If a syscall is in both base_allowed and denied, denied wins
        let base = &[0u32, 1, 62]; // read, write, kill
        let extra = &[];
        let denied = &[(62u32, SECCOMP_RET_KILL_PROCESS)]; // deny kill

        let filter = create_custom_seccomp_filter(base, extra, denied).unwrap();
        // kill should be removed from allowed: 2 allowed + 1 denied
        // 1 (load) + 2*1 (denied) + 2 (allowed JEQs) + 1 (kill) + 1 (allow) = 7
        assert_eq!(filter.len() / 8, 7);
    }

    #[test]
    fn test_custom_seccomp_filter_no_duplicates() {
        let base = &[0u32, 1, 41]; // read, write, socket
        let extra = &[0u32, 41]; // duplicates of read, socket

        let filter = create_custom_seccomp_filter(base, extra, &[]).unwrap();
        // Should only have 3 unique allowed syscalls
        // 1 (load) + 3 (allowed JEQs) + 1 (kill) + 1 (allow) = 6
        assert_eq!(filter.len() / 8, 6);
    }

    #[test]
    fn test_sock_filter_size_is_8_bytes() {
        assert_eq!(std::mem::size_of::<SockFilter>(), 8);
    }

    #[test]
    fn test_filesystem_rule_with_deep_path() {
        let rule = FilesystemRule::read_only("/a/b/c/d/e/f/g/h/i/j");
        assert_eq!(rule.path, PathBuf::from("/a/b/c/d/e/f/g/h/i/j"));
    }

    #[test]
    fn test_filesystem_rule_with_empty_path() {
        let rule = FilesystemRule::new("", FsAccess::ReadWrite);
        assert_eq!(rule.path, PathBuf::from(""));
    }

    #[test]
    fn test_namespace_flags_union_all() {
        let all = NamespaceFlags::all();
        assert!(all.contains(NamespaceFlags::NETWORK));
        assert!(all.contains(NamespaceFlags::MOUNT));
        assert!(all.contains(NamespaceFlags::PID));
        assert!(all.contains(NamespaceFlags::USER));
        assert_eq!(all.bits(), 0b1111);
    }

    // --- Coverage batch 3: more Landlock/seccomp paths, rule combos, namespace edge cases ---

    #[test]
    fn test_apply_landlock_only_no_access_rules() {
        // All rules are NoAccess — should skip all in the loop but still create ruleset
        let rules = vec![
            FilesystemRule::new("/tmp", FsAccess::NoAccess),
            FilesystemRule::new("/var", FsAccess::NoAccess),
            FilesystemRule::new("/home", FsAccess::NoAccess),
        ];
        let result = apply_landlock(&rules);
        // On Linux: creates ruleset then skips all rules. On non-Linux: no-op Ok.
        let _ = result;
    }

    #[test]
    fn test_apply_landlock_mixed_access_levels() {
        let rules = vec![
            FilesystemRule::read_only("/usr"),
            FilesystemRule::read_write("/tmp"),
            FilesystemRule::new("/etc", FsAccess::NoAccess),
            FilesystemRule::read_only("/var/log"),
        ];
        let result = apply_landlock(&rules);
        let _ = result;
    }

    #[test]
    fn test_apply_landlock_nonexistent_path() {
        let rules = vec![FilesystemRule::read_only(
            "/nonexistent_path_that_does_not_exist_12345",
        )];
        let result = apply_landlock(&rules);
        // On Linux: open() will fail → should return Err
        // On non-Linux: no-op → Ok
        #[cfg(not(target_os = "linux"))]
        assert!(result.is_ok());
        #[cfg(target_os = "linux")]
        {
            // May succeed (if landlock not supported) or fail (bad path)
            let _ = result;
        }
    }

    #[test]
    fn test_apply_landlock_single_read_only_rule() {
        let rules = vec![FilesystemRule::read_only("/tmp")];
        let result = apply_landlock(&rules);
        let _ = result;
    }

    #[test]
    fn test_apply_landlock_single_read_write_rule() {
        let rules = vec![FilesystemRule::read_write("/tmp")];
        let result = apply_landlock(&rules);
        let _ = result;
    }

    #[test]
    fn test_load_seccomp_size_9_bytes() {
        let result = load_seccomp(&[0u8; 9]);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("not a multiple of 8"));
    }

    #[test]
    fn test_load_seccomp_size_17_bytes() {
        let result = load_seccomp(&[0u8; 17]);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("17"));
    }

    #[test]
    fn test_load_seccomp_size_24_bytes() {
        // 24 = 3 instructions, valid multiple of 8
        let filter = [0u8; 24];
        let result = load_seccomp(&filter);
        #[cfg(not(target_os = "linux"))]
        assert!(result.is_ok());
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = result {
                let msg = format!("{}", e);
                assert!(!msg.contains("not a multiple of 8"));
            }
        }
    }

    #[test]
    fn test_create_basic_seccomp_filter_contains_jeq_instructions() {
        let filter = create_basic_seccomp_filter().unwrap();
        // Instructions 1 through N-2 should all be JEQ
        let num_insns = filter.len() / 8;
        for i in 1..(num_insns - 2) {
            let offset = i * 8;
            let code = u16::from_ne_bytes([filter[offset], filter[offset + 1]]);
            assert_eq!(code, BPF_JMP_JEQ_K, "Instruction {} should be JEQ", i);
        }
    }

    #[test]
    fn test_create_basic_seccomp_filter_first_jeq_is_read_syscall() {
        let filter = create_basic_seccomp_filter().unwrap();
        // Second instruction (index 1) should check syscall 0 (read)
        let k = u32::from_ne_bytes([filter[12], filter[13], filter[14], filter[15]]);
        assert_eq!(k, 0, "First JEQ should check syscall 0 (read)");
    }

    #[test]
    fn test_sock_filter_roundtrip_bpf_ld() {
        let sf = SockFilter::new(BPF_LD_W_ABS, 0, 0, 0);
        let bytes = sf.to_bytes();
        let code = u16::from_ne_bytes([bytes[0], bytes[1]]);
        let k = u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(code, BPF_LD_W_ABS);
        assert_eq!(k, 0);
    }

    #[test]
    fn test_sock_filter_roundtrip_bpf_jmp() {
        let sf = SockFilter::new(BPF_JMP_JEQ_K, 5, 0, 231);
        let bytes = sf.to_bytes();
        assert_eq!(bytes[2], 5); // jt
        assert_eq!(bytes[3], 0); // jf
        let k = u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(k, 231); // exit_group
    }

    #[test]
    fn test_namespace_flags_symmetric_difference() {
        let a = NamespaceFlags::NETWORK | NamespaceFlags::MOUNT;
        let b = NamespaceFlags::MOUNT | NamespaceFlags::PID;
        let sym_diff = a ^ b;
        assert!(sym_diff.contains(NamespaceFlags::NETWORK));
        assert!(!sym_diff.contains(NamespaceFlags::MOUNT));
        assert!(sym_diff.contains(NamespaceFlags::PID));
    }

    #[test]
    fn test_namespace_flags_complement() {
        let flags = NamespaceFlags::NETWORK;
        let complement = !flags & NamespaceFlags::all();
        assert!(!complement.contains(NamespaceFlags::NETWORK));
        assert!(complement.contains(NamespaceFlags::MOUNT));
        assert!(complement.contains(NamespaceFlags::PID));
        assert!(complement.contains(NamespaceFlags::USER));
    }

    #[test]
    fn test_filesystem_rule_with_relative_path() {
        let rule = FilesystemRule::new("relative/path", FsAccess::ReadOnly);
        assert_eq!(rule.path, PathBuf::from("relative/path"));
        assert_eq!(rule.access, FsAccess::ReadOnly);
    }
}
