mod calendar_ui;
mod dialogs;
mod dictionary_ui;
mod kanban;
mod kanban_ui;
mod keyboard;
mod shell;
mod tasks;
mod contacts_ui;
mod map_ui;

use std::collections::{BTreeMap, BTreeSet};

use eframe::egui::{self, Color32, Vec2};
use eframe::{App, CreationContext, NativeOptions};
use uuid::Uuid;

use crate::calendar::{
    calendar_title, entries_for_day, entries_for_month, month_days, month_grid_start, period_entries,
    period_stats, quarter_months, shift_focus, should_show_entry_in_list, should_show_marker,
    visible_calendar_entries, week_days, year_months,
};
use crate::persistence::PersistedUiState;
use crate::runtime::RuntimeConfigService;
use crate::services::{can_complete_task, IcsImportResult, TaskService};
use crate::tags::{board_id_from_tags, set_single_tag_value, split_tags, BOARD_TAG_KEY};
use crate::types::{
    CalendarView, DueFilter, ImportedCalendarSource, KanbanBoard, PriorityFilter, StatusFilter, TagSchema,
    TaskCreate, TaskDto, TaskFilters, TaskPatch, TaskPriority, TaskStatus, TaskUpdateArgs,
    ThemeMode, WorkspaceTab,
};
use self::kanban::apply_drop_to_tags;

const TASK_SEARCH_ID: &str = "tasks.search";
const KANBAN_SEARCH_ID: &str = "kanban.search";

pub fn run() -> Result<(), String> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(1400.0, 920.0))
            .with_min_inner_size(Vec2::new(1100.0, 720.0))
            .with_title("Rivetr"),
        ..NativeOptions::default()
    };

    let result = eframe::run_native(
        "Rivetr",
        native_options,
        Box::new(|cc| {
            let app = RivetApp::new(cc).map_err(|error| error.to_string())?;
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(()) => Ok(()),
        Err(error) => Err(format_runtime_error(&error.to_string())),
    }
}

struct RivetApp {
    runtime: RuntimeConfigService,
    service: TaskService,
    ui_state: PersistedUiState,
    tasks: Vec<TaskDto>,
    selected_task: Option<Uuid>,
    selected_tasks: BTreeSet<Uuid>,
    task_editor: Option<TaskEditor>,
    board_editor: BoardEditor,
    bulk_project_input: String,
    bulk_tag_input: String,
    last_message: Option<String>,
    last_error: Option<String>,
    import_busy: bool,
    show_shortcuts: bool,
    dirty_ui_state: bool,
    dictionary_ui: dictionary_ui::DictionaryUi,
}

#[derive(Debug, Clone)]
struct TaskEditor {
    task_id: Option<Uuid>,
    title: String,
    description: String,
    project: String,
    tags: String,
    due: String,
    wait: String,
    scheduled: String,
    priority: Option<TaskPriority>,
    board_id: Option<String>,
}

#[derive(Default)]
struct BoardEditor {
    create_name: String,
    rename_name: String,
}

impl RivetApp {
    fn new(cc: &CreationContext<'_>) -> anyhow::Result<Self> {
        let runtime = RuntimeConfigService::load()?;
        let service = TaskService::open(&runtime.calendar, runtime.tag_schema.clone())?;
        let mut ui_state = PersistedUiState::load().unwrap_or_default();
        if matches!(ui_state.theme_mode, ThemeMode::Day) && matches!(runtime.theme, ThemeMode::Night) {
            ui_state.theme_mode = ThemeMode::Night;
        }
        apply_theme(&cc.egui_ctx, ui_state.theme_mode);
        let tasks = service.list_all()?;
        let rename_name = ui_state
            .active_board_id
            .as_ref()
            .and_then(|id| ui_state.kanban_boards.iter().find(|board| &board.id == id))
            .map(|board| board.name.clone())
            .unwrap_or_default();
            
        let dictionary_config = runtime.dictionary.clone();

        Ok(Self {
            runtime,
            service,
            ui_state,
            tasks,
            selected_task: None,
            selected_tasks: BTreeSet::new(),
            task_editor: None,
            board_editor: BoardEditor {
                create_name: String::new(),
                rename_name,
            },
            bulk_project_input: String::new(),
            bulk_tag_input: String::new(),
            last_message: None,
            last_error: None,
            import_busy: false,
            show_shortcuts: false,
            dirty_ui_state: false,
            dictionary_ui: dictionary_ui::DictionaryUi::new(dictionary_config),
        })
    }

    fn refresh_tasks(&mut self) {
        match self.service.list_all() {
            Ok(tasks) => {
                self.tasks = tasks;
                if self
                    .selected_task
                    .is_some_and(|uuid| !self.tasks.iter().any(|task| task.uuid == uuid))
                {
                    self.selected_task = None;
                }
                self.selected_tasks
                    .retain(|uuid| self.tasks.iter().any(|task| task.uuid == *uuid));
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_message(&mut self, message: impl Into<String>) {
        self.last_message = Some(message.into());
        self.last_error = None;
    }

    fn set_error(&mut self, error: anyhow::Error) {
        self.last_error = Some(format!("{error:#}"));
    }

    fn persist_ui_state(&mut self) {
        if !self.dirty_ui_state {
            return;
        }
        if let Err(error) = self.ui_state.save() {
            self.last_error = Some(format!("{error:#}"));
        } else {
            self.dirty_ui_state = false;
        }
    }

    fn task_schema(&self) -> &TagSchema {
        &self.runtime.tag_schema
    }

    fn mark_ui_dirty(&mut self) {
        self.dirty_ui_state = true;
    }

    fn visible_tasks<'a>(&'a self, filters: &TaskFilters) -> Vec<&'a TaskDto> {
        self.tasks
            .iter()
            .filter(|task| task_matches(task, filters))
            .collect()
    }

    fn selected_task_ref(&self) -> Option<&TaskDto> {
        let selected = self.selected_task?;
        self.tasks.iter().find(|task| task.uuid == selected)
    }

    fn apply_selected_action(&mut self, action: BulkAction) {
        if self.selected_tasks.is_empty()
            && let Some(uuid) = self.selected_task
        {
            self.selected_tasks.insert(uuid);
        }
        self.bulk_action(action);
    }

    fn open_new_task(&mut self, board_id: Option<String>) {
        self.task_editor = Some(TaskEditor {
            task_id: None,
            title: String::new(),
            description: String::new(),
            project: String::new(),
            tags: String::new(),
            due: String::new(),
            wait: String::new(),
            scheduled: String::new(),
            priority: None,
            board_id,
        });
    }

    fn open_edit_task(&mut self, task: &TaskDto) {
        self.task_editor = Some(TaskEditor {
            task_id: Some(task.uuid),
            title: task.title.clone(),
            description: task.description.clone(),
            project: task.project.clone().unwrap_or_default(),
            tags: task.tags.join(" "),
            due: task.due.clone().unwrap_or_default(),
            wait: task.wait.clone().unwrap_or_default(),
            scheduled: task.scheduled.clone().unwrap_or_default(),
            priority: task.priority,
            board_id: board_id_from_tags(&task.tags),
        });
    }

    fn save_task_editor(&mut self) {
        let Some(editor) = self.task_editor.clone() else {
            return;
        };
        let mut tags = split_tags(&editor.tags);
        set_single_tag_value(&mut tags, BOARD_TAG_KEY, editor.board_id.as_deref());

        let result = if let Some(task_id) = editor.task_id {
            self.service.update(TaskUpdateArgs {
                uuid: task_id,
                patch: TaskPatch {
                    title: Some(editor.title),
                    description: Some(editor.description),
                    project: Some(if editor.project.trim().is_empty() {
                        None
                    } else {
                        Some(editor.project.trim().to_string())
                    }),
                    tags: Some(tags),
                    priority: Some(editor.priority),
                    due: Some(text_to_optional(editor.due)),
                    wait: Some(text_to_optional(editor.wait)),
                    scheduled: Some(text_to_optional(editor.scheduled)),
                },
            })
            .map(|task| format!("Updated {}", task.title))
        } else {
            self.service
                .add(TaskCreate {
                    title: editor.title,
                    description: editor.description,
                    project: text_to_optional(editor.project),
                    tags,
                    priority: editor.priority,
                    due: text_to_optional(editor.due),
                    wait: text_to_optional(editor.wait),
                    scheduled: text_to_optional(editor.scheduled),
                })
                .map(|task| format!("Created {}", task.title))
        };

        match result {
            Ok(message) => {
                self.task_editor = None;
                self.refresh_tasks();
                self.set_message(message);
            }
            Err(error) => self.set_error(error),
        }
    }

    fn bulk_action(&mut self, action: BulkAction) {
        let targets = self.selected_tasks.iter().copied().collect::<Vec<_>>();
        if targets.is_empty() {
            return;
        }
        for uuid in targets {
            let Some(task) = self.tasks.iter().find(|task| task.uuid == uuid).cloned() else {
                continue;
            };
            let result = match action {
                BulkAction::Done if can_complete_task(&task) => self.service.done(uuid).map(|_| ()),
                BulkAction::Undone => self.service.uncomplete(uuid).map(|_| ()),
                BulkAction::Delete => self.service.delete(uuid),
                BulkAction::ApplyProject(ref project) => self
                    .service
                    .update(TaskUpdateArgs {
                        uuid,
                        patch: TaskPatch {
                            project: Some(if project.trim().is_empty() {
                                None
                            } else {
                                Some(project.trim().to_string())
                            }),
                            ..TaskPatch::default()
                        },
                    })
                    .map(|_| ()),
                BulkAction::ApplyTag(ref tag) => {
                    if tag.trim().is_empty() {
                        continue;
                    }
                    let mut tags = task.tags.clone();
                    for token in split_tags(tag) {
                        if !tags.iter().any(|existing| existing == &token) {
                            tags.push(token);
                        }
                    }
                    self.service
                        .update(TaskUpdateArgs {
                            uuid,
                            patch: TaskPatch {
                                tags: Some(tags),
                                ..TaskPatch::default()
                            },
                        })
                        .map(|_| ())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                self.set_error(error);
                break;
            }
        }
        self.refresh_tasks();
    }

    fn import_ics(&mut self, path: std::path::PathBuf) {
        self.import_busy = true;
        self.set_message(format!("Importing {}…", path.display()));
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Imported Calendar")
            .to_string();
        let color = next_calendar_color(&self.ui_state.imported_calendars);
        match self.service.import_ics(&path, &name, &color) {
            Ok(result) => self.finish_import(result),
            Err(error) => {
                self.import_busy = false;
                self.set_error(error);
            }
        }
    }

    pub fn import_json_bundle(&mut self, path: std::path::PathBuf) {
        self.import_busy = true;
        self.set_message(format!("Importing JSON bundle {}…", path.display()));
        match self.service.import_json_bundle(&path) {
            Ok((created, sources)) => {
                self.import_busy = false;
                self.set_message(format!("Imported {} items from JSON bundle", created));
                for source in sources {
                    self.ui_state.imported_calendars.retain(|s| s.id != source.id);
                    self.ui_state.imported_calendars.push(source);
                }
                self.ui_state.imported_calendars.sort_by(|left, right| left.name.cmp(&right.name));
                self.refresh_tasks();
                self.mark_ui_dirty();
            }
            Err(error) => {
                self.import_busy = false;
                self.set_error(error);
            }
        }
    }

    fn finish_import(&mut self, result: IcsImportResult) {
        self.ui_state
            .imported_calendars
            .retain(|source| source.id != result.source.id);
        self.ui_state.imported_calendars.push(result.source.clone());
        self.ui_state
            .imported_calendars
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.mark_ui_dirty();
        self.refresh_tasks();
        self.import_busy = false;
        self.set_message(format!(
            "Imported {} events from {} ({} created, {} updated, {} deleted)",
            result.remote_events, result.source.name, result.created, result.updated, result.deleted
        ));
    }

    fn reimport_calendar(&mut self, source: ImportedCalendarSource) {
        self.import_busy = true;
        self.set_message(format!("Re-importing {}…", source.name));
        match self
            .service
            .import_ics(&source.path, &source.name, &source.color)
        {
            Ok(result) => self.finish_import(result),
            Err(error) => {
                self.import_busy = false;
                self.set_error(error);
            }
        }
    }

    fn move_task_to_kanban_target(
        &mut self,
        task_id: Uuid,
        board_id: Option<&str>,
        lane: Option<&str>,
    ) {
        let Some(task) = self.tasks.iter().find(|task| task.uuid == task_id).cloned() else {
            return;
        };
        let mut tags = task.tags.clone();
        apply_drop_to_tags(&mut tags, board_id, lane);
        let result = self.service.update(TaskUpdateArgs {
            uuid: task_id,
            patch: TaskPatch {
                tags: Some(tags),
                ..TaskPatch::default()
            },
        });
        match result {
            Ok(updated) => {
                self.selected_task = Some(updated.uuid);
                self.selected_tasks.insert(updated.uuid);
                self.refresh_tasks();
                self.set_message(format!("Moved {}.", updated.title));
            }
            Err(error) => self.set_error(error),
        }
    }

    fn focus_calendar_entry(&mut self, entry: &crate::types::CalendarEntry, timezone: chrono_tz::Tz) {
        self.ui_state.set_focus_date(entry.due_utc.with_timezone(&timezone).date_naive());
        self.ui_state.calendar_view = CalendarView::Day;
        self.selected_task = Some(entry.task.uuid);
        self.selected_tasks.clear();
        self.selected_tasks.insert(entry.task.uuid);
        self.mark_ui_dirty();
    }

}

impl App for RivetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);
        self.ui_shell(ctx);
        match self.ui_state.active_tab {
            WorkspaceTab::Tasks => self.ui_tasks(ctx),
            WorkspaceTab::Kanban => self.ui_kanban(ctx),
            WorkspaceTab::Calendar => self.ui_calendar(ctx),
            WorkspaceTab::Dictionary => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.dictionary_ui.render(ui, ctx);
                });
            }
            WorkspaceTab::Contacts => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Contacts");
                    ui.label("Contacts workspace coming soon...");
                });
            }
            WorkspaceTab::Map => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Map");
                    ui.label("Map workspace coming soon...");
                });
            }
        }
        self.ui_task_editor(ctx);
        self.persist_ui_state();
    }
}

fn apply_theme(ctx: &egui::Context, theme: ThemeMode) {
    let mut visuals = match theme {
        ThemeMode::Day => egui::Visuals::light(),
        ThemeMode::Night => egui::Visuals::dark(),
    };
    visuals.window_corner_radius = 10.0.into();
    visuals.widgets.active.corner_radius = 8.0.into();
    visuals.widgets.hovered.corner_radius = 8.0.into();
    visuals.widgets.inactive.corner_radius = 8.0.into();
    visuals.selection.bg_fill = Color32::from_rgb(47, 125, 246);
    visuals.panel_fill = match theme {
        ThemeMode::Day => Color32::from_rgb(245, 246, 248),
        ThemeMode::Night => Color32::from_rgb(17, 21, 28),
    };
    ctx.set_visuals(visuals);
}

fn text_to_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn task_matches(task: &TaskDto, filters: &TaskFilters) -> bool {
    let search = filters.search.trim().to_ascii_lowercase();
    if !search.is_empty() {
        let haystack = format!("{} {} {}", task.title, task.description, task.project.clone().unwrap_or_default())
            .to_ascii_lowercase();
        if !haystack.contains(&search)
            && !task
                .tags
                .iter()
                .any(|tag| tag.to_ascii_lowercase().contains(&search))
        {
            return false;
        }
    }

    if !filters.status.matches(task.status) {
        return false;
    }
    if !filters.project.trim().is_empty() && task.project.as_deref() != Some(filters.project.trim()) {
        return false;
    }
    if !filters.tag.trim().is_empty() && !task.tags.iter().any(|tag| tag == filters.tag.trim()) {
        return false;
    }
    match filters.priority {
        PriorityFilter::All => {}
        PriorityFilter::Low if task.priority != Some(TaskPriority::Low) => return false,
        PriorityFilter::Medium if task.priority != Some(TaskPriority::Medium) => return false,
        PriorityFilter::High if task.priority != Some(TaskPriority::High) => return false,
        PriorityFilter::None if task.priority.is_some() => return false,
        _ => {}
    }
    match filters.due {
        DueFilter::All => {}
        DueFilter::HasDue if task.due.is_none() => return false,
        DueFilter::NoDue if task.due.is_some() => return false,
        _ => {}
    }
    true
}

fn collect_project_facets(tasks: &[TaskDto]) -> Vec<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for task in tasks {
        if let Some(project) = task.project.as_ref().filter(|value| !value.trim().is_empty()) {
            *counts.entry(project.clone()).or_default() += 1;
        }
    }
    counts.into_keys().collect()
}

fn collect_tag_facets(tasks: &[TaskDto]) -> Vec<String> {
    let mut counts = BTreeSet::<String>::new();
    for task in tasks {
        for tag in &task.tags {
            counts.insert(tag.clone());
        }
    }
    counts.into_iter().collect()
}

fn filter_bar(
    ui: &mut egui::Ui,
    filters: &mut TaskFilters,
    projects: &[String],
    tags: &[String],
    search_id: Option<egui::Id>,
) -> bool {
    let before = filters.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label("Search");
        let mut search = egui::TextEdit::singleline(&mut filters.search).desired_width(180.0);
        if let Some(id) = search_id {
            search = search.id(id);
        }
        ui.add(search);
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(filters.status.label())
            .show_ui(ui, |ui| {
                for status in StatusFilter::ALL {
                    ui.selectable_value(&mut filters.status, status, status.label());
                }
            });
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(if filters.project.is_empty() { "All Projects" } else { &filters.project })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut filters.project, String::new(), "All Projects");
                for project in projects {
                    ui.selectable_value(&mut filters.project, project.clone(), project);
                }
            });
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(if filters.tag.is_empty() { "All Tags" } else { &filters.tag })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut filters.tag, String::new(), "All Tags");
                for tag in tags {
                    ui.selectable_value(&mut filters.tag, tag.clone(), tag);
                }
            });
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(filters.priority.label())
            .show_ui(ui, |ui| {
                for priority in PriorityFilter::ALL {
                    ui.selectable_value(&mut filters.priority, priority, priority.label());
                }
            });
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(filters.due.label())
            .show_ui(ui, |ui| {
                for due in DueFilter::ALL {
                    ui.selectable_value(&mut filters.due, due, due.label());
                }
            });
        if ui.button("Clear").clicked() {
            *filters = TaskFilters::default();
        }
    });
    *filters != before
}

fn active_board_matches(task: &TaskDto, active_board_id: Option<&str>) -> bool {
    match (active_board_id, board_id_from_tags(&task.tags)) {
        (Some(expected), Some(actual)) => expected == actual,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn next_lane<'a>(current: &'a str, columns: &'a [String]) -> &'a str {
    let index = columns.iter().position(|column| column == current).unwrap_or(0);
    let next = (index + 1) % columns.len().max(1);
    columns.get(next).map(String::as_str).unwrap_or(current)
}

fn parse_color(raw: &str) -> Color32 {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#')
        && hex.len() == 6
    {
        let red = u8::from_str_radix(&hex[0..2], 16).ok();
        let green = u8::from_str_radix(&hex[2..4], 16).ok();
        let blue = u8::from_str_radix(&hex[4..6], 16).ok();
        if let (Some(red), Some(green), Some(blue)) = (red, green, blue) {
            return Color32::from_rgb(red, green, blue);
        }
    }
    Color32::from_rgb(127, 134, 145)
}

fn status_color(status: TaskStatus) -> Color32 {
    match status {
        TaskStatus::Pending => Color32::from_rgb(90, 180, 255),
        TaskStatus::Waiting => Color32::from_rgb(246, 182, 84),
        TaskStatus::Completed => Color32::from_rgb(90, 206, 135),
        TaskStatus::Deleted => Color32::from_rgb(220, 113, 113),
    }
}

fn priority_color(priority: TaskPriority) -> Color32 {
    match priority {
        TaskPriority::Low => Color32::from_rgb(112, 191, 134),
        TaskPriority::Medium => Color32::from_rgb(250, 194, 86),
        TaskPriority::High => Color32::from_rgb(232, 98, 98),
    }
}

fn due_color(task: &TaskDto) -> Color32 {
    if matches!(task.status, TaskStatus::Completed) {
        return Color32::from_gray(160);
    }
    if can_complete_task(task) {
        Color32::from_rgb(255, 211, 110)
    } else {
        Color32::from_rgb(255, 118, 118)
    }
}

fn tag_badge(ui: &mut egui::Ui, tag: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 18))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.small(tag);
        });
}

fn primary_modifier_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd +"
    } else {
        "Ctrl +"
    }
}

fn shortcut_row(ui: &mut egui::Ui, prefix: &str, key: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.monospace(format!("{prefix}{key}"));
        ui.label(description);
    });
}

fn format_runtime_error(raw: &str) -> String {
    if raw.contains("neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set") {
        return "Rivetr could not open a native window because no desktop display is available. On Linux, run it inside a graphical session with DISPLAY or WAYLAND set.".to_string();
    }
    raw.to_string()
}

fn next_board_color(boards: &[KanbanBoard]) -> String {
    let palette = ["#2f7df6", "#20a46b", "#f18a2b", "#d9485f", "#8466f6", "#0891b2"];
    palette[boards.len() % palette.len()].to_string()
}

fn next_calendar_color(sources: &[ImportedCalendarSource]) -> String {
    let palette = ["#ff6b35", "#3b82f6", "#10b981", "#e11d48", "#8b5cf6", "#f59e0b"];
    palette[sources.len() % palette.len()].to_string()
}

fn slug(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

enum BulkAction {
    Done,
    Undone,
    Delete,
    ApplyProject(String),
    ApplyTag(String),
}
