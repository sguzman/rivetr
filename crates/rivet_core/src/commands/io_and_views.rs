#[instrument(skip(store))]
fn cmd_undo(
  store: &mut DataStore
) -> anyhow::Result<()> {
  info!("command undo");

  let Some((pending, completed)) =
    store.pop_undo_snapshot()?
  else {
    println!(
      "No undo transactions available."
    );
    return Ok(());
  };

  store.save_pending(&pending)?;
  store.save_completed(&completed)?;

  println!("Undo completed.");
  Ok(())
}

#[instrument(skip(
  store,
  filter_terms,
  now
))]
fn cmd_export(
  store: &mut DataStore,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command export");

  let pending = store.load_pending()?;
  let completed =
    store.load_completed()?;
  let filter =
    Filter::parse(filter_terms, now)?;

  let rows: Vec<Task> = pending
    .into_iter()
    .chain(completed)
    .filter(|task| {
      filter
        .matches_without_waiting_guard(
          task, now
        )
    })
    .collect();

  let out =
    serde_json::to_string(&rows)?;
  println!("{out}");
  Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct ImportTask {
  #[serde(default)]
  uuid:        Option<uuid::Uuid>,
  #[serde(default)]
  description: Option<String>,
  #[serde(default)]
  status:      Option<Status>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  entry: Option<chrono::DateTime<Utc>>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  modified:
    Option<chrono::DateTime<Utc>>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  end: Option<chrono::DateTime<Utc>>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  start: Option<chrono::DateTime<Utc>>,
  #[serde(default)]
  project:     Option<String>,
  #[serde(default)]
  priority:    Option<String>,
  #[serde(default)]
  tags:        Vec<String>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  due: Option<chrono::DateTime<Utc>>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  scheduled:
    Option<chrono::DateTime<Utc>>,
  #[serde(
    default,
    with = "crate::datetime::taskwarrior_date_serde::option"
  )]
  wait: Option<chrono::DateTime<Utc>>,
  #[serde(default)]
  depends:     Vec<uuid::Uuid>,
  #[serde(default)]
  annotations: Vec<Annotation>,
  #[serde(flatten)]
  extra:       BTreeMap<String, Value>
}

#[instrument(skip(store, hooks))]
fn cmd_import(
  store: &mut DataStore,
  hooks: &HookRunner
) -> anyhow::Result<()> {
  info!("command import");
  let now = Utc::now();

  let mut stdin = String::new();
  io::stdin()
    .read_to_string(&mut stdin)
    .context("failed reading stdin")?;

  let trimmed = stdin.trim();
  if trimmed.is_empty() {
    return Err(anyhow!(
      "import: empty input"
    ));
  }

  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();

  let imported =
    parse_import_items(trimmed)?;
  let mut adds = 0_u64;
  let mut mods = 0_u64;

  for row in imported {
    let existing =
      row.uuid.and_then(|uuid| {
        find_task_by_uuid(
          &pending, &completed, uuid
        )
      });
    let mut task =
      normalize_import_item(row, now);
    normalize_import_identity_and_status(&mut task, existing.as_ref(), store.next_id(&pending));

    if let Some(old) = existing.as_ref()
    {
      task = hooks
        .apply_on_modify(old, &task)?;
      mods += 1;
    } else {
      task =
        hooks.apply_on_add(&task)?;
      adds += 1;
    }
    normalize_import_identity_and_status(&mut task, existing.as_ref(), store.next_id(&pending));
    upsert_imported_task(
      &mut pending,
      &mut completed,
      task,
      existing.as_ref().map(|t| t.uuid)
    );
  }

  let imported_count = adds + mods;
  if imported_count > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&pending)?;
    store.save_completed(&completed)?;
  }

  println!(
    "Imported {imported_count} \
     task(s)."
  );
  Ok(())
}

fn parse_import_items(
  trimmed: &str
) -> anyhow::Result<Vec<ImportTask>> {
  if trimmed.starts_with('[') {
    return serde_json::from_str(
      trimmed
    )
    .context(
      "failed parsing JSON array"
    );
  }

  if trimmed.starts_with('{') {
    if let Ok(item) =
      serde_json::from_str::<ImportTask>(
        trimmed
      )
    {
      return Ok(vec![item]);
    }
  }

  let mut out = Vec::new();
  for (idx, line) in
    trimmed.lines().enumerate()
  {
    let token = line.trim();
    if token.is_empty() {
      continue;
    }
    let item: ImportTask =
      serde_json::from_str(token)
        .with_context(|| {
          format!(
            "failed parsing import \
             line {}",
            idx + 1
          )
        })?;
    out.push(item);
  }

  if out.is_empty() {
    return Err(anyhow!(
      "import: empty input"
    ));
  }

  Ok(out)
}

fn normalize_import_item(
  item: ImportTask,
  now: chrono::DateTime<Utc>
) -> Task {
  let status = item
    .status
    .unwrap_or(Status::Pending);
  let entry = item.entry.unwrap_or(now);
  let modified =
    item.modified.unwrap_or(now);
  let mut task = Task {
    uuid: item.uuid.unwrap_or_else(
      uuid::Uuid::new_v4
    ),
    id: None,
    description: item
      .description
      .unwrap_or_default(),
    status,
    entry,
    modified,
    end: item.end,
    start: item.start,
    project: item.project,
    priority: item.priority,
    tags: item.tags,
    due: item.due,
    scheduled: item.scheduled,
    wait: item.wait,
    depends: item.depends,
    annotations: item.annotations,
    extra: item.extra
  };
  normalize_import_status(&mut task);
  task
}

fn normalize_import_status(
  task: &mut Task
) {
  if task.status == Status::Waiting {
    task.status = Status::Pending;
  }

  match task.status {
    | Status::Pending => {
      task.end = None;
    }
    | Status::Completed
    | Status::Deleted => {
      if task.end.is_none() {
        task.end = Some(task.modified);
      }
    }
    | Status::Waiting => {}
  }
}

fn normalize_import_identity_and_status(
  task: &mut Task,
  old: Option<&Task>,
  next_id: u64
) {
  normalize_import_status(task);
  match task.status {
    | Status::Pending => {
      task.id = old
        .filter(|prev| {
          prev.status == Status::Pending
            || prev.status
              == Status::Waiting
        })
        .and_then(|prev| prev.id)
        .or(Some(next_id));
    }
    | Status::Completed
    | Status::Deleted => {
      task.id = None;
    }
    | Status::Waiting => {}
  }
}

fn find_task_by_uuid(
  pending: &[Task],
  completed: &[Task],
  uuid: uuid::Uuid
) -> Option<Task> {
  pending
    .iter()
    .find(|task| task.uuid == uuid)
    .cloned()
    .or_else(|| {
      completed
        .iter()
        .find(|task| task.uuid == uuid)
        .cloned()
    })
}

fn upsert_imported_task(
  pending: &mut Vec<Task>,
  completed: &mut Vec<Task>,
  task: Task,
  old_uuid: Option<uuid::Uuid>
) {
  let old_uuid =
    old_uuid.unwrap_or(task.uuid);
  pending.retain(|row| {
    row.uuid != old_uuid
      && row.uuid != task.uuid
  });
  completed.retain(|row| {
    row.uuid != old_uuid
      && row.uuid != task.uuid
  });

  match task.status {
    | Status::Completed => {
      completed.push(task)
    }
    | Status::Pending
    | Status::Deleted
    | Status::Waiting => {
      pending.push(task)
    }
  }

  pending.sort_by_key(|row| {
    row.id.unwrap_or(u64::MAX)
  });
  completed.sort_by_key(|row| {
    (row.end, row.id)
  });
}

#[instrument(skip(store))]
fn cmd_projects(
  store: &mut DataStore
) -> anyhow::Result<()> {
  let pending = store.load_pending()?;
  let mut set = BTreeSet::new();
  for task in pending {
    if let Some(project) = task.project
    {
      set.insert(project);
    }
  }

  for project in set {
    println!("{project}");
  }
  Ok(())
}

#[instrument(skip(store))]
fn cmd_tags(
  store: &mut DataStore
) -> anyhow::Result<()> {
  let pending = store.load_pending()?;
  let mut set = BTreeSet::new();
  for task in pending {
    for tag in task.tags {
      set.insert(tag);
    }
  }

  for tag in set {
    println!("{tag}");
  }
  Ok(())
}

#[instrument(skip(store, cfg, args))]
fn cmd_context(
  store: &mut DataStore,
  cfg: &Config,
  args: &[String]
) -> anyhow::Result<()> {
  if args.is_empty() {
    let active =
      store.get_active_context()?;
    println!(
      "active={}",
      active.unwrap_or_else(|| {
        "none".to_string()
      })
    );

    for (key, value) in cfg.iter() {
      if let Some(name) =
        key.strip_prefix("context.")
      {
        println!("{name} {value}");
      }
    }
    return Ok(());
  }

  let cmd =
    args[0].to_ascii_lowercase();
  if cmd == "none" || cmd == "clear" {
    store.set_active_context(None)?;
    println!("Context cleared.");
    return Ok(());
  }

  let name = args[0].as_str();
  let key = format!("context.{name}");
  if cfg.get(&key).is_none() {
    return Err(anyhow!(
      "unknown context: {name}"
    ));
  }

  store
    .set_active_context(Some(name))?;
  println!("Context set: {name}");
  Ok(())
}

fn cmd_commands() -> anyhow::Result<()>
{
  for command in known_command_names() {
    println!("{command}");
  }
  Ok(())
}

fn cmd_show(
  cfg: &Config
) -> anyhow::Result<()> {
  for (k, v) in cfg.iter() {
    println!("{k}={v}");
  }
  Ok(())
}

fn cmd_unique(
  store: &mut DataStore,
  args: &[String]
) -> anyhow::Result<()> {
  if args.is_empty() {
    println!("project");
    println!("tag");
    println!("status");
    return Ok(());
  }

  match args[0].as_str() {
    | "project" | "projects" => {
      cmd_projects(store)
    }
    | "tag" | "tags" => cmd_tags(store),
    | "status" => {
      println!("pending");
      println!("completed");
      println!("deleted");
      println!("waiting");
      Ok(())
    }
    | _ => Ok(())
  }
}

fn cmd_help() -> anyhow::Result<()> {
  println!(
    "Implemented commands: add, \
     append, prepend, list/next, \
     info, modify, start, stop, \
     annotate, denotate, duplicate, \
     log, done, delete, undo, export, \
     import, projects, tags, context"
  );
  Ok(())
}

#[instrument(skip(
  store,
  cfg,
  command,
  filter_terms
))]
fn resolve_effective_filter_terms(
  store: &DataStore,
  cfg: &Config,
  command: &str,
  filter_terms: &[String]
) -> anyhow::Result<Vec<String>> {
  if !command_uses_filter(cfg, command)
  {
    return Ok(filter_terms.to_vec());
  }

  let mut out = Vec::new();
  if let Some(active) =
    store.get_active_context()?
  {
    let key =
      format!("context.{active}");
    if let Some(expr) = cfg.get(&key) {
      out.extend(
        expr
          .split_whitespace()
          .map(ToString::to_string)
      );
    }
  }
  out.extend(
    filter_terms.iter().cloned()
  );
  Ok(out)
}

fn command_uses_filter(
  cfg: &Config,
  command: &str
) -> bool {
  matches!(
    command,
    "append"
      | "prepend"
      | "list"
      | "next"
      | "info"
      | "modify"
      | "start"
      | "stop"
      | "annotate"
      | "denotate"
      | "duplicate"
      | "done"
      | "delete"
  ) || is_report_command(cfg, command)
}

