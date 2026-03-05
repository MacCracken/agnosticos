//! Raw syscall wrappers for AGNOS kernel interfaces

use crate::error::{Result, SysError};

// AGNOS-specific syscall numbers (these will be allocated by kernel)
pub const SYS_AGNOS_AGENT_CREATE: i64 = 500;
pub const SYS_AGNOS_AGENT_TERMINATE: i64 = 501;
pub const SYS_AGNOS_AGENT_SET_LIMITS: i64 = 502;
pub const SYS_AGNOS_AGENT_GET_INFO: i64 = 503;
pub const SYS_AGNOS_LLM_LOAD_MODEL: i64 = 510;
pub const SYS_AGNOS_LLM_INFERENCE: i64 = 511;
pub const SYS_AGNOS_LLM_UNLOAD_MODEL: i64 = 512;
pub const SYS_AGNOS_AUDIT_LOG: i64 = 520;
pub const SYS_AGNOS_AUDIT_READ: i64 = 521;

/// Raw syscall wrapper for single-argument syscalls.
///
/// # Safety
/// - `num` must be a valid syscall number for the current kernel.
/// - `arg1` must be a valid value for the given syscall (e.g., a valid pointer
///   cast to `i64` where the syscall expects a pointer).
/// - Caller is responsible for ensuring memory safety of any pointers passed.
#[inline(always)]
pub unsafe fn syscall1(num: i64, arg1: i64) -> i64 {
    libc::syscall(num, arg1)
}

/// Raw syscall wrapper for two-argument syscalls.
///
/// # Safety
/// - `num` must be a valid syscall number for the current kernel.
/// - `arg1` and `arg2` must be valid values for the given syscall.
/// - Caller is responsible for ensuring memory safety of any pointers passed.
#[inline(always)]
pub unsafe fn syscall2(num: i64, arg1: i64, arg2: i64) -> i64 {
    libc::syscall(num, arg1, arg2)
}

/// Raw syscall wrapper for three-argument syscalls.
///
/// # Safety
/// - `num` must be a valid syscall number for the current kernel.
/// - `arg1`, `arg2`, and `arg3` must be valid values for the given syscall.
/// - Caller is responsible for ensuring memory safety of any pointers passed.
#[inline(always)]
pub unsafe fn syscall3(num: i64, arg1: i64, arg2: i64, arg3: i64) -> i64 {
    libc::syscall(num, arg1, arg2, arg3)
}

/// Check if AGNOS kernel modules are available
pub fn kernel_modules_available() -> bool {
    unsafe {
        let result = syscall1(SYS_AGNOS_AGENT_GET_INFO, 0);
        result >= 0 || errno() != libc::ENOSYS
    }
}

/// Read the current thread-local errno value.
///
/// # Safety rationale
/// `libc::__errno_location()` returns a pointer to thread-local storage,
/// which is always valid for the current thread. The dereference is safe
/// because the pointer is guaranteed non-null and properly aligned by libc.
fn errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

/// Convert syscall result to Result.
///
/// Linux `libc::syscall` returns -1 on error and sets `errno` separately.
/// Previous code incorrectly negated the return value; we now read `errno()`
/// which contains the actual error code set by the kernel.
fn check_result(result: i64) -> Result<i64> {
    if result < 0 {
        Err(SysError::from_errno(errno()))
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_constants() {
        assert_eq!(SYS_AGNOS_AGENT_CREATE, 500);
        assert_eq!(SYS_AGNOS_AGENT_TERMINATE, 501);
        assert_eq!(SYS_AGNOS_AGENT_SET_LIMITS, 502);
        assert_eq!(SYS_AGNOS_AGENT_GET_INFO, 503);
        assert_eq!(SYS_AGNOS_LLM_LOAD_MODEL, 510);
        assert_eq!(SYS_AGNOS_LLM_INFERENCE, 511);
        assert_eq!(SYS_AGNOS_LLM_UNLOAD_MODEL, 512);
        assert_eq!(SYS_AGNOS_AUDIT_LOG, 520);
        assert_eq!(SYS_AGNOS_AUDIT_READ, 521);
    }

    #[test]
    fn test_check_result_success() {
        let result = check_result(0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_check_result_error() {
        let result = check_result(-1);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_result_positive() {
        let result = check_result(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_check_result_large_negative() {
        let result = check_result(-100);
        assert!(result.is_err());
    }

    #[test]
    fn test_kernel_modules_available() {
        // On a non-AGNOS kernel this returns false (ENOSYS)
        let available = kernel_modules_available();
        // Just verify it doesn't panic
        let _ = available;
    }

    #[test]
    fn test_errno_readable() {
        // errno() should return a valid value without panicking
        let e = errno();
        // errno is always >= 0
        let _ = e;
    }

    #[test]
    fn test_syscall1_invalid_number() {
        // Calling a non-existent syscall should return -1 and set ENOSYS
        let result = unsafe { syscall1(9999, 0) };
        assert!(result < 0);
    }

    #[test]
    fn test_syscall2_invalid_number() {
        let result = unsafe { syscall2(9999, 0, 0) };
        assert!(result < 0);
    }

    #[test]
    fn test_syscall3_invalid_number() {
        let result = unsafe { syscall3(9999, 0, 0, 0) };
        assert!(result < 0);
    }
}
