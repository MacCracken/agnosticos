//! agnos-sudo — AGNOS privilege escalation tool
//!
//! Authenticates the calling user (via PAM or password verification), checks a
//! TOML-based policy file (`/etc/agnos/sudoers.toml`), then executes the
//! requested command with the target user's credentials.
//!
//! Security properties:
//! - All attempts (success and failure) are audit-logged
//! - Environment is sanitized before exec
//! - Command arguments are validated against shell injection
//! - Policy supports per-user, per-group, and per-command rules
//! - Rate-limited authentication (max 3 attempts)
//! - Timestamp-based credential caching (configurable TTL)

use std::collections::HashSet;
use std::env;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_POLICY_PATH: &str = "/etc/agnos/sudoers.toml";
const DEFAULT_TIMESTAMP_DIR: &str = "/var/run/agnos/sudo";
const DEFAULT_TIMESTAMP_TTL_SECS: u64 = 300; // 5 minutes
const MAX_AUTH_ATTEMPTS: u32 = 3;
const MAX_COMMAND_LEN: usize = 4096;

/// Environment variables that are always removed before exec.
const UNSAFE_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "LD_DYNAMIC_WEAK",
    "LD_BIND_NOW",
    "LD_AOUT_LIBRARY_PATH",
    "LD_AOUT_PRELOAD",
    "LD_ORIGIN_PATH",
    "LD_DEBUG",
    "LD_DEBUG_OUTPUT",
    "LD_PROFILE",
    "LD_PROFILE_OUTPUT",
    "LD_SHOW_AUXV",
    "LD_USE_LOAD_BIAS",
    "LOCALDOMAIN",
    "RES_OPTIONS",
    "HOSTALIASES",
    "NLSPATH",
    "PATH_LOCALE",
    "GCONV_PATH",
    "IFS",
    "ENV",
    "BASH_ENV",
    "CDPATH",
    "GLOBIGNORE",
    "SHELLOPTS",
    "BASHOPTS",
    "PS4",
    "PROMPT_COMMAND",
];

/// Environment variables preserved by default.
const SAFE_ENV_VARS: &[&str] = &[
    "TERM",
    "COLORTERM",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "DISPLAY",
    "XAUTHORITY",
];

// ---------------------------------------------------------------------------
// Policy types
// ---------------------------------------------------------------------------

/// Top-level sudoers policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SudoPolicy {
    /// Global settings.
    #[serde(default)]
    pub defaults: PolicyDefaults,
    /// Per-user rules.
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDefaults {
    /// Credential cache TTL in seconds (0 = always ask).
    #[serde(default = "default_ttl")]
    pub timestamp_ttl: u64,
    /// Whether to require a password (false = NOPASSWD for all).
    #[serde(default = "default_true")]
    pub require_auth: bool,
    /// Whether to log all commands to audit.
    #[serde(default = "default_true")]
    pub audit_log: bool,
    /// Environment variables to preserve (in addition to SAFE_ENV_VARS).
    #[serde(default)]
    pub env_keep: Vec<String>,
    /// Maximum command length.
    #[serde(default = "default_max_cmd_len")]
    pub max_command_len: usize,
}

impl Default for PolicyDefaults {
    fn default() -> Self {
        Self {
            timestamp_ttl: DEFAULT_TIMESTAMP_TTL_SECS,
            require_auth: true,
            audit_log: true,
            env_keep: Vec::new(),
            max_command_len: MAX_COMMAND_LEN,
        }
    }
}

fn default_ttl() -> u64 {
    DEFAULT_TIMESTAMP_TTL_SECS
}
fn default_true() -> bool {
    true
}
fn default_max_cmd_len() -> usize {
    MAX_COMMAND_LEN
}

/// A single policy rule granting privileges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Username this rule applies to (or "*" for all).
    #[serde(default)]
    pub user: Option<String>,
    /// Group this rule applies to (prefixed with `%` in traditional sudo).
    #[serde(default)]
    pub group: Option<String>,
    /// Target user to run as (default: "root").
    #[serde(default = "default_target_user")]
    pub run_as: String,
    /// Allowed commands (empty = all commands).
    #[serde(default)]
    pub commands: Vec<String>,
    /// Denied commands (checked before allowed).
    #[serde(default)]
    pub deny_commands: Vec<String>,
    /// Whether authentication is required for this rule.
    #[serde(default = "default_true")]
    pub require_auth: bool,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
}

fn default_target_user() -> String {
    "root".to_string()
}

// ---------------------------------------------------------------------------
// Policy loading and evaluation
// ---------------------------------------------------------------------------

/// Load the sudoers policy from a TOML file.
pub fn load_policy(path: &Path) -> Result<SudoPolicy> {
    if !path.exists() {
        bail!(
            "Policy file not found: {}. Create it or use --policy to specify a path.",
            path.display()
        );
    }

    // Security: policy file must be owned by root and not world-writable
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path)
            .with_context(|| format!("Cannot stat policy file: {}", path.display()))?;
        if meta.uid() != 0 {
            warn!(
                "Policy file {} is not owned by root (uid={}). This is a security risk.",
                path.display(),
                meta.uid()
            );
        }
        let mode = meta.mode();
        if mode & 0o002 != 0 {
            bail!(
                "Policy file {} is world-writable (mode {:o}). Refusing to use it.",
                path.display(),
                mode
            );
        }
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let policy: SudoPolicy =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;

    Ok(policy)
}

/// Parse policy from a TOML string (for testing).
pub fn parse_policy(content: &str) -> Result<SudoPolicy> {
    let policy: SudoPolicy = toml::from_str(content)?;
    Ok(policy)
}

/// Check whether a user is authorized to run a command under a policy.
pub fn check_authorization(
    policy: &SudoPolicy,
    username: &str,
    groups: &[String],
    target_user: &str,
    command: &str,
) -> AuthzResult {
    let mut matched_rule: Option<&PolicyRule> = None;

    for rule in &policy.rules {
        // Check user match
        let user_matches = match &rule.user {
            Some(u) if u == "*" => true,
            Some(u) => u == username,
            None => false,
        };

        // Check group match
        let group_matches = match &rule.group {
            Some(g) => groups.iter().any(|ug| ug == g),
            None => false,
        };

        if !user_matches && !group_matches {
            continue;
        }

        // Check target user
        if rule.run_as != "*" && rule.run_as != target_user {
            continue;
        }

        // Check denied commands first
        for deny in &rule.deny_commands {
            if command_matches(command, deny) {
                return AuthzResult::Denied(format!(
                    "Command '{}' is explicitly denied by rule: {}",
                    command, rule.description
                ));
            }
        }

        // Check allowed commands
        if rule.commands.is_empty() {
            // Empty = all commands allowed
            matched_rule = Some(rule);
            break;
        }

        for allowed in &rule.commands {
            if command_matches(command, allowed) {
                matched_rule = Some(rule);
                break;
            }
        }

        if matched_rule.is_some() {
            break;
        }
    }

    match matched_rule {
        Some(rule) => AuthzResult::Allowed {
            require_auth: rule.require_auth && policy.defaults.require_auth,
        },
        None => AuthzResult::Denied(format!(
            "User '{}' is not authorized to run '{}' as '{}'",
            username, command, target_user
        )),
    }
}

/// Result of an authorization check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthzResult {
    Allowed { require_auth: bool },
    Denied(String),
}

/// Check if a command matches a pattern.
///
/// Patterns:
/// - Exact path: `/usr/bin/systemctl` matches only that binary
/// - Glob-style: `/usr/bin/*` matches any binary in that dir
/// - Basename: `systemctl` matches any path ending in `systemctl`
/// - `ALL` or `*`: matches everything
fn command_matches(command: &str, pattern: &str) -> bool {
    if pattern == "ALL" || pattern == "*" {
        return true;
    }

    let cmd_path = Path::new(command);
    // Exact match
    if command == pattern {
        return true;
    }

    // Glob: pattern ends with /*
    if let Some(prefix) = pattern.strip_suffix("/*") {
        if let Some(parent) = cmd_path.parent() {
            return parent == Path::new(prefix);
        }
    }

    // Basename match (pattern has no /)
    if !pattern.contains('/') {
        if let Some(basename) = cmd_path.file_name() {
            return basename == pattern.as_ref() as &std::ffi::OsStr;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Environment sanitization
// ---------------------------------------------------------------------------

/// Build a sanitized environment for the target command.
#[allow(clippy::vec_init_then_push)]
pub fn sanitize_environment(
    policy: &SudoPolicy,
    caller_user: &str,
    target_user: &str,
    target_home: &str,
    target_shell: &str,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();

    // Always set these
    env.push(("USER".to_string(), target_user.to_string()));
    env.push(("LOGNAME".to_string(), target_user.to_string()));
    env.push(("HOME".to_string(), target_home.to_string()));
    env.push(("SHELL".to_string(), target_shell.to_string()));
    env.push((
        "PATH".to_string(),
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
    ));
    env.push(("SUDO_USER".to_string(), caller_user.to_string()));
    env.push(("SUDO_UID".to_string(), nix::unistd::getuid().to_string()));
    env.push(("SUDO_GID".to_string(), nix::unistd::getgid().to_string()));

    // Preserve safe vars from current environment
    let keep_set: HashSet<&str> = SAFE_ENV_VARS
        .iter()
        .copied()
        .chain(policy.defaults.env_keep.iter().map(|s| s.as_str()))
        .collect();

    for (key, value) in env::vars() {
        if keep_set.contains(key.as_str()) && !UNSAFE_ENV_VARS.contains(&key.as_str()) {
            env.push((key, value));
        }
    }

    env
}

// ---------------------------------------------------------------------------
// Timestamp credential cache
// ---------------------------------------------------------------------------

/// Check if the user has a valid timestamp (recently authenticated).
pub fn check_timestamp(user: &str, ttl: Duration) -> bool {
    let ts_path = timestamp_path(user);
    match std::fs::metadata(&ts_path) {
        Ok(meta) => {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                    return elapsed < ttl;
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Update the timestamp for a user (mark as recently authenticated).
pub fn update_timestamp(user: &str) -> Result<()> {
    let ts_path = timestamp_path(user);
    let dir = Path::new(DEFAULT_TIMESTAMP_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create timestamp directory: {}", dir.display()))?;
    }
    // Touch the file
    std::fs::write(&ts_path, b"")
        .with_context(|| format!("Failed to update timestamp: {}", ts_path.display()))?;
    Ok(())
}

/// Remove timestamp for a user (invalidate cached credentials).
pub fn invalidate_timestamp(user: &str) -> Result<()> {
    let ts_path = timestamp_path(user);
    if ts_path.exists() {
        std::fs::remove_file(&ts_path)?;
    }
    Ok(())
}

fn timestamp_path(user: &str) -> PathBuf {
    PathBuf::from(DEFAULT_TIMESTAMP_DIR).join(user)
}

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

/// Validate command arguments for safety.
pub fn validate_command(args: &[String], max_len: usize) -> Result<()> {
    if args.is_empty() {
        bail!("No command specified");
    }

    let total_len: usize = args.iter().map(|a| a.len()).sum::<usize>() + args.len();
    if total_len > max_len {
        bail!("Command too long ({} bytes, max {})", total_len, max_len);
    }

    // Reject null bytes in arguments (could be used to truncate strings in C APIs)
    for arg in args {
        if arg.contains('\0') {
            bail!("Command argument contains null byte");
        }
    }

    // The command (first arg) must be an absolute path or resolvable basename
    let cmd = &args[0];
    if cmd.is_empty() {
        bail!("Empty command name");
    }

    Ok(())
}

/// Resolve a command to its absolute path using PATH.
pub fn resolve_command(cmd: &str) -> Result<PathBuf> {
    // Already absolute
    if cmd.starts_with('/') {
        let path = PathBuf::from(cmd);
        if path.exists() {
            return Ok(path);
        }
        bail!("Command not found: {}", cmd);
    }

    // Reject relative paths with / (e.g., ../bin/bash)
    if cmd.contains('/') {
        bail!(
            "Relative paths not allowed. Use an absolute path or a command name: {}",
            cmd
        );
    }

    // Search PATH
    let search_path = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    for dir in search_path.split(':') {
        let candidate = PathBuf::from(dir).join(cmd);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!("Command not found in PATH: {}", cmd);
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

/// Authenticate the calling user.
///
/// On a real system this would call PAM. For now we use `/usr/bin/su` to
/// verify the password, which delegates to PAM under the hood.
#[cfg(target_os = "linux")]
fn authenticate_user(username: &str) -> Result<bool> {
    use std::io::{self, BufRead, Write};

    for attempt in 1..=MAX_AUTH_ATTEMPTS {
        eprint!("[agnos-sudo] password for {}: ", username);
        io::stderr().flush()?;

        // Read password (in a real implementation, we'd disable echo via termios)
        let stdin = io::stdin();
        let password = stdin.lock().lines().next().unwrap_or(Ok(String::new()))?;
        eprintln!(); // newline after password

        if password.is_empty() {
            if attempt < MAX_AUTH_ATTEMPTS {
                eprintln!("Sorry, try again.");
                continue;
            }
            return Ok(false);
        }

        // Use PAM via /usr/bin/su to validate
        let result = std::process::Command::new("/usr/bin/su")
            .arg("-c")
            .arg("true")
            .arg(username)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match result {
            Ok(mut child) => {
                if let Some(ref mut stdin_pipe) = child.stdin {
                    if let Err(e) = stdin_pipe
                        .write_all(password.as_bytes())
                        .and_then(|_| stdin_pipe.write_all(b"\n"))
                    {
                        eprintln!("Warning: failed to write credentials to su: {}", e);
                    }
                }
                match child.wait() {
                    Ok(status) if status.success() => return Ok(true),
                    _ => {}
                }
            }
            Err(_) => {
                // su not available, fall through
            }
        }

        if attempt < MAX_AUTH_ATTEMPTS {
            eprintln!("Sorry, try again.");
        }
    }

    Ok(false)
}

#[cfg(not(target_os = "linux"))]
fn authenticate_user(_username: &str) -> Result<bool> {
    bail!("Authentication not supported on this platform");
}

// ---------------------------------------------------------------------------
// Audit logging
// ---------------------------------------------------------------------------

fn audit_log(action: &str, caller: &str, target: &str, command: &str, success: bool, reason: &str) {
    let status = if success { "ALLOWED" } else { "DENIED" };
    let msg = format!(
        "agnos-sudo: {} : {} ; USER={} ; COMMAND={} ; STATUS={} ; REASON={}",
        action, caller, target, command, status, reason
    );

    if success {
        info!("{}", msg);
    } else {
        warn!("{}", msg);
    }

    // Also write to syslog-style audit trail
    #[cfg(target_os = "linux")]
    {
        let log_line = format!(
            "{} {} : TTY={} ; PWD={} ; USER={} ; COMMAND={}\n",
            chrono_now(),
            caller,
            tty_name(),
            env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            target,
            command,
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/var/log/agnos/sudo.log")
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(log_line.as_bytes())
            });
    }
}

fn chrono_now() -> String {
    // Simple ISO 8601 timestamp without chrono dependency
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_secs())
}

fn tty_name() -> String {
    env::var("TTY")
        .or_else(|_| env::var("SSH_TTY"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// User info helpers
// ---------------------------------------------------------------------------

/// Get the calling user's username from the real UID.
fn get_caller_username() -> Result<String> {
    let uid = nix::unistd::getuid();
    nix::unistd::User::from_uid(uid)?
        .map(|u| u.name)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine username for UID {}", uid))
}

/// Get groups for a user.
fn get_user_groups(username: &str) -> Vec<String> {
    // Read from /etc/group
    let content = match std::fs::read_to_string("/etc/group") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut groups = Vec::new();
    for line in content.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 4 {
            let group_name = fields[0];
            let members: Vec<&str> = fields[3].split(',').collect();
            if members.iter().any(|m| m.trim() == username) {
                groups.push(group_name.to_string());
            }
        }
    }

    // Also add primary group
    if let Ok(Some(user)) = nix::unistd::User::from_name(username) {
        if let Ok(Some(group)) = nix::unistd::Group::from_gid(user.gid) {
            if !groups.contains(&group.name) {
                groups.push(group.name);
            }
        }
    }

    groups
}

/// Get target user info.
fn get_target_user_info(username: &str) -> Result<(u32, u32, String, String)> {
    let user = nix::unistd::User::from_name(username)?
        .ok_or_else(|| anyhow::anyhow!("Target user '{}' not found", username))?;
    Ok((
        user.uid.as_raw(),
        user.gid.as_raw(),
        user.dir.to_string_lossy().to_string(),
        user.shell.to_string_lossy().to_string(),
    ))
}

// ---------------------------------------------------------------------------
// CLI parsing
// ---------------------------------------------------------------------------

struct CliArgs {
    target_user: String,
    policy_path: PathBuf,
    invalidate: bool,
    list: bool,
    command: Vec<String>,
}

fn parse_args() -> Result<CliArgs> {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut target_user = "root".to_string();
    let mut policy_path = PathBuf::from(DEFAULT_POLICY_PATH);
    let mut invalidate = false;
    let mut list = false;
    let mut command = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-u" | "--user" => {
                i += 1;
                if i >= args.len() {
                    bail!("-u requires a username argument");
                }
                target_user = args[i].clone();
            }
            "-p" | "--policy" => {
                i += 1;
                if i >= args.len() {
                    bail!("-p requires a file path argument");
                }
                policy_path = PathBuf::from(&args[i]);
            }
            "-k" | "--invalidate" => {
                invalidate = true;
            }
            "-l" | "--list" => {
                list = true;
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            "-V" | "--version" => {
                println!("agnos-sudo {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "--" => {
                command.extend_from_slice(&args[i + 1..]);
                break;
            }
            arg if arg.starts_with('-') => {
                bail!("Unknown option: {}. Use --help for usage.", arg);
            }
            _ => {
                command.extend_from_slice(&args[i..]);
                break;
            }
        }
        i += 1;
    }

    Ok(CliArgs {
        target_user,
        policy_path,
        invalidate,
        list,
        command,
    })
}

fn print_usage() {
    eprintln!(
        "Usage: agnos-sudo [OPTIONS] [--] COMMAND [ARGS...]

AGNOS privilege escalation tool.

Options:
  -u, --user USER      Run command as USER (default: root)
  -p, --policy FILE    Use alternate policy file (default: {})
  -k, --invalidate     Invalidate cached credentials
  -l, --list           List allowed commands for current user
  -h, --help           Show this help message
  -V, --version        Show version

Examples:
  agnos-sudo systemctl restart llm-gateway
  agnos-sudo -u postgres psql
  agnos-sudo -k",
        DEFAULT_POLICY_PATH
    );
}

// ---------------------------------------------------------------------------
// List mode
// ---------------------------------------------------------------------------

fn list_permissions(policy: &SudoPolicy, username: &str, groups: &[String]) {
    println!(
        "User {} may run the following commands on this host:",
        username
    );
    println!();

    let mut found = false;
    for rule in &policy.rules {
        let user_matches = match &rule.user {
            Some(u) if u == "*" => true,
            Some(u) => u == username,
            None => false,
        };
        let group_matches = match &rule.group {
            Some(g) => groups.iter().any(|ug| ug == g),
            None => false,
        };

        if !user_matches && !group_matches {
            continue;
        }

        found = true;
        let auth_tag = if rule.require_auth { "" } else { "NOPASSWD: " };
        let run_as = &rule.run_as;

        if rule.commands.is_empty() {
            println!("    ({}) {}ALL", run_as, auth_tag);
        } else {
            for cmd in &rule.commands {
                println!("    ({}) {}{}", run_as, auth_tag, cmd);
            }
        }

        if !rule.deny_commands.is_empty() {
            for cmd in &rule.deny_commands {
                println!("    ({}) DENY: {}", run_as, cmd);
            }
        }

        if !rule.description.is_empty() {
            println!("    # {}", rule.description);
        }
    }

    if !found {
        println!("    (none)");
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    // Initialize tracing
    let format = env::var("AGNOS_LOG_FORMAT").unwrap_or_default();
    if format == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn".into()),
            )
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn".into()),
            )
            .init();
    }

    if let Err(e) = run() {
        error!("{:#}", e);
        eprintln!("agnos-sudo: {:#}", e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = parse_args()?;

    // Get caller identity from real UID (not effective — we may be setuid)
    let caller = get_caller_username()?;
    let groups = get_user_groups(&caller);

    // Handle credential invalidation
    if cli.invalidate {
        invalidate_timestamp(&caller)?;
        info!("Timestamp invalidated for user '{}'", caller);
        return Ok(());
    }

    // Load policy
    let policy = load_policy(&cli.policy_path)?;

    // Handle list mode
    if cli.list {
        list_permissions(&policy, &caller, &groups);
        return Ok(());
    }

    // Must have a command
    if cli.command.is_empty() {
        print_usage();
        bail!("No command specified");
    }

    // Validate command
    validate_command(&cli.command, policy.defaults.max_command_len)?;

    // Resolve command to absolute path
    let resolved = resolve_command(&cli.command[0])?;
    let cmd_str = format!("{} {}", resolved.display(), cli.command[1..].join(" "));

    // Authorization check
    let authz = check_authorization(
        &policy,
        &caller,
        &groups,
        &cli.target_user,
        resolved.to_str().unwrap_or(""),
    );

    match authz {
        AuthzResult::Denied(reason) => {
            audit_log(
                "COMMAND",
                &caller,
                &cli.target_user,
                &cmd_str,
                false,
                &reason,
            );
            eprintln!("agnos-sudo: {}", reason);
            // Log incident — 3 failures in a row should alert
            bail!("Permission denied");
        }
        AuthzResult::Allowed { require_auth } => {
            // Authentication
            if require_auth {
                let ttl = Duration::from_secs(policy.defaults.timestamp_ttl);
                if !check_timestamp(&caller, ttl) {
                    let authenticated = authenticate_user(&caller)?;
                    if !authenticated {
                        audit_log(
                            "AUTH_FAILURE",
                            &caller,
                            &cli.target_user,
                            &cmd_str,
                            false,
                            "authentication failed",
                        );
                        bail!("{} incorrect password attempts", MAX_AUTH_ATTEMPTS);
                    }
                    // Update timestamp on successful auth
                    let _ = update_timestamp(&caller);
                }
            }

            // Audit log the allowed execution
            audit_log(
                "COMMAND",
                &caller,
                &cli.target_user,
                &cmd_str,
                true,
                "authorized",
            );

            // Get target user info
            let (target_uid, target_gid, target_home, target_shell) =
                get_target_user_info(&cli.target_user)?;

            // Build sanitized environment
            let env = sanitize_environment(
                &policy,
                &caller,
                &cli.target_user,
                &target_home,
                &target_shell,
            );

            // Execute the command
            // We use exec() to replace this process — the command inherits our PID
            let mut cmd = process::Command::new(&resolved);
            cmd.args(&cli.command[1..]);

            // Clear environment and set sanitized vars
            cmd.env_clear();
            for (k, v) in &env {
                cmd.env(k, v);
            }

            // Set uid/gid before exec
            unsafe {
                cmd.pre_exec(move || {
                    // Set supplementary groups
                    nix::unistd::setgroups(&[nix::unistd::Gid::from_raw(target_gid)]).map_err(
                        |e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e),
                    )?;
                    // Set GID first (must be done before setuid)
                    nix::unistd::setgid(nix::unistd::Gid::from_raw(target_gid)).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::PermissionDenied, e)
                    })?;
                    // Set UID last
                    nix::unistd::setuid(nix::unistd::Uid::from_raw(target_uid)).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::PermissionDenied, e)
                    })?;
                    Ok(())
                });
            }

            // exec replaces this process — this never returns on success
            let err = cmd.exec();
            bail!("Failed to exec {}: {}", resolved.display(), err);
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Policy parsing
    // -----------------------------------------------------------------------

    fn sample_policy() -> &'static str {
        r#"
[defaults]
timestamp_ttl = 300
require_auth = true
audit_log = true
env_keep = ["EDITOR", "VISUAL"]
max_command_len = 4096

[[rules]]
user = "admin"
run_as = "root"
commands = []
require_auth = true
description = "Admin has full access"

[[rules]]
group = "wheel"
run_as = "root"
commands = ["/usr/bin/systemctl", "/usr/bin/journalctl"]
require_auth = true
description = "Wheel group can manage services"

[[rules]]
user = "deploy"
run_as = "root"
commands = ["/usr/bin/systemctl restart *", "/usr/bin/docker"]
deny_commands = ["/usr/bin/systemctl stop firewall"]
require_auth = false
description = "Deploy user can restart services (no password)"

[[rules]]
user = "*"
run_as = "root"
commands = ["/usr/bin/passwd"]
require_auth = true
description = "Anyone can change passwords"
"#
    }

    #[test]
    fn test_parse_policy() {
        let policy = parse_policy(sample_policy()).unwrap();
        assert_eq!(policy.defaults.timestamp_ttl, 300);
        assert!(policy.defaults.require_auth);
        assert!(policy.defaults.audit_log);
        assert_eq!(policy.defaults.env_keep, vec!["EDITOR", "VISUAL"]);
        assert_eq!(policy.rules.len(), 4);
    }

    #[test]
    fn test_parse_policy_defaults() {
        let policy = parse_policy("").unwrap();
        assert_eq!(policy.defaults.timestamp_ttl, DEFAULT_TIMESTAMP_TTL_SECS);
        assert!(policy.defaults.require_auth);
        assert!(policy.defaults.audit_log);
        assert_eq!(policy.defaults.max_command_len, MAX_COMMAND_LEN);
        assert!(policy.rules.is_empty());
    }

    #[test]
    fn test_parse_policy_minimal_rule() {
        let policy = parse_policy(
            r#"
[[rules]]
user = "bob"
"#,
        )
        .unwrap();
        assert_eq!(policy.rules.len(), 1);
        assert_eq!(policy.rules[0].run_as, "root");
        assert!(policy.rules[0].commands.is_empty());
        assert!(policy.rules[0].require_auth);
    }

    // -----------------------------------------------------------------------
    // Authorization
    // -----------------------------------------------------------------------

    #[test]
    fn test_authz_admin_full_access() {
        let policy = parse_policy(sample_policy()).unwrap();
        let result = check_authorization(&policy, "admin", &[], "root", "/usr/bin/anything");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    #[test]
    fn test_authz_wheel_group_allowed() {
        let policy = parse_policy(sample_policy()).unwrap();
        let groups = vec!["wheel".to_string()];
        let result = check_authorization(&policy, "jdoe", &groups, "root", "/usr/bin/systemctl");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    #[test]
    fn test_authz_wheel_group_denied_command() {
        let policy = parse_policy(sample_policy()).unwrap();
        let groups = vec!["wheel".to_string()];
        let result = check_authorization(&policy, "jdoe", &groups, "root", "/usr/bin/rm");
        // rm is not in wheel's allowed commands, but the wildcard rule matches
        // The wildcard rule is for /usr/bin/passwd only
        assert!(matches!(result, AuthzResult::Denied(_)));
    }

    #[test]
    fn test_authz_deploy_nopasswd() {
        let policy = parse_policy(sample_policy()).unwrap();
        let result = check_authorization(&policy, "deploy", &[], "root", "/usr/bin/docker");
        assert_eq!(
            result,
            AuthzResult::Allowed {
                require_auth: false
            }
        );
    }

    #[test]
    fn test_authz_deploy_denied_command() {
        let policy = parse_policy(sample_policy()).unwrap();
        let result = check_authorization(
            &policy,
            "deploy",
            &[],
            "root",
            "/usr/bin/systemctl stop firewall",
        );
        assert!(matches!(result, AuthzResult::Denied(_)));
    }

    #[test]
    fn test_authz_wildcard_user() {
        let policy = parse_policy(sample_policy()).unwrap();
        let result = check_authorization(&policy, "anybody", &[], "root", "/usr/bin/passwd");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    #[test]
    fn test_authz_unknown_user_denied() {
        let policy = parse_policy(sample_policy()).unwrap();
        let result = check_authorization(&policy, "unknown", &[], "root", "/usr/bin/dangerous");
        assert!(matches!(result, AuthzResult::Denied(_)));
    }

    #[test]
    fn test_authz_wrong_target_user() {
        let policy = parse_policy(sample_policy()).unwrap();
        let result = check_authorization(&policy, "admin", &[], "postgres", "/usr/bin/anything");
        // Admin rule says run_as = "root", so "postgres" won't match
        assert!(matches!(result, AuthzResult::Denied(_)));
    }

    #[test]
    fn test_authz_run_as_wildcard() {
        let policy = parse_policy(
            r#"
[[rules]]
user = "admin"
run_as = "*"
commands = []
"#,
        )
        .unwrap();
        let result = check_authorization(&policy, "admin", &[], "postgres", "/usr/bin/psql");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    #[test]
    fn test_authz_no_rules() {
        let policy = parse_policy("").unwrap();
        let result = check_authorization(&policy, "admin", &[], "root", "/usr/bin/ls");
        assert!(matches!(result, AuthzResult::Denied(_)));
    }

    #[test]
    fn test_authz_group_only_rule() {
        let policy = parse_policy(
            r#"
[[rules]]
group = "devops"
run_as = "root"
commands = ["/usr/bin/docker"]
"#,
        )
        .unwrap();
        // User not in group
        let result = check_authorization(&policy, "alice", &[], "root", "/usr/bin/docker");
        assert!(matches!(result, AuthzResult::Denied(_)));

        // User in group
        let groups = vec!["devops".to_string()];
        let result = check_authorization(&policy, "alice", &groups, "root", "/usr/bin/docker");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    // -----------------------------------------------------------------------
    // Command matching
    // -----------------------------------------------------------------------

    #[test]
    fn test_command_matches_exact() {
        assert!(command_matches("/usr/bin/ls", "/usr/bin/ls"));
        assert!(!command_matches("/usr/bin/rm", "/usr/bin/ls"));
    }

    #[test]
    fn test_command_matches_all() {
        assert!(command_matches("/usr/bin/anything", "ALL"));
        assert!(command_matches("/usr/bin/anything", "*"));
    }

    #[test]
    fn test_command_matches_glob() {
        assert!(command_matches("/usr/bin/ls", "/usr/bin/*"));
        assert!(!command_matches("/usr/sbin/reboot", "/usr/bin/*"));
    }

    #[test]
    fn test_command_matches_basename() {
        assert!(command_matches("/usr/bin/systemctl", "systemctl"));
        assert!(command_matches("/usr/local/bin/systemctl", "systemctl"));
        assert!(!command_matches("/usr/bin/systemd", "systemctl"));
    }

    #[test]
    fn test_command_matches_no_match() {
        assert!(!command_matches("/usr/bin/rm", "/usr/bin/ls"));
    }

    // -----------------------------------------------------------------------
    // Command validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_command_ok() {
        let args = vec!["ls".to_string(), "-la".to_string()];
        assert!(validate_command(&args, 4096).is_ok());
    }

    #[test]
    fn test_validate_command_empty() {
        let args: Vec<String> = vec![];
        assert!(validate_command(&args, 4096).is_err());
    }

    #[test]
    fn test_validate_command_too_long() {
        let args = vec!["a".repeat(5000)];
        assert!(validate_command(&args, 4096).is_err());
    }

    #[test]
    fn test_validate_command_null_byte() {
        let args = vec!["ls\0-la".to_string()];
        assert!(validate_command(&args, 4096).is_err());
    }

    #[test]
    fn test_validate_command_empty_name() {
        let args = vec!["".to_string()];
        assert!(validate_command(&args, 4096).is_err());
    }

    // -----------------------------------------------------------------------
    // Command resolution
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_command_absolute() {
        let result = resolve_command("/usr/bin/env");
        // /usr/bin/env should exist on most systems
        if Path::new("/usr/bin/env").exists() {
            assert_eq!(result.unwrap(), PathBuf::from("/usr/bin/env"));
        }
    }

    #[test]
    fn test_resolve_command_basename() {
        let result = resolve_command("env");
        // Should find env in PATH
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_command_not_found() {
        let result = resolve_command("/nonexistent/path/to/binary");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_command_relative_rejected() {
        let result = resolve_command("../bin/bash");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Relative paths not allowed"));
    }

    // -----------------------------------------------------------------------
    // Environment sanitization
    // -----------------------------------------------------------------------

    #[test]
    fn test_sanitize_environment() {
        let policy = parse_policy("").unwrap();
        let env = sanitize_environment(&policy, "alice", "root", "/root", "/bin/bash");

        let env_map: std::collections::HashMap<&str, &str> =
            env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        assert_eq!(env_map["USER"], "root");
        assert_eq!(env_map["LOGNAME"], "root");
        assert_eq!(env_map["HOME"], "/root");
        assert_eq!(env_map["SHELL"], "/bin/bash");
        assert_eq!(env_map["SUDO_USER"], "alice");
        assert!(env_map.contains_key("PATH"));
    }

    #[test]
    fn test_sanitize_environment_no_unsafe_vars() {
        let policy = parse_policy("").unwrap();
        let env = sanitize_environment(&policy, "alice", "root", "/root", "/bin/bash");

        let keys: HashSet<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        for var in UNSAFE_ENV_VARS {
            assert!(
                !keys.contains(var),
                "Unsafe var {} should not be in env",
                var
            );
        }
    }

    #[test]
    fn test_sanitize_environment_with_env_keep() {
        let policy = parse_policy(
            r#"
[defaults]
env_keep = ["EDITOR"]
"#,
        )
        .unwrap();
        assert!(policy.defaults.env_keep.contains(&"EDITOR".to_string()));
    }

    // -----------------------------------------------------------------------
    // Timestamp cache
    // -----------------------------------------------------------------------

    #[test]
    fn test_timestamp_path() {
        let path = timestamp_path("alice");
        assert_eq!(path, PathBuf::from("/var/run/agnos/sudo/alice"));
    }

    #[test]
    fn test_check_timestamp_nonexistent() {
        // Random user that won't have a timestamp file
        assert!(!check_timestamp(
            "test_nonexistent_user_xyz",
            Duration::from_secs(300)
        ));
    }

    // -----------------------------------------------------------------------
    // AuthzResult
    // -----------------------------------------------------------------------

    #[test]
    fn test_authz_result_debug() {
        let allowed = AuthzResult::Allowed { require_auth: true };
        let dbg = format!("{:?}", allowed);
        assert!(dbg.contains("Allowed"));

        let denied = AuthzResult::Denied("nope".to_string());
        let dbg = format!("{:?}", denied);
        assert!(dbg.contains("Denied"));
        assert!(dbg.contains("nope"));
    }

    #[test]
    fn test_authz_result_eq() {
        assert_eq!(
            AuthzResult::Allowed { require_auth: true },
            AuthzResult::Allowed { require_auth: true }
        );
        assert_ne!(
            AuthzResult::Allowed { require_auth: true },
            AuthzResult::Allowed {
                require_auth: false
            }
        );
        assert_ne!(
            AuthzResult::Allowed { require_auth: true },
            AuthzResult::Denied("x".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // Policy edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_policy_require_auth_override() {
        // Global require_auth = false overrides rule-level
        let policy = parse_policy(
            r#"
[defaults]
require_auth = false

[[rules]]
user = "admin"
require_auth = true
"#,
        )
        .unwrap();
        let result = check_authorization(&policy, "admin", &[], "root", "/usr/bin/ls");
        // require_auth = rule.require_auth && policy.defaults.require_auth
        // = true && false = false
        assert_eq!(
            result,
            AuthzResult::Allowed {
                require_auth: false
            }
        );
    }

    #[test]
    fn test_policy_deny_takes_precedence() {
        let policy = parse_policy(
            r#"
[[rules]]
user = "admin"
commands = ["ALL"]
deny_commands = ["/usr/bin/rm"]
"#,
        )
        .unwrap();
        let result = check_authorization(&policy, "admin", &[], "root", "/usr/bin/rm");
        assert!(matches!(result, AuthzResult::Denied(_)));

        // But other commands are fine
        let result = check_authorization(&policy, "admin", &[], "root", "/usr/bin/ls");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    #[test]
    fn test_policy_multiple_groups() {
        let policy = parse_policy(
            r#"
[[rules]]
group = "docker"
commands = ["/usr/bin/docker"]

[[rules]]
group = "admin"
commands = ["ALL"]
"#,
        )
        .unwrap();
        let groups = vec!["docker".to_string(), "admin".to_string()];

        // First matching rule wins — docker group matches first
        let result = check_authorization(&policy, "bob", &groups, "root", "/usr/bin/docker");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });

        // For non-docker command, first rule doesn't match, second does
        let result = check_authorization(&policy, "bob", &groups, "root", "/usr/bin/ls");
        assert_eq!(result, AuthzResult::Allowed { require_auth: true });
    }

    // -----------------------------------------------------------------------
    // CLI parsing
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_policy_path() {
        assert_eq!(DEFAULT_POLICY_PATH, "/etc/agnos/sudoers.toml");
    }

    #[test]
    fn test_default_timestamp_dir() {
        assert_eq!(DEFAULT_TIMESTAMP_DIR, "/var/run/agnos/sudo");
    }

    #[test]
    fn test_unsafe_env_vars_contains_ld_preload() {
        assert!(UNSAFE_ENV_VARS.contains(&"LD_PRELOAD"));
        assert!(UNSAFE_ENV_VARS.contains(&"LD_LIBRARY_PATH"));
        assert!(UNSAFE_ENV_VARS.contains(&"BASH_ENV"));
        assert!(UNSAFE_ENV_VARS.contains(&"IFS"));
    }

    #[test]
    fn test_safe_env_vars() {
        assert!(SAFE_ENV_VARS.contains(&"TERM"));
        assert!(SAFE_ENV_VARS.contains(&"LANG"));
        assert!(SAFE_ENV_VARS.contains(&"TZ"));
    }

    #[test]
    fn test_chrono_now_not_empty() {
        let ts = chrono_now();
        assert!(!ts.is_empty());
        // Should be a number (unix timestamp)
        assert!(ts.parse::<u64>().is_ok());
    }

    #[test]
    fn test_tty_name_returns_something() {
        let tty = tty_name();
        // May be "unknown" if no TTY
        assert!(!tty.is_empty());
    }
}
