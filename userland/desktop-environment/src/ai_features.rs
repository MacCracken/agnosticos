use chrono::Timelike;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AIFeatureError {
    #[error("Context not found: {0}")]
    ContextNotFound(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("Model error: {0}")]
    ModelError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionType {
    WindowPlacement,
    ContextSwitch,
    TaskRecommendation,
    ResourceOptimization,
    SecurityAlert,
    Productivity,
}

#[derive(Debug, Clone)]
pub struct AISuggestion {
    pub id: Uuid,
    pub suggestion_type: SuggestionType,
    pub title: String,
    pub description: String,
    pub confidence: f32,
    pub action: Option<String>,
    pub is_dismissed: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Default for AISuggestion {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            suggestion_type: SuggestionType::Productivity,
            title: String::new(),
            description: String::new(),
            confidence: 0.0,
            action: None,
            is_dismissed: false,
            timestamp: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentHUDState {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub status: AgentStatus,
    pub current_task: String,
    pub progress: f32,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub resource_usage: ResourceMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Acting,
    Waiting,
    Error,
}

#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub gpu_percent: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ContextEvent {
    pub id: Uuid,
    pub event_type: ContextEventType,
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEventType {
    WindowOpened,
    WindowClosed,
    AppSwitched,
    FileOpened,
    CommandExecuted,
    MeetingStarted,
    MeetingEnded,
    UserAway,
    UserPresent,
}

#[derive(Debug)]
pub struct AIDesktopFeatures {
    suggestions: Arc<RwLock<Vec<AISuggestion>>>,
    agent_hud: Arc<RwLock<HashMap<Uuid, AgentHUDState>>>,
    context_history: Arc<RwLock<Vec<ContextEvent>>>,
    current_context: Arc<RwLock<DesktopContext>>,
    proactive_mode: Arc<RwLock<bool>>,
    ambient_enabled: Arc<RwLock<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopContext {
    pub context_type: ContextType,
    pub active_apps: Vec<String>,
    pub open_files: Vec<String>,
    pub recent_commands: Vec<String>,
    pub time_of_day: TimeOfDay,
    pub user_activity_level: ActivityLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextType {
    Development,
    Writing,
    Design,
    Communication,
    Browsing,
    Gaming,
    Idle,
    Meeting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityLevel {
    High,
    Medium,
    Low,
    Idle,
}

impl Default for DesktopContext {
    fn default() -> Self {
        Self {
            context_type: ContextType::Idle,
            active_apps: Vec::new(),
            open_files: Vec::new(),
            recent_commands: Vec::new(),
            time_of_day: TimeOfDay::Afternoon,
            user_activity_level: ActivityLevel::Medium,
        }
    }
}

impl AIDesktopFeatures {
    pub fn new() -> Self {
        Self {
            suggestions: Arc::new(RwLock::new(Vec::new())),
            agent_hud: Arc::new(RwLock::new(HashMap::new())),
            context_history: Arc::new(RwLock::new(Vec::new())),
            current_context: Arc::new(RwLock::new(DesktopContext::default())),
            proactive_mode: Arc::new(RwLock::new(true)),
            ambient_enabled: Arc::new(RwLock::new(true)),
        }
    }

    pub fn analyze_context(&self) -> DesktopContext {
        let mut context = self.current_context.write().unwrap();

        let hour = chrono::Utc::now().hour();
        context.time_of_day = match hour {
            5..=11 => TimeOfDay::Morning,
            12..=16 => TimeOfDay::Afternoon,
            17..=20 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        };

        info!(
            "Context analyzed: {:?} at {:?}",
            context.context_type, context.time_of_day
        );
        context.clone()
    }

    pub fn update_context(&self, event: ContextEvent) {
        let mut history = self.context_history.write().unwrap();
        history.push(event.clone());

        let len = history.len();
        if len > 100 {
            history.drain(0..len - 100);
        }

        let mut context = self.current_context.write().unwrap();
        match event.event_type {
            ContextEventType::WindowOpened => {
                if let Some(app) = event.metadata.get("app") {
                    if !context.active_apps.contains(app) {
                        context.active_apps.push(app.clone());
                    }
                }
            }
            ContextEventType::FileOpened => {
                if let Some(file) = event.metadata.get("file") {
                    context.open_files.push(file.clone());
                    let len = context.open_files.len();
                    if len > 10 {
                        context.open_files.drain(0..len - 10);
                    }
                }
            }
            _ => {}
        }

        self.detect_context_type();
        debug!("Context updated: {:?}", context.context_type);
    }

    fn detect_context_type(&self) {
        let mut context = self.current_context.write().unwrap();

        let has_dev_tools = context
            .active_apps
            .iter()
            .any(|app| app.contains("code") || app.contains("terminal") || app.contains("editor"));
        let has_design = context
            .active_apps
            .iter()
            .any(|app| app.contains("gimp") || app.contains("figma") || app.contains("blender"));
        let has_comm = context
            .active_apps
            .iter()
            .any(|app| app.contains("slack") || app.contains("discord") || app.contains("mail"));

        context.context_type = if has_dev_tools {
            ContextType::Development
        } else if has_design {
            ContextType::Design
        } else if has_comm {
            ContextType::Communication
        } else if context.active_apps.iter().any(|a| a.contains("browser")) {
            ContextType::Browsing
        } else {
            ContextType::Idle
        };
    }

    pub fn generate_suggestion(
        &self,
        suggestion_type: SuggestionType,
        title: String,
        description: String,
        confidence: f32,
    ) -> AISuggestion {
        AISuggestion {
            id: Uuid::new_v4(),
            suggestion_type,
            title,
            description,
            confidence,
            action: None,
            is_dismissed: false,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn add_suggestion(&self, suggestion: AISuggestion) {
        let title = suggestion.title.clone();
        let mut suggestions = self.suggestions.write().unwrap();

        suggestions.retain(|s| {
            s.suggestion_type != suggestion.suggestion_type || s.title != suggestion.title
        });

        suggestions.push(suggestion);
        suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        debug!("Suggestion added: {}", title);
    }

    pub fn proactive_suggestions(&self) -> Vec<AISuggestion> {
        let context = self.analyze_context();
        let suggestions = self.suggestions.write().unwrap();

        let relevant: Vec<_> = suggestions
            .iter()
            .filter(|s| !s.is_dismissed && s.confidence > 0.5)
            .cloned()
            .collect();

        relevant
    }

    pub fn smart_window_placement(&self, app_id: &str) -> (i32, i32, u32, u32) {
        let context = self.current_context.read().unwrap();
        match context.context_type {
            ContextType::Development => (100, 100, 1400, 900),
            ContextType::Design => (50, 50, 1820, 1000),
            ContextType::Communication => (200, 500, 600, 400),
            _ => (100, 100, 1200, 800),
        }
    }

    pub fn register_agent_hud(&self, agent_id: Uuid, name: String) {
        let name_clone = name.clone();
        let mut hud = self.agent_hud.write().unwrap();
        hud.insert(
            agent_id,
            AgentHUDState {
                agent_id,
                agent_name: name_clone,
                status: AgentStatus::Idle,
                current_task: String::new(),
                progress: 0.0,
                last_activity: chrono::Utc::now(),
                resource_usage: ResourceMetrics {
                    cpu_percent: 0.0,
                    memory_mb: 0,
                    gpu_percent: None,
                },
            },
        );
        info!("Agent HUD registered: {}", name);
    }

    pub fn update_agent_hud(
        &self,
        agent_id: Uuid,
        status: AgentStatus,
        task: String,
        progress: f32,
    ) {
        let mut hud = self.agent_hud.write().unwrap();
        if let Some(state) = hud.get_mut(&agent_id) {
            state.status = status;
            state.current_task = task;
            state.progress = progress;
            state.last_activity = chrono::Utc::now();
        }
    }

    pub fn unregister_agent_hud(&self, agent_id: Uuid) {
        let mut hud = self.agent_hud.write().unwrap();
        hud.remove(&agent_id);
        debug!("Agent HUD unregistered: {}", agent_id);
    }

    pub fn get_agent_hud_states(&self) -> Vec<AgentHUDState> {
        self.agent_hud.read().unwrap().values().cloned().collect()
    }

    pub fn set_proactive_mode(&self, enabled: bool) {
        *self.proactive_mode.write().unwrap() = enabled;
        info!("Proactive mode: {}", enabled);
    }

    pub fn set_ambient_enabled(&self, enabled: bool) {
        *self.ambient_enabled.write().unwrap() = enabled;
        info!("Ambient intelligence: {}", enabled);
    }

    pub fn dismiss_suggestion(&self, suggestion_id: Uuid) {
        let mut suggestions = self.suggestions.write().unwrap();
        if let Some(s) = suggestions.iter_mut().find(|s| s.id == suggestion_id) {
            s.is_dismissed = true;
            debug!("Suggestion dismissed: {}", suggestion_id);
        }
    }

    pub fn get_context(&self) -> DesktopContext {
        self.current_context.read().unwrap().clone()
    }

    pub fn suggest_workspace_switch(&self, from: usize, to: usize, reason: String) -> AISuggestion {
        self.generate_suggestion(
            SuggestionType::ContextSwitch,
            format!("Switch to Workspace {}", to + 1),
            reason,
            0.75,
        )
    }

    pub fn optimize_resources(&self) -> AISuggestion {
        self.generate_suggestion(
            SuggestionType::ResourceOptimization,
            "Resource Optimization".to_string(),
            "Consider suspending idle agents to free resources".to_string(),
            0.65,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestion_type_variants() {
        assert!(matches!(
            SuggestionType::WindowPlacement,
            SuggestionType::WindowPlacement
        ));
        assert!(matches!(
            SuggestionType::ContextSwitch,
            SuggestionType::ContextSwitch
        ));
        assert!(matches!(
            SuggestionType::TaskRecommendation,
            SuggestionType::TaskRecommendation
        ));
        assert!(matches!(
            SuggestionType::SecurityAlert,
            SuggestionType::SecurityAlert
        ));
    }

    #[test]
    fn test_ai_suggestion_default() {
        let suggestion = AISuggestion::default();
        assert_eq!(suggestion.suggestion_type, SuggestionType::Productivity);
        assert!(!suggestion.is_dismissed);
        assert_eq!(suggestion.confidence, 0.0);
    }

    #[test]
    fn test_ai_suggestion_custom() {
        let suggestion = AISuggestion {
            id: Uuid::new_v4(),
            suggestion_type: SuggestionType::TaskRecommendation,
            title: "Take a break".to_string(),
            description: "You've been working for 2 hours".to_string(),
            confidence: 0.85,
            action: Some("break".to_string()),
            is_dismissed: false,
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(suggestion.confidence, 0.85);
        assert!(suggestion.action.is_some());
    }

    #[test]
    fn test_agent_status_variants() {
        assert!(matches!(AgentStatus::Idle, AgentStatus::Idle));
        assert!(matches!(AgentStatus::Thinking, AgentStatus::Thinking));
        assert!(matches!(AgentStatus::Acting, AgentStatus::Acting));
        assert!(matches!(AgentStatus::Waiting, AgentStatus::Waiting));
        assert!(matches!(AgentStatus::Error, AgentStatus::Error));
    }

    #[test]
    fn test_agent_hud_state() {
        let state = AgentHUDState {
            agent_id: Uuid::new_v4(),
            agent_name: "test-agent".to_string(),
            status: AgentStatus::Thinking,
            current_task: "Processing data".to_string(),
            progress: 0.5,
            last_activity: chrono::Utc::now(),
            resource_usage: ResourceMetrics {
                cpu_percent: 25.0,
                memory_mb: 512,
                gpu_percent: Some(50.0),
            },
        };
        assert_eq!(state.status, AgentStatus::Thinking);
        assert_eq!(state.progress, 0.5);
    }

    #[test]
    fn test_resource_metrics() {
        let metrics = ResourceMetrics {
            cpu_percent: 30.0,
            memory_mb: 1024,
            gpu_percent: Some(75.0),
        };
        assert_eq!(metrics.cpu_percent, 30.0);
        assert_eq!(metrics.memory_mb, 1024);
        assert!(metrics.gpu_percent.is_some());
    }

    #[test]
    fn test_time_of_day_variants() {
        assert!(matches!(TimeOfDay::Morning, TimeOfDay::Morning));
        assert!(matches!(TimeOfDay::Afternoon, TimeOfDay::Afternoon));
        assert!(matches!(TimeOfDay::Evening, TimeOfDay::Evening));
        assert!(matches!(TimeOfDay::Night, TimeOfDay::Night));
    }

    #[test]
    fn test_activity_level_variants() {
        assert!(matches!(ActivityLevel::Low, ActivityLevel::Low));
        assert!(matches!(ActivityLevel::Medium, ActivityLevel::Medium));
        assert!(matches!(ActivityLevel::High, ActivityLevel::High));
        assert!(matches!(ActivityLevel::Idle, ActivityLevel::Idle));
    }

    #[test]
    fn test_context_type_variants() {
        assert!(matches!(ContextType::Development, ContextType::Development));
        assert!(matches!(
            ContextType::Communication,
            ContextType::Communication
        ));
        assert!(matches!(ContextType::Design, ContextType::Design));
    }

    #[test]
    fn test_context_event_type_variants() {
        assert!(matches!(
            ContextEventType::WindowOpened,
            ContextEventType::WindowOpened
        ));
        assert!(matches!(
            ContextEventType::WindowClosed,
            ContextEventType::WindowClosed
        ));
        assert!(matches!(
            ContextEventType::FileOpened,
            ContextEventType::FileOpened
        ));
    }
}
