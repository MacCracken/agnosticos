//! AGNOS System Interface
//!
//! Provides safe Rust bindings to AGNOS-specific kernel syscalls and features.

pub mod agent;
pub mod audit;
pub mod bootloader;
pub mod certpin;
pub mod dmverity;
pub mod fuse;
pub mod ima;
pub mod journald;
pub mod llm;
pub mod luks;
pub mod mac;
pub mod netns;
pub mod pam;
pub mod secureboot;
pub mod security;
pub mod syscall;
pub mod tpm;
pub mod udev;
pub mod update;

pub use agent::{Agent, AgentContext, AgentRuntime};
pub use error::{Result, SysError};

pub mod error {
    use thiserror::Error;

    pub type Result<T> = std::result::Result<T, SysError>;

    #[derive(Error, Debug)]
    pub enum SysError {
        #[error("System call failed with errno {0}: {1}")]
        SyscallFailed(i32, String),

        #[error("Invalid argument: {0}")]
        InvalidArgument(String),

        #[error("Permission denied")]
        PermissionDenied,

        #[error("Resource temporarily unavailable")]
        WouldBlock,

        #[error("Kernel module not loaded")]
        ModuleNotLoaded,

        #[error("Feature not supported")]
        NotSupported,

        #[error("Unknown error: {0}")]
        Unknown(String),
    }

    impl SysError {
        pub fn from_errno(errno: i32) -> Self {
            match errno {
                libc::EPERM => Self::PermissionDenied,
                libc::EAGAIN => Self::WouldBlock,
                libc::EINVAL => Self::InvalidArgument("invalid argument".into()),
                libc::ENOSYS => Self::NotSupported,
                _ => {
                    Self::SyscallFailed(errno, std::io::Error::from_raw_os_error(errno).to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sys_error_from_errno_eperm() {
        let err = SysError::from_errno(libc::EPERM);
        assert!(matches!(err, SysError::PermissionDenied));
    }

    #[test]
    fn test_sys_error_from_errno_eagain() {
        let err = SysError::from_errno(libc::EAGAIN);
        assert!(matches!(err, SysError::WouldBlock));
    }

    #[test]
    fn test_sys_error_from_errno_ewouldblock() {
        let err = SysError::from_errno(libc::EWOULDBLOCK);
        assert!(matches!(err, SysError::WouldBlock));
    }

    #[test]
    fn test_sys_error_from_errno_einval() {
        let err = SysError::from_errno(libc::EINVAL);
        assert!(matches!(err, SysError::InvalidArgument(_)));
    }

    #[test]
    fn test_sys_error_from_errno_enosys() {
        let err = SysError::from_errno(libc::ENOSYS);
        assert!(matches!(err, SysError::NotSupported));
    }

    #[test]
    fn test_sys_error_from_errno_unknown() {
        let err = SysError::from_errno(999);
        assert!(matches!(err, SysError::SyscallFailed(999, _)));
    }

    #[test]
    fn test_sys_error_display() {
        let err = SysError::PermissionDenied;
        assert!(err.to_string().contains("denied"));

        let err = SysError::ModuleNotLoaded;
        assert!(err.to_string().contains("module"));

        let err = SysError::Unknown("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
