#[derive(Debug, Clone)]
enum Mod {
  TagAdd(String),
  TagRemove(String),
  Project(String),
  Priority(String),
  Due(chrono::DateTime<Utc>),
  Scheduled(chrono::DateTime<Utc>),
  Wait(chrono::DateTime<Utc>),
  Depends(uuid::Uuid)
}

#[instrument(skip(args, now))]
fn parse_desc_and_mods(
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<(String, Vec<Mod>)>
{
  let mut desc_parts = Vec::new();
  let mut mods = Vec::new();

  let mut literal = false;
  for arg in args {
    if arg == "--" {
      literal = true;
      continue;
    }

    if !literal
      && let Some(one_mod) =
        parse_one_mod(arg, now)?
    {
      mods.push(one_mod);
      continue;
    }

    desc_parts.push(arg.clone());
  }

  if desc_parts.is_empty() {
    return Err(anyhow!(
      "add/log: description is \
       required"
    ));
  }

  Ok((desc_parts.join(" "), mods))
}

#[instrument(skip(args, now))]
fn parse_mods(
  args: &[String],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<Vec<Mod>> {
  let mut mods = Vec::new();
  for arg in args {
    if let Some(one_mod) =
      parse_one_mod(arg, now)?
    {
      mods.push(one_mod);
    } else {
      warn!(arg = %arg, "unrecognized modifier token ignored");
    }
  }
  Ok(mods)
}

fn parse_one_mod(
  tok: &str,
  now: chrono::DateTime<Utc>
) -> anyhow::Result<Option<Mod>> {
  if let Some(tag) =
    tok.strip_prefix('+')
  {
    return Ok(Some(Mod::TagAdd(
      tag.to_string()
    )));
  }
  if let Some(tag) =
    tok.strip_prefix('-')
  {
    return Ok(Some(Mod::TagRemove(
      tag.to_string()
    )));
  }

  let (key, value) =
    if let Some((k, v)) =
      tok.split_once(':')
    {
      (k, v)
    } else if let Some((k, v)) =
      tok.split_once('=')
    {
      (k, v)
    } else {
      return Ok(None);
    };

  let key = key.to_ascii_lowercase();

  match key.as_str() {
    | "project" => {
      Ok(Some(Mod::Project(
        value.to_string()
      )))
    }
    | "pri" | "priority" => {
      Ok(Some(Mod::Priority(
        value.to_string()
      )))
    }
    | "due" => {
      Ok(Some(Mod::Due(
        parse_date_expr(value, now)?
      )))
    }
    | "scheduled" => {
      Ok(Some(Mod::Scheduled(
        parse_date_expr(value, now)?
      )))
    }
    | "wait" => {
      Ok(Some(Mod::Wait(
        parse_date_expr(value, now)?
      )))
    }
    | "depends" => {
      let uuid =
        uuid::Uuid::parse_str(value)?;
      Ok(Some(Mod::Depends(uuid)))
    }
    | _ => Ok(None)
  }
}

fn apply_mods(
  task: &mut Task,
  mods: &[Mod],
  now: chrono::DateTime<Utc>
) -> anyhow::Result<()> {
  for one_mod in mods {
    match one_mod {
      | Mod::TagAdd(tag) => {
        if task.tags.iter().all(
          |existing| existing != tag
        ) {
          task.tags.push(tag.clone());
        }
      }
      | Mod::TagRemove(tag) => {
        task.tags.retain(|existing| {
          existing != tag
        });
      }
      | Mod::Project(project) => {
        task.project =
          Some(project.clone());
      }
      | Mod::Priority(priority) => {
        task.priority =
          Some(priority.clone());
      }
      | Mod::Due(dt) => {
        task.due = Some(*dt);
      }
      | Mod::Scheduled(dt) => {
        task.scheduled = Some(*dt);
      }
      | Mod::Wait(dt) => {
        task.wait = Some(*dt);
        if *dt <= now
          && task.status
            == Status::Waiting
        {
          task.status = Status::Pending;
        }
      }
      | Mod::Depends(dep) => {
        if task.depends.iter().all(
          |existing| existing != dep
        ) {
          task.depends.push(*dep);
        }
      }
    }
  }

  Ok(())
}
