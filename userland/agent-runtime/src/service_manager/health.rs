//! Health monitoring, cron scheduling, and task scheduler for the service manager.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Scheduled / Cron Agent Tasks
// ---------------------------------------------------------------------------

/// Parsed cron-like schedule expression.
///
/// Supports standard 5-field cron: `minute hour day_of_month month day_of_week`
///
/// Field syntax:
/// - `*` — match any value
/// - `N` — match exact value
/// - `*/N` — match every N-th value (step)
///
/// Examples: `*/5 * * * *` (every 5 min), `0 2 * * 0` (Sunday 2am), `0 */6 * * *` (every 6 hours)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule {
    /// The original expression string.
    pub expression: String,
    /// Human-readable description.
    pub description: String,
    // Parsed fields (private)
    #[serde(skip)]
    minute: CronField,
    #[serde(skip)]
    hour: CronField,
    #[serde(skip)]
    day_of_month: CronField,
    #[serde(skip)]
    month: CronField,
    #[serde(skip)]
    day_of_week: CronField,
}

#[derive(Debug, Clone, Default)]
enum CronField {
    #[default]
    Any,
    Exact(u32),
    Step(u32),
}

impl CronField {
    fn matches(&self, value: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Exact(v) => *v == value,
            CronField::Step(step) => {
                if *step == 0 {
                    return true;
                }
                value.is_multiple_of(*step)
            }
        }
    }

    fn parse(field: &str) -> anyhow::Result<Self> {
        if field == "*" {
            return Ok(CronField::Any);
        }
        if let Some(step) = field.strip_prefix("*/") {
            let n: u32 = step
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid step value: {}", step))?;
            if n == 0 {
                anyhow::bail!("Step value must be > 0");
            }
            return Ok(CronField::Step(n));
        }
        let n: u32 = field
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid cron field: {}", field))?;
        Ok(CronField::Exact(n))
    }
}

impl CronSchedule {
    /// Parse a cron expression string (5 fields: minute hour day month weekday).
    pub fn new(expression: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = expression.split_whitespace().collect();
        if parts.len() != 5 {
            anyhow::bail!(
                "Invalid cron expression '{}': expected 5 fields (minute hour day month weekday), got {}",
                expression,
                parts.len()
            );
        }

        let minute = CronField::parse(parts[0])?;
        let hour = CronField::parse(parts[1])?;
        let day_of_month = CronField::parse(parts[2])?;
        let month = CronField::parse(parts[3])?;
        let day_of_week = CronField::parse(parts[4])?;

        Ok(Self {
            expression: expression.to_string(),
            description: String::new(),
            minute,
            hour,
            day_of_month,
            month,
            day_of_week,
        })
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Check if a given datetime matches this schedule.
    pub fn matches(&self, dt: &DateTime<Utc>) -> bool {
        self.minute.matches(dt.minute())
            && self.hour.matches(dt.hour())
            && self.day_of_month.matches(dt.day())
            && self.month.matches(dt.month())
            && self
                .day_of_week
                .matches(dt.weekday().num_days_from_sunday())
    }

    /// Compute the next run time after the given datetime.
    ///
    /// Scans forward minute-by-minute up to 366 days. Returns None if no match is found.
    pub fn next_run_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        use chrono::Duration as CDuration;

        // Start from the next minute boundary
        let mut candidate = after.with_second(0)?.with_nanosecond(0)? + CDuration::minutes(1);

        // Scan up to 366 days (527040 minutes)
        let max_iterations = 366 * 24 * 60;
        for _ in 0..max_iterations {
            if self.matches(&candidate) {
                return Some(candidate);
            }
            candidate += CDuration::minutes(1);
        }

        None
    }
}

/// A scheduled task bound to a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    /// Unique task identifier.
    pub id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Name of the service to run.
    pub service_name: String,
    /// Schedule expression.
    pub schedule: CronSchedule,
    /// Whether the task is active.
    pub enabled: bool,
    /// When the task last ran.
    pub last_run: Option<DateTime<Utc>>,
    /// Computed next run time.
    pub next_run: Option<DateTime<Utc>>,
}

impl ScheduledTask {
    pub fn new(
        name: impl Into<String>,
        service_name: impl Into<String>,
        schedule: CronSchedule,
    ) -> Self {
        let next = schedule.next_run_after(Utc::now());
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            service_name: service_name.into(),
            schedule,
            enabled: true,
            last_run: None,
            next_run: next,
        }
    }
}

/// Manages scheduled tasks and determines which are due.
pub struct TaskScheduler {
    pub(crate) tasks: HashMap<Uuid, ScheduledTask>,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Register a new scheduled task.
    pub fn add_task(&mut self, task: ScheduledTask) -> anyhow::Result<()> {
        if task.name.is_empty() {
            anyhow::bail!("Task name cannot be empty");
        }
        info!(task_id = %task.id, name = %task.name, schedule = %task.schedule.expression, "Added scheduled task");
        self.tasks.insert(task.id, task);
        Ok(())
    }

    /// Remove a scheduled task by ID.
    pub fn remove_task(&mut self, id: &Uuid) -> Option<ScheduledTask> {
        self.tasks.remove(id)
    }

    /// Get all tasks whose next_run is at or before `now` and are enabled.
    pub fn due_tasks(&self, now: &DateTime<Utc>) -> Vec<&ScheduledTask> {
        self.tasks
            .values()
            .filter(|t| t.enabled && t.next_run.is_some_and(|nr| nr <= *now))
            .collect()
    }

    /// List all tasks.
    pub fn list_tasks(&self) -> Vec<&ScheduledTask> {
        self.tasks.values().collect()
    }

    /// Mark a task as completed and compute the next run time.
    pub fn mark_completed(&mut self, id: &Uuid, completed_at: DateTime<Utc>) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.last_run = Some(completed_at);
            task.next_run = task.schedule.next_run_after(completed_at);
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}
