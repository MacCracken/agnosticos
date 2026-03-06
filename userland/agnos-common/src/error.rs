//! Error types for AGNOS

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AgnosError>;

#[derive(Error, Debug)]
pub enum AgnosError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Sandbox violation: {0}")]
    SandboxViolation(String),

    #[error("LLM error: {0}")]
    LlmError(String),

    #[error("Audit error: {0}")]
    AuditError(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Kernel error: {0}")]
    KernelError(i32),

    #[error("System call failed: {0}")]
    SyscallFailed(String),

    #[error("Timeout")]
    Timeout,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl AgnosError {
    /// Returns `true` for transient errors that are safe to retry.
    ///
    /// Note: not all `Io` errors are retriable — permanent errors like
    /// `PermissionDenied` or `NotFound` are excluded.
    pub fn is_retriable(&self) -> bool {
        match self {
            AgnosError::Timeout => true,
            AgnosError::KernelError(_) => true,
            AgnosError::Io(e) => matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_agent_not_found() {
        let err = AgnosError::AgentNotFound("test-agent".to_string());
        assert!(err.to_string().contains("test-agent"));
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_permission_denied() {
        let err = AgnosError::PermissionDenied("access denied".to_string());
        assert!(err.to_string().contains("access denied"));
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_sandbox_violation() {
        let err = AgnosError::SandboxViolation("file access blocked".to_string());
        assert!(err.to_string().contains("file access blocked"));
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_llm() {
        let err = AgnosError::LlmError("model failed".to_string());
        assert!(err.to_string().contains("model failed"));
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_resource_limit() {
        let err = AgnosError::ResourceLimitExceeded("memory".to_string());
        assert!(err.to_string().contains("memory"));
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_timeout_is_retriable() {
        let err = AgnosError::Timeout;
        assert!(err.is_retriable());
        assert!(err.to_string().contains("Timeout"));
    }

    #[test]
    fn test_error_kernel_is_retriable() {
        let err = AgnosError::KernelError(1);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_transient_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_permanent_is_not_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = AgnosError::Io(io_err);
        assert!(!err.is_retriable());

        let io_err2 = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err2 = AgnosError::Io(io_err2);
        assert!(!err2.is_retriable());
    }

    #[test]
    fn test_error_audit() {
        let err = AgnosError::AuditError("log write failed".to_string());
        assert!(err.to_string().contains("log write failed"));
    }

    #[test]
    fn test_error_invalid_config() {
        let err = AgnosError::InvalidConfig("missing field".to_string());
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn test_error_syscall_failed() {
        let err = AgnosError::SyscallFailed("permission denied".to_string());
        assert!(err.to_string().contains("permission denied"));
    }

    #[test]
    fn test_error_unknown() {
        let err = AgnosError::Unknown("unexpected error".to_string());
        assert!(err.to_string().contains("unexpected error"));
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_from_io_error() {
        let io_err = std::io::Error::other("disk failure");
        let err: AgnosError = io_err.into();
        assert!(matches!(err, AgnosError::Io(_)));
        assert!(err.to_string().contains("disk failure"));
    }

    #[test]
    fn test_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("{{bad json").unwrap_err();
        let err: AgnosError = json_err.into();
        assert!(matches!(err, AgnosError::Serialization(_)));
        assert!(err.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_error_io_would_block_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::WouldBlock, "would block");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_interrupted_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Interrupted, "interrupted");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_connection_reset_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_connection_aborted_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "aborted");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_broken_pipe_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_io_already_exists_not_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "exists");
        let err = AgnosError::Io(io_err);
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_kernel_display() {
        let err = AgnosError::KernelError(42);
        assert_eq!(err.to_string(), "Kernel error: 42");
        assert!(err.is_retriable());
    }

    #[test]
    fn test_error_kernel_negative() {
        let err = AgnosError::KernelError(-1);
        assert_eq!(err.to_string(), "Kernel error: -1");
    }

    #[test]
    fn test_error_serialization_not_retriable() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AgnosError::Serialization(json_err);
        assert!(!err.is_retriable());
    }

    #[test]
    fn test_error_debug_impl() {
        let err = AgnosError::Timeout;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Timeout"));
    }

    #[test]
    fn test_error_display_all_variants() {
        let variants: Vec<AgnosError> = vec![
            AgnosError::Io(std::io::Error::other("io")),
            AgnosError::AgentNotFound("a".into()),
            AgnosError::PermissionDenied("p".into()),
            AgnosError::ResourceLimitExceeded("r".into()),
            AgnosError::SandboxViolation("s".into()),
            AgnosError::LlmError("l".into()),
            AgnosError::AuditError("au".into()),
            AgnosError::InvalidConfig("c".into()),
            AgnosError::KernelError(0),
            AgnosError::SyscallFailed("sc".into()),
            AgnosError::Timeout,
            AgnosError::Unknown("u".into()),
        ];
        for err in &variants {
            let display = err.to_string();
            assert!(!display.is_empty(), "Display should not be empty for {:?}", err);
        }
    }

    #[test]
    fn test_error_is_std_error() {
        let err = AgnosError::Timeout;
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.to_string().contains("Timeout"));
    }
}
