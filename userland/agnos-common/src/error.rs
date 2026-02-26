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
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            AgnosError::Timeout | AgnosError::Io(_) | AgnosError::KernelError(_)
        )
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
    fn test_error_io_is_retriable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = AgnosError::Io(io_err);
        assert!(err.is_retriable());
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
}
