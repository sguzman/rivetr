use eframe::egui::{self, Color32, RichText};

use super::kanban::{board_from_task_tags, lane_from_task_tags, KanbanDragPayload};
use super::{
    active_board_matches, collect_project_facets, collect_tag_facets, due_color, filter_bar,
    next_board_color, next_lane, parse_color, slug, status_color, tag_badge, RivetApp,
    KANBAN_SEARCH_ID,
};
use crate::services::can_complete_task;
use crate::tags::kanban_columns;
use crate::types::{KanbanBoard, TaskStatus};

impl RivetApp {
    pub(super) fn ui_kanban(&mut self, ctx: &egui::Context) {
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
                        self.move_task_to_kanban_target(
                            payload.task_id,
                            Some(&board.id),
                            Some(&payload.from_lane),
                        );
                    }
                }
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                            self.ui_state
                                .kanban_boards
                                .sort_by(|left, right| left.name.cmp(&right.name));
                            self.board_editor.create_name.clear();
                            self.mark_ui_dirty();
                        }
                    }
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::singleline(&mut self.board_editor.create_name),
                    );
                });
                if let Some(active_id) = active_board_id.as_deref() {
                    ui.separator();
                    ui.label("Rename active");
                    ui.add_sized(
                        [ui.available_width(), 28.0],
                        egui::TextEdit::singleline(&mut self.board_editor.rename_name),
                    );
                    if ui.button("Rename").clicked()
                        && let Some(board) = self
                            .ui_state
                            .kanban_boards
                            .iter_mut()
                            .find(|board| board.id == active_id)
                    {
                        board.name = self.board_editor.rename_name.trim().to_string();
                        self.mark_ui_dirty();
                    }
                    if ui.button("Delete Board").clicked() && self.ui_state.kanban_boards.len() > 1 {
                        self.ui_state.kanban_boards.retain(|board| board.id != active_id);
                        self.ui_state.active_board_id = self
                            .ui_state
                            .kanban_boards
                            .first()
                            .map(|board| board.id.clone());
                        self.mark_ui_dirty();
                    }
                }
                ui.separator();
                if ui
                    .checkbox(&mut self.ui_state.kanban_compact, "Compact cards")
                    .changed()
                {
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
                                crate::tags::lane_from_tags(&task.tags, &schema) == *column
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
                                                        ui.colored_label(
                                                            status_color(task.status),
                                                            task.status.label(),
                                                        );
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
                                                        if matches!(
                                                            task.status,
                                                            TaskStatus::Pending | TaskStatus::Waiting
                                                        ) && ui
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
                                                            if let Err(error) = self.service.uncomplete(task.uuid)
                                                            {
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
}
