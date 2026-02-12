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
