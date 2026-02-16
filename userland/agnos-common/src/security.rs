//! Security-related types and structures

use serde::{Deserialize, Serialize};

/// Security context for a process/agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    pub user_id: String,
    pub group_id: String,
    pub capabilities: Vec<Capability>,
    pub selinux_context: String,
    pub landlock_ruleset_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    DacReadSearch,
    DacOverride,
    Fowner,
    Fsetid,
    Kill,
    Setgid,
    Setuid,
    Setpcap,
    LinuxImmutable,
    NetBindService,
    NetBroadcast,
    NetAdmin,
    NetRaw,
    IpcLock,
    IpcOwner,
    SysModule,
    SysRawio,
    SysChroot,
    SysPtrace,
    SysPacct,
    SysAdmin,
    SysBoot,
    SysNice,
    SysResource,
    SysTime,
    SysTtyConfig,
    Mknod,
    Lease,
    AuditWrite,
    AuditControl,
    Setfcap,
}

/// Security policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub name: String,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub subject: String,
    pub action: String,
    pub resource: String,
    pub effect: PolicyEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyEffect {
    Allow,
    Deny,
}

/// Cryptographic key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    pub id: String,
    pub key_type: KeyType,
    pub algorithm: String,
    pub key_size_bits: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    Symmetric,
    AsymmetricPublic,
    AsymmetricPrivate,
    Hmac,
}

/// Authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub user_id: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scopes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_context() {
        let ctx = SecurityContext {
            user_id: "1000".to_string(),
            group_id: "1000".to_string(),
            capabilities: vec![Capability::DacReadSearch, Capability::NetBindService],
            selinux_context: "unconfined_t".to_string(),
            landlock_ruleset_id: Some(1),
        };
        assert_eq!(ctx.user_id, "1000");
        assert!(ctx.capabilities.len() == 2);
    }

    #[test]
    fn test_capability_variants() {
        assert!(matches!(Capability::SysAdmin, Capability::SysAdmin));
        assert!(matches!(Capability::NetAdmin, Capability::NetAdmin));
        assert!(matches!(Capability::AuditWrite, Capability::AuditWrite));
    }

    #[test]
    fn test_security_policy() {
        let policy = SecurityPolicy {
            name: "agent-policy".to_string(),
            rules: vec![
                PolicyRule {
                    subject: "agent-1".to_string(),
                    action: "read".to_string(),
                    resource: "/home/*".to_string(),
                    effect: PolicyEffect::Allow,
                },
                PolicyRule {
                    subject: "agent-1".to_string(),
                    action: "write".to_string(),
                    resource: "/etc/*".to_string(),
                    effect: PolicyEffect::Deny,
                },
            ],
        };
        assert_eq!(policy.rules.len(), 2);
        assert!(matches!(policy.rules[0].effect, PolicyEffect::Allow));
    }

    #[test]
    fn test_policy_effect() {
        assert!(matches!(PolicyEffect::Allow, PolicyEffect::Allow));
        assert!(matches!(PolicyEffect::Deny, PolicyEffect::Deny));
    }

    #[test]
    fn test_key_info() {
        let key = KeyInfo {
            id: "key-123".to_string(),
            key_type: KeyType::Symmetric,
            algorithm: "AES-256-GCM".to_string(),
            key_size_bits: 256,
            created_at: chrono::Utc::now(),
            expires_at: None,
        };
        assert_eq!(key.key_size_bits, 256);
        assert!(key.expires_at.is_none());
    }

    #[test]
    fn test_key_type_variants() {
        assert!(matches!(KeyType::Symmetric, KeyType::Symmetric));
        assert!(matches!(
            KeyType::AsymmetricPublic,
            KeyType::AsymmetricPublic
        ));
        assert!(matches!(KeyType::Hmac, KeyType::Hmac));
    }

    #[test]
    fn test_auth_token() {
        let now = chrono::Utc::now();
        let token = AuthToken {
            token: "abc123".to_string(),
            user_id: "user-1".to_string(),
            issued_at: now,
            expires_at: now + chrono::Duration::hours(1),
            scopes: vec!["read".to_string(), "write".to_string()],
        };
        assert_eq!(token.scopes.len(), 2);
    }
}
