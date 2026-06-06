use chrono::{Datelike, Local, Timelike, Utc};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Sense, Stroke, Vec2};

use super::{
    calendar_title, entries_for_day, entries_for_month, month_days, month_grid_start, parse_color,
    period_entries, period_stats, quarter_months, shift_focus, should_show_entry_in_list,
    should_show_marker, truncate, visible_calendar_entries, week_days, year_months, RivetApp,
};
use crate::types::{CalendarEntry, CalendarMarkerKind, CalendarView, ThemeMode};

const LEFT_PANEL_WIDTH: f32 = 300.0;
const RIGHT_PANEL_WIDTH: f32 = 312.0;
const YEAR_CARD_HEIGHT: f32 = 118.0;
const QUARTER_CARD_HEIGHT: f32 = 132.0;
const MONTH_CELL_HEIGHT: f32 = 118.0;
const PERIOD_CARD_GAP: f32 = 12.0;
const DAY_VIEW_MAX_WIDTH: f32 = 760.0;

impl RivetApp {
    pub(super) fn ui_calendar(&mut self, ctx: &egui::Context) {
        let focus = self.ui_state.focus_date();
        let now_utc = Utc::now();
        let timezone = self
            .runtime
            .calendar
            .timezone
            .parse()
            .unwrap_or(chrono_tz::America::Mexico_City);
        let today = Local::now().date_naive();
        let entries = visible_calendar_entries(
            &self.tasks,
            &self.ui_state.kanban_boards,
            &self.runtime.calendar,
            now_utc,
            &self.ui_state.calendar_tag_filters,
        );
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
        let mut navigate_to_entry: Option<CalendarEntry> = None;
        let mut navigate_to_month: Option<chrono::NaiveDate> = None;

        egui::TopBottomPanel::top("calendar_toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(calendar_title(self.ui_state.calendar_view, focus));
                ui.separator();
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
                ui.separator();
                if ui
                    .add(egui::Button::new("Sources").selected(self.ui_state.calendar_show_left_panel))
                    .clicked()
                {
                    self.ui_state.calendar_show_left_panel = !self.ui_state.calendar_show_left_panel;
                    self.mark_ui_dirty();
                }
                if ui
                    .add(egui::Button::new("Details").selected(self.ui_state.calendar_show_right_panel))
                    .clicked()
                {
                    self.ui_state.calendar_show_right_panel = !self.ui_state.calendar_show_right_panel;
                    self.mark_ui_dirty();
                }
                ui.separator();
                for view in CalendarView::ALL {
                    if ui
                        .add_sized(
                            [70.0, 28.0],
                            egui::Button::new(view.label()).selected(self.ui_state.calendar_view == view),
                        )
                        .clicked()
                    {
                        self.ui_state.calendar_view = view;
                        self.mark_ui_dirty();
                    }
                }
                ui.separator();
                ui.small(format!(
                    "Week starts on {}",
                    if self.runtime.calendar.week_start_monday {
                        "Monday"
                    } else {
                        "Sunday"
                    }
                ));
                if self.import_busy {
                    ui.separator();
                    ui.spinner();
                    ui.small("Import busy");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let available_height = ui.available_height();

                if self.ui_state.calendar_show_left_panel {
                    ui.allocate_ui_with_layout(
                        Vec2::new(LEFT_PANEL_WIDTH, available_height),
                        Layout::top_down(Align::Min),
                        |ui| render_left_panel(self, ui, focus, &entries),
                    );
                    ui.add_space(10.0);
                }

                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if self.ui_state.calendar_show_right_panel {
                        ui.allocate_ui_with_layout(
                            Vec2::new(RIGHT_PANEL_WIDTH, available_height),
                            Layout::top_down(Align::Min),
                            |ui| render_right_panel(self, ui, &period_visible, &stats, timezone),
                        );
                        ui.add_space(10.0);
                    }

                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), available_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            let fill = match self.ui_state.theme_mode {
                                ThemeMode::Day => Color32::from_rgb(251, 251, 248),
                                ThemeMode::Night => Color32::from_rgb(24, 28, 36),
                            };
                            egui::Frame::new()
                                .fill(fill)
                                .corner_radius(CornerRadius::same(18))
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                ))
                                .inner_margin(14.0)
                                .show(ui, |ui| match self.ui_state.calendar_view {
                                    CalendarView::Year => render_year_view(
                                        ui,
                                        &entries,
                                        focus,
                                        timezone,
                                        today,
                                        &self.runtime.calendar,
                                        now_utc,
                                        &mut navigate_to_month,
                                    ),
                                    CalendarView::Quarter => render_quarter_view(
                                        ui,
                                        &entries,
                                        focus,
                                        timezone,
                                        today,
                                        &self.runtime.calendar,
                                        now_utc,
                                        &mut navigate_to_month,
                                    ),
                                    CalendarView::Month => render_month_view(
                                        ui,
                                        &entries,
                                        focus,
                                        timezone,
                                        today,
                                        &self.runtime.calendar,
                                        now_utc,
                                        &mut navigate_to_entry,
                                    ),
                                    CalendarView::Week => render_week_view(
                                        ui,
                                        &entries,
                                        focus,
                                        timezone,
                                        today,
                                        &self.runtime.calendar,
                                        now_utc,
                                        &mut navigate_to_entry,
                                    ),
                                    CalendarView::Day => render_day_view(
                                        ui,
                                        &entries,
                                        focus,
                                        timezone,
                                        today,
                                        &self.runtime.calendar,
                                        now_utc,
                                        &mut navigate_to_entry,
                                    ),
                                });
                        },
                    );
                });
            });
        });

        if let Some(month) = navigate_to_month {
            self.ui_state.set_focus_date(month);
            self.ui_state.calendar_view = CalendarView::Month;
            self.mark_ui_dirty();
        }

        if let Some(entry) = navigate_to_entry {
            self.focus_calendar_entry(&entry, timezone);
        }
    }
}

fn render_left_panel(app: &mut RivetApp, ui: &mut egui::Ui, focus: chrono::NaiveDate, entries: &[CalendarEntry]) {
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.heading("Calendar");
            ui.small(format!("Timezone: {}", app.runtime.calendar.timezone));
            ui.small(format!("Focus: {}", focus.format("%Y-%m-%d")));
            ui.small(format!("Items: {}", entries.len()));
        });
    ui.add_space(10.0);

    let legend_height = 110.0;
    let remaining_height = (ui.available_height() - legend_height - 180.0).max(180.0);
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_height(remaining_height);
            ui.heading("Imported Calendars");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!app.import_busy, egui::Button::new("Import ICS"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("ICS", &["ics"]).pick_file()
                {
                    app.import_ics(path);
                }
                if ui
                    .add_enabled(!app.import_busy, egui::Button::new("Import JSON"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file()
                {
                    app.import_json_bundle(path);
                }
            });
            ui.add_space(8.0);
            if app.ui_state.imported_calendars.is_empty() {
                ui.label(RichText::new("No imported calendars yet.").weak());
            } else {
                let calendars = app.ui_state.imported_calendars.clone();
                let row_height = 80.0;
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - 4.0).max(120.0))
                    .show_rows(ui, row_height, calendars.len(), |ui, row_range| {
                        for index in row_range {
                            let source = &calendars[index];
                            egui::Frame::group(ui.style())
                                .fill(ui.visuals().faint_bg_color)
                                .corner_radius(CornerRadius::same(12))
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        paint_marker(ui, parse_color(&source.color), CalendarMarkerKind::ExternalCalendar, 10.0);
                                        ui.label(RichText::new(&source.name).strong());
                                    });
                                    ui.small(source.path.display().to_string());
                                    ui.small(format!("Imported {}", source.last_imported_at));
                                    ui.add_space(4.0);
                                    if ui
                                        .add_enabled(!app.import_busy, egui::Button::new("Re-import"))
                                        .clicked()
                                    {
                                        app.reimport_calendar(source.clone());
                                    }
                                });
                            ui.add_space(6.0);
                        }
                    });
            }
        });

    ui.add_space(10.0);
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.heading("Filters");
            ui.add_space(8.0);
            let mut all_tags = std::collections::BTreeSet::new();
            for entry in entries {
                for tag in &entry.task.tags {
                    if tag.starts_with("cat:") {
                        all_tags.insert(tag.clone());
                    }
                }
            }
            if all_tags.is_empty() {
                ui.label(RichText::new("No categories available.").weak());
            } else {
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for tag in all_tags {
                                let label = tag.trim_start_matches("cat:");
                                let is_active = app.ui_state.calendar_tag_filters.contains(&tag);
                                if ui.selectable_label(is_active, label).clicked() {
                                    if is_active {
                                        app.ui_state.calendar_tag_filters.remove(&tag);
                                    } else {
                                        app.ui_state.calendar_tag_filters.insert(tag);
                                    }
                                    app.mark_ui_dirty();
                                }
                            }
                        });
                    });
            }
            if !app.ui_state.calendar_tag_filters.is_empty() {
                ui.add_space(8.0);
                if ui.button("Clear Filters").clicked() {
                    app.ui_state.calendar_tag_filters.clear();
                    app.mark_ui_dirty();
                }
            }
        });

    ui.add_space(10.0);
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_height(legend_height);
            ui.label(RichText::new("Marker Legend").strong());
            legend_row(ui, Color32::from_rgb(214, 69, 69), CalendarMarkerKind::ExternalCalendar, "External calendar");
            legend_row(ui, Color32::from_rgb(47, 125, 246), CalendarMarkerKind::KanbanBoard, "Kanban board task");
            legend_row(ui, Color32::from_rgb(127, 134, 145), CalendarMarkerKind::Unassigned, "Unassigned task");
        });
}

fn render_right_panel(
    app: &mut RivetApp,
    ui: &mut egui::Ui,
    period_visible: &[CalendarEntry],
    stats: &(usize, usize, usize, usize, usize),
    timezone: chrono_tz::Tz,
) {
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.heading("Period Stats");
            stat_row(ui, "Items", stats.0);
            stat_row(ui, "Pending", stats.1);
            stat_row(ui, "Waiting", stats.2);
            stat_row(ui, "Completed", stats.3);
            stat_row(ui, "Deleted", stats.4);
        });

    ui.add_space(10.0);
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.heading("Tasks In Period");
            ui.small(if app.runtime.calendar.filter_before_now {
                "Past items are filtered from this list."
            } else {
                "Showing all items in this period."
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if period_visible.is_empty() {
                    ui.label(RichText::new("No tasks due in this period.").weak());
                    return;
                }
                for entry in period_visible.iter().take(app.runtime.calendar.task_list_limit) {
                    let response = egui::Frame::group(ui.style())
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                paint_marker(ui, parse_color(&entry.color), entry.marker_kind, 10.0);
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
                            ui.horizontal_wrapped(|ui| {
                                if let Some(source) = entry.source_id.as_deref() {
                                    super::tag_badge(ui, &format!("cal:{source}"));
                                }
                                if let Some(board) = entry.board_id.as_deref() {
                                    super::tag_badge(ui, &format!("board:{board}"));
                                }
                                for tag in entry.task.tags.iter().take(3) {
                                    super::tag_badge(ui, tag);
                                }
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if response.clicked() {
                        app.focus_calendar_entry(entry, timezone);
                    }
                    ui.add_space(6.0);
                }
            });
        });
}

fn render_year_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
    navigate_to_month: &mut Option<chrono::NaiveDate>,
) {
    let row_width = ui.available_width();
    let card_width = ((row_width - (PERIOD_CARD_GAP * 2.0)) / 3.0).max(10.0);
    for row in year_months(focus).chunks(3) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = PERIOD_CARD_GAP;
            for month in row {
                ui.allocate_ui_with_layout(
                    Vec2::new(card_width, YEAR_CARD_HEIGHT),
                    Layout::top_down(Align::Min),
                    |ui| {
                        let month_entries = entries_for_month(entries, *month, timezone);
                        period_card(
                            ui,
                            Vec2::new(card_width, YEAR_CARD_HEIGHT),
                            month.format("%B").to_string(),
                            month_entries,
                            *month,
                            today,
                            config,
                            now_utc,
                            navigate_to_month,
                        );
                    },
                );
            }
        });
        ui.add_space(12.0);
    }
}

fn render_quarter_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
    navigate_to_month: &mut Option<chrono::NaiveDate>,
) {
    let row_width = ui.available_width();
    let card_width = ((row_width - (PERIOD_CARD_GAP * 2.0)) / 3.0).max(10.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PERIOD_CARD_GAP;
        let months = quarter_months(focus);
        for month in &months {
            ui.allocate_ui_with_layout(
                Vec2::new(card_width, QUARTER_CARD_HEIGHT),
                Layout::top_down(Align::Min),
                |ui| {
                    let month_entries = entries_for_month(entries, *month, timezone);
                    period_card(
                        ui,
                        Vec2::new(card_width, QUARTER_CARD_HEIGHT),
                        month.format("%B").to_string(),
                        month_entries,
                        *month,
                        today,
                        config,
                        now_utc,
                        navigate_to_month,
                    );
                },
            );
        }
    });
}

fn render_month_column(
    ui: &mut egui::Ui,
    days: &[chrono::NaiveDate],
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
    navigate_to_entry: &mut Option<CalendarEntry>,
) {
    for day in days {
        let day_entries = entries_for_day(entries, *day, timezone);
        let is_today = *day == today;
        let is_outside = day.month() != focus.month();
        let is_past = *day < today;
        let fill = if is_today {
            Color32::from_rgba_unmultiplied(47, 125, 246, 26)
        } else if config.de_emphasize_past_periods && is_past {
            Color32::from_rgba_unmultiplied(127, 134, 145, 10)
        } else {
            ui.visuals().faint_bg_color
        };

        let response = egui::Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(12))
            .stroke(Stroke::new(
                1.0_f32,
                if is_today {
                    Color32::from_rgb(47, 125, 246)
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke.color
                },
            ))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), MONTH_CELL_HEIGHT));
                let heading = if is_outside {
                    RichText::new(day.day().to_string()).weak()
                } else if is_today {
                    RichText::new(day.day().to_string()).strong().color(Color32::from_rgb(47, 125, 246))
                } else {
                    RichText::new(day.day().to_string()).strong()
                };
                ui.label(heading);
                marker_row(ui, &day_entries, config, now_utc, 6);
                for entry in day_entries
                    .iter()
                    .filter(|entry| should_show_marker(entry, config, now_utc) || !config.hide_past_markers)
                    .take(2)
                {
                    let clicked = ui
                        .horizontal(|ui| {
                            paint_marker(ui, parse_color(&entry.color), entry.marker_kind, 8.0);
                            ui.add(
                                egui::Label::new(RichText::new(truncate(&entry.label, 18)).small())
                                    .sense(Sense::click()),
                            )
                            .clicked()
                        })
                        .inner;
                    if clicked {
                        *navigate_to_entry = Some((*entry).clone());
                    }
                }
            })
            .response
            .interact(Sense::click());

        if response.clicked() && let Some(first) = day_entries.first() {
            *navigate_to_entry = Some(first.clone());
        }
        ui.add_space(8.0);
    }
}

fn render_month_view(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    focus: chrono::NaiveDate,
    timezone: chrono_tz::Tz,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
    navigate_to_entry: &mut Option<CalendarEntry>,
) {
    let weekdays = if config.week_start_monday {
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    } else {
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    };
    let days = month_days(month_grid_start(focus, config.week_start_monday));

    ui.columns(7, |columns| {
        for (index, label) in weekdays.iter().enumerate() {
            columns[index].label(RichText::new(*label).small().strong());
            columns[index].add_space(6.0);
            let column_days = (0..6)
                .filter_map(|row| days.get(index + row * 7).copied())
                .collect::<Vec<_>>();
            render_month_column(
                &mut columns[index],
                &column_days,
                entries,
                focus,
                timezone,
                today,
                config,
                now_utc,
                navigate_to_entry,
            );
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
    navigate_to_entry: &mut Option<CalendarEntry>,
) {
    ui.columns(7, |columns| {
        for (index, day) in week_days(focus, config.week_start_monday).iter().enumerate() {
            let day_entries = entries_for_day(entries, *day, timezone);
            let fill = if *day == today {
                Color32::from_rgba_unmultiplied(47, 125, 246, 26)
            } else if config.de_emphasize_past_periods && *day < today {
                Color32::from_rgba_unmultiplied(127, 134, 145, 10)
            } else {
                columns[index].visuals().faint_bg_color
            };
            egui::Frame::new()
                .fill(fill)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(
                    1.0_f32,
                    columns[index].visuals().widgets.noninteractive.bg_stroke.color,
                ))
                .inner_margin(8.0)
                .show(&mut columns[index], |ui| {
                    ui.set_min_size(Vec2::new(ui.available_width(), 240.0));
                    ui.label(RichText::new(day.format("%a %e").to_string()).strong());
                    ui.small(format!("{} items", day_entries.len()));
                    marker_row(ui, &day_entries, config, now_utc, 6);
                    ui.add_space(4.0);
                    for entry in day_entries.iter().take(5) {
                        let color = if should_show_entry_in_list(entry, config, now_utc) {
                            parse_color(&entry.color)
                        } else {
                            Color32::from_gray(145)
                        };
                        let clicked = ui
                            .horizontal(|ui| {
                                paint_marker(ui, color, entry.marker_kind, 8.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!(
                                            "{} {}",
                                            entry.due_utc.with_timezone(&timezone).format("%H:%M"),
                                            truncate(&entry.label, 18)
                                        ))
                                        .small()
                                        .color(color),
                                    )
                                    .sense(Sense::click()),
                                )
                                .clicked()
                            })
                            .inner;
                        if clicked {
                            *navigate_to_entry = Some((*entry).clone());
                        }
                    }
                    if day_entries.len() > 5 {
                        ui.small(RichText::new(format!("+ {} more", day_entries.len() - 5)).weak());
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
    navigate_to_entry: &mut Option<CalendarEntry>,
) {
    let day_entries = entries_for_day(entries, focus, timezone);
    let now_local = now_utc.with_timezone(&timezone);
    let is_today = focus == today;

    ui.label(RichText::new(focus.format("%A %B %e, %Y").to_string()).strong().size(20.0));
    ui.add_space(8.0);
    let full_height = ui.available_height().max(420.0);
    let target_width = ui.available_width().min(DAY_VIEW_MAX_WIDTH);
    ui.horizontal(|ui| {
        let pad = ((ui.available_width() - target_width) * 0.5).max(0.0);
        if pad > 0.0 {
            ui.add_space(pad);
        }
        ui.allocate_ui_with_layout(
            Vec2::new(target_width, full_height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(full_height);
                egui::ScrollArea::vertical()
                    .max_height(full_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for hour in config.day_view_hour_start..=config.day_view_hour_end {
                            egui::Frame::new()
                                .fill(if is_today && now_local.hour() == u32::from(hour) {
                                    Color32::from_rgba_unmultiplied(47, 125, 246, 18)
                                } else {
                                    ui.visuals().faint_bg_color
                                })
                                .corner_radius(CornerRadius::same(10))
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
                                                let limit = 8;
                                                for entry in hour_entries.iter().take(limit) {
                                                    let color = if should_show_entry_in_list(entry, config, now_utc) {
                                                        parse_color(&entry.color)
                                                    } else {
                                                        Color32::from_gray(145)
                                                    };
                                                    let clicked = ui
                                                        .horizontal(|ui| {
                                                            paint_marker(ui, color, entry.marker_kind, 9.0);
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(format!(
                                                                        "{}  {}",
                                                                        entry.due_utc.with_timezone(&timezone).format("%H:%M"),
                                                                        entry.label
                                                                    ))
                                                                    .color(color),
                                                                )
                                                                .sense(Sense::click()),
                                                            )
                                                            .clicked()
                                                        })
                                                        .inner;
                                                    if clicked {
                                                        *navigate_to_entry = Some((*entry).clone());
                                                    }
                                                    if !entry.task.description.is_empty() {
                                                        ui.small(truncate(&entry.task.description, 90));
                                                    }
                                                }
                                                if hour_entries.len() > limit {
                                                    ui.add_space(4.0);
                                                    ui.small(RichText::new(format!("+ {} more events...", hour_entries.len() - limit)).weak());
                                                }
                                            }
                                        });
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    });
            },
        );
    });
}

fn period_card(
    ui: &mut egui::Ui,
    size: Vec2,
    title: String,
    entries: Vec<CalendarEntry>,
    month: chrono::NaiveDate,
    today: chrono::NaiveDate,
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
    navigate_to_month: &mut Option<chrono::NaiveDate>,
) {
    let is_current = month.year() == today.year() && month.month() == today.month();
    let current_month_start = chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let fill = if is_current {
        Color32::from_rgba_unmultiplied(47, 125, 246, 24)
    } else if config.de_emphasize_past_periods && month < current_month_start {
        Color32::from_rgba_unmultiplied(127, 134, 145, 10)
    } else {
        ui.visuals().faint_bg_color
    };

    ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
        let response = egui::Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(14))
            .stroke(Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_size(size - Vec2::new(4.0, 4.0));
                ui.label(RichText::new(title).strong());
                ui.small(format!("{} items", entries.len()));
                ui.add_space(8.0);
                marker_row(ui, &entries, config, now_utc, 10);
                ui.add_space(8.0);
                let pending = entries
                    .iter()
                    .filter(|entry| should_show_entry_in_list(entry, config, now_utc))
                    .count();
                ui.small(format!("{pending} active in view"));
            })
            .response
            .interact(Sense::click());
        if response.clicked() {
            *navigate_to_month = Some(month);
        }
    });
}

fn marker_row(
    ui: &mut egui::Ui,
    entries: &[CalendarEntry],
    config: &crate::types::CalendarConfig,
    now_utc: chrono::DateTime<Utc>,
    max_markers: usize,
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
        for entry in markers.iter().take(max_markers) {
            paint_marker(ui, parse_color(&entry.color), entry.marker_kind, 8.0);
        }
        if markers.len() > max_markers {
            ui.small(format!("+{}", markers.len() - max_markers));
        }
    });
}

fn legend_row(ui: &mut egui::Ui, color: Color32, kind: CalendarMarkerKind, label: &str) {
    ui.horizontal(|ui| {
        paint_marker(ui, color, kind, 10.0);
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

fn paint_marker(ui: &mut egui::Ui, color: Color32, kind: CalendarMarkerKind, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    match kind {
        CalendarMarkerKind::ExternalCalendar => {
            painter.circle_filled(rect.center(), size * 0.35, color);
        }
        CalendarMarkerKind::KanbanBoard => {
            let top = egui::pos2(rect.center().x, rect.top() + size * 0.15);
            let left = egui::pos2(rect.left() + size * 0.15, rect.bottom() - size * 0.15);
            let right = egui::pos2(rect.right() - size * 0.15, rect.bottom() - size * 0.15);
            painter.add(egui::Shape::convex_polygon(vec![top, left, right], color, Stroke::NONE));
        }
        CalendarMarkerKind::Unassigned => {
            let inner = rect.shrink(size * 0.2);
            painter.rect_filled(inner, CornerRadius::same(1), color);
        }
    }
}
