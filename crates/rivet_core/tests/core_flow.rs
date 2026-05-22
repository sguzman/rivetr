use chrono::Utc;
use rivet_core::datastore::DataStore;
use rivet_core::filter::Filter;
use rivet_core::task::{
  Status,
  Task
};
use tempfile::tempdir;

#[test]
fn datastore_roundtrip_and_filtering() {
  let temp =
    tempdir().expect("tempdir");
  let store =
    DataStore::open(temp.path())
      .expect("open datastore");

  let now = Utc::now();
  let mut task = Task::new_pending(
    "Write parity harness".to_string(),
    now,
    1
  );
  task.tags = vec![
    "core".to_string(),
    "urgent".to_string(),
  ];
  task.project =
    Some("rivet".to_string());

  store
    .add_task(vec![], task.clone())
    .expect("add task should succeed");

  let pending = store
    .load_pending()
    .expect("load pending");
  assert_eq!(pending.len(), 1);

  let filter = Filter::parse(
    &["+urgent".to_string()],
    now
  )
  .expect("parse filter");
  assert!(
    filter.matches(&pending[0], now)
  );

  let mut done_task =
    pending[0].clone();
  done_task.status = Status::Completed;
  done_task.end = Some(now);

  store
    .save_completed(&[done_task])
    .expect("save completed");
  assert_eq!(
    store
      .load_completed()
      .expect("load completed")
      .len(),
    1
  );
}
