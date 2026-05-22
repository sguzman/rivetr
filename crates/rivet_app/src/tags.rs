use crate::types::TagSchema;

pub const KANBAN_TAG_KEY: &str = "kanban";
pub const BOARD_TAG_KEY: &str = "board";
pub const CAL_SOURCE_TAG_KEY: &str = "cal_source";
pub const CAL_EVENT_TAG_KEY: &str = "cal_event";
pub const CAL_COLOR_TAG_KEY: &str = "cal_color";
pub const DEFAULT_CALENDAR_COLOR: &str = "#7f8691";

pub fn split_tags(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn split_tag(tag: &str) -> Option<(&str, &str)> {
    let (key, value) = tag.split_once(':')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

pub fn first_tag_value(tags: &[String], key: &str) -> Option<String> {
    tags.iter().find_map(|tag| {
        let (entry_key, entry_value) = split_tag(tag)?;
        if entry_key == key {
            Some(entry_value.to_string())
        } else {
            None
        }
    })
}

pub fn task_has_tag_value(tags: &[String], key: &str, value: &str) -> bool {
    tags.iter().any(|tag| {
        matches!(split_tag(tag), Some((entry_key, entry_value)) if entry_key == key && entry_value == value)
    })
}

pub fn push_tag_unique(tags: &mut Vec<String>, tag: impl Into<String>) {
    let tag = tag.into();
    if !tags.iter().any(|existing| existing == &tag) {
        tags.push(tag);
    }
}

pub fn remove_tags_for_key(tags: &mut Vec<String>, key: &str) {
    tags.retain(|tag| !matches!(split_tag(tag), Some((entry_key, _)) if entry_key == key));
}

pub fn default_kanban_lane(schema: &TagSchema) -> String {
    schema
        .keys
        .iter()
        .find(|key| key.id == KANBAN_TAG_KEY)
        .and_then(|key| key.values.iter().find(|value| !value.trim().is_empty()))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "todo".to_string())
}

pub fn kanban_columns(schema: &TagSchema) -> Vec<String> {
    let values = schema
        .keys
        .iter()
        .find(|key| key.id == KANBAN_TAG_KEY)
        .map(|key| {
            key.values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if values.is_empty() {
        vec!["todo".to_string(), "working".to_string(), "finished".to_string()]
    } else {
        values
    }
}

pub fn board_id_from_tags(tags: &[String]) -> Option<String> {
    first_tag_value(tags, BOARD_TAG_KEY)
}

pub fn lane_from_tags(tags: &[String], schema: &TagSchema) -> String {
    first_tag_value(tags, KANBAN_TAG_KEY).unwrap_or_else(|| default_kanban_lane(schema))
}

pub fn set_single_tag_value(tags: &mut Vec<String>, key: &str, value: Option<&str>) {
    remove_tags_for_key(tags, key);
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        push_tag_unique(tags, format!("{key}:{}", value.trim()));
    }
}

pub fn normalize_tag_value(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let collapsed = out
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if collapsed.is_empty() {
        "value".to_string()
    } else {
        collapsed
    }
}

pub fn ensure_default_kanban_lane_tag(tags: &mut Vec<String>, schema: &TagSchema) {
    if tags
        .iter()
        .any(|tag| matches!(split_tag(tag), Some((key, _)) if key == KANBAN_TAG_KEY))
    {
        return;
    }
    push_tag_unique(tags, format!("{KANBAN_TAG_KEY}:{}", default_kanban_lane(schema)));
}
