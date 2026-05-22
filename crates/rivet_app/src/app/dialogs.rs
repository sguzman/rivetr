use eframe::egui;

use super::RivetApp;
use crate::types::TaskPriority;

impl RivetApp {
    pub(super) fn ui_task_editor(&mut self, ctx: &egui::Context) {
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
