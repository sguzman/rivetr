#[instrument(skip(
  store, hooks, _cfg, _renderer, args,
  now
))]
fn cmd_add(
  store: &mut DataStore,
  hooks: &HookRunner,
  _cfg: &Config,
  _renderer: &mut Renderer,
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command add");

  let mut pending =
    store.load_pending()?;
  let completed =
    store.load_completed()?;
  let pending_before = pending.clone();

  let next_id = store.next_id(&pending);
  let (description, mods) =
    parse_desc_and_mods(args, now)?;
  let mut task = Task::new_pending(
    description,
    now,
    next_id
  );
  apply_mods(&mut task, &mods, now)?;
  task = hooks.apply_on_add(&task)?;
  if task.id.is_none() {
    task.id = Some(next_id);
  }

  pending = store
    .add_task(pending, task.clone())?;
  store.push_undo_snapshot(
    &pending_before,
    &completed
  )?;

  debug!(
    pending_count = pending.len(),
    "task added"
  );
  println!(
    "Created task {}.",
    task.id.unwrap_or(next_id)
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  args,
  now
))]
fn cmd_append(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command append");

  if args.is_empty() {
    return Err(anyhow!(
      "append requires text argument"
    ));
  }
  let suffix = args.join(" ");

  let filter =
    Filter::parse(filter_terms, now)?;
  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();
  let include_non_pending = filter
    .has_explicit_status_filter()
    || filter.has_identity_selector();

  let mut changed = 0_u64;
  for task in &mut pending {
    if !include_non_pending
      && task.status != Status::Pending
      && task.status != Status::Waiting
    {
      continue;
    }
    if filter.matches(task, now) {
      let old = task.clone();
      task.description = format!(
        "{} {}",
        task.description, suffix
      )
      .trim()
      .to_string();
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      changed += 1;
    }
  }

  if include_non_pending {
    for task in &mut completed {
      if filter.matches(task, now) {
        let old = task.clone();
        task.description = format!(
          "{} {}",
          task.description, suffix
        )
        .trim()
        .to_string();
        task.modified = now;
        *task = hooks.apply_on_modify(
          &old, task
        )?;
        changed += 1;
      }
    }
  }

  if changed > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&pending)?;
    if include_non_pending {
      store
        .save_completed(&completed)?;
    }
  }

  println!(
    "Modified {changed} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  args,
  now
))]
fn cmd_prepend(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command prepend");

  if args.is_empty() {
    return Err(anyhow!(
      "prepend requires text argument"
    ));
  }
  let prefix = args.join(" ");

  let filter =
    Filter::parse(filter_terms, now)?;
  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();
  let include_non_pending = filter
    .has_explicit_status_filter()
    || filter.has_identity_selector();

  let mut changed = 0_u64;
  for task in &mut pending {
    if !include_non_pending
      && task.status != Status::Pending
      && task.status != Status::Waiting
    {
      continue;
    }
    if filter.matches(task, now) {
      let old = task.clone();
      task.description = format!(
        "{} {}",
        prefix, task.description
      )
      .trim()
      .to_string();
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      changed += 1;
    }
  }

  if include_non_pending {
    for task in &mut completed {
      if filter.matches(task, now) {
        let old = task.clone();
        task.description = format!(
          "{} {}",
          prefix, task.description
        )
        .trim()
        .to_string();
        task.modified = now;
        *task = hooks.apply_on_modify(
          &old, task
        )?;
        changed += 1;
      }
    }
  }

  if changed > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&pending)?;
    if include_non_pending {
      store
        .save_completed(&completed)?;
    }
  }

  println!(
    "Modified {changed} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  cfg,
  renderer,
  report_name,
  filter_terms,
  now
))]
fn cmd_list(
  store: &mut DataStore,
  cfg: &Config,
  renderer: &mut Renderer,
  report_name: &str,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command list/next");

  let effective_report_name =
    if report_name == "list" {
      "next"
    } else {
      report_name
    };
  if let Some(spec) = load_report_spec(
    cfg,
    effective_report_name
  ) {
    return run_report(
      store,
      renderer,
      &spec,
      filter_terms,
      now
    );
  }

  let mut pending =
    store.load_pending()?;
  pending.retain(|task| {
    task.status == Status::Pending
      || task.status == Status::Waiting
  });

  let filter =
    Filter::parse(filter_terms, now)?;
  let mut rows: Vec<Task> = pending
    .into_iter()
    .filter(|task| {
      filter.matches(task, now)
    })
    .collect();

  rows.sort_by_key(|task| {
    (task.due, task.id)
  });
  renderer
    .print_task_table(&rows, now)?;
  Ok(())
}

#[instrument(skip(
  store,
  cfg,
  renderer,
  filter_terms,
  now
))]
fn cmd_report(
  store: &mut DataStore,
  cfg: &Config,
  renderer: &mut Renderer,
  report_name: &str,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  let spec =
    load_report_spec(cfg, report_name)
      .ok_or_else(|| {
        anyhow!(
          "unknown report: \
           {report_name}"
        )
      })?;
  run_report(
    store,
    renderer,
    &spec,
    filter_terms,
    now
  )
}

#[instrument(skip(
  store,
  renderer,
  spec,
  cli_filter_terms,
  now
))]
fn run_report(
  store: &mut DataStore,
  renderer: &mut Renderer,
  spec: &ReportSpec,
  cli_filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!(report = %spec.name, "command report");

  let pending = store.load_pending()?;
  let completed =
    store.load_completed()?;

  let mut effective_filter_terms =
    spec.filter_terms.clone();
  effective_filter_terms.extend(
    cli_filter_terms.iter().cloned()
  );

  let filter = Filter::parse(
    &effective_filter_terms,
    now
  )?;

  let mut rows: Vec<Task> = pending
    .into_iter()
    .chain(completed)
    .filter(|task| {
      filter.matches(task, now)
    })
    .collect();

  rows.sort_by(|a, b| {
    compare_tasks_for_report(
      a, b, &spec.sort, now
    )
  });
  if let Some(limit) = spec.limit {
    rows.truncate(limit);
  }

  let table_rows: Vec<Vec<String>> =
    rows
      .iter()
      .map(|task| {
        spec
          .columns
          .iter()
          .map(|col| {
            format_report_cell(
              task, *col, now
            )
          })
          .collect()
      })
      .collect();

  renderer.print_report_table(
    &spec.labels,
    &table_rows
  )?;
  Ok(())
}

#[instrument(skip(
  store,
  renderer,
  filter_terms,
  now
))]
fn cmd_info(
  store: &mut DataStore,
  renderer: &mut Renderer,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command info");

  let pending = store.load_pending()?;
  let completed =
    store.load_completed()?;
  let filter =
    Filter::parse(filter_terms, now)?;

  let mut rows: Vec<Task> = pending
    .into_iter()
    .chain(completed)
    .filter(|task| {
      filter.matches(task, now)
    })
    .collect();

  rows.sort_by_key(|task| {
    task.id.unwrap_or(u64::MAX)
  });

  if rows.is_empty() {
    return Err(anyhow!(
      "no matching tasks"
    ));
  }

  for task in rows {
    renderer.print_task_info(&task)?;
    println!();
  }

  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  args,
  now
))]
fn cmd_modify(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command modify");

  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();

  let filter =
    Filter::parse(filter_terms, now)?;
  let include_non_pending = filter
    .has_explicit_status_filter()
    || filter.has_identity_selector();
  let mods = parse_mods(args, now)?;

  let mut changed = 0_u64;
  for task in &mut pending {
    if !include_non_pending
      && task.status != Status::Pending
      && task.status != Status::Waiting
    {
      continue;
    }
    if filter.matches(task, now) {
      let old = task.clone();
      apply_mods(task, &mods, now)?;
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      changed += 1;
    }
  }

  if include_non_pending {
    for task in &mut completed {
      if filter.matches(task, now) {
        let old = task.clone();
        apply_mods(task, &mods, now)?;
        task.modified = now;
        *task = hooks.apply_on_modify(
          &old, task
        )?;
        changed += 1;
      }
    }
  }

  if changed > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&pending)?;
    if include_non_pending {
      store
        .save_completed(&completed)?;
    }
  }

  println!(
    "Modified {changed} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  now
))]
fn cmd_start(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command start");

  let mut pending =
    store.load_pending()?;
  let pending_before = pending.clone();
  let filter =
    Filter::parse(filter_terms, now)?;

  let mut started = 0_u64;
  for task in &mut pending {
    if task.status != Status::Pending
      || task.is_waiting(now)
    {
      continue;
    }
    if filter.matches(task, now)
      && task.start.is_none()
    {
      let old = task.clone();
      task.start = Some(now);
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      started += 1;
    }
  }

  if started > 0 {
    let completed =
      store.load_completed()?;
    store.push_undo_snapshot(
      &pending_before,
      &completed
    )?;
    store.update_pending(&pending)?;
  }

  println!(
    "Started {started} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  now
))]
fn cmd_stop(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command stop");

  let mut pending =
    store.load_pending()?;
  let pending_before = pending.clone();
  let filter =
    Filter::parse(filter_terms, now)?;

  let mut stopped = 0_u64;
  for task in &mut pending {
    if task.status != Status::Pending {
      continue;
    }
    if filter.matches(task, now)
      && task.start.is_some()
    {
      let old = task.clone();
      task.start = None;
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      stopped += 1;
    }
  }

  if stopped > 0 {
    let completed =
      store.load_completed()?;
    store.push_undo_snapshot(
      &pending_before,
      &completed
    )?;
    store.update_pending(&pending)?;
  }

  println!(
    "Stopped {stopped} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  args,
  now
))]
fn cmd_annotate(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command annotate");

  if args.is_empty() {
    return Err(anyhow!(
      "annotate requires annotation \
       text"
    ));
  }
  let note = args.join(" ");

  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();

  let filter =
    Filter::parse(filter_terms, now)?;
  let mut touched = 0_u64;

  for task in &mut pending {
    if filter.matches(task, now) {
      let old = task.clone();
      task.annotations.push(
        Annotation {
          entry:       now,
          description: note.clone()
        }
      );
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      touched += 1;
    }
  }

  for task in &mut completed {
    if filter.matches(task, now) {
      let old = task.clone();
      task.annotations.push(
        Annotation {
          entry:       now,
          description: note.clone()
        }
      );
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      touched += 1;
    }
  }

  if touched > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&pending)?;
    store.save_completed(&completed)?;
  }

  println!(
    "Annotated {touched} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  args,
  now
))]
fn cmd_denotate(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command denotate");

  if args.is_empty() {
    return Err(anyhow!(
      "denotate requires an index or \
       text selector"
    ));
  }

  let selector_idx = if args.len() == 1
  {
    args[0]
      .parse::<usize>()
      .ok()
      .filter(|idx| *idx > 0)
  } else {
    None
  };
  let selector_text =
    if selector_idx.is_none() {
      Some(
        args
          .join(" ")
          .to_ascii_lowercase()
      )
    } else {
      None
    };

  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();

  let filter =
    Filter::parse(filter_terms, now)?;

  let (tasks_touched_p, removed_p) =
    denotate_tasks(
      &mut pending,
      hooks,
      &filter,
      selector_idx,
      selector_text.as_deref(),
      now
    )?;
  let (tasks_touched_c, removed_c) =
    denotate_tasks(
      &mut completed,
      hooks,
      &filter,
      selector_idx,
      selector_text.as_deref(),
      now
    )?;

  let tasks_touched =
    tasks_touched_p + tasks_touched_c;
  let removed = removed_p + removed_c;

  if removed > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&pending)?;
    store.save_completed(&completed)?;
  }

  println!(
    "Removed {removed} annotation(s) \
     from {tasks_touched} task(s)."
  );
  Ok(())
}

fn denotate_tasks(
  tasks: &mut [Task],
  hooks: &HookRunner,
  filter: &Filter,
  selector_idx: Option<usize>,
  selector_text: Option<&str>,
  now: chrono::DateTime<Utc>
) -> anyhow::Result<(u64, u64)> {
  let mut touched = 0_u64;
  let mut removed = 0_u64;

  for task in tasks {
    if !filter.matches(task, now) {
      continue;
    }

    let old = task.clone();
    let before = task.annotations.len();
    if let Some(idx) = selector_idx {
      if idx <= task.annotations.len() {
        task
          .annotations
          .remove(idx - 1);
      }
    } else if let Some(text) =
      selector_text
    {
      task.annotations.retain(|ann| {
        !ann
          .description
          .to_ascii_lowercase()
          .contains(text)
      });
    }

    let after = task.annotations.len();
    if after < before {
      touched += 1;
      removed +=
        (before - after) as u64;
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
    }
  }

  Ok((touched, removed))
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  now
))]
fn cmd_duplicate(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command duplicate");

  let mut pending =
    store.load_pending()?;
  let pending_before = pending.clone();
  let completed =
    store.load_completed()?;

  let filter =
    Filter::parse(filter_terms, now)?;
  let mut next_id =
    store.next_id(&pending);

  let mut clones = Vec::new();
  for task in &pending {
    if task.status == Status::Deleted {
      continue;
    }
    if filter.matches(task, now) {
      let mut duplicate = task.clone();
      duplicate.uuid =
        uuid::Uuid::new_v4();
      duplicate.id = Some(next_id);
      duplicate.status =
        Status::Pending;
      duplicate.entry = now;
      duplicate.modified = now;
      duplicate.end = None;
      duplicate.start = None;
      duplicate = hooks
        .apply_on_add(&duplicate)?;
      if duplicate.id.is_none() {
        duplicate.id = Some(next_id);
      }
      next_id += 1;
      clones.push(duplicate);
    }
  }

  let duplicated = clones.len() as u64;
  if duplicated > 0 {
    pending.extend(clones);
    pending.sort_by_key(|task| {
      task.id.unwrap_or(u64::MAX)
    });
    store.push_undo_snapshot(
      &pending_before,
      &completed
    )?;
    store.save_pending(&pending)?;
  }

  println!(
    "Duplicated {duplicated} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store, hooks, args, now
))]
fn cmd_log(
  store: &mut DataStore,
  hooks: &HookRunner,
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command log");

  let pending = store.load_pending()?;
  let completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();

  let next_id = store.next_id(&pending);
  let (description, mods) =
    parse_desc_and_mods(args, now)?;

  let mut task = Task::new_pending(
    description,
    now,
    next_id
  );
  apply_mods(&mut task, &mods, now)?;
  task.status = Status::Completed;
  task.end = Some(now);
  task.start = None;
  task.modified = now;
  task = hooks.apply_on_add(&task)?;
  if task.id.is_none() {
    task.id = Some(next_id);
  }

  let mut completed_new = completed;
  completed_new.push(task.clone());
  completed_new
    .sort_by_key(|t| (t.end, t.id));

  store.push_undo_snapshot(
    &pending_before,
    &completed_before
  )?;
  store
    .save_completed(&completed_new)?;

  println!(
    "Logged task {}.",
    task.id.unwrap_or(next_id)
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  now
))]
fn cmd_done(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command done");

  let mut pending =
    store.load_pending()?;
  let mut completed =
    store.load_completed()?;
  let pending_before = pending.clone();
  let completed_before =
    completed.clone();

  let filter =
    Filter::parse(filter_terms, now)?;

  let mut moved = 0_u64;
  let mut keep =
    Vec::with_capacity(pending.len());

  for mut task in pending.drain(..) {
    if (task.status == Status::Pending
      || task.status == Status::Waiting)
      && filter.matches(&task, now)
    {
      let old = task.clone();
      task.status = Status::Completed;
      task.end = Some(now);
      task.start = None;
      task.modified = now;
      task = hooks
        .apply_on_modify(&old, &task)?;

      match task.status {
        | Status::Completed => {
          completed.push(task)
        }
        | Status::Deleted
        | Status::Pending
        | Status::Waiting => {
          keep.push(task)
        }
      }
      moved += 1;
    } else {
      keep.push(task);
    }
  }

  if moved > 0 {
    store.push_undo_snapshot(
      &pending_before,
      &completed_before
    )?;
    store.save_pending(&keep)?;
    store.save_completed(&completed)?;
  }

  println!(
    "Completed {moved} task(s)."
  );
  Ok(())
}

#[instrument(skip(
  store,
  hooks,
  filter_terms,
  now
))]
fn cmd_delete(
  store: &mut DataStore,
  hooks: &HookRunner,
  filter_terms: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  info!("command delete");

  let mut pending =
    store.load_pending()?;
  let pending_before = pending.clone();
  let filter =
    Filter::parse(filter_terms, now)?;

  let mut deleted = 0_u64;
  for task in &mut pending {
    if (task.status == Status::Pending
      || task.status == Status::Waiting)
      && filter.matches(task, now)
    {
      let old = task.clone();
      task.status = Status::Deleted;
      task.start = None;
      task.end = Some(now);
      task.modified = now;
      *task = hooks
        .apply_on_modify(&old, task)?;
      deleted += 1;
    }
  }

  if deleted > 0 {
    let completed =
      store.load_completed()?;
    store.push_undo_snapshot(
      &pending_before,
      &completed
    )?;
    store.save_pending(&pending)?;
  }

  println!(
    "Deleted {deleted} task(s) \
     (soft-delete)."
  );
  Ok(())
}

