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

/// Raw syscall wrapper
#[inline(always)]
pub unsafe fn syscall1(num: i64, arg1: i64) -> i64 {
    libc::syscall(num, arg1)
}

#[inline(always)]
pub unsafe fn syscall2(num: i64, arg1: i64, arg2: i64) -> i64 {
    libc::syscall(num, arg1, arg2)
}

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

fn errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

/// Convert syscall result to Result
fn check_result(result: i64) -> Result<i64> {
    if result < 0 {
        Err(SysError::from_errno(-result as i32))
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
}
