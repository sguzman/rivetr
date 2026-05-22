use uuid::Uuid;

use crate::tags::{board_id_from_tags, first_tag_value, set_single_tag_value, BOARD_TAG_KEY, KANBAN_TAG_KEY};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KanbanDragPayload {
    pub task_id: Uuid,
    pub from_board_id: Option<String>,
    pub from_lane: String,
}

pub fn lane_from_task_tags(tags: &[String]) -> Option<String> {
    first_tag_value(tags, KANBAN_TAG_KEY)
}

pub fn apply_drop_to_tags(tags: &mut Vec<String>, board_id: Option<&str>, lane: Option<&str>) {
    if let Some(board_id) = board_id {
        set_single_tag_value(tags, BOARD_TAG_KEY, Some(board_id));
    }
    if let Some(lane) = lane {
        set_single_tag_value(tags, KANBAN_TAG_KEY, Some(lane));
    }
}

pub fn board_from_task_tags(tags: &[String]) -> Option<String> {
    board_id_from_tags(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_drop_updates_board_and_lane() {
        let mut tags = vec!["kanban:todo".to_string(), "board:main".to_string()];
        apply_drop_to_tags(&mut tags, Some("work"), Some("doing"));
        assert!(tags.iter().any(|tag| tag == "board:work"));
        assert!(tags.iter().any(|tag| tag == "kanban:doing"));
        assert_eq!(tags.iter().filter(|tag| tag.starts_with("board:")).count(), 1);
        assert_eq!(tags.iter().filter(|tag| tag.starts_with("kanban:")).count(), 1);
    }
}
