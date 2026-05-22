use std::fs;
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
        let path = resolve_ui_state_path();
        if !path.is_file() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read UI state {}", path.display()))?;
        let state = serde_json::from_str::<Self>(&raw)
            .with_context(|| format!("failed to parse UI state {}", path.display()))?;
        Ok(state)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = resolve_ui_state_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(self).context("failed to encode UI state")?;
        fs::write(&path, raw).with_context(|| format!("failed to write UI state {}", path.display()))
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
}
