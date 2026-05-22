use chrono::{Datelike, Local, Utc};
use eframe::egui::{self, RichText, Vec2};

use super::{
    calendar_title, current_period_entries, entries_for_day, month_days, month_grid_start,
    parse_color, shift_focus, truncate, visible_calendar_entries, week_days, RivetApp,
};
use crate::types::CalendarView;

impl RivetApp {
    pub(super) fn ui_calendar(&mut self, ctx: &egui::Context) {
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
        let current_period = current_period_entries(
            &entries,
            self.ui_state.calendar_view,
            focus,
            timezone,
            self.runtime.calendar.week_start_monday,
        );

        egui::SidePanel::right("calendar_side")
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.heading("Imported Calendars");
                if ui
                    .add_enabled(!self.import_busy, egui::Button::new("Import ICS"))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new().add_filter("ICS", &["ics"]).pick_file()
                    {
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
                        if ui
                            .add_enabled(!self.import_busy, egui::Button::new("Re-import"))
                            .clicked()
                        {
                            self.reimport_calendar(source.clone());
                        }
                    });
                }
                ui.separator();
                ui.heading("Tasks In View");
                for entry in current_period
                    .iter()
                    .take(self.runtime.calendar.task_list_limit)
                {
                    ui.colored_label(
                        parse_color(&entry.color),
                        format!(
                            "{}  {}",
                            entry.due_utc.with_timezone(&timezone).format("%Y-%m-%d %H:%M"),
                            entry.label
                        ),
                    );
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
                    if ui
                        .selectable_label(self.ui_state.calendar_view == view, view.label())
                        .clicked()
                    {
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
                                for entry in entries_for_day(&entries, *day, timezone).into_iter().take(4)
                                {
                                    ui.colored_label(
                                        parse_color(&entry.color),
                                        truncate(&entry.label, 18),
                                    );
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
                                format!(
                                    "{} {}",
                                    entry.due_utc.with_timezone(&timezone).format("%H:%M"),
                                    entry.label
                                ),
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
}
