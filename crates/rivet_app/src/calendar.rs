use chrono::{Datelike, DateTime, Days, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::tags::{first_tag_value, CAL_COLOR_TAG_KEY, DEFAULT_CALENDAR_COLOR};
use crate::types::{CalendarConfig, CalendarEntry, CalendarView, KanbanBoard, TaskDto, TaskStatus};

pub fn visible_calendar_entries(
    tasks: &[TaskDto],
    boards: &[KanbanBoard],
    config: &CalendarConfig,
    now_utc: DateTime<Utc>,
) -> Vec<CalendarEntry> {
    let timezone = config
        .timezone
        .parse::<Tz>()
        .unwrap_or(chrono_tz::America::Mexico_City);
    let mut entries = tasks
        .iter()
        .filter_map(|task| task_to_entry(task, boards, timezone))
        .filter(|entry| visibility_allows(config, entry.task.status))
        .filter(|entry| !config.filter_before_now || entry.due_utc >= now_utc)
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
    let board_color = first_tag_value(&task.tags, "board")
        .and_then(|board_id| boards.iter().find(|board| board.id == board_id).map(|board| board.color.clone()));
    let tag_color = first_tag_value(&task.tags, CAL_COLOR_TAG_KEY)
        .map(|value| format!("#{value}"));
    Some(CalendarEntry {
        task: task.clone(),
        due_utc,
        label: task.title.clone(),
        color: tag_color.or(board_color).unwrap_or_else(|| DEFAULT_CALENDAR_COLOR.to_string()),
    })
}

pub fn parse_task_datetime(raw: &str, timezone: Tz) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    if raw.ends_with('Z') {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%SZ") {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
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
        CalendarView::Month => {
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
        CalendarView::Week => focus + Duration::days(i64::from(amount) * 7),
        CalendarView::Day => focus + Duration::days(i64::from(amount)),
    }
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

pub fn week_days(focus: NaiveDate, monday_start: bool) -> Vec<NaiveDate> {
    let weekday = if monday_start {
        focus.weekday().num_days_from_monday()
    } else {
        focus.weekday().num_days_from_sunday()
    };
    let start = focus - Duration::days(i64::from(weekday));
    month_days(start).into_iter().take(7).collect()
}

pub fn entries_for_day(entries: &[CalendarEntry], day: NaiveDate, timezone: Tz) -> Vec<CalendarEntry> {
    entries
        .iter()
        .filter(|entry| entry.due_utc.with_timezone(&timezone).date_naive() == day)
        .cloned()
        .collect()
}

pub fn calendar_title(view: CalendarView, focus: NaiveDate) -> String {
    match view {
        CalendarView::Month => focus.format("%B %Y").to_string(),
        CalendarView::Week => {
            let end = focus + Duration::days(6);
            format!("{} - {}", focus.format("%b %e"), end.format("%b %e, %Y"))
        }
        CalendarView::Day => focus.format("%A, %B %e, %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
