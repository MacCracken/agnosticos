//! Permission management

use crate::security::PermissionLevel;

/// Check if user has permission for an action
pub fn check_permission(
    user: &str,
    required: PermissionLevel,
) -> bool {
    // Root can do anything
    if user == "root" {
        return true;
    }
    
    // Otherwise check based on level
    match required {
        PermissionLevel::Safe | PermissionLevel::ReadOnly => true,
        PermissionLevel::UserWrite => true,
        PermissionLevel::SystemWrite => false, // Requires escalation
        PermissionLevel::Admin => false,        // Requires root
        PermissionLevel::Blocked => false,      // Never allowed
    }
}
