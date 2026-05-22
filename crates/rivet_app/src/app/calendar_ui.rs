use chrono::{Datelike, Local, Timelike, Utc};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

use super::{
    calendar_title, entries_for_day, entries_for_month, month_days, month_grid_start, parse_color,
    period_entries, period_stats, quarter_months, shift_focus, should_show_entry_in_list,
    should_show_marker, truncate, visible_calendar_entries, week_days, year_months, RivetApp,
};
use crate::types::{CalendarEntry, CalendarMarkerKind, CalendarView};

impl RivetApp {
    pub(super) fn ui_calendar(&mut self, ctx: &egui::Context) {
        let focus = self.ui_state.focus_date();
        let now_utc = Utc::now();
        let entries = visible_calendar_entries(
            &self.tasks,
            &self.ui_state.kanban_boards,
            &self.runtime.calendar,
            now_utc,
        );
        let timezone = self
            .runtime
            .calendar
            .timezone
            .parse()
            .unwrap_or(chrono_tz::America::Mexico_City);
        let period_all = period_entries(
            &entries,
            self.ui_state.calendar_view,
            focus,
            timezone,
            self.runtime.calendar.week_start_monday,
        );
        let period_visible = period_all
            .iter()
            .filter(|entry| should_show_entry_in_list(entry, &self.runtime.calendar, now_utc))
            .cloned()
            .collect::<Vec<_>>();
        let stats = period_stats(&period_all);
        let today = Local::now().date_naive();

        egui::SidePanel::left("calendar_left")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Calendar Views");
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for view in CalendarView::ALL {
                        if ui
                            .add_sized(
                                [72.0, 28.0],
                                egui::Button::new(view.label()).selected(self.ui_state.calendar_view == view),
                            )
                            .clicked()
                        {
                            self.ui_state.calendar_view = view;
                            self.mark_ui_dirty();
                        }
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Prev").clicked() {
                        let next = shift_focus(self.ui_state.calendar_view, focus, -1);
                        self.ui_state.set_focus_date(next);
                        self.mark_ui_dirty();
                    }
                    if ui.button("Today").clicked() {
                        self.ui_state.set_focus_date(today);
                        self.mark_ui_dirty();
                    }
                    if ui.button("Next").clicked() {
                        let next = shift_focus(self.ui_state.calendar_view, focus, 1);
                        self.ui_state.set_focus_date(next);
                        self.mark_ui_dirty();
                    }
                });
                ui.add_space(8.0);
                ui.small(format!("Timezone: {}", self.runtime.calendar.timezone));
                ui.small(format!("Focus: {}", focus.format("%Y-%m-%d")));
                ui.small(format!("Calendar items: {}", entries.len()));

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.heading("Imported Calendars");
                if ui
                    .add_enabled(!self.import_busy, egui::Button::new("Import ICS"))
                    .clicked()
                    && let Some(path) =
                        rfd::FileDialog::new().add_filter("ICS", &["ics"]).pick_file()
                {
                    self.import_ics(path);
                }
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        if self.ui_state.imported_calendars.is_empty() {
                            ui.label(RichText::new("No imported calendars yet.").weak());
                        }
                        let calendars = self.ui_state.imported_calendars.clone();
                        for source in calendars {
                            egui::Frame::group(ui.style())
                                .fill(ui.visuals().faint_bg_color)
                                .corner_radius(12.0)
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.colored_label(parse_color(&source.color), "●");
                                        ui.label(RichText::new(&source.name).strong());
                                    });
                                    ui.small(source.path.display().to_string());
                                    ui.small(format!("Imported {}", source.last_imported_at));
                                    ui.add_space(6.0);
                                    if ui
                                        .add_enabled(!self.import_busy, egui::Button::new("Re-import"))
                                        .clicked()
                                    {
                                        self.reimport_calendar(source.clone());
                                    }
                                });
                            ui.add_space(6.0);
                        }
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(RichText::new("Marker legend").strong());
                marker_legend_row(ui, Color32::from_rgb(214, 69, 69), "External calendar");
                marker_legend_row(ui, Color32::from_rgb(47, 125, 246), "Kanban board task");
                marker_legend_row(ui, Color32::from_rgb(127, 134, 145), "Unassigned task");
            });

        egui::SidePanel::right("calendar_right")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Calendar Stats");
                stat_row(ui, "Items in period", stats.0);
                stat_row(ui, "Pending", stats.1);
                stat_row(ui, "Waiting", stats.2);
                stat_row(ui, "Completed", stats.3);
                stat_row(ui, "Deleted", stats.4);
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                ui.heading("Tasks In Period");
                ui.small(if self.runtime.calendar.filter_before_now {
                    "Past items are muted in the calendar and filtered from this list."
                } else {
                    "Showing all period items, including past ones."
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if period_visible.is_empty() {
                        ui.label(RichText::new("No tasks due in this calendar period.").weak());
                        return;
                    }
                    for entry in period_visible.iter().take(self.runtime.calendar.task_list_limit) {
                        egui::Frame::group(ui.style())
                            .fill(ui.visuals().faint_bg_color)
                            .corner_radius(12.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.colored_label(parse_color(&entry.color), "●");
                                    ui.label(RichText::new(&entry.label).strong());
                                });
                                ui.small(
                                    entry
                                        .due_utc
                                        .with_timezone(&timezone)
                                        .format("%Y-%m-%d %H:%M")
                                        .to_string(),
                                );
                                if let Some(project) = entry.task.project.as_deref() {
                                    ui.small(format!("project:{project}"));
                                }
                                if !entry.task.tags.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        for tag in entry.task.tags.iter().take(4) {
                                            super::tag_badge(ui, tag);
                                        }
                                    });
                                }
                            });
                        ui.add_space(6.0);
                    }
                });
            });

        egui::TopBottomPanel::top("calendar_toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(calendar_title(self.ui_state.calendar_view, focus));
                ui.separator();
                ui.small(format!("Week starts on {}", if self.runtime.calendar.week_start_monday { "Monday" } else { "Sunday" }));
                if self.import_busy {
                    ui.separator();
                    ui.spinner();
                    ui.small("Import busy");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::group(ui.style())
                .fill(match self.ui_state.theme_mode {
                    crate::types::ThemeMode::Day => Color32::from_rgb(252, 252, 250),
                    crate::types::ThemeMode::Night => Color32::from_rgb(24, 28, 36),
                })
                .corner_radius(18.0)
                .stroke(Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color))
                .inner_margin(14.0)
                .show(ui, |ui| match self.ui_state.calendar_view {
                    CalendarView::Year => render_year_view(ui, &entries, focus, timezone, today, &self.runtime.calendar, now_utc),
                    CalendarView::Quarter => {
                        render_quarter_view(ui, &entries, focus, timezone, today, &self.runtime.calendar, now_utc)
                    }
                    CalendarView::Month => {
                        render_month_view(ui, &entries, focus, timezone, today, &self.runtime.calendar, now_utc)
                    }
                    CalendarView::Week => {
                        render_week_view(ui, &entries, focus, timezone, today, &self.runtime.calendar, now_utc)
                    }
                    CalendarView::Day => {
                        render_day_view(ui, &entries, focus, timezone, today, &self.runtime.calendar, now_utc)
                    }
                });
        });
    }
}

fn render_year_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    egui::Grid::new("calendar_year_grid")
        .num_columns(3)
        .spacing(Vec2::new(12.0, 12.0))
        .show(ui, |ui| {
            for (index, month) in year_months(focus).iter().enumerate() {
                let month_entries = entries_for_month(entries, *month, timezone);
                period_card(ui, month.format("%B").to_string(), month_entries, *month, today, config, now_utc);
                if (index + 1) % 3 == 0 {
                    ui.end_row();
                }
            }
        });
}

fn render_quarter_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    egui::Grid::new("calendar_quarter_grid")
        .num_columns(3)
        .spacing(Vec2::new(12.0, 12.0))
        .show(ui, |ui| {
            for month in quarter_months(focus) {
                let month_entries = entries_for_month(entries, month, timezone);
                period_card(ui, month.format("%B").to_string(), month_entries, month, today, config, now_utc);
            }
        });
}

fn render_month_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    let weekdays = if config.week_start_monday {
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    } else {
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    };
    ui.columns(7, |columns| {
        for (index, label) in weekdays.iter().enumerate() {
            columns[index].label(RichText::new(*label).small().strong());
        }
    });
    ui.add_space(6.0);
    let start = month_grid_start(focus, config.week_start_monday);
    let days = month_days(start);
    egui::Grid::new("calendar_month_grid")
        .num_columns(7)
        .spacing(Vec2::new(8.0, 8.0))
        .show(ui, |ui| {
            for (index, day) in days.iter().enumerate() {
                let day_entries = entries_for_day(entries, *day, timezone);
                let is_today = *day == today;
                let is_outside = day.month() != focus.month();
                let is_past = *day < today;
                let muted = config.de_emphasize_past_periods && is_past;
                let fill = if is_today {
                    Color32::from_rgba_unmultiplied(47, 125, 246, 28)
                } else if muted {
                    Color32::from_rgba_unmultiplied(127, 134, 145, 12)
                } else {
                    ui.visuals().faint_bg_color
                };
                egui::Frame::group(ui.style())
                    .fill(fill)
                    .corner_radius(12.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_min_size(Vec2::new(120.0, 110.0));
                        let heading = if is_outside {
                            RichText::new(day.day().to_string()).weak()
                        } else if is_today {
                            RichText::new(day.day().to_string()).strong().color(Color32::from_rgb(47, 125, 246))
                        } else {
                            RichText::new(day.day().to_string()).strong()
                        };
                        ui.label(heading);
                        marker_row(ui, &day_entries, config, now_utc);
                        for entry in day_entries
                            .into_iter()
                            .filter(|entry| should_show_marker(entry, config, now_utc) || !config.hide_past_markers)
                            .take(3)
                        {
                            let text = truncate(&entry.label, 18);
                            ui.colored_label(parse_color(&entry.color), text);
                        }
                    });
                if (index + 1) % 7 == 0 {
                    ui.end_row();
                }
            }
        });
}

fn render_week_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    let days = week_days(focus, config.week_start_monday);
    ui.columns(7, |columns| {
        for (index, day) in days.iter().enumerate() {
            let day_entries = entries_for_day(entries, *day, timezone);
            let fill = if *day == today {
                Color32::from_rgba_unmultiplied(47, 125, 246, 28)
            } else if config.de_emphasize_past_periods && *day < today {
                Color32::from_rgba_unmultiplied(127, 134, 145, 12)
            } else {
                columns[index].visuals().faint_bg_color
            };
            egui::Frame::group(columns[index].style())
                .fill(fill)
                .corner_radius(12.0)
                .inner_margin(8.0)
                .show(&mut columns[index], |ui| {
                    ui.set_min_size(Vec2::new(120.0, 220.0));
                    ui.label(RichText::new(day.format("%a %e").to_string()).strong());
                    ui.small(format!("{} items", day_entries.len()));
                    marker_row(ui, &day_entries, config, now_utc);
                    ui.add_space(4.0);
                    for entry in day_entries.iter().take(6) {
                        let time = entry.due_utc.with_timezone(&timezone).format("%H:%M");
                        let text = format!("{time} {}", truncate(&entry.label, 18));
                        let color = if should_show_entry_in_list(entry, config, now_utc) {
                            parse_color(&entry.color)
                        } else {
                            Color32::from_gray(140)
                        };
                        ui.colored_label(color, text);
                    }
                });
        }
    });
}

fn render_day_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    let day_entries = entries_for_day(entries, focus, timezone);
    let is_today = focus == today;
    let now_local = now_utc.with_timezone(&timezone);
    let hour_start = config.day_view_hour_start;
    let hour_end = config.day_view_hour_end;

    ui.label(RichText::new(focus.format("%A %B %e, %Y").to_string()).strong().size(20.0));
    ui.add_space(8.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for hour in hour_start..=hour_end {
            egui::Frame::group(ui.style())
                .fill(if is_today && now_local.hour() == u32::from(hour) {
                    Color32::from_rgba_unmultiplied(47, 125, 246, 24)
                } else {
                    ui.visuals().faint_bg_color
                })
                .corner_radius(10.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{hour:02}:00"));
                        ui.separator();
                        ui.vertical(|ui| {
                            let hour_entries = day_entries
                                .iter()
                                .filter(|entry| entry.due_utc.with_timezone(&timezone).hour() == u32::from(hour))
                                .collect::<Vec<_>>();
                            if hour_entries.is_empty() {
                                ui.small(RichText::new("No items").weak());
                            } else {
                                for entry in hour_entries {
                                    let color = if should_show_entry_in_list(entry, config, now_utc) {
                                        parse_color(&entry.color)
                                    } else {
                                        Color32::from_gray(140)
                                    };
                                    ui.colored_label(
                                        color,
                                        format!(
                                            "{}  {}",
                                            entry.due_utc.with_timezone(&timezone).format("%H:%M"),
                                            entry.label
                                        ),
                                    );
                                    if !entry.task.description.is_empty() {
                                        ui.small(truncate(&entry.task.description, 80));
                                    }
                                }
                            }
                        });
                    });
                });
            ui.add_space(6.0);
        }
    });
}

fn period_card(
    ui: &mut egui::Ui,
    title: String,
    entries: Vec<CalendarEntry>,
    month: chrono::NaiveDate,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    let is_current = month.year() == today.year() && month.month() == today.month();
    let is_past = month < chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let fill = if is_current {
        Color32::from_rgba_unmultiplied(47, 125, 246, 24)
    } else if config.de_emphasize_past_periods && is_past {
        Color32::from_rgba_unmultiplied(127, 134, 145, 12)
    } else {
        ui.visuals().faint_bg_color
    };
    egui::Frame::group(ui.style())
        .fill(fill)
        .corner_radius(14.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(170.0, 120.0));
            ui.label(RichText::new(title).strong());
            ui.small(format!("{} items", entries.len()));
            marker_row(ui, &entries, config, now_utc);
            ui.add_space(4.0);
            for entry in entries.iter().take(4) {
                let color = if should_show_entry_in_list(entry, config, now_utc) {
                    parse_color(&entry.color)
                } else {
                    Color32::from_gray(140)
                };
                ui.colored_label(color, truncate(&entry.label, 20));
            }
        });
}

fn marker_row(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
) {
    let markers = entries
        .iter()
        .filter(|entry| should_show_marker(entry, config, now_utc))
        .take(config.red_dot_limit)
        .collect::<Vec<_>>();
    if markers.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for entry in markers.into_iter().take(8) {
            let glyph = match entry.marker_kind {
                CalendarMarkerKind::ExternalCalendar => "●",
                CalendarMarkerKind::KanbanBoard => "▲",
                CalendarMarkerKind::Unassigned => "■",
            };
            ui.colored_label(parse_color(&entry.color), glyph);
        }
        if entries.len() > 8 {
            ui.small(format!("+{}", entries.len() - 8));
        }
    });
}

fn marker_legend_row(ui: &mut egui::Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.label(label);
    });
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.separator();
        ui.label(RichText::new(value.to_string()).strong());
    });
}
