use std::fs;
use std::path::PathBuf;

use rivet_app::services::TaskService;
use rivet_app::types::{CalendarConfig, TagSchema, TaskStatus};
use tempfile::tempdir;

#[test]
fn opens_realistic_fixture_datastore() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = tempdir().expect("tempdir");
    fs::copy(
        fixture_root.join("pending.data"),
        dir.path().join("pending.data"),
    )
    .expect("copy pending");
    fs::copy(
        fixture_root.join("completed.data"),
        dir.path().join("completed.data"),
    )
    .expect("copy completed");
    fs::write(dir.path().join("undo.data"), "").expect("undo");
    fs::write(dir.path().join("context.data"), "").expect("context");

    let service = TaskService::open_at(
        dir.path().to_path_buf(),
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
    .expect("open service");

    let tasks = service.list_all().expect("list tasks");
    assert_eq!(tasks.len(), 3);
    assert!(tasks.iter().any(|task| task.status == TaskStatus::Waiting));
    assert!(tasks.iter().any(|task| task.status == TaskStatus::Completed));
    assert!(tasks.iter().any(|task| task.description == "Fixture details one"));
}
