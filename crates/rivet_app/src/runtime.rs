use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono_tz::Tz;

use crate::types::{CalendarConfig, RuntimeConfig, TagSchema, ThemeMode};

pub struct RuntimeConfigService {
    pub config_path: Option<PathBuf>,
    pub tag_schema: TagSchema,
    pub calendar: CalendarConfig,
    pub theme: ThemeMode,
}

impl RuntimeConfigService {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = resolve_config_path("rivet.toml");
        let runtime = if let Some(path) = config_path.as_ref().filter(|path| path.is_file()) {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("failed to read runtime config {}", path.display()))?;
            toml::from_str::<RuntimeConfig>(&raw)
                .with_context(|| format!("failed to parse runtime config {}", path.display()))?
        } else {
            RuntimeConfig {
                mode: None,
                app: None,
                time: None,
                ui: None,
                calendar: None,
            }
        };

        let tag_schema_path = resolve_config_path("assets/tags.toml")
            .ok_or_else(|| anyhow::anyhow!("failed to locate assets/tags.toml"))?;
        let tag_schema_raw = fs::read_to_string(&tag_schema_path)
            .with_context(|| format!("failed to read tag schema {}", tag_schema_path.display()))?;
        let tag_schema = toml::from_str::<TagSchema>(&tag_schema_raw)
            .with_context(|| format!("failed to parse tag schema {}", tag_schema_path.display()))?;

        let timezone = runtime
            .calendar
            .as_ref()
            .and_then(|calendar| calendar.timezone.clone())
            .or_else(|| runtime.time.as_ref().and_then(|time| time.timezone.clone()))
            .unwrap_or_else(|| "America/Mexico_City".to_string());
        let _validated_tz = timezone
            .parse::<Tz>()
            .with_context(|| format!("invalid timezone {timezone}"))?;

        let day_view_hour_start = runtime
            .calendar
            .as_ref()
            .and_then(|calendar| calendar.day_view.as_ref())
            .and_then(|day_view| day_view.hour_start)
            .unwrap_or(0)
            .min(23);
        let day_view_hour_end = runtime
            .calendar
            .as_ref()
            .and_then(|calendar| calendar.day_view.as_ref())
            .and_then(|day_view| day_view.hour_end)
            .unwrap_or(23)
            .min(23)
            .max(day_view_hour_start);

        let calendar = CalendarConfig {
            timezone,
            week_start_monday: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.policies.as_ref())
                .and_then(|policies| policies.week_start.as_ref())
                .map(|value| value.eq_ignore_ascii_case("monday"))
                .unwrap_or(false),
            red_dot_limit: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.policies.as_ref())
                .and_then(|policies| policies.red_dot_limit)
                .unwrap_or(5),
            task_list_limit: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.policies.as_ref())
                .and_then(|policies| policies.task_list_limit)
                .unwrap_or(200),
            task_list_window_days: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.policies.as_ref())
                .and_then(|policies| policies.task_list_window_days)
                .unwrap_or(365),
            visibility_pending: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.visibility.as_ref())
                .and_then(|visibility| visibility.pending)
                .unwrap_or(true),
            visibility_waiting: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.visibility.as_ref())
                .and_then(|visibility| visibility.waiting)
                .unwrap_or(true),
            visibility_completed: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.visibility.as_ref())
                .and_then(|visibility| visibility.completed)
                .unwrap_or(true),
            visibility_deleted: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.visibility.as_ref())
                .and_then(|visibility| visibility.deleted)
                .unwrap_or(true),
            de_emphasize_past_periods: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.toggles.as_ref())
                .and_then(|toggles| toggles.de_emphasize_past_periods)
                .unwrap_or(true),
            filter_before_now: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.toggles.as_ref())
                .and_then(|toggles| toggles.filter_tasks_before_now)
                .unwrap_or(true),
            hide_past_markers: runtime
                .calendar
                .as_ref()
                .and_then(|calendar| calendar.toggles.as_ref())
                .and_then(|toggles| toggles.hide_past_markers)
                .unwrap_or(true),
            day_view_hour_start,
            day_view_hour_end,
        };

        let theme = resolve_theme_mode(&runtime);

        Ok(Self {
            config_path,
            tag_schema,
            calendar,
            theme,
        })
    }
}

fn resolve_theme_mode(runtime: &RuntimeConfig) -> ThemeMode {
    let raw = runtime
        .ui
        .as_ref()
        .and_then(|ui| ui.theme.as_ref())
        .and_then(|theme| theme.mode.as_deref())
        .or_else(|| runtime.ui.as_ref().and_then(|ui| ui.default_theme.as_deref()));
    match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("night") | Some("dark") => ThemeMode::Night,
        _ => ThemeMode::Day,
    }
}

pub fn resolve_config_path(relative: &str) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut cursor = Some(cwd.as_path());
    while let Some(path) = cursor {
        let candidate = path.join(relative);
        if candidate.exists() {
            return Some(candidate);
        }
        cursor = path.parent();
    }
    None
}

pub fn resolve_gui_data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("RIVET_GUI_DATA") {
        return PathBuf::from(path);
    }
    if let Some(path) = dirs::data_local_dir() {
        return path.join("rivetr").join("gui_data");
    }
    Path::new(".rivet_gui_data").to_path_buf()
}

pub fn resolve_ui_state_path() -> PathBuf {
    if let Some(path) = dirs::data_local_dir() {
        return path.join("rivetr").join("ui-state.json");
    }
    Path::new(".rivetr-ui-state.json").to_path_buf()
}
