use std::collections::BTreeMap;

use chrono::{
  DateTime,
  Utc
};
use serde::{
  Deserialize,
  Serialize
};
use uuid::Uuid;

use crate::datetime::taskwarrior_date_serde;

#[derive(
  Debug,
  Clone,
  Serialize,
  Deserialize,
  PartialEq,
  Eq,
)]
#[serde(rename_all = "lowercase")]
pub enum Status {
  Pending,
  Completed,
  Deleted,
  Waiting
}

#[derive(
  Debug, Clone, Serialize, Deserialize,
)]
pub struct Annotation {
  #[serde(
    with = "taskwarrior_date_serde"
  )]
  pub entry:       DateTime<Utc>,
  pub description: String
}

#[derive(
  Debug, Clone, Serialize, Deserialize,
)]
pub struct Task {
  pub uuid: Uuid,

  #[serde(default)]
  pub id: Option<u64>,

  pub description: String,

  pub status: Status,

  #[serde(
    with = "taskwarrior_date_serde"
  )]
  pub entry: DateTime<Utc>,

  #[serde(
    with = "taskwarrior_date_serde"
  )]
  pub modified: DateTime<Utc>,

  #[serde(
    default,
    with = "taskwarrior_date_serde::option"
  )]
  pub end: Option<DateTime<Utc>>,

  #[serde(
    default,
    with = "taskwarrior_date_serde::option"
  )]
  pub start: Option<DateTime<Utc>>,

  #[serde(default)]
  pub project: Option<String>,

  #[serde(default)]
  pub priority: Option<String>,

  #[serde(default)]
  pub tags: Vec<String>,

  #[serde(
    default,
    with = "taskwarrior_date_serde::option"
  )]
  pub due: Option<DateTime<Utc>>,

  #[serde(
    default,
    with = "taskwarrior_date_serde::option"
  )]
  pub scheduled: Option<DateTime<Utc>>,

  #[serde(
    default,
    with = "taskwarrior_date_serde::option"
  )]
  pub wait: Option<DateTime<Utc>>,

  #[serde(default)]
  pub depends: Vec<Uuid>,

  #[serde(default)]
  pub annotations: Vec<Annotation>,

  #[serde(flatten)]
  pub extra:
    BTreeMap<String, serde_json::Value>
}

impl Task {
  pub fn new_pending(
    description: String,
    now: DateTime<Utc>,
    id: u64
  ) -> Self {
    Self {
      uuid: Uuid::new_v4(),
      id: Some(id),
      description,
      status: Status::Pending,
      entry: now,
      modified: now,
      end: None,
      start: None,
      project: None,
      priority: None,
      tags: vec![],
      due: None,
      scheduled: None,
      wait: None,
      depends: vec![],
      annotations: vec![],
      extra: BTreeMap::new()
    }
  }

  pub fn is_waiting(
    &self,
    now: DateTime<Utc>
  ) -> bool {
    self.status == Status::Waiting
      || self
        .wait
        .map(|w| w > now)
        .unwrap_or(false)
  }
}
