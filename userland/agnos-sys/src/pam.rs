//! PAM Authentication and User/Session Management
//!
//! Provides PAM service configuration, user management (via `/etc/passwd` and
//! standard tools like `useradd`/`userdel`/`usermod`), and session listing
//! (via `who`/`loginctl`).
//!
//! On non-Linux platforms, all operations return `SysError::NotSupported`.

use crate::error::{Result, SysError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// PAM service identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PamService {
    Login,
    Sudo,
    Sshd,
    AgnosAgent,
    Custom(String),
}

impl PamService {
    /// Returns the service name string used in `/etc/pam.d/`.
    pub fn service_name(&self) -> &str {
        match self {
            PamService::Login => "login",
            PamService::Sudo => "sudo",
            PamService::Sshd => "sshd",
            PamService::AgnosAgent => "agnos-agent",
            PamService::Custom(name) => name.as_str(),
        }
    }
}

impl fmt::Display for PamService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.service_name())
    }
}

/// Result of a PAM authentication attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthResult {
    Success,
    Denied(String),
    AccountExpired,
    PasswordExpired,
    SessionError(String),
    Unknown(i32),
}

impl fmt::Display for AuthResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthResult::Success => write!(f, "Authentication successful"),
            AuthResult::Denied(reason) => write!(f, "Authentication denied: {}", reason),
            AuthResult::AccountExpired => write!(f, "Account expired"),
            AuthResult::PasswordExpired => write!(f, "Password expired"),
            AuthResult::SessionError(msg) => write!(f, "Session error: {}", msg),
            AuthResult::Unknown(code) => write!(f, "Unknown PAM result code: {}", code),
        }
    }
}

/// Information about a system user (parsed from `/etc/passwd` and groups).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub home_dir: PathBuf,
    pub shell: String,
    pub groups: Vec<String>,
    pub is_system_user: bool,
}

impl UserInfo {
    /// Heuristic: UIDs below 1000 (or 500 on some distros) are system users.
    fn detect_system_user(uid: u32) -> bool {
        uid < 1000
    }
}

/// Information about an active login session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub user: String,
    pub login_time: String,
    pub tty: Option<String>,
    pub remote_host: Option<String>,
    pub pid: u32,
}

/// PAM configuration for a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PamConfig {
    pub service: PamService,
    pub rules: Vec<PamRule>,
}

/// A single PAM rule line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PamRule {
    pub rule_type: PamRuleType,
    pub control: PamControl,
    pub module: String,
    pub args: Vec<String>,
}

impl fmt::Display for PamRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let line = render_pam_rule(self);
        write!(f, "{}", line)
    }
}

/// PAM rule type (which stack the rule belongs to).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PamRuleType {
    Auth,
    Account,
    Session,
    Password,
}

impl PamRuleType {
    pub fn as_str(&self) -> &str {
        match self {
            PamRuleType::Auth => "auth",
            PamRuleType::Account => "account",
            PamRuleType::Session => "session",
            PamRuleType::Password => "password",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auth" => Ok(PamRuleType::Auth),
            "account" => Ok(PamRuleType::Account),
            "session" => Ok(PamRuleType::Session),
            "password" => Ok(PamRuleType::Password),
            _ => Err(SysError::InvalidArgument(format!(
                "Unknown PAM rule type: {}",
                s
            ))),
        }
    }
}

impl fmt::Display for PamRuleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// PAM control flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PamControl {
    Required,
    Requisite,
    Sufficient,
    Optional,
    Include,
}

impl PamControl {
    pub fn as_str(&self) -> &str {
        match self {
            PamControl::Required => "required",
            PamControl::Requisite => "requisite",
            PamControl::Sufficient => "sufficient",
            PamControl::Optional => "optional",
            PamControl::Include => "include",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "required" => Ok(PamControl::Required),
            "requisite" => Ok(PamControl::Requisite),
            "sufficient" => Ok(PamControl::Sufficient),
            "optional" => Ok(PamControl::Optional),
            "include" => Ok(PamControl::Include),
            _ => Err(SysError::InvalidArgument(format!(
                "Unknown PAM control flag: {}",
                s
            ))),
        }
    }
}

impl fmt::Display for PamControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Username validation
// ---------------------------------------------------------------------------

/// Validate a UNIX username.
///
/// Rules: max 32 chars, must start with a lowercase letter or underscore,
/// may contain lowercase letters, digits, hyphens, underscores; no shell
/// metacharacters.
pub fn validate_username(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(SysError::InvalidArgument(
            "Username cannot be empty".into(),
        ));
    }
    if name.len() > 32 {
        return Err(SysError::InvalidArgument(format!(
            "Username too long ({} chars, max 32)",
            name.len()
        )));
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_lowercase() && first != '_' {
        return Err(SysError::InvalidArgument(format!(
            "Username must start with a lowercase letter or underscore, got '{}'",
            first
        )));
    }
    for ch in name.chars() {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_') {
            return Err(SysError::InvalidArgument(format!(
                "Username contains invalid character: '{}'",
                ch
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Passwd parsing (pure)
// ---------------------------------------------------------------------------

/// Parse a single `/etc/passwd` line into a [`UserInfo`].
///
/// Format: `username:x:uid:gid:gecos:home:shell`
pub fn parse_passwd_line(line: &str) -> Result<UserInfo> {
    let fields: Vec<&str> = line.split(':').collect();
    if fields.len() < 7 {
        return Err(SysError::InvalidArgument(format!(
            "Malformed passwd line (expected 7 fields, got {}): {}",
            fields.len(),
            line
        )));
    }
    let username = fields[0].to_string();
    let uid: u32 = fields[2].parse().map_err(|_| {
        SysError::InvalidArgument(format!("Invalid UID '{}' in passwd line", fields[2]))
    })?;
    let gid: u32 = fields[3].parse().map_err(|_| {
        SysError::InvalidArgument(format!("Invalid GID '{}' in passwd line", fields[3]))
    })?;
    let home_dir = PathBuf::from(fields[5]);
    let shell = fields[6].to_string();

    Ok(UserInfo {
        username,
        uid,
        gid,
        home_dir,
        shell,
        groups: Vec::new(), // populated separately
        is_system_user: UserInfo::detect_system_user(uid),
    })
}

// ---------------------------------------------------------------------------
// Who output parsing (pure)
// ---------------------------------------------------------------------------

/// Parse the output of the `who` command into a list of [`SessionInfo`].
///
/// Typical `who` output lines:
/// ```text
/// user1    pts/0        2026-03-06 10:30 (192.168.1.5)
/// user2    tty1         2026-03-06 09:15
/// ```
pub fn parse_who_output(output: &str) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    for (idx, line) in output.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let user = parts[0].to_string();
        let tty = parts[1].to_string();

        // Date + time may be in columns 2+3
        let login_time = if parts.len() >= 4 {
            format!("{} {}", parts[2], parts[3])
        } else {
            parts[2].to_string()
        };

        // Remote host is in parens at the end, e.g. "(192.168.1.5)"
        let remote_host = parts.last().and_then(|p| {
            if p.starts_with('(') && p.ends_with(')') {
                Some(p[1..p.len() - 1].to_string())
            } else {
                None
            }
        });

        sessions.push(SessionInfo {
            session_id: format!("who-{}", idx),
            user,
            login_time,
            tty: Some(tty),
            remote_host,
            pid: 0, // `who` does not report PID by default
        });
    }
    sessions
}

// ---------------------------------------------------------------------------
// PAM config parsing / rendering (pure)
// ---------------------------------------------------------------------------

/// Parse PAM config file content into a list of [`PamRule`]s.
///
/// Each non-blank, non-comment line has the format:
/// ```text
/// type  control  module  [args...]
/// ```
pub fn parse_pam_config(content: &str) -> Result<Vec<PamRule>> {
    let mut rules = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(SysError::InvalidArgument(format!(
                "Malformed PAM rule (need at least type, control, module): {}",
                line
            )));
        }
        let rule_type = PamRuleType::from_str(parts[0])?;
        let control = PamControl::from_str(parts[1])?;
        let module = parts[2].to_string();
        let args: Vec<String> = parts[3..].iter().map(|s| s.to_string()).collect();
        rules.push(PamRule {
            rule_type,
            control,
            module,
            args,
        });
    }
    Ok(rules)
}

/// Render a single PAM rule to its config-file representation.
fn render_pam_rule(rule: &PamRule) -> String {
    let mut line = format!(
        "{}\t{}\t{}",
        rule.rule_type.as_str(),
        rule.control.as_str(),
        rule.module
    );
    for arg in &rule.args {
        line.push('\t');
        line.push_str(arg);
    }
    line
}

/// Render a list of PAM rules to a full config file string.
pub fn render_pam_config(rules: &[PamRule]) -> String {
    let mut out = String::new();
    out.push_str("# Generated by AGNOS PAM manager\n");
    for rule in rules {
        out.push_str(&render_pam_rule(rule));
        out.push('\n');
    }
    out
}

/// Validate a PAM rule: check module path looks reasonable and args have no
/// shell metacharacters.
pub fn validate_pam_rule(rule: &PamRule) -> Result<()> {
    if rule.module.is_empty() {
        return Err(SysError::InvalidArgument(
            "PAM module path cannot be empty".into(),
        ));
    }
    // Module must be a bare name (e.g. pam_unix.so) or an absolute path
    if rule.module.contains("..") {
        return Err(SysError::InvalidArgument(format!(
            "PAM module path must not contain '..': {}",
            rule.module
        )));
    }
    // Reject shell metacharacters in module name
    let dangerous = ['|', ';', '&', '$', '`', '>', '<', '!', '{', '}'];
    for ch in dangerous {
        if rule.module.contains(ch) {
            return Err(SysError::InvalidArgument(format!(
                "PAM module path contains dangerous character '{}': {}",
                ch, rule.module
            )));
        }
    }
    // Same check for arguments
    for arg in &rule.args {
        for ch in dangerous {
            if arg.contains(ch) {
                return Err(SysError::InvalidArgument(format!(
                    "PAM argument contains dangerous character '{}': {}",
                    ch, arg
                )));
            }
        }
    }
    Ok(())
}

/// Get the filesystem path for a PAM service config file.
pub fn get_pam_service_path(service: &PamService) -> PathBuf {
    PathBuf::from(format!("/etc/pam.d/{}", service.service_name()))
}

// ---------------------------------------------------------------------------
// System-interacting functions (Linux only)
// ---------------------------------------------------------------------------

/// Retrieve information about a user by name.
///
/// Reads `/etc/passwd` for basic fields, then runs `id -Gn <username>` to
/// resolve group memberships.
#[cfg(target_os = "linux")]
pub fn get_user_info(username: &str) -> Result<UserInfo> {
    validate_username(username)?;

    let passwd_content = std::fs::read_to_string("/etc/passwd").map_err(|e| {
        SysError::Unknown(format!("Failed to read /etc/passwd: {}", e))
    })?;

    let mut info = passwd_content
        .lines()
        .find(|line| {
            line.split(':').next() == Some(username)
        })
        .ok_or_else(|| {
            SysError::InvalidArgument(format!("User not found: {}", username))
        })
        .and_then(parse_passwd_line)?;

    // Resolve groups via `id`
    let output = std::process::Command::new("id")
        .args(["-Gn", username])
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run id: {}", e)))?;

    if output.status.success() {
        let groups_str = String::from_utf8_lossy(&output.stdout);
        info.groups = groups_str
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
    }

    Ok(info)
}

#[cfg(not(target_os = "linux"))]
pub fn get_user_info(_username: &str) -> Result<UserInfo> {
    Err(SysError::NotSupported)
}

/// List all users from `/etc/passwd`.
#[cfg(target_os = "linux")]
pub fn list_users() -> Result<Vec<UserInfo>> {
    let passwd_content = std::fs::read_to_string("/etc/passwd").map_err(|e| {
        SysError::Unknown(format!("Failed to read /etc/passwd: {}", e))
    })?;

    let mut users = Vec::new();
    for line in passwd_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_passwd_line(line) {
            Ok(info) => users.push(info),
            Err(_) => continue, // skip malformed lines
        }
    }
    Ok(users)
}

#[cfg(not(target_os = "linux"))]
pub fn list_users() -> Result<Vec<UserInfo>> {
    Err(SysError::NotSupported)
}

/// List active login sessions by parsing `who` output.
#[cfg(target_os = "linux")]
pub fn list_sessions() -> Result<Vec<SessionInfo>> {
    let output = std::process::Command::new("who")
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run who: {}", e)))?;

    if !output.status.success() {
        return Err(SysError::Unknown(format!(
            "who exited with status {}",
            output.status
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_who_output(&stdout))
}

#[cfg(not(target_os = "linux"))]
pub fn list_sessions() -> Result<Vec<SessionInfo>> {
    Err(SysError::NotSupported)
}

/// Create a system user with `useradd --system`.
#[cfg(target_os = "linux")]
pub fn create_system_user(username: &str, home_dir: Option<&Path>) -> Result<()> {
    validate_username(username)?;

    let mut cmd = std::process::Command::new("useradd");
    cmd.arg("--system");
    if let Some(home) = home_dir {
        cmd.args(["--home-dir", &home.to_string_lossy()]);
        cmd.arg("--create-home");
    }
    cmd.arg(username);

    let output = cmd
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run useradd: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "useradd failed (exit {}): {}",
            output.status, stderr
        )));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn create_system_user(_username: &str, _home_dir: Option<&Path>) -> Result<()> {
    Err(SysError::NotSupported)
}

/// Delete a user with `userdel`.
#[cfg(target_os = "linux")]
pub fn delete_user(username: &str) -> Result<()> {
    validate_username(username)?;

    let output = std::process::Command::new("userdel")
        .arg(username)
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run userdel: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "userdel failed (exit {}): {}",
            output.status, stderr
        )));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn delete_user(_username: &str) -> Result<()> {
    Err(SysError::NotSupported)
}

/// Add a user to a group with `usermod -aG`.
#[cfg(target_os = "linux")]
pub fn add_user_to_group(username: &str, group: &str) -> Result<()> {
    validate_username(username)?;
    // Validate group name with same rules
    validate_username(group).map_err(|_| {
        SysError::InvalidArgument(format!("Invalid group name: {}", group))
    })?;

    let output = std::process::Command::new("usermod")
        .args(["-aG", group, username])
        .output()
        .map_err(|e| SysError::Unknown(format!("Failed to run usermod: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SysError::Unknown(format!(
            "usermod failed (exit {}): {}",
            output.status, stderr
        )));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn add_user_to_group(_username: &str, _group: &str) -> Result<()> {
    Err(SysError::NotSupported)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Username validation ------------------------------------------------

    #[test]
    fn test_validate_username_valid() {
        assert!(validate_username("agnos").is_ok());
        assert!(validate_username("_system").is_ok());
        assert!(validate_username("user-01").is_ok());
        assert!(validate_username("a").is_ok());
        assert!(validate_username("abc_def-123").is_ok());
    }

    #[test]
    fn test_validate_username_empty() {
        let err = validate_username("").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_validate_username_too_long() {
        let long = "a".repeat(33);
        let err = validate_username(&long).unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_validate_username_starts_with_digit() {
        let err = validate_username("1user").unwrap_err();
        assert!(err.to_string().contains("must start with"));
    }

    #[test]
    fn test_validate_username_uppercase() {
        let err = validate_username("Root").unwrap_err();
        assert!(err.to_string().contains("must start with"));
    }

    #[test]
    fn test_validate_username_shell_metachar() {
        assert!(validate_username("user;rm").is_err());
        assert!(validate_username("user$HOME").is_err());
        assert!(validate_username("user`id`").is_err());
    }

    #[test]
    fn test_validate_username_max_length_exact() {
        let name = "a".repeat(32);
        assert!(validate_username(&name).is_ok());
    }

    // -- Passwd parsing -----------------------------------------------------

    #[test]
    fn test_parse_passwd_line_root() {
        let line = "root:x:0:0:root:/root:/bin/bash";
        let info = parse_passwd_line(line).unwrap();
        assert_eq!(info.username, "root");
        assert_eq!(info.uid, 0);
        assert_eq!(info.gid, 0);
        assert_eq!(info.home_dir, PathBuf::from("/root"));
        assert_eq!(info.shell, "/bin/bash");
        assert!(info.is_system_user);
        assert!(info.groups.is_empty());
    }

    #[test]
    fn test_parse_passwd_line_regular_user() {
        let line = "agnos:x:1000:1000:AGNOS User:/home/agnos:/bin/zsh";
        let info = parse_passwd_line(line).unwrap();
        assert_eq!(info.username, "agnos");
        assert_eq!(info.uid, 1000);
        assert_eq!(info.gid, 1000);
        assert_eq!(info.home_dir, PathBuf::from("/home/agnos"));
        assert_eq!(info.shell, "/bin/zsh");
        assert!(!info.is_system_user);
    }

    #[test]
    fn test_parse_passwd_line_system_user() {
        let line = "daemon:x:2:2:daemon:/usr/sbin:/usr/sbin/nologin";
        let info = parse_passwd_line(line).unwrap();
        assert!(info.is_system_user);
    }

    #[test]
    fn test_parse_passwd_line_malformed_too_few_fields() {
        let line = "badline:x:1000";
        let err = parse_passwd_line(line).unwrap_err();
        assert!(err.to_string().contains("Malformed"));
    }

    #[test]
    fn test_parse_passwd_line_invalid_uid() {
        let line = "user:x:notanumber:1000::/home/user:/bin/sh";
        let err = parse_passwd_line(line).unwrap_err();
        assert!(err.to_string().contains("Invalid UID"));
    }

    #[test]
    fn test_parse_passwd_line_invalid_gid() {
        let line = "user:x:1000:bad::/home/user:/bin/sh";
        let err = parse_passwd_line(line).unwrap_err();
        assert!(err.to_string().contains("Invalid GID"));
    }

    // -- Who output parsing -------------------------------------------------

    #[test]
    fn test_parse_who_output_basic() {
        let output = "alice    pts/0        2026-03-06 10:30 (192.168.1.5)\nbob      tty1         2026-03-06 09:15\n";
        let sessions = parse_who_output(output);
        assert_eq!(sessions.len(), 2);

        assert_eq!(sessions[0].user, "alice");
        assert_eq!(sessions[0].tty, Some("pts/0".to_string()));
        assert_eq!(sessions[0].login_time, "2026-03-06 10:30");
        assert_eq!(
            sessions[0].remote_host,
            Some("192.168.1.5".to_string())
        );

        assert_eq!(sessions[1].user, "bob");
        assert_eq!(sessions[1].tty, Some("tty1".to_string()));
        assert!(sessions[1].remote_host.is_none());
    }

    #[test]
    fn test_parse_who_output_empty() {
        let sessions = parse_who_output("");
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_parse_who_output_blank_lines() {
        let output = "\n   \n\n";
        let sessions = parse_who_output(output);
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_parse_who_session_ids_are_unique() {
        let output = "a pts/0 2026-01-01 00:00\nb pts/1 2026-01-01 00:01\n";
        let sessions = parse_who_output(output);
        assert_ne!(sessions[0].session_id, sessions[1].session_id);
    }

    // -- PAM config parsing / rendering -------------------------------------

    #[test]
    fn test_parse_pam_config_basic() {
        let content = "# PAM config\nauth\trequired\tpam_unix.so\naccount\tsufficient\tpam_permit.so\n";
        let rules = parse_pam_config(content).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].rule_type, PamRuleType::Auth);
        assert_eq!(rules[0].control, PamControl::Required);
        assert_eq!(rules[0].module, "pam_unix.so");
        assert!(rules[0].args.is_empty());
        assert_eq!(rules[1].rule_type, PamRuleType::Account);
        assert_eq!(rules[1].control, PamControl::Sufficient);
    }

    #[test]
    fn test_parse_pam_config_with_args() {
        let content = "session\toptional\tpam_limits.so\tconf=/etc/limits.conf\n";
        let rules = parse_pam_config(content).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].args, vec!["conf=/etc/limits.conf"]);
    }

    #[test]
    fn test_parse_pam_config_skips_comments_and_blanks() {
        let content = "# comment\n\n   \n# another comment\nauth required pam_deny.so\n";
        let rules = parse_pam_config(content).unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_parse_pam_config_malformed_line() {
        let content = "auth required\n"; // missing module
        let err = parse_pam_config(content).unwrap_err();
        assert!(err.to_string().contains("Malformed"));
    }

    #[test]
    fn test_parse_pam_config_unknown_type() {
        let content = "badtype required pam_unix.so\n";
        let err = parse_pam_config(content).unwrap_err();
        assert!(err.to_string().contains("Unknown PAM rule type"));
    }

    #[test]
    fn test_parse_pam_config_unknown_control() {
        let content = "auth badcontrol pam_unix.so\n";
        let err = parse_pam_config(content).unwrap_err();
        assert!(err.to_string().contains("Unknown PAM control flag"));
    }

    #[test]
    fn test_render_pam_config_roundtrip() {
        let rules = vec![
            PamRule {
                rule_type: PamRuleType::Auth,
                control: PamControl::Required,
                module: "pam_unix.so".to_string(),
                args: vec![],
            },
            PamRule {
                rule_type: PamRuleType::Session,
                control: PamControl::Optional,
                module: "pam_systemd.so".to_string(),
                args: vec!["kill-session-processes=1".to_string()],
            },
        ];
        let rendered = render_pam_config(&rules);
        let parsed_back = parse_pam_config(&rendered).unwrap();
        assert_eq!(parsed_back.len(), 2);
        assert_eq!(parsed_back[0].module, "pam_unix.so");
        assert_eq!(parsed_back[1].module, "pam_systemd.so");
        assert_eq!(parsed_back[1].args, vec!["kill-session-processes=1"]);
    }

    // -- PAM rule validation ------------------------------------------------

    #[test]
    fn test_validate_pam_rule_ok() {
        let rule = PamRule {
            rule_type: PamRuleType::Auth,
            control: PamControl::Required,
            module: "pam_unix.so".to_string(),
            args: vec!["try_first_pass".to_string()],
        };
        assert!(validate_pam_rule(&rule).is_ok());
    }

    #[test]
    fn test_validate_pam_rule_empty_module() {
        let rule = PamRule {
            rule_type: PamRuleType::Auth,
            control: PamControl::Required,
            module: String::new(),
            args: vec![],
        };
        let err = validate_pam_rule(&rule).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_validate_pam_rule_dotdot_module() {
        let rule = PamRule {
            rule_type: PamRuleType::Auth,
            control: PamControl::Required,
            module: "/etc/../evil.so".to_string(),
            args: vec![],
        };
        assert!(validate_pam_rule(&rule).is_err());
    }

    #[test]
    fn test_validate_pam_rule_dangerous_chars() {
        for ch in ['|', ';', '&', '$', '`', '>', '<'] {
            let rule = PamRule {
                rule_type: PamRuleType::Auth,
                control: PamControl::Required,
                module: format!("pam_evil{}so", ch),
                args: vec![],
            };
            assert!(
                validate_pam_rule(&rule).is_err(),
                "Should reject '{}' in module",
                ch
            );
        }
    }

    #[test]
    fn test_validate_pam_rule_dangerous_arg() {
        let rule = PamRule {
            rule_type: PamRuleType::Auth,
            control: PamControl::Required,
            module: "pam_unix.so".to_string(),
            args: vec!["arg;evil".to_string()],
        };
        assert!(validate_pam_rule(&rule).is_err());
    }

    // -- Service paths ------------------------------------------------------

    #[test]
    fn test_get_pam_service_path_builtins() {
        assert_eq!(
            get_pam_service_path(&PamService::Login),
            PathBuf::from("/etc/pam.d/login")
        );
        assert_eq!(
            get_pam_service_path(&PamService::Sudo),
            PathBuf::from("/etc/pam.d/sudo")
        );
        assert_eq!(
            get_pam_service_path(&PamService::Sshd),
            PathBuf::from("/etc/pam.d/sshd")
        );
        assert_eq!(
            get_pam_service_path(&PamService::AgnosAgent),
            PathBuf::from("/etc/pam.d/agnos-agent")
        );
    }

    #[test]
    fn test_get_pam_service_path_custom() {
        assert_eq!(
            get_pam_service_path(&PamService::Custom("myservice".to_string())),
            PathBuf::from("/etc/pam.d/myservice")
        );
    }

    // -- Display impls ------------------------------------------------------

    #[test]
    fn test_auth_result_display() {
        assert_eq!(AuthResult::Success.to_string(), "Authentication successful");
        assert!(AuthResult::Denied("bad password".into())
            .to_string()
            .contains("bad password"));
        assert!(AuthResult::AccountExpired
            .to_string()
            .contains("expired"));
        assert!(AuthResult::PasswordExpired
            .to_string()
            .contains("expired"));
        assert!(AuthResult::SessionError("oops".into())
            .to_string()
            .contains("oops"));
        assert!(AuthResult::Unknown(42).to_string().contains("42"));
    }

    #[test]
    fn test_pam_service_display() {
        assert_eq!(PamService::Login.to_string(), "login");
        assert_eq!(PamService::AgnosAgent.to_string(), "agnos-agent");
        assert_eq!(
            PamService::Custom("my-svc".to_string()).to_string(),
            "my-svc"
        );
    }

    #[test]
    fn test_pam_rule_type_display() {
        assert_eq!(PamRuleType::Auth.to_string(), "auth");
        assert_eq!(PamRuleType::Account.to_string(), "account");
        assert_eq!(PamRuleType::Session.to_string(), "session");
        assert_eq!(PamRuleType::Password.to_string(), "password");
    }

    #[test]
    fn test_pam_control_display() {
        assert_eq!(PamControl::Required.to_string(), "required");
        assert_eq!(PamControl::Requisite.to_string(), "requisite");
        assert_eq!(PamControl::Sufficient.to_string(), "sufficient");
        assert_eq!(PamControl::Optional.to_string(), "optional");
        assert_eq!(PamControl::Include.to_string(), "include");
    }

    // -- Serialization roundtrip --------------------------------------------

    #[test]
    fn test_user_info_serde_roundtrip() {
        let info = UserInfo {
            username: "agnos".to_string(),
            uid: 1000,
            gid: 1000,
            home_dir: PathBuf::from("/home/agnos"),
            shell: "/bin/bash".to_string(),
            groups: vec!["wheel".to_string(), "docker".to_string()],
            is_system_user: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: UserInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_session_info_serde_roundtrip() {
        let info = SessionInfo {
            session_id: "sess-1".to_string(),
            user: "root".to_string(),
            login_time: "2026-03-06 10:00".to_string(),
            tty: Some("pts/0".to_string()),
            remote_host: None,
            pid: 1234,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_pam_rule_display() {
        let rule = PamRule {
            rule_type: PamRuleType::Auth,
            control: PamControl::Required,
            module: "pam_unix.so".to_string(),
            args: vec!["try_first_pass".to_string()],
        };
        let display = rule.to_string();
        assert!(display.contains("auth"));
        assert!(display.contains("required"));
        assert!(display.contains("pam_unix.so"));
        assert!(display.contains("try_first_pass"));
    }

    #[test]
    fn test_validate_pam_rule_absolute_path_ok() {
        let rule = PamRule {
            rule_type: PamRuleType::Auth,
            control: PamControl::Required,
            module: "/lib/security/pam_unix.so".to_string(),
            args: vec![],
        };
        assert!(validate_pam_rule(&rule).is_ok());
    }

    #[test]
    fn test_parse_passwd_line_nologin_shell() {
        let line = "sshd:x:74:74:Privilege-separated SSH:/var/empty/sshd:/usr/sbin/nologin";
        let info = parse_passwd_line(line).unwrap();
        assert_eq!(info.shell, "/usr/sbin/nologin");
        assert!(info.is_system_user); // UID 74 < 1000
    }
}
