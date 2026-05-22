use std::fs;
use std::path::Path;
use anyhow::Context;
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::runtime::resolve_ui_state_path;
use crate::types::{
    CalendarView, ImportedCalendarSource, KanbanBoard, TaskFilters, ThemeMode, WorkspaceTab,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedUiState {
    pub version: u32,
    pub active_tab: WorkspaceTab,
    pub theme_mode: ThemeMode,
    pub task_filters: TaskFilters,
    pub kanban_filters: TaskFilters,
    pub active_board_id: Option<String>,
    pub kanban_boards: Vec<KanbanBoard>,
    pub kanban_compact: bool,
    pub calendar_view: CalendarView,
    pub calendar_focus_date: String,
    pub imported_calendars: Vec<ImportedCalendarSource>,
}

impl Default for PersistedUiState {
    fn default() -> Self {
        Self {
            version: 1,
            active_tab: WorkspaceTab::Tasks,
            theme_mode: ThemeMode::Day,
            task_filters: TaskFilters::default(),
            kanban_filters: TaskFilters::default(),
            active_board_id: Some("main".to_string()),
            kanban_boards: vec![KanbanBoard {
                id: "main".to_string(),
                name: "Main".to_string(),
                color: "#2f7df6".to_string(),
            }],
            kanban_compact: false,
            calendar_view: CalendarView::Month,
            calendar_focus_date: Local::now().date_naive().to_string(),
            imported_calendars: Vec::new(),
        }
    }
}

impl PersistedUiState {
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from_path(&resolve_ui_state_path())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to_path(&resolve_ui_state_path())
    }

    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read UI state {}", path.display()))?;
        match serde_json::from_str::<Self>(&raw) {
            Ok(state) => Ok(state),
            Err(_) => Ok(Self::default()),
        }
    }

    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(self).context("failed to encode UI state")?;
        fs::write(path, raw).with_context(|| format!("failed to write UI state {}", path.display()))
    }

    pub fn focus_date(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.calendar_focus_date, "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive())
    }

    pub fn set_focus_date(&mut self, date: NaiveDate) {
        self.calendar_focus_date = date.format("%Y-%m-%d").to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_state_has_main_board() {
        let state = PersistedUiState::default();
        assert_eq!(state.kanban_boards.len(), 1);
        assert_eq!(state.kanban_boards[0].id, "main");
    }

    #[test]
    fn focus_date_falls_back() {
        let state = PersistedUiState {
            calendar_focus_date: "bad-date".to_string(),
            ..PersistedUiState::default()
        };
        let parsed = state.focus_date();
        assert!(parsed <= Local::now().date_naive());
    }

    #[test]
    fn invalid_json_falls_back_to_default() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ui-state.json");
        fs::write(&path, "{ not valid json }").expect("write invalid state");
        let loaded = PersistedUiState::load_from_path(&path).expect("load fallback");
        assert_eq!(loaded.version, PersistedUiState::default().version);
        assert_eq!(loaded.active_tab, WorkspaceTab::Tasks);
    }

    #[test]
    fn imported_calendar_windows_path_roundtrips() {
        let state = PersistedUiState {
            imported_calendars: vec![ImportedCalendarSource {
                id: "calendar".to_string(),
                name: "Calendar".to_string(),
                color: "#ff0000".to_string(),
                path: std::path::PathBuf::from(r"C:\Users\me\calendar.ics"),
                last_imported_at: "2026-05-22T00:00:00Z".to_string(),
            }],
            ..PersistedUiState::default()
        };
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ui-state.json");
        state.save_to_path(&path).expect("save state");
        let loaded = PersistedUiState::load_from_path(&path).expect("load state");
        assert_eq!(
            loaded.imported_calendars[0].path,
            std::path::PathBuf::from(r"C:\Users\me\calendar.ics")
        );
    }
}
