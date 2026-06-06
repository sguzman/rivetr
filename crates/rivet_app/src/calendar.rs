use chrono::{Datelike, DateTime, Days, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::tags::{board_id_from_tags, first_tag_value, CAL_COLOR_TAG_KEY, CAL_SOURCE_TAG_KEY, DEFAULT_CALENDAR_COLOR};
use crate::types::{
    CalendarConfig, CalendarEntry, CalendarMarkerKind, CalendarView, KanbanBoard, TaskDto, TaskStatus,
};

pub fn visible_calendar_entries(
    tasks: &[TaskDto],
    boards: &[KanbanBoard],
    config: &CalendarConfig,
    _now_utc: DateTime<Utc>,
    active_tags: &std::collections::BTreeSet<String>,
) -> Vec<CalendarEntry> {
    let timezone = config
        .timezone
        .parse::<Tz>()
        .unwrap_or(chrono_tz::America::Mexico_City);
    let mut entries = tasks
        .iter()
        .filter_map(|task| task_to_entry(task, boards, timezone))
        .filter(|entry| visibility_allows(config, entry.task.status))
        .filter(|entry| {
            active_tags.is_empty()
                || entry.task.tags.iter().any(|t| active_tags.contains(t))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.due_utc);
    entries
}

fn visibility_allows(config: &CalendarConfig, status: TaskStatus) -> bool {
    match status {
        TaskStatus::Pending => config.visibility_pending,
        TaskStatus::Waiting => config.visibility_waiting,
        TaskStatus::Completed => config.visibility_completed,
        TaskStatus::Deleted => config.visibility_deleted,
    }
}

fn task_to_entry(task: &TaskDto, boards: &[KanbanBoard], timezone: Tz) -> Option<CalendarEntry> {
    let due_raw = task.due.as_deref().or(task.scheduled.as_deref())?;
    let due_utc = parse_task_datetime(due_raw, timezone)?;
    let board_id = board_id_from_tags(&task.tags);
    let source_id = first_tag_value(&task.tags, CAL_SOURCE_TAG_KEY);
    let board_color = board_id.as_ref().and_then(|board_id| {
        boards
            .iter()
            .find(|board| board.id == *board_id)
            .map(|board| board.color.clone())
    });
    let tag_color = first_tag_value(&task.tags, CAL_COLOR_TAG_KEY).map(|value| format!("#{value}"));
    let marker_kind = if source_id.is_some() {
        CalendarMarkerKind::ExternalCalendar
    } else if board_id.is_some() {
        CalendarMarkerKind::KanbanBoard
    } else {
        CalendarMarkerKind::Unassigned
    };
    Some(CalendarEntry {
        task: task.clone(),
        due_utc,
        label: task.title.clone(),
        color: tag_color.or(board_color).unwrap_or_else(|| DEFAULT_CALENDAR_COLOR.to_string()),
        marker_kind,
        board_id,
        source_id,
    })
}

pub fn parse_task_datetime(raw: &str, timezone: Tz) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    if raw.ends_with('Z')
        && let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%SZ")
    {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%S") {
        return local_naive_to_utc(timezone, naive);
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y%m%d") {
        return local_naive_to_utc(timezone, date.and_hms_opt(0, 0, 0)?);
    }
    None
}

fn local_naive_to_utc(timezone: Tz, naive: NaiveDateTime) -> Option<DateTime<Utc>> {
    match timezone.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(first, second) => Some(first.min(second).with_timezone(&Utc)),
        LocalResult::None => None,
    }
}

pub fn shift_focus(view: CalendarView, focus: NaiveDate, amount: i32) -> NaiveDate {
    match view {
        CalendarView::Year => NaiveDate::from_ymd_opt(focus.year() + amount, 1, 1).unwrap_or(focus),
        CalendarView::Quarter => shift_month_focus(focus, amount * 3),
        CalendarView::Month => shift_month_focus(focus, amount),
        CalendarView::Week => focus + Duration::days(i64::from(amount) * 7),
        CalendarView::Day => focus + Duration::days(i64::from(amount)),
    }
}

fn shift_month_focus(focus: NaiveDate, amount: i32) -> NaiveDate {
    let mut year = focus.year();
    let mut month = focus.month() as i32 + amount;
    while month < 1 {
        month += 12;
        year -= 1;
    }
    while month > 12 {
        month -= 12;
        year += 1;
    }
    NaiveDate::from_ymd_opt(year, month as u32, 1).unwrap_or(focus)
}

pub fn month_grid_start(focus: NaiveDate, monday_start: bool) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(focus.year(), focus.month(), 1).unwrap_or(focus);
    let weekday = if monday_start {
        first.weekday().num_days_from_monday()
    } else {
        first.weekday().num_days_from_sunday()
    };
    first - Duration::days(i64::from(weekday))
}

pub fn month_days(start: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::with_capacity(42);
    for offset in 0..42 {
        if let Some(day) = start.checked_add_days(Days::new(offset)) {
            out.push(day);
        }
    }
    out
}

pub fn quarter_months(focus: NaiveDate) -> Vec<NaiveDate> {
    let quarter_start = ((focus.month() - 1) / 3) * 3 + 1;
    (0..3)
        .filter_map(|offset| NaiveDate::from_ymd_opt(focus.year(), quarter_start + offset, 1))
        .collect()
}

pub fn week_days(focus: NaiveDate, monday_start: bool) -> Vec<NaiveDate> {
    let weekday = if monday_start {
        focus.weekday().num_days_from_monday()
    } else {
        focus.weekday().num_days_from_sunday()
    };
    let start = focus - Duration::days(i64::from(weekday));
    month_days(start).into_iter().take(7).collect()
}

pub fn year_months(focus: NaiveDate) -> Vec<NaiveDate> {
    (1..=12)
        .filter_map(|month| NaiveDate::from_ymd_opt(focus.year(), month, 1))
        .collect()
}

pub fn entries_for_day(entries: &[CalendarEntry], day: NaiveDate, timezone: Tz) -> Vec<CalendarEntry> {
    entries
        .iter()
        .filter(|entry| entry.due_utc.with_timezone(&timezone).date_naive() == day)
        .cloned()
        .collect()
}

pub fn entries_for_month(entries: &[CalendarEntry], month: NaiveDate, timezone: Tz) -> Vec<CalendarEntry> {
    entries
        .iter()
        .filter(|entry| {
            let local_day = entry.due_utc.with_timezone(&timezone).date_naive();
            local_day.year() == month.year() && local_day.month() == month.month()
        })
        .cloned()
        .collect()
}

pub fn calendar_title(view: CalendarView, focus: NaiveDate) -> String {
    match view {
        CalendarView::Year => focus.format("%Y").to_string(),
        CalendarView::Quarter => {
            let quarter = ((focus.month() - 1) / 3) + 1;
            format!("Q{quarter} {}", focus.year())
        }
        CalendarView::Month => focus.format("%B %Y").to_string(),
        CalendarView::Week => {
            let start = focus;
            let end = focus + Duration::days(6);
            format!("{} - {}", start.format("%b %e"), end.format("%b %e, %Y"))
        }
        CalendarView::Day => focus.format("%A, %B %e, %Y").to_string(),
    }
}

pub fn period_entries(
    entries: &[CalendarEntry],
    view: CalendarView,
    focus: NaiveDate,
    timezone: Tz,
    monday_start: bool,
) -> Vec<CalendarEntry> {
    match view {
        CalendarView::Year => year_months(focus)
            .into_iter()
            .flat_map(|month| entries_for_month(entries, month, timezone))
            .collect(),
        CalendarView::Quarter => quarter_months(focus)
            .into_iter()
            .flat_map(|month| entries_for_month(entries, month, timezone))
            .collect(),
        CalendarView::Month => entries_for_month(
            entries,
            NaiveDate::from_ymd_opt(focus.year(), focus.month(), 1).unwrap_or(focus),
            timezone,
        ),
        CalendarView::Week => week_days(focus, monday_start)
            .into_iter()
            .flat_map(|day| entries_for_day(entries, day, timezone))
            .collect(),
        CalendarView::Day => entries_for_day(entries, focus, timezone),
    }
}

pub fn should_show_entry_in_list(entry: &CalendarEntry, config: &CalendarConfig, now_utc: DateTime<Utc>) -> bool {
    !config.filter_before_now || entry.due_utc >= now_utc
}

pub fn should_show_marker(entry: &CalendarEntry, config: &CalendarConfig, now_utc: DateTime<Utc>) -> bool {
    !config.hide_past_markers || entry.due_utc >= now_utc
}

pub fn period_stats(entries: &[CalendarEntry]) -> (usize, usize, usize, usize, usize) {
    let mut pending = 0;
    let mut waiting = 0;
    let mut completed = 0;
    let mut deleted = 0;
    for entry in entries {
        match entry.task.status {
            TaskStatus::Pending => pending += 1,
            TaskStatus::Waiting => waiting += 1,
            TaskStatus::Completed => completed += 1,
            TaskStatus::Deleted => deleted += 1,
        }
    }
    (entries.len(), pending, waiting, completed, deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CalendarMarkerKind;
    use uuid::Uuid;

    fn sample_task(due: &str) -> TaskDto {
        TaskDto {
            uuid: Uuid::nil(),
            id: Some(1),
            title: "Event".to_string(),
            description: String::new(),
            status: TaskStatus::Pending,
            project: None,
            tags: vec!["cal_source:test".to_string(), "cal_color:ff0000".to_string()],
            priority: None,
            due: Some(due.to_string()),
            wait: None,
            scheduled: None,
            created: None,
            modified: None,
        }
    }

    #[test]
    fn parse_task_datetime_handles_taskwarrior_utc() {
        let parsed = parse_task_datetime("20260522T120000Z", chrono_tz::UTC);
        assert!(parsed.is_some());
    }

    #[test]
    fn shift_focus_week_moves_by_seven_days() {
        let focus = NaiveDate::from_ymd_opt(2026, 5, 22).expect("valid test date");
        assert_eq!(shift_focus(CalendarView::Week, focus, 1), focus + Duration::days(7));
    }

    #[test]
    fn visible_calendar_entries_keeps_past_entries_for_rendering() {
        let task = sample_task("20260501T120000Z");
        let config = CalendarConfig {
            timezone: "UTC".to_string(),
            week_start_monday: true,
            red_dot_limit: 5,
            task_list_limit: 20,
            task_list_window_days: 30,
            visibility_pending: true,
            visibility_waiting: true,
            visibility_completed: true,
            visibility_deleted: true,
            de_emphasize_past_periods: true,
            filter_before_now: true,
            hide_past_markers: true,
            day_view_hour_start: 0,
            day_view_hour_end: 23,
        };
        let entries = visible_calendar_entries(&[task], &[], &config, Utc::now(), &std::collections::BTreeSet::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].marker_kind, CalendarMarkerKind::ExternalCalendar);
    }
}
