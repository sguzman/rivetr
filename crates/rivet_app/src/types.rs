use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceTab {
    Tasks,
    Kanban,
    Calendar,
}

impl WorkspaceTab {
    pub const ALL: [Self; 3] = [Self::Tasks, Self::Kanban, Self::Calendar];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Tasks => "Tasks",
            Self::Kanban => "Kanban",
            Self::Calendar => "Calendar",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Day,
    Night,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Completed,
    Deleted,
    Waiting,
}

impl TaskStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Completed => "Completed",
            Self::Deleted => "Deleted",
            Self::Waiting => "Waiting",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
}

impl TaskPriority {
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDto {
    pub uuid: Uuid,
    pub id: Option<u64>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub priority: Option<TaskPriority>,
    pub due: Option<String>,
    pub wait: Option<String>,
    pub scheduled: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCreate {
    pub title: String,
    pub description: String,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub priority: Option<TaskPriority>,
    pub due: Option<String>,
    pub wait: Option<String>,
    pub scheduled: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub project: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub priority: Option<Option<TaskPriority>>,
    pub due: Option<Option<String>>,
    pub wait: Option<Option<String>>,
    pub scheduled: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUpdateArgs {
    pub uuid: Uuid,
    pub patch: TaskPatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StatusFilter {
    #[default]
    All,
    Pending,
    Waiting,
    Completed,
    Deleted,
}

impl StatusFilter {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Pending,
        Self::Waiting,
        Self::Completed,
        Self::Deleted,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Pending => "Pending",
            Self::Waiting => "Waiting",
            Self::Completed => "Completed",
            Self::Deleted => "Deleted",
        }
    }

    pub const fn matches(self, status: TaskStatus) -> bool {
        match self {
            Self::All => true,
            Self::Pending => matches!(status, TaskStatus::Pending),
            Self::Waiting => matches!(status, TaskStatus::Waiting),
            Self::Completed => matches!(status, TaskStatus::Completed),
            Self::Deleted => matches!(status, TaskStatus::Deleted),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PriorityFilter {
    #[default]
    All,
    Low,
    Medium,
    High,
    None,
}

impl PriorityFilter {
    pub const ALL: [Self; 5] = [Self::All, Self::Low, Self::Medium, Self::High, Self::None];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::None => "None",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DueFilter {
    #[default]
    All,
    HasDue,
    NoDue,
}

impl DueFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::HasDue, Self::NoDue];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::HasDue => "Has Due",
            Self::NoDue => "No Due",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskFilters {
    pub search: String,
    pub status: StatusFilter,
    pub project: String,
    pub tag: String,
    pub priority: PriorityFilter,
    pub due: DueFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KanbanBoard {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarView {
    Year,
    Quarter,
    Month,
    Week,
    Day,
}

impl CalendarView {
    pub const ALL: [Self; 5] = [Self::Year, Self::Quarter, Self::Month, Self::Week, Self::Day];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Year => "Year",
            Self::Quarter => "Quarter",
            Self::Month => "Month",
            Self::Week => "Week",
            Self::Day => "Day",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedCalendarSource {
    pub id: String,
    pub name: String,
    pub color: String,
    pub path: PathBuf,
    pub last_imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSchema {
    pub version: Option<u32>,
    #[serde(default)]
    pub keys: Vec<TagKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagKey {
    pub id: String,
    pub label: Option<String>,
    pub selection: Option<String>,
    pub color: Option<String>,
    pub allow_custom_values: Option<bool>,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub mode: Option<String>,
    pub app: Option<RuntimeAppConfig>,
    pub time: Option<RuntimeTimeConfig>,
    pub ui: Option<RuntimeUiConfig>,
    pub calendar: Option<RuntimeCalendarConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeAppConfig {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTimeConfig {
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeUiConfig {
    pub default_theme: Option<String>,
    pub theme: Option<RuntimeThemeConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeThemeConfig {
    pub mode: Option<String>,
    pub follow_system: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCalendarConfig {
    pub timezone: Option<String>,
    pub policies: Option<RuntimeCalendarPolicies>,
    pub visibility: Option<RuntimeCalendarVisibility>,
    pub day_view: Option<RuntimeCalendarDayView>,
    pub toggles: Option<RuntimeCalendarToggles>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCalendarPolicies {
    pub week_start: Option<String>,
    pub red_dot_limit: Option<usize>,
    pub task_list_limit: Option<usize>,
    pub task_list_window_days: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCalendarVisibility {
    pub pending: Option<bool>,
    pub waiting: Option<bool>,
    pub completed: Option<bool>,
    pub deleted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCalendarDayView {
    pub hour_start: Option<u8>,
    pub hour_end: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCalendarToggles {
    pub de_emphasize_past_periods: Option<bool>,
    pub filter_tasks_before_now: Option<bool>,
    pub hide_past_markers: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarConfig {
    pub timezone: String,
    pub week_start_monday: bool,
    pub red_dot_limit: usize,
    pub task_list_limit: usize,
    pub task_list_window_days: i64,
    pub visibility_pending: bool,
    pub visibility_waiting: bool,
    pub visibility_completed: bool,
    pub visibility_deleted: bool,
    pub de_emphasize_past_periods: bool,
    pub filter_before_now: bool,
    pub hide_past_markers: bool,
    pub day_view_hour_start: u8,
    pub day_view_hour_end: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarMarkerKind {
    ExternalCalendar,
    KanbanBoard,
    Unassigned,
}

#[derive(Debug, Clone)]
pub struct CalendarEntry {
    pub task: TaskDto,
    pub due_utc: chrono::DateTime<chrono::Utc>,
    pub label: String,
    pub color: String,
    pub marker_kind: CalendarMarkerKind,
    pub board_id: Option<String>,
    pub source_id: Option<String>,
}
