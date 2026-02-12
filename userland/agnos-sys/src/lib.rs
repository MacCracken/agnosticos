//! AGNOS System Interface
//!
//! Provides safe Rust bindings to AGNOS-specific kernel syscalls and features.

pub mod agent;
pub mod llm;
pub mod security;
pub mod syscall;

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
                libc::EAGAIN | libc::EWOULDBLOCK => Self::WouldBlock,
                libc::EINVAL => Self::InvalidArgument("invalid argument".into()),
                libc::ENOSYS => Self::NotSupported,
                _ => Self::SyscallFailed(errno, std::io::Error::from_raw_os_error(errno).to_string()),
            }
        }
    }
}
