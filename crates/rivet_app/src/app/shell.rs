use eframe::egui::{self, Color32, RichText};

use super::{
    apply_theme, primary_modifier_label, shortcut_row, BulkAction, RivetApp, KANBAN_SEARCH_ID,
    TASK_SEARCH_ID,
};
use crate::types::{TaskDto, ThemeMode, WorkspaceTab};

impl RivetApp {
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let editor_open = self.task_editor.is_some();
        let wants_keyboard_input = ctx.wants_keyboard_input();
        let Some(action) = super::keyboard::resolve_shortcut(ctx, editor_open, wants_keyboard_input) else {
            return;
        };

        match action {
            super::keyboard::ShortcutAction::ToggleHelp => {
                self.show_shortcuts = !self.show_shortcuts;
            }
            super::keyboard::ShortcutAction::OpenNewTask => {
                self.open_new_task(self.ui_state.active_board_id.clone());
            }
            super::keyboard::ShortcutAction::SaveEditor => self.save_task_editor(),
            super::keyboard::ShortcutAction::CancelEditor => self.task_editor = None,
            super::keyboard::ShortcutAction::SwitchTab(tab) => {
                self.ui_state.active_tab = tab;
                self.mark_ui_dirty();
            }
            super::keyboard::ShortcutAction::FocusSearch => {
                let id = match self.ui_state.active_tab {
                    WorkspaceTab::Tasks => egui::Id::new(TASK_SEARCH_ID),
                    WorkspaceTab::Kanban => return,
                    WorkspaceTab::Calendar => return,
                    WorkspaceTab::Dictionary => return,
                    WorkspaceTab::Contacts => return,
                    WorkspaceTab::Map => return,
                };
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
            super::keyboard::ShortcutAction::MoveSelection(delta) => {
                if matches!(self.ui_state.active_tab, WorkspaceTab::Tasks) {
                    let visible = self
                        .visible_tasks(&self.ui_state.task_filters)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.move_selection(&visible, delta);
                }
            }
            super::keyboard::ShortcutAction::DoneSelected => {
                self.apply_selected_action(BulkAction::Done)
            }
            super::keyboard::ShortcutAction::UncompleteSelected => {
                self.apply_selected_action(BulkAction::Undone)
            }
            super::keyboard::ShortcutAction::DeleteSelected => {
                self.apply_selected_action(BulkAction::Delete)
            }
        }
    }

    fn move_selection(&mut self, visible: &[TaskDto], delta: i32) {
        let current_index = self
            .selected_task
            .and_then(|selected| visible.iter().position(|task| task.uuid == selected));
        let Some(next_index) = super::keyboard::move_index(current_index, visible.len(), delta) else {
            return;
        };
        let task = &visible[next_index];
        self.selected_task = Some(task.uuid);
        self.selected_tasks.clear();
        self.selected_tasks.insert(task.uuid);
    }

    pub(super) fn ui_shell(&mut self, ctx: &egui::Context) {
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
                if self.import_busy {
                    ui.separator();
                    ui.spinner();
                    ui.small("Import busy");
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
}
