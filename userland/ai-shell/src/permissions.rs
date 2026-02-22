//! Permission management

use crate::security::PermissionLevel;

/// Check if user has permission for an action
pub fn check_permission(user: &str, required: PermissionLevel) -> bool {
    // Root can do anything
    if user == "root" {
        return true;
    }

    // Otherwise check based on level
    match required {
        PermissionLevel::Safe | PermissionLevel::ReadOnly => true,
        PermissionLevel::UserWrite => true,
        PermissionLevel::SystemWrite => false, // Requires escalation
        PermissionLevel::Admin => false,       // Requires root
        PermissionLevel::Blocked => false,     // Never allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_permission_root_safe() {
        assert!(check_permission("root", PermissionLevel::Safe));
    }

    #[test]
    fn test_check_permission_root_admin() {
        assert!(check_permission("root", PermissionLevel::Admin));
    }

    #[test]
    fn test_check_permission_root_blocked() {
        assert!(check_permission("root", PermissionLevel::Blocked));
    }

    #[test]
    fn test_check_permission_user_safe() {
        assert!(check_permission("user", PermissionLevel::Safe));
    }

    #[test]
    fn test_check_permission_user_read_only() {
        assert!(check_permission("user", PermissionLevel::ReadOnly));
    }

    #[test]
    fn test_check_permission_user_write() {
        assert!(check_permission("user", PermissionLevel::UserWrite));
    }

    #[test]
    fn test_check_permission_user_system_write() {
        assert!(!check_permission("user", PermissionLevel::SystemWrite));
    }

    #[test]
    fn test_check_permission_user_admin() {
        assert!(!check_permission("user", PermissionLevel::Admin));
    }

    #[test]
    fn test_check_permission_user_blocked() {
        assert!(!check_permission("user", PermissionLevel::Blocked));
    }

    #[test]
    fn test_check_permission_arbitrary_user_safe() {
        assert!(check_permission("alice", PermissionLevel::Safe));
        assert!(check_permission("bob", PermissionLevel::ReadOnly));
    }
}
