use std::collections::BTreeSet;

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use super::{
    collect_project_facets, collect_tag_facets, due_color, filter_bar, status_color, tag_badge,
    BulkAction, RivetApp, TASK_SEARCH_ID,
};
use crate::services::can_complete_task;
use crate::types::TaskStatus;

impl RivetApp {
    pub(super) fn ui_tasks(&mut self, ctx: &egui::Context) {
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
                                ui.colored_label(
                                    super::priority_color(priority),
                                    format!("Priority {}", priority.label()),
                                );
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
                ui.add_sized(
                    [160.0, 28.0],
                    egui::TextEdit::singleline(&mut self.bulk_project_input),
                );
                if ui.button("Apply Project").clicked() {
                    let project = self.bulk_project_input.trim().to_string();
                    self.bulk_action(BulkAction::ApplyProject(project));
                }
                ui.label("Tags");
                ui.add_sized(
                    [180.0, 28.0],
                    egui::TextEdit::singleline(&mut self.bulk_tag_input),
                );
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
                            && visible_tasks
                                .iter()
                                .all(|task| self.selected_tasks.contains(&task.uuid));
                        let mut value = all_selected;
                        if ui.checkbox(&mut value, "").clicked() {
                            if value {
                                self.selected_tasks = visible_tasks
                                    .iter()
                                    .map(|task| task.uuid)
                                    .collect::<BTreeSet<_>>();
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
                            ui.label(
                                task.id
                                    .map(|id| id.to_string())
                                    .unwrap_or_else(|| "•".to_string()),
                            );
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
}
