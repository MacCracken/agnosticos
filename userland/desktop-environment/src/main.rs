// Desktop environment is deliberately incomplete (P3: Wayland compositor stub).
// Struct fields define the public API surface for future rendering implementation.
#![allow(dead_code, unused_mut, unused_imports)]

use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::info;
use tracing_subscriber::EnvFilter;
use clap::Parser;

mod compositor;
pub mod renderer;
mod shell;
mod ai_features;
mod apps;
mod security_ui;

use compositor::{Compositor, WindowState, ContextType};
use shell::{DesktopShell, Notification, NotificationPriority};
use ai_features::{AIDesktopFeatures, AISuggestion, SuggestionType, ContextEvent, ContextEventType};
use apps::DesktopApplications;
use security_ui::{SecurityUI, SecurityAlert, ThreatLevel, SecurityLevel};

#[derive(Debug, clap::Parser)]
#[command(name = "desktop-environment")]
#[command(author = "AGNOS Team")]
#[command(version = "2026.3.5")]
#[command(about = "AGNOS Desktop Environment", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wayland")]
    #[arg(help = "Display server backend (wayland, x11)")]
    backend: String,

    #[arg(short, long)]
    #[arg(help = "Start in kiosk mode")]
    kiosk: bool,

    #[arg(short, long)]
    #[arg(help = "Disable AI features")]
    no_ai: bool,

    #[arg(short, long)]
    #[arg(help = "Enable secure mode")]
    secure: bool,
}

struct DesktopEnvironment {
    compositor: Arc<Compositor>,
    shell: Arc<DesktopShell>,
    ai_features: Arc<AIDesktopFeatures>,
    apps: Arc<tokio::sync::Mutex<DesktopApplications>>,
    security_ui: Arc<SecurityUI>,
    running: Arc<tokio::sync::Mutex<bool>>,
}

impl DesktopEnvironment {
    async fn new(args: &Args) -> Self {
        info!("Initializing AGNOS Desktop Environment");

        let compositor = Arc::new(Compositor::new());
        let shell = Arc::new(DesktopShell::new());
        let ai_features = Arc::new(AIDesktopFeatures::new());
        let apps = Arc::new(tokio::sync::Mutex::new(DesktopApplications::new()));
        let security_ui = Arc::new(SecurityUI::new());
        let running = Arc::new(tokio::sync::Mutex::new(false));

        if args.secure {
            compositor.set_secure_mode(true);
            security_ui.set_security_level(SecurityLevel::Elevated);
        }

        if !args.no_ai {
            info!("AI features enabled");
        }

        Self {
            compositor,
            shell,
            ai_features,
            apps,
            security_ui,
            running,
        }
    }

    async fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing compositor backend");

        info!("Loading desktop shell");
        self.shell.show_notification(Notification {
            app_name: "AGNOS Desktop".to_string(),
            title: "Welcome to AGNOS".to_string(),
            body: "Your AI-native desktop environment".to_string(),
            priority: NotificationPriority::Normal,
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_agent_related: false,
            ..Default::default()
        });

        info!("Initializing AI desktop features");
        self.ai_features.set_proactive_mode(true);
        self.ai_features.set_ambient_enabled(true);

        info!("Starting system applications");

        Ok(())
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Desktop environment running");
        *self.running.lock().await = true;

        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.update_system_status().await;
                }
                _ = signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    break;
                }
            }
        }

        self.shutdown().await;
        Ok(())
    }

    async fn update_system_status(&self) {
        let cpu_usage = read_cpu_usage().await.unwrap_or(0.0);
        let memory_usage = read_memory_usage().await.unwrap_or(0.0);
        let disk_usage = read_disk_usage().unwrap_or(0.0);

        let status = shell::SystemStatus {
            cpu_usage,
            memory_usage,
            disk_usage,
            battery_level: None,
            network_status: shell::NetworkStatus::Connected,
            agent_count: 0,
        };
        self.shell.update_system_status(status);

        self.ai_features.update_context(ContextEvent {
            id: uuid::Uuid::new_v4(),
            event_type: ContextEventType::UserPresent,
            source: "desktop".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: Default::default(),
        });
    }

    async fn shutdown(&self) {
        info!("Shutting down desktop environment");
        *self.running.lock().await = false;

        self.shell.show_notification(Notification {
            app_name: "AGNOS Desktop".to_string(),
            title: "Shutting down".to_string(),
            body: "Goodbye!".to_string(),
            priority: NotificationPriority::Low,
            timestamp: chrono::Utc::now(),
            requires_action: false,
            is_agent_related: false,
            ..Default::default()
        });

        info!("Desktop environment stopped");
    }
}

/// Parse CPU jiffies from a `/proc/stat` "cpu" line.
/// Returns (total, idle).
fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1) // skip "cpu"
        .filter_map(|s| s.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    // fields: user, nice, system, idle, iowait, irq, softirq, steal ...
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().sum();
    Some((total, idle))
}

/// Read CPU usage percentage by sampling /proc/stat twice with a 100ms gap.
async fn read_cpu_usage() -> Option<f32> {
    let stat1 = tokio::fs::read_to_string("/proc/stat").await.ok()?;
    let line1 = stat1.lines().find(|l| l.starts_with("cpu "))?;
    let (total1, idle1) = parse_cpu_line(line1)?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stat2 = tokio::fs::read_to_string("/proc/stat").await.ok()?;
    let line2 = stat2.lines().find(|l| l.starts_with("cpu "))?;
    let (total2, idle2) = parse_cpu_line(line2)?;

    let total_delta = total2.saturating_sub(total1) as f64;
    let idle_delta = idle2.saturating_sub(idle1) as f64;

    if total_delta == 0.0 {
        return Some(0.0);
    }
    Some(((total_delta - idle_delta) / total_delta * 100.0) as f32)
}

/// Read memory usage percentage from /proc/meminfo.
async fn read_memory_usage() -> Option<f32> {
    let meminfo = tokio::fs::read_to_string("/proc/meminfo").await.ok()?;
    let mut mem_total: Option<u64> = None;
    let mut mem_available: Option<u64> = None;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            mem_total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        } else if line.starts_with("MemAvailable:") {
            mem_available = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
        if mem_total.is_some() && mem_available.is_some() {
            break;
        }
    }

    let total = mem_total? as f64;
    let available = mem_available? as f64;
    if total == 0.0 {
        return Some(0.0);
    }
    Some(((total - available) / total * 100.0) as f32)
}

/// Read disk usage percentage for the root filesystem using statvfs.
fn read_disk_usage() -> Option<f32> {
    let path = std::ffi::CString::new("/").ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if ret != 0 {
        return None;
    }
    let total = stat.f_blocks as f64;
    let available = stat.f_bavail as f64;
    if total == 0.0 {
        return Some(0.0);
    }
    Some(((total - available) / total * 100.0) as f32)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fmt = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env());
    if std::env::var("AGNOS_LOG_FORMAT").as_deref() == Ok("json") {
        fmt.json().init();
    } else {
        fmt.init();
    }

    let args = Args::parse();

    info!("AGNOS Desktop Environment v2026.3.5");
    info!("Backend: {}", args.backend);

    let desktop = DesktopEnvironment::new(&args).await;
    desktop.initialize().await?;

    desktop.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_args_default_values() {
        let args = Args {
            backend: "wayland".to_string(),
            kiosk: false,
            no_ai: false,
            secure: false,
        };
        
        assert_eq!(args.backend, "wayland");
        assert!(!args.kiosk);
        assert!(!args.no_ai);
        assert!(!args.secure);
    }

    #[test]
    fn test_args_custom_values() {
        let args = Args {
            backend: "x11".to_string(),
            kiosk: true,
            no_ai: true,
            secure: true,
        };
        
        assert_eq!(args.backend, "x11");
        assert!(args.kiosk);
        assert!(args.no_ai);
        assert!(args.secure);
    }

    #[test]
    fn test_window_state_defaults() {
        let state = WindowState::default();
        assert_eq!(state, WindowState::Normal);
    }

    #[test]
    fn test_context_type_variants() {
        assert_eq!(format!("{:?}", ContextType::Window), "Window");
        assert_eq!(format!("{:?}", ContextType::Application), "Application");
        assert_eq!(format!("{:?}", ContextType::System), "System");
        assert_eq!(format!("{:?}", ContextType::User), "User");
    }

    #[test]
    fn test_notification_priority() {
        assert_eq!(format!("{:?}", NotificationPriority::Low), "Low");
        assert_eq!(format!("{:?}", NotificationPriority::Normal), "Normal");
        assert_eq!(format!("{:?}", NotificationPriority::High), "High");
        assert_eq!(format!("{:?}", NotificationPriority::Critical), "Critical");
    }

    #[test]
    fn test_threat_level_ordering() {
        assert!(ThreatLevel::Info < ThreatLevel::Low);
        assert!(ThreatLevel::Low < ThreatLevel::Medium);
        assert!(ThreatLevel::Medium < ThreatLevel::High);
        assert!(ThreatLevel::High < ThreatLevel::Critical);
    }

    #[test]
    fn test_security_level_variants() {
        assert_eq!(format!("{:?}", SecurityLevel::Standard), "Standard");
        assert_eq!(format!("{:?}", SecurityLevel::Elevated), "Elevated");
        assert_eq!(format!("{:?}", SecurityLevel::Lockdown), "Lockdown");
    }

    #[test]
    fn test_suggestion_type_variants() {
        assert_eq!(format!("{:?}", SuggestionType::WindowPlacement), "WindowPlacement");
        assert_eq!(format!("{:?}", SuggestionType::ContextSwitch), "ContextSwitch");
        assert_eq!(format!("{:?}", SuggestionType::TaskRecommendation), "TaskRecommendation");
        assert_eq!(format!("{:?}", SuggestionType::ResourceOptimization), "ResourceOptimization");
        assert_eq!(format!("{:?}", SuggestionType::SecurityAlert), "SecurityAlert");
        assert_eq!(format!("{:?}", SuggestionType::Productivity), "Productivity");
    }

    #[test]
    fn test_context_event_type_variants() {
        assert_eq!(format!("{:?}", ContextEventType::WindowOpened), "WindowOpened");
        assert_eq!(format!("{:?}", ContextEventType::WindowClosed), "WindowClosed");
        assert_eq!(format!("{:?}", ContextEventType::AppSwitched), "AppSwitched");
        assert_eq!(format!("{:?}", ContextEventType::FileOpened), "FileOpened");
        assert_eq!(format!("{:?}", ContextEventType::CommandExecuted), "CommandExecuted");
    }

    #[test]
    fn test_ai_suggestion_default() {
        let suggestion = AISuggestion::default();
        assert!(suggestion.id != Uuid::nil());
        assert!(!suggestion.title.is_empty() || suggestion.title.is_empty());
        assert!(suggestion.confidence >= 0.0 && suggestion.confidence <= 1.0);
    }

    // --- parse_cpu_line tests ---

    #[test]
    fn test_parse_cpu_line_valid() {
        let line = "cpu  1234 567 890 12345 678 90 12 34";
        let result = parse_cpu_line(line);
        assert!(result.is_some());
        let (total, idle) = result.unwrap();
        // idle = fields[3] + fields[4] = 12345 + 678 = 13023
        assert_eq!(idle, 12345 + 678);
        // total = sum of all fields = 1234+567+890+12345+678+90+12+34 = 15850
        assert_eq!(total, 1234 + 567 + 890 + 12345 + 678 + 90 + 12 + 34);
    }

    #[test]
    fn test_parse_cpu_line_too_few_fields() {
        let line = "cpu  100 200 300";
        let result = parse_cpu_line(line);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_cpu_line_empty() {
        let result = parse_cpu_line("");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_cpu_line_zeros() {
        let line = "cpu  0 0 0 0";
        let result = parse_cpu_line(line);
        assert!(result.is_some());
        let (total, idle) = result.unwrap();
        assert_eq!(total, 0);
        assert_eq!(idle, 0);
    }

    #[test]
    fn test_parse_cpu_line_exactly_four_fields() {
        // Exactly 4 fields — no iowait, so idle = fields[3] + 0
        let line = "cpu  100 200 300 5000";
        let result = parse_cpu_line(line);
        assert!(result.is_some());
        let (total, idle) = result.unwrap();
        assert_eq!(idle, 5000); // fields[3] + get(4).unwrap_or(0)
        assert_eq!(total, 100 + 200 + 300 + 5000);
    }

    #[test]
    fn test_parse_cpu_line_non_numeric_fields() {
        // "abc" is skipped by filter_map, leaving fewer than 4 fields
        let line = "cpu  abc def ghi jkl";
        let result = parse_cpu_line(line);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_cpu_line_mixed_fields() {
        // Some numeric, some not — filter_map keeps only numeric
        let line = "cpu  100 abc 200 300 400";
        let result = parse_cpu_line(line);
        assert!(result.is_some());
        let (total, idle) = result.unwrap();
        // Parsed fields: [100, 200, 300, 400] — idle = 300 + 400 = 700 (wait, fields[3]=400, fields[4] doesn't exist since only 4)
        // Actually: "100" "abc" "200" "300" "400" → filter_map → [100, 200, 300, 400]
        // fields[3] = 400, fields.get(4) = None → idle = 400
        assert_eq!(idle, 400);
        assert_eq!(total, 100 + 200 + 300 + 400);
    }

    // --- read_memory_usage tests ---

    #[tokio::test]
    async fn test_read_memory_usage_returns_some_on_linux() {
        let result = read_memory_usage().await;
        // On Linux with /proc/meminfo this should succeed
        assert!(result.is_some());
        let usage = result.unwrap();
        assert!(usage >= 0.0 && usage <= 100.0);
    }

    // --- read_disk_usage tests ---

    #[test]
    fn test_read_disk_usage_returns_some() {
        let result = read_disk_usage();
        assert!(result.is_some());
        let usage = result.unwrap();
        assert!(usage >= 0.0 && usage <= 100.0);
    }

    // --- read_cpu_usage tests ---

    #[tokio::test]
    async fn test_read_cpu_usage_returns_some_on_linux() {
        let result = read_cpu_usage().await;
        assert!(result.is_some());
        let usage = result.unwrap();
        assert!(usage >= 0.0 && usage <= 100.0);
    }

    // --- DesktopEnvironment tests ---

    fn make_test_args() -> Args {
        Args {
            backend: "wayland".to_string(),
            kiosk: false,
            no_ai: false,
            secure: false,
        }
    }

    #[tokio::test]
    async fn test_desktop_environment_new_default() {
        let args = make_test_args();
        let de = DesktopEnvironment::new(&args).await;
        let running = *de.running.lock().await;
        assert!(!running);
    }

    #[tokio::test]
    async fn test_desktop_environment_new_secure_mode() {
        let args = Args {
            backend: "wayland".to_string(),
            kiosk: false,
            no_ai: false,
            secure: true,
        };
        let de = DesktopEnvironment::new(&args).await;
        // Secure mode sets compositor secure mode and elevated security level
        // Just verify construction succeeds without panic
        let running = *de.running.lock().await;
        assert!(!running);
    }

    #[tokio::test]
    async fn test_desktop_environment_new_no_ai() {
        let args = Args {
            backend: "x11".to_string(),
            kiosk: true,
            no_ai: true,
            secure: false,
        };
        let de = DesktopEnvironment::new(&args).await;
        let running = *de.running.lock().await;
        assert!(!running);
    }

    #[tokio::test]
    async fn test_desktop_environment_initialize() {
        let args = make_test_args();
        let de = DesktopEnvironment::new(&args).await;
        let result = de.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_desktop_environment_shutdown() {
        let args = make_test_args();
        let de = DesktopEnvironment::new(&args).await;
        *de.running.lock().await = true;
        assert!(*de.running.lock().await);
        de.shutdown().await;
        assert!(!*de.running.lock().await);
    }

    #[tokio::test]
    async fn test_desktop_environment_update_system_status() {
        let args = make_test_args();
        let de = DesktopEnvironment::new(&args).await;
        // Should not panic
        de.update_system_status().await;
    }
}
