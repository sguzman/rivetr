use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use ical::parser::ical::component::IcalEvent;
use ical::property::Property;
use parking_lot::Mutex;
use rivet_core::datastore::DataStore;
use rivet_core::datetime::parse_date_expr;
use rivet_core::task::{Status, Task};
use serde_json::Value;
use uuid::Uuid;

use crate::runtime::resolve_gui_data_dir;
use crate::tags::{
    ensure_default_kanban_lane_tag, first_tag_value, normalize_tag_value, push_tag_unique,
    task_has_tag_value, CAL_COLOR_TAG_KEY, CAL_EVENT_TAG_KEY, CAL_SOURCE_TAG_KEY,
};
use crate::types::{
    CalendarConfig, ImportedCalendarSource, TagSchema, TaskCreate, TaskDto, TaskPatch,
    TaskPriority, TaskStatus, TaskUpdateArgs,
};

const RIVET_DETAIL_KEY: &str = "rivet_description";

pub struct TaskService {
    store: Mutex<DataStore>,
    timezone: Tz,
    tag_schema: TagSchema,
}

#[derive(Debug, Clone)]
pub struct IcsImportResult {
    pub source: ImportedCalendarSource,
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub remote_events: usize,
}

#[derive(Debug, Clone)]
struct ExternalCalendarEvent {
    uid: String,
    title: String,
    description: String,
    due_rfc3339: String,
    tags: Vec<String>,
}

impl TaskService {
    pub fn open(calendar: &CalendarConfig, tag_schema: TagSchema) -> anyhow::Result<Self> {
        Self::open_at(resolve_gui_data_dir(), calendar, tag_schema)
    }

    pub fn open_at(
        data_dir: PathBuf,
        calendar: &CalendarConfig,
        tag_schema: TagSchema,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create gui data dir {}", data_dir.display()))?;
        let store = DataStore::open(&data_dir)
            .with_context(|| format!("failed to open datastore {}", data_dir.display()))?;
        Ok(Self {
            store: Mutex::new(store),
            timezone: calendar
                .timezone
                .parse::<Tz>()
                .with_context(|| format!("invalid timezone {}", calendar.timezone))?,
            tag_schema,
        })
    }

    pub fn data_dir(&self) -> PathBuf {
        self.store.lock().data_dir.clone()
    }

    pub fn list_all(&self) -> anyhow::Result<Vec<TaskDto>> {
        let store = self.store.lock();
        let mut tasks = store.load_pending()?;
        tasks.extend(store.load_completed()?);
        Ok(tasks.into_iter().map(task_to_dto).collect())
    }

    pub fn add(&self, create: TaskCreate) -> anyhow::Result<TaskDto> {
        let now = Utc::now();
        let store = self.store.lock();
        let mut pending = store.load_pending()?;
        let next_id = store.next_id(&pending);

        let title = create.title.trim();
        if title.is_empty() {
            anyhow::bail!("task title is required");
        }

        let mut task = Task::new_pending(title.to_string(), now, next_id);
        set_task_detail_description(&mut task, &create.description);
        task.project = create.project.filter(|value| !value.trim().is_empty());
        task.tags = create.tags;
        ensure_default_kanban_lane_tag(&mut task.tags, &self.tag_schema);
        task.priority = create.priority.map(priority_to_core);

        if let Some(due) = create.due.as_deref().filter(|value| !value.trim().is_empty()) {
            task.due = Some(parse_date_expr(due, now)?);
        }
        if let Some(wait) = create.wait.as_deref().filter(|value| !value.trim().is_empty()) {
            task.wait = Some(parse_date_expr(wait, now)?);
        }
        if let Some(scheduled) = create
            .scheduled
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            task.scheduled = Some(parse_date_expr(scheduled, now)?);
        }

        pending.push(task.clone());
        pending.sort_by_key(|entry| entry.id.unwrap_or(u64::MAX));
        store.save_pending(&pending)?;
        Ok(task_to_dto(task))
    }

    pub fn update(&self, update: TaskUpdateArgs) -> anyhow::Result<TaskDto> {
        let now = Utc::now();
        let store = self.store.lock();
        let mut pending = store.load_pending()?;
        let updated = {
            let task = pending
                .iter_mut()
                .find(|task| task.uuid == update.uuid)
                .ok_or_else(|| anyhow::anyhow!("task not found"))?;
            apply_patch(task, update.patch, now, &self.tag_schema)?;
            task.modified = now;
            task.clone()
        };
        store.save_pending(&pending)?;
        Ok(task_to_dto(updated))
    }

    pub fn done(&self, uuid: Uuid) -> anyhow::Result<TaskDto> {
        let now = Utc::now();
        let store = self.store.lock();
        let mut pending = store.load_pending()?;
        let mut completed = store.load_completed()?;

        let idx = pending
            .iter()
            .position(|task| task.uuid == uuid)
            .ok_or_else(|| anyhow::anyhow!("task not found"))?;
        let mut task = pending.remove(idx);
        task.status = Status::Completed;
        task.end = Some(now);
        task.modified = now;
        completed.push(task.clone());
        store.save_pending(&pending)?;
        store.save_completed(&completed)?;
        Ok(task_to_dto(task))
    }

    pub fn uncomplete(&self, uuid: Uuid) -> anyhow::Result<TaskDto> {
        let now = Utc::now();
        let store = self.store.lock();
        let mut pending = store.load_pending()?;
        let mut completed = store.load_completed()?;

        let idx = completed
            .iter()
            .position(|task| task.uuid == uuid)
            .ok_or_else(|| anyhow::anyhow!("task not found"))?;
        let mut task = completed.remove(idx);
        task.status = Status::Pending;
        task.end = None;
        task.modified = now;
        pending.push(task.clone());
        pending.sort_by_key(|entry| entry.id.unwrap_or(u64::MAX));
        store.save_pending(&pending)?;
        store.save_completed(&completed)?;
        Ok(task_to_dto(task))
    }

    pub fn delete(&self, uuid: Uuid) -> anyhow::Result<()> {
        let now = Utc::now();
        let store = self.store.lock();
        let mut pending = store.load_pending()?;
        if let Some(task) = pending.iter_mut().find(|task| task.uuid == uuid) {
            task.status = Status::Deleted;
            task.modified = now;
            store.save_pending(&pending)?;
            return Ok(());
        }

        let mut completed = store.load_completed()?;
        let before = completed.len();
        completed.retain(|task| task.uuid != uuid);
        if completed.len() != before {
            store.save_completed(&completed)?;
            return Ok(());
        }
        anyhow::bail!("task not found");
    }

    pub fn import_ics(
        &self,
        path: &Path,
        source_name: &str,
        color: &str,
    ) -> anyhow::Result<IcsImportResult> {
        let ics_text = fs::read_to_string(path)
            .with_context(|| format!("failed to read ICS file {}", path.display()))?;
        if ics_text.trim().is_empty() {
            anyhow::bail!("ICS file is empty");
        }

        let source = ImportedCalendarSource {
            id: normalize_tag_value(
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("calendar"),
            ),
            name: source_name.trim().to_string(),
            color: color.trim().to_string(),
            path: path.display().to_string(),
            last_imported_at: Utc::now().to_rfc3339(),
        };
        let events = parse_ics_events(&ics_text, &source, self.timezone)?;
        let remote_events = events.len();

        let all_tasks = self.list_all()?;
        let mut existing_by_uid = BTreeMap::<String, TaskDto>::new();
        for task in all_tasks {
            if !task_has_tag_value(&task.tags, CAL_SOURCE_TAG_KEY, &normalize_tag_value(&source.id)) {
                continue;
            }
            if task.status == TaskStatus::Deleted {
                continue;
            }
            if let Some(uid) = first_tag_value(&task.tags, CAL_EVENT_TAG_KEY) {
                existing_by_uid.insert(uid, task);
            }
        }

        let mut created = 0;
        let mut updated = 0;
        let mut deleted = 0;
        let mut seen_uids = BTreeSet::new();

        for event in events {
            seen_uids.insert(event.uid.clone());
            match existing_by_uid.get(&event.uid) {
                Some(existing) if matches!(existing.status, TaskStatus::Pending | TaskStatus::Waiting) => {
                    let update = TaskUpdateArgs {
                        uuid: existing.uuid,
                        patch: TaskPatch {
                            title: Some(event.title.clone()),
                            description: Some(event.description.clone()),
                            project: Some(Some(format!("calendar/{}", source.name))),
                            tags: Some(merge_calendar_tags(&existing.tags, &event.tags)),
                            due: Some(Some(event.due_rfc3339.clone())),
                            wait: Some(None),
                            scheduled: Some(None),
                            ..TaskPatch::default()
                        },
                    };
                    let _ = self.update(update)?;
                    updated += 1;
                }
                Some(_) => {}
                None => {
                    let _ = self.add(TaskCreate {
                        title: event.title.clone(),
                        description: event.description.clone(),
                        project: Some(format!("calendar/{}", source.name)),
                        tags: event.tags.clone(),
                        priority: None,
                        due: Some(event.due_rfc3339.clone()),
                        wait: None,
                        scheduled: None,
                    })?;
                    created += 1;
                }
            }
        }

        for (uid, task) in existing_by_uid {
            if !seen_uids.contains(&uid) {
                self.delete(task.uuid)?;
                deleted += 1;
            }
        }

        Ok(IcsImportResult {
            source,
            created,
            updated,
            deleted,
            remote_events,
        })
    }
}

pub fn can_complete_task(task: &TaskDto) -> bool {
    if !task.tags.iter().any(|tag| matches!(tag.split_once(':'), Some((key, _)) if key == CAL_EVENT_TAG_KEY)) {
        return true;
    }
    let Some(raw_due) = task.due.as_deref() else {
        return true;
    };
    let due = DateTime::parse_from_rfc3339(raw_due)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| DateTime::parse_from_str(raw_due, "%Y%m%dT%H%M%SZ").map(|value| value.with_timezone(&Utc)))
        .ok();
    due.is_none_or(|due| due <= Utc::now())
}

fn priority_to_core(priority: TaskPriority) -> String {
    match priority {
        TaskPriority::Low => "L".to_string(),
        TaskPriority::Medium => "M".to_string(),
        TaskPriority::High => "H".to_string(),
    }
}

fn priority_from_core(priority: Option<String>) -> Option<TaskPriority> {
    match priority.as_deref() {
        Some("L") | Some("low") => Some(TaskPriority::Low),
        Some("M") | Some("med") | Some("medium") => Some(TaskPriority::Medium),
        Some("H") | Some("high") => Some(TaskPriority::High),
        _ => None,
    }
}

fn task_status_for_view(task: &Task) -> TaskStatus {
    if task.status == Status::Pending && task.wait.is_some_and(|wait| wait > Utc::now()) {
        return TaskStatus::Waiting;
    }
    match task.status {
        Status::Pending => TaskStatus::Pending,
        Status::Completed => TaskStatus::Completed,
        Status::Deleted => TaskStatus::Deleted,
        Status::Waiting => TaskStatus::Waiting,
    }
}

fn task_to_dto(task: Task) -> TaskDto {
    TaskDto {
        uuid: task.uuid,
        id: task.id,
        title: task.description.clone(),
        description: task_detail_description(&task).unwrap_or_default(),
        status: task_status_for_view(&task),
        project: task.project,
        tags: task.tags,
        priority: priority_from_core(task.priority),
        due: task.due.map(format_task_datetime),
        wait: task.wait.map(format_task_datetime),
        scheduled: task.scheduled.map(format_task_datetime),
        created: Some(format_task_datetime(task.entry)),
        modified: Some(format_task_datetime(task.modified)),
    }
}

fn format_task_datetime(value: DateTime<Utc>) -> String {
    value.format("%Y%m%dT%H%M%SZ").to_string()
}

fn apply_patch(
    task: &mut Task,
    patch: TaskPatch,
    now: DateTime<Utc>,
    tag_schema: &TagSchema,
) -> anyhow::Result<()> {
    if let Some(title) = patch.title {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            anyhow::bail!("task title is required");
        }
        task.description = trimmed.to_string();
    }
    if let Some(description) = patch.description {
        set_task_detail_description(task, &description);
    }
    if let Some(project) = patch.project {
        task.project = project.filter(|value| !value.trim().is_empty());
    }
    if let Some(mut tags) = patch.tags {
        ensure_default_kanban_lane_tag(&mut tags, tag_schema);
        task.tags = tags;
    }
    if let Some(priority) = patch.priority {
        task.priority = priority.map(priority_to_core);
    }
    if let Some(due) = patch.due {
        task.due = due
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_date_expr(value, now))
            .transpose()?;
    }
    if let Some(wait) = patch.wait {
        task.wait = wait
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_date_expr(value, now))
            .transpose()?;
    }
    if let Some(scheduled) = patch.scheduled {
        task.scheduled = scheduled
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_date_expr(value, now))
            .transpose()?;
    }
    if task.wait.is_some_and(|wait| wait <= now) && task.status == Status::Waiting {
        task.status = Status::Pending;
    }
    Ok(())
}

fn task_detail_description(task: &Task) -> Option<String> {
    task.extra
        .get(RIVET_DETAIL_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn set_task_detail_description(task: &mut Task, description: &str) {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        task.extra.remove(RIVET_DETAIL_KEY);
    } else {
        task.extra
            .insert(RIVET_DETAIL_KEY.to_string(), Value::String(trimmed.to_string()));
    }
}

fn merge_calendar_tags(existing: &[String], managed: &[String]) -> Vec<String> {
    let mut tags = existing
        .iter()
        .filter(|tag| {
            !matches!(
                tag.split_once(':'),
                Some((key, _)) if key == CAL_SOURCE_TAG_KEY || key == CAL_EVENT_TAG_KEY || key == CAL_COLOR_TAG_KEY
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    for tag in managed {
        push_tag_unique(&mut tags, tag.clone());
    }
    tags
}

fn parse_ics_events(
    raw: &str,
    source: &ImportedCalendarSource,
    timezone: Tz,
) -> anyhow::Result<Vec<ExternalCalendarEvent>> {
    let mut out = Vec::new();
    let parser = ical::IcalParser::new(BufReader::new(raw.as_bytes()));
    for calendar in parser {
        let calendar = calendar.map_err(anyhow::Error::new).context("failed reading ICS payload")?;
        for event in calendar.events {
            if let Some(normalized) = normalize_ical_event(&event, source, timezone) {
                out.push(normalized);
            }
        }
    }
    if out.is_empty() {
        anyhow::bail!("ICS file contained no usable events");
    }
    Ok(out)
}

fn normalize_ical_event(
    event: &IcalEvent,
    source: &ImportedCalendarSource,
    timezone: Tz,
) -> Option<ExternalCalendarEvent> {
    let uid_raw = property_value(&event.properties, "UID")?;
    let uid = normalize_tag_value(&uid_raw);
    let title = property_value(&event.properties, "SUMMARY").unwrap_or_else(|| "Calendar Event".to_string());
    let description = {
        let mut parts = Vec::new();
        if let Some(value) = property_value(&event.properties, "DESCRIPTION").filter(|value| !value.trim().is_empty()) {
            parts.push(value.trim().to_string());
        }
        if let Some(value) = property_value(&event.properties, "LOCATION").filter(|value| !value.trim().is_empty()) {
            parts.push(format!("Location: {}", value.trim()));
        }
        parts.join("\n")
    };
    let dtstart = find_property(&event.properties, "DTSTART")?;
    let due_utc = parse_ics_dtstart(dtstart, timezone)?;
    Some(ExternalCalendarEvent {
        uid: uid.clone(),
        title,
        description,
        due_rfc3339: due_utc.to_rfc3339(),
        tags: vec![
            format!("{CAL_SOURCE_TAG_KEY}:{}", normalize_tag_value(&source.id)),
            format!("{CAL_EVENT_TAG_KEY}:{uid}"),
            format!(
                "{CAL_COLOR_TAG_KEY}:{}",
                normalize_tag_value(source.color.trim_start_matches('#'))
            ),
        ],
    })
}

fn parse_ics_dtstart(property: &Property, timezone: Tz) -> Option<DateTime<Utc>> {
    let raw = property.value.as_ref()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    if raw.ends_with('Z') {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%SZ") {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }
    if raw.len() == 8 {
        if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y%m%d") {
            return local_naive_to_utc(property_timezone(property, timezone), date.and_hms_opt(0, 0, 0)?);
        }
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%S") {
        return local_naive_to_utc(property_timezone(property, timezone), naive);
    }
    None
}

fn property_timezone(property: &Property, fallback: Tz) -> Tz {
    let Some(params) = property.params.as_ref() else {
        return fallback;
    };
    for (key, values) in params {
        if key == "TZID" {
            if let Some(value) = values.first() {
                if let Ok(tz) = value.trim().parse::<Tz>() {
                    return tz;
                }
            }
        }
    }
    fallback
}

fn local_naive_to_utc(timezone: Tz, naive: NaiveDateTime) -> Option<DateTime<Utc>> {
    match timezone.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(first, second) => Some(first.min(second).with_timezone(&Utc)),
        LocalResult::None => None,
    }
}

fn find_property<'a>(properties: &'a [Property], name: &str) -> Option<&'a Property> {
    properties.iter().find(|property| property.name == name)
}

fn property_value(properties: &[Property], name: &str) -> Option<String> {
    find_property(properties, name)?
        .value
        .as_ref()
        .map(|value| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn service() -> TaskService {
        let dir = tempdir().expect("tempdir");
        let path = dir.keep();
        TaskService::open_at(
            path,
            &CalendarConfig {
                timezone: "UTC".to_string(),
                week_start_monday: true,
                task_list_limit: 20,
                task_list_window_days: 30,
                visibility_pending: true,
                visibility_waiting: true,
                visibility_completed: true,
                visibility_deleted: true,
                filter_before_now: false,
                hide_past_markers: false,
            },
            TagSchema {
                version: Some(1),
                keys: vec![],
            },
        )
        .expect("service")
    }

    #[test]
    fn add_keeps_title_description_split() {
        let service = service();
        let task = service
            .add(TaskCreate {
                title: "Title".to_string(),
                description: "Details".to_string(),
                project: None,
                tags: vec![],
                priority: Some(TaskPriority::High),
                due: None,
                wait: None,
                scheduled: None,
            })
            .expect("add task");
        assert_eq!(task.title, "Title");
        assert_eq!(task.description, "Details");
        assert!(task.tags.iter().any(|tag| tag == "kanban:todo"));
    }

    #[test]
    fn import_ics_creates_calendar_task() {
        let service = service();
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("sample.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:abc\nSUMMARY:Meet\nDTSTART:20260522T120000Z\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("write");
        let result = service.import_ics(&path, "Sample", "#ff0000").expect("import");
        assert_eq!(result.created, 1);
    }
}
