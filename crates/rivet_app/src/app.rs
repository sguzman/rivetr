mod kanban;
mod keyboard;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Local, NaiveDate, Utc};
use eframe::egui::{self, Color32, RichText, Vec2};
use eframe::{App, CreationContext, NativeOptions};
use egui_extras::{Column, TableBuilder};
use uuid::Uuid;

use crate::calendar::{
    calendar_title, entries_for_day, month_days, month_grid_start, shift_focus, visible_calendar_entries,
    week_days,
};
use crate::persistence::PersistedUiState;
use crate::runtime::RuntimeConfigService;
use crate::services::{can_complete_task, IcsImportResult, TaskService};
use crate::tags::{board_id_from_tags, kanban_columns, lane_from_tags, set_single_tag_value, split_tags, BOARD_TAG_KEY};
use crate::types::{
    CalendarEntry, CalendarView, DueFilter, ImportedCalendarSource, KanbanBoard, PriorityFilter,
    StatusFilter, TagSchema, TaskCreate, TaskDto, TaskFilters, TaskPatch, TaskPriority,
    TaskStatus, TaskUpdateArgs, ThemeMode, WorkspaceTab,
};
use self::kanban::{apply_drop_to_tags, board_from_task_tags, lane_from_task_tags, KanbanDragPayload};
use self::keyboard::{move_index, resolve_shortcut, ShortcutAction};

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

    eframe::run_native(
        "Rivetr",
        native_options,
        Box::new(|cc| {
            let app = RivetApp::new(cc).map_err(|error| error.to_string())?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| error.to_string())
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
    show_shortcuts: bool,
    dirty_ui_state: bool,
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
            show_shortcuts: false,
            dirty_ui_state: false,
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

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let editor_open = self.task_editor.is_some();
        let wants_keyboard_input = ctx.wants_keyboard_input();
        let Some(action) = resolve_shortcut(ctx, editor_open, wants_keyboard_input) else {
            return;
        };

        match action {
            ShortcutAction::ToggleHelp => {
                self.show_shortcuts = !self.show_shortcuts;
            }
            ShortcutAction::OpenNewTask => self.open_new_task(self.ui_state.active_board_id.clone()),
            ShortcutAction::SaveEditor => self.save_task_editor(),
            ShortcutAction::CancelEditor => self.task_editor = None,
            ShortcutAction::SwitchTab(tab) => {
                self.ui_state.active_tab = tab;
                self.mark_ui_dirty();
            }
            ShortcutAction::FocusSearch => {
                let id = match self.ui_state.active_tab {
                    WorkspaceTab::Tasks => egui::Id::new(TASK_SEARCH_ID),
                    WorkspaceTab::Kanban => egui::Id::new(KANBAN_SEARCH_ID),
                    WorkspaceTab::Calendar => return,
                };
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
            ShortcutAction::MoveSelection(delta) => {
                if matches!(self.ui_state.active_tab, WorkspaceTab::Tasks) {
                    let visible = self
                        .visible_tasks(&self.ui_state.task_filters)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.move_selection(&visible, delta);
                }
            }
            ShortcutAction::DoneSelected => self.apply_selected_action(BulkAction::Done),
            ShortcutAction::UncompleteSelected => self.apply_selected_action(BulkAction::Undone),
            ShortcutAction::DeleteSelected => self.apply_selected_action(BulkAction::Delete),
        }
    }

    fn move_selection(&mut self, visible: &[TaskDto], delta: i32) {
        let current_index = self
            .selected_task
            .and_then(|selected| visible.iter().position(|task| task.uuid == selected));
        let Some(next_index) = move_index(current_index, visible.len(), delta) else {
            return;
        };
        let task = &visible[next_index];
        self.selected_task = Some(task.uuid);
        self.selected_tasks.clear();
        self.selected_tasks.insert(task.uuid);
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
        if self.selected_tasks.is_empty() {
            if let Some(uuid) = self.selected_task {
                self.selected_tasks.insert(uuid);
            }
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
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Imported Calendar")
            .to_string();
        let color = next_calendar_color(&self.ui_state.imported_calendars);
        match self.service.import_ics(&path, &name, &color) {
            Ok(result) => self.finish_import(result),
            Err(error) => self.set_error(error),
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
        self.set_message(format!(
            "Imported {} events from {} ({} created, {} updated, {} deleted)",
            result.remote_events, result.source.name, result.created, result.updated, result.deleted
        ));
    }

    fn reimport_calendar(&mut self, source: ImportedCalendarSource) {
        match self
            .service
            .import_ics(&source.path, &source.name, &source.color)
        {
            Ok(result) => self.finish_import(result),
            Err(error) => self.set_error(error),
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

    fn ui_shell(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading(RichText::new("Rivetr").size(24.0).strong());
                ui.separator();
                for tab in WorkspaceTab::ALL {
                    let selected = self.ui_state.active_tab == tab;
                    if ui
                        .add_sized(
                            [84.0, 28.0],
                            egui::Button::new(tab.label()).selected(selected),
                        )
                        .clicked()
                    {
                        self.ui_state.active_tab = tab;
                        self.mark_ui_dirty();
                    }
                }
                ui.separator();
                if ui.button("New Task").clicked() {
                    self.open_new_task(self.ui_state.active_board_id.clone());
                }
                if ui.button("Refresh").clicked() {
                    self.refresh_tasks();
                }
                if ui.button("Shortcuts").clicked() {
                    self.show_shortcuts = true;
                }
                if ui.button("Theme").clicked() {
                    self.ui_state.theme_mode = match self.ui_state.theme_mode {
                        ThemeMode::Day => ThemeMode::Night,
                        ThemeMode::Night => ThemeMode::Day,
                    };
                    apply_theme(ctx, self.ui_state.theme_mode);
                    self.mark_ui_dirty();
                }
                ui.separator();
                ui.label(format!("Data: {}", self.service.data_dir().display()));
                if let Some(path) = self.runtime.config_path.as_ref() {
                    ui.separator();
                    ui.small(format!("Config: {}", path.display()));
                }
            });
            ui.add_space(4.0);

            if let Some(message) = self.last_message.as_deref() {
                egui::Frame::new()
                    .fill(Color32::from_rgb(23, 61, 39))
                    .corner_radius(8.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::from_rgb(164, 233, 184), message);
                    });
            }
            if let Some(error) = self.last_error.as_deref() {
                egui::Frame::new()
                    .fill(Color32::from_rgb(77, 31, 31))
                    .corner_radius(8.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::from_rgb(255, 173, 173), error);
                    });
            }
        });

        if self.show_shortcuts {
            let mut open = self.show_shortcuts;
            egui::Window::new("Shortcuts")
                .open(&mut open)
                .resizable(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    shortcut_row(ui, primary_modifier_label(), "N", "New task");
                    shortcut_row(ui, primary_modifier_label(), "F", "Focus search");
                    shortcut_row(ui, primary_modifier_label(), "1 / 2 / 3", "Switch workspace");
                    shortcut_row(ui, "", "↑/↓ or J/K", "Move task selection");
                    shortcut_row(ui, "", "X", "Complete selected task");
                    shortcut_row(ui, "Shift +", "X", "Uncomplete selected task");
                    shortcut_row(ui, "", "Backspace", "Delete selected task");
                    shortcut_row(ui, primary_modifier_label(), "S", "Save task editor");
                    shortcut_row(ui, "", "Esc", "Cancel task editor");
                    shortcut_row(ui, "", "F1 or Shift+/", "Toggle help");
                });
            self.show_shortcuts = open;
        }
    }

    fn ui_tasks(&mut self, ctx: &egui::Context) {
        let visible_tasks = self
            .visible_tasks(&self.ui_state.task_filters)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let projects = collect_project_facets(&visible_tasks);
        let tags = collect_tag_facets(&visible_tasks);

        egui::SidePanel::right("task_details")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.heading("Task Details");
                if let Some(task) = self.selected_task_ref().cloned() {
                    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
                        ui.label(RichText::new(task.title.clone()).strong().size(20.0));
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(status_color(task.status), task.status.label());
                            if let Some(priority) = task.priority {
                                ui.colored_label(priority_color(priority), format!("Priority {}", priority.label()));
                            }
                            if let Some(project) = task.project.as_deref() {
                                ui.label(format!("Project {project}"));
                            }
                        });
                        if let Some(due) = task.due.as_deref() {
                            ui.colored_label(due_color(&task), format!("Due {due}"));
                        }
                        if !task.description.is_empty() {
                            ui.add_space(6.0);
                            ui.label(task.description.clone());
                        }
                    });
                    if !task.tags.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Tags").strong());
                        ui.horizontal_wrapped(|ui| {
                            for tag in &task.tags {
                                tag_badge(ui, tag);
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Edit").clicked() {
                            self.open_edit_task(&task);
                        }
                        if matches!(task.status, TaskStatus::Pending | TaskStatus::Waiting)
                            && ui
                                .add_enabled(can_complete_task(&task), egui::Button::new("Done"))
                                .clicked()
                        {
                            match self.service.done(task.uuid) {
                                Ok(_) => {
                                    self.refresh_tasks();
                                    self.set_message("Task completed");
                                }
                                Err(error) => self.set_error(error),
                            }
                        }
                        if matches!(task.status, TaskStatus::Completed)
                            && ui.button("Uncomplete").clicked()
                        {
                            match self.service.uncomplete(task.uuid) {
                                Ok(_) => {
                                    self.refresh_tasks();
                                    self.set_message("Task reopened");
                                }
                                Err(error) => self.set_error(error),
                            }
                        }
                        if ui.button("Delete").clicked() {
                            match self.service.delete(task.uuid) {
                                Ok(_) => {
                                    self.refresh_tasks();
                                    self.set_message("Task deleted");
                                }
                                Err(error) => self.set_error(error),
                            }
                        }
                    });
                } else {
                    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
                        ui.label("Select a task to inspect details, edit fields, or change status.");
                    });
                }
            });

        egui::TopBottomPanel::top("task_filters").show(ctx, |ui| {
            if filter_bar(
                ui,
                &mut self.ui_state.task_filters,
                &projects,
                &tags,
                Some(egui::Id::new(TASK_SEARCH_ID)),
            ) {
                self.mark_ui_dirty();
            }
        });

        egui::TopBottomPanel::bottom("task_bulk").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Selected: {}", self.selected_tasks.len()));
                if ui.button("Done").clicked() {
                    self.bulk_action(BulkAction::Done);
                }
                if ui.button("Uncomplete").clicked() {
                    self.bulk_action(BulkAction::Undone);
                }
                if ui.button("Delete").clicked() {
                    self.bulk_action(BulkAction::Delete);
                }
                ui.separator();
                ui.label("Project");
                ui.add_sized([160.0, 28.0], egui::TextEdit::singleline(&mut self.bulk_project_input));
                if ui.button("Apply Project").clicked() {
                    let project = self.bulk_project_input.trim().to_string();
                    self.bulk_action(BulkAction::ApplyProject(project));
                }
                ui.label("Tags");
                ui.add_sized([180.0, 28.0], egui::TextEdit::singleline(&mut self.bulk_tag_input));
                if ui.button("Apply Tags").clicked() {
                    self.bulk_action(BulkAction::ApplyTag(self.bulk_tag_input.clone()));
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let row_height = 28.0;
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::auto())
                .column(Column::auto())
                .column(Column::remainder())
                .column(Column::remainder())
                .column(Column::remainder())
                .column(Column::auto())
                .header(26.0, |mut header| {
                    header.col(|ui| {
                        let all_selected = !visible_tasks.is_empty()
                            && visible_tasks.iter().all(|task| self.selected_tasks.contains(&task.uuid));
                        let mut value = all_selected;
                        if ui.checkbox(&mut value, "").clicked() {
                            if value {
                                self.selected_tasks =
                                    visible_tasks.iter().map(|task| task.uuid).collect::<BTreeSet<_>>();
                            } else {
                                self.selected_tasks.clear();
                            }
                        }
                    });
                    header.col(|ui| {
                        ui.strong("#");
                    });
                    header.col(|ui| {
                        ui.strong("Title");
                    });
                    header.col(|ui| {
                        ui.strong("Project");
                    });
                    header.col(|ui| {
                        ui.strong("Tags");
                    });
                    header.col(|ui| {
                        ui.strong("Due");
                    });
                })
                .body(|body| {
                    body.rows(row_height, visible_tasks.len(), |mut row| {
                        let task = &visible_tasks[row.index()];
                        row.col(|ui| {
                            let mut selected = self.selected_tasks.contains(&task.uuid);
                            if ui.checkbox(&mut selected, "").changed() {
                                if selected {
                                    self.selected_tasks.insert(task.uuid);
                                } else {
                                    self.selected_tasks.remove(&task.uuid);
                                }
                            }
                        });
                        row.col(|ui| {
                            ui.label(task.id.map(|id| id.to_string()).unwrap_or_else(|| "•".to_string()));
                        });
                        row.col(|ui| {
                            let selected = self.selected_task == Some(task.uuid);
                            let response = ui.selectable_label(selected, &task.title);
                            if response.clicked() {
                                self.selected_task = Some(task.uuid);
                                self.selected_tasks.insert(task.uuid);
                            }
                            if response.double_clicked() {
                                self.open_edit_task(task);
                            }
                        });
                        row.col(|ui| {
                            ui.colored_label(
                                status_color(task.status),
                                task.project.clone().unwrap_or_else(|| "—".to_string()),
                            );
                        });
                        row.col(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                for tag in task.tags.iter().take(3) {
                                    tag_badge(ui, tag);
                                }
                            });
                        });
                        row.col(|ui| {
                            let due = task.due.clone().unwrap_or_else(|| "—".to_string());
                            ui.colored_label(due_color(task), due);
                        });
                    });
                });
        });
    }

    fn ui_kanban(&mut self, ctx: &egui::Context) {
        let visible_tasks = self
            .visible_tasks(&self.ui_state.kanban_filters)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let projects = collect_project_facets(&visible_tasks);
        let tags = collect_tag_facets(&visible_tasks);
        let schema = self.task_schema().clone();
        let columns = kanban_columns(&schema);
        let active_board_id = self.ui_state.active_board_id.clone();
        let board_list = self.ui_state.kanban_boards.clone();

        egui::SidePanel::left("kanban_sidebar")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("Boards");
                for board in &board_list {
                    let selected = self.ui_state.active_board_id.as_deref() == Some(board.id.as_str());
                    let (drop_zone, dropped) = ui.dnd_drop_zone::<KanbanDragPayload, _>(
                        egui::Frame::group(ui.style()).inner_margin(6.0),
                        |ui| {
                            let response = ui.selectable_label(selected, &board.name);
                            if response.clicked() {
                                self.ui_state.active_board_id = Some(board.id.clone());
                                self.board_editor.rename_name = board.name.clone();
                                self.mark_ui_dirty();
                            }
                            ui.small(RichText::new(&board.color).color(parse_color(&board.color)));
                        },
                    );
                    if drop_zone.response.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                    if let Some(payload) = dropped {
                        self.move_task_to_kanban_target(payload.task_id, Some(&board.id), Some(&payload.from_lane));
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.board_editor.create_name);
                    if ui.button("Add").clicked() {
                        let name = self.board_editor.create_name.trim();
                        if !name.is_empty() {
                            let board = KanbanBoard {
                                id: slug(name),
                                name: name.to_string(),
                                color: next_board_color(&self.ui_state.kanban_boards),
                            };
                            self.ui_state.active_board_id = Some(board.id.clone());
                            self.ui_state.kanban_boards.push(board);
                            self.ui_state.kanban_boards.sort_by(|left, right| left.name.cmp(&right.name));
                            self.board_editor.create_name.clear();
                            self.mark_ui_dirty();
                        }
                    }
                });
                if let Some(active_id) = active_board_id.as_deref() {
                    ui.separator();
                    ui.label("Rename active");
                    ui.text_edit_singleline(&mut self.board_editor.rename_name);
                    if ui.button("Rename").clicked() {
                        if let Some(board) = self
                            .ui_state
                            .kanban_boards
                            .iter_mut()
                            .find(|board| board.id == active_id)
                        {
                            board.name = self.board_editor.rename_name.trim().to_string();
                            self.mark_ui_dirty();
                        }
                    }
                    if ui.button("Delete Board").clicked() && self.ui_state.kanban_boards.len() > 1 {
                        self.ui_state.kanban_boards.retain(|board| board.id != active_id);
                        self.ui_state.active_board_id = self.ui_state.kanban_boards.first().map(|board| board.id.clone());
                        self.mark_ui_dirty();
                    }
                }
                ui.separator();
                if ui.checkbox(&mut self.ui_state.kanban_compact, "Compact cards").changed() {
                    self.mark_ui_dirty();
                }
                ui.separator();
                if filter_bar(
                    ui,
                    &mut self.ui_state.kanban_filters,
                    &projects,
                    &tags,
                    Some(egui::Id::new(KANBAN_SEARCH_ID)),
                ) {
                    self.mark_ui_dirty();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New Task in Board").clicked() {
                    self.open_new_task(self.ui_state.active_board_id.clone());
                }
            });
            ui.separator();
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for column in &columns {
                        let lane_tasks = visible_tasks
                            .iter()
                            .filter(|task| {
                                lane_from_tags(&task.tags, &schema) == *column
                                    && active_board_matches(task, active_board_id.as_deref())
                            })
                            .cloned()
                            .collect::<Vec<_>>();

                        let (drop_zone, dropped) = ui.dnd_drop_zone::<KanbanDragPayload, _>(
                            egui::Frame::group(ui.style())
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 8))
                                .inner_margin(10.0),
                            |ui| {
                                ui.set_width(320.0);
                                ui.horizontal(|ui| {
                                    ui.heading(column);
                                    ui.small(format!("{} cards", lane_tasks.len()));
                                });
                                ui.separator();
                                for task in lane_tasks {
                                    let payload = KanbanDragPayload {
                                        task_id: task.uuid,
                                        from_board_id: board_from_task_tags(&task.tags),
                                        from_lane: lane_from_task_tags(&task.tags)
                                            .unwrap_or_else(|| column.clone()),
                                    };
                                    let card = ui.dnd_drag_source(
                                        egui::Id::new(("kanban-card", task.uuid)),
                                        payload,
                                        |ui| {
                                            egui::Frame::group(ui.style())
                                                .fill(if self.selected_task == Some(task.uuid) {
                                                    Color32::from_rgba_unmultiplied(47, 125, 246, 32)
                                                } else {
                                                    ui.visuals().faint_bg_color
                                                })
                                                .inner_margin(10.0)
                                                .show(ui, |ui| {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(RichText::new(&task.title).strong());
                                                        ui.colored_label(status_color(task.status), task.status.label());
                                                    });
                                                    if !self.ui_state.kanban_compact {
                                                        if let Some(project) = task.project.as_deref() {
                                                            ui.small(project);
                                                        }
                                                        if let Some(due) = task.due.as_deref() {
                                                            ui.colored_label(due_color(&task), due);
                                                        }
                                                        ui.horizontal_wrapped(|ui| {
                                                            for tag in task.tags.iter().take(3) {
                                                                tag_badge(ui, tag);
                                                            }
                                                        });
                                                    }
                                                    ui.horizontal_wrapped(|ui| {
                                                        if ui.button("Edit").clicked() {
                                                            self.open_edit_task(&task);
                                                        }
                                                        if ui.button("Next").clicked() {
                                                            let next_lane = next_lane(column, &columns);
                                                            self.move_task_to_kanban_target(
                                                                task.uuid,
                                                                active_board_id.as_deref(),
                                                                Some(next_lane),
                                                            );
                                                        }
                                                        if matches!(task.status, TaskStatus::Pending | TaskStatus::Waiting)
                                                            && ui
                                                                .add_enabled(
                                                                    can_complete_task(&task),
                                                                    egui::Button::new("Done"),
                                                                )
                                                                .clicked()
                                                        {
                                                            if let Err(error) = self.service.done(task.uuid) {
                                                                self.set_error(error);
                                                            } else {
                                                                self.refresh_tasks();
                                                            }
                                                        }
                                                        if matches!(task.status, TaskStatus::Completed)
                                                            && ui.button("Undo").clicked()
                                                        {
                                                            if let Err(error) = self.service.uncomplete(task.uuid) {
                                                                self.set_error(error);
                                                            } else {
                                                                self.refresh_tasks();
                                                            }
                                                        }
                                                        if ui.button("Delete").clicked() {
                                                            if let Err(error) = self.service.delete(task.uuid) {
                                                                self.set_error(error);
                                                            } else {
                                                                self.refresh_tasks();
                                                            }
                                                        }
                                                    });
                                                });
                                        },
                                    );
                                    if card.response.clicked() {
                                        self.selected_task = Some(task.uuid);
                                        self.selected_tasks.insert(task.uuid);
                                    }
                                }
                            },
                        );
                        if drop_zone.response.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        if let Some(payload) = dropped {
                            self.move_task_to_kanban_target(
                                payload.task_id,
                                active_board_id.as_deref(),
                                Some(column),
                            );
                        }
                    }
                });
            });
        });
    }

    fn ui_calendar(&mut self, ctx: &egui::Context) {
        let focus = self.ui_state.focus_date();
        let entries = visible_calendar_entries(
            &self.tasks,
            &self.ui_state.kanban_boards,
            &self.runtime.calendar,
            Utc::now(),
        );
        let timezone = self
            .runtime
            .calendar
            .timezone
            .parse()
            .unwrap_or(chrono_tz::America::Mexico_City);
        let current_period = current_period_entries(&entries, self.ui_state.calendar_view, focus, timezone, self.runtime.calendar.week_start_monday);

        egui::SidePanel::right("calendar_side")
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.heading("Imported Calendars");
                if ui.button("Import ICS").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("ICS", &["ics"]).pick_file() {
                        self.import_ics(path);
                    }
                }
                ui.separator();
                let calendars = self.ui_state.imported_calendars.clone();
                for source in calendars {
                    egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
                        ui.colored_label(parse_color(&source.color), &source.name);
                        ui.label(source.path.display().to_string());
                        ui.small(format!("Imported {}", source.last_imported_at));
                        if ui.button("Re-import").clicked() {
                            self.reimport_calendar(source.clone());
                        }
                    });
                }
                ui.separator();
                ui.heading("Tasks In View");
                for entry in current_period.iter().take(self.runtime.calendar.task_list_limit) {
                    ui.colored_label(parse_color(&entry.color), format!("{}  {}", entry.due_utc.with_timezone(&timezone).format("%Y-%m-%d %H:%M"), entry.label));
                }
            });

        egui::TopBottomPanel::top("calendar_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("<").clicked() {
                    let next = shift_focus(self.ui_state.calendar_view, focus, -1);
                    self.ui_state.set_focus_date(next);
                    self.mark_ui_dirty();
                }
                ui.label(calendar_title(self.ui_state.calendar_view, focus));
                if ui.button(">").clicked() {
                    let next = shift_focus(self.ui_state.calendar_view, focus, 1);
                    self.ui_state.set_focus_date(next);
                    self.mark_ui_dirty();
                }
                for view in CalendarView::ALL {
                    if ui.selectable_label(self.ui_state.calendar_view == view, view.label()).clicked() {
                        self.ui_state.calendar_view = view;
                        self.mark_ui_dirty();
                    }
                }
                if ui.button("Today").clicked() {
                    self.ui_state.set_focus_date(Local::now().date_naive());
                    self.mark_ui_dirty();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.ui_state.calendar_view {
            CalendarView::Month => {
                let start = month_grid_start(focus, self.runtime.calendar.week_start_monday);
                let days = month_days(start);
                egui::Grid::new("calendar_month_grid")
                    .num_columns(7)
                    .spacing(Vec2::new(8.0, 8.0))
                    .show(ui, |ui| {
                        for (index, day) in days.iter().enumerate() {
                            ui.group(|ui| {
                                let in_month = day.month() == focus.month();
                                let heading = if in_month {
                                    RichText::new(day.day().to_string()).strong()
                                } else {
                                    RichText::new(day.day().to_string()).weak()
                                };
                                ui.label(heading);
                                for entry in entries_for_day(&entries, *day, timezone).into_iter().take(4) {
                                    ui.colored_label(parse_color(&entry.color), truncate(&entry.label, 18));
                                }
                            });
                            if (index + 1) % 7 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            }
            CalendarView::Week => {
                let days = week_days(focus, self.runtime.calendar.week_start_monday);
                ui.columns(7, |columns| {
                    for (index, day) in days.iter().enumerate() {
                        columns[index].heading(day.format("%a %e").to_string());
                        for entry in entries_for_day(&entries, *day, timezone) {
                            columns[index].colored_label(
                                parse_color(&entry.color),
                                format!("{} {}", entry.due_utc.with_timezone(&timezone).format("%H:%M"), entry.label),
                            );
                        }
                    }
                });
            }
            CalendarView::Day => {
                let day_entries = entries_for_day(&entries, focus, timezone);
                ui.heading(focus.format("%A %B %e, %Y").to_string());
                for entry in day_entries {
                    ui.group(|ui| {
                        ui.colored_label(parse_color(&entry.color), entry.label);
                        ui.label(entry.due_utc.with_timezone(&timezone).format("%H:%M").to_string());
                        if !entry.task.description.is_empty() {
                            ui.label(entry.task.description);
                        }
                    });
                }
            }
        });
    }

    fn ui_task_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.task_editor.clone() else {
            return;
        };
        let mut should_save = false;
        let mut open = true;
        let mut cancel = false;
        let board_options = self.ui_state.kanban_boards.clone();
        egui::Window::new(if editor.task_id.is_some() { "Edit Task" } else { "New Task" })
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut editor.title);
                ui.label("Description");
                ui.text_edit_multiline(&mut editor.description);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Project");
                        ui.text_edit_singleline(&mut editor.project);
                    });
                    ui.vertical(|ui| {
                        ui.label("Board");
                        egui::ComboBox::from_id_salt("task_editor_board")
                            .selected_text(
                                editor
                                    .board_id
                                    .as_ref()
                                    .and_then(|id| {
                                        self.ui_state
                                            .kanban_boards
                                            .iter()
                                            .find(|board| &board.id == id)
                                            .map(|board| board.name.clone())
                                    })
                                    .unwrap_or_else(|| "None".to_string()),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut editor.board_id, None, "None");
                                for board in &board_options {
                                    ui.selectable_value(
                                        &mut editor.board_id,
                                        Some(board.id.clone()),
                                        &board.name,
                                    );
                                }
                            });
                    });
                    ui.vertical(|ui| {
                        ui.label("Priority");
                        egui::ComboBox::from_id_salt("task_editor_priority")
                            .selected_text(
                                editor
                                    .priority
                                    .map(|priority| priority.label().to_string())
                                    .unwrap_or_else(|| "None".to_string()),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut editor.priority, None, "None");
                                for priority in TaskPriority::ALL {
                                    ui.selectable_value(
                                        &mut editor.priority,
                                        Some(priority),
                                        priority.label(),
                                    );
                                }
                            });
                    });
                });
                ui.label("Tags");
                ui.text_edit_singleline(&mut editor.tags);
                ui.columns(3, |columns| {
                    columns[0].label("Due");
                    columns[0].text_edit_singleline(&mut editor.due);
                    columns[1].label("Wait");
                    columns[1].text_edit_singleline(&mut editor.wait);
                    columns[2].label("Scheduled");
                    columns[2].text_edit_singleline(&mut editor.scheduled);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        should_save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if cancel {
            open = false;
        }
        if !open {
            self.task_editor = None;
        } else {
            self.task_editor = Some(editor);
        }
        if should_save {
            self.save_task_editor();
        }
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

fn current_period_entries(
    entries: &[CalendarEntry],
    view: CalendarView,
    focus: NaiveDate,
    timezone: chrono_tz::Tz,
    monday_start: bool,
) -> Vec<CalendarEntry> {
    let days = match view {
        CalendarView::Month => {
            let start = month_grid_start(focus, monday_start);
            month_days(start)
        }
        CalendarView::Week => week_days(focus, monday_start),
        CalendarView::Day => vec![focus],
    };
    entries
        .iter()
        .filter(|entry| days.contains(&entry.due_utc.with_timezone(&timezone).date_naive()))
        .cloned()
        .collect()
}

fn parse_color(raw: &str) -> Color32 {
    let raw = raw.trim();
    if let Some(hex) = raw.strip_prefix('#') {
        if hex.len() == 6 {
            let red = u8::from_str_radix(&hex[0..2], 16).ok();
            let green = u8::from_str_radix(&hex[2..4], 16).ok();
            let blue = u8::from_str_radix(&hex[4..6], 16).ok();
            if let (Some(red), Some(green), Some(blue)) = (red, green, blue) {
                return Color32::from_rgb(red, green, blue);
            }
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
