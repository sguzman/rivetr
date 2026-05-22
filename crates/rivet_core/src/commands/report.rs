#[derive(Debug, Clone, Copy)]
enum ReportColumn {
  Id,
  Uuid,
  Status,
  Project,
  Tags,
  Priority,
  Due,
  Scheduled,
  Wait,
  Entry,
  Modified,
  End,
  Start,
  Description,
  Urgency
}

impl ReportColumn {
  fn parse(
    token: &str
  ) -> Option<Self> {
    match token
      .to_ascii_lowercase()
      .as_str()
    {
      | "id" => Some(Self::Id),
      | "uuid" => Some(Self::Uuid),
      | "status" => Some(Self::Status),
      | "project" => {
        Some(Self::Project)
      }
      | "tags" | "tag" => {
        Some(Self::Tags)
      }
      | "priority" | "pri" => {
        Some(Self::Priority)
      }
      | "due" => Some(Self::Due),
      | "scheduled" => {
        Some(Self::Scheduled)
      }
      | "wait" => Some(Self::Wait),
      | "entry" => Some(Self::Entry),
      | "modified" => {
        Some(Self::Modified)
      }
      | "end" => Some(Self::End),
      | "start" => Some(Self::Start),
      | "description" | "desc" => {
        Some(Self::Description)
      }
      | "urgency" => {
        Some(Self::Urgency)
      }
      | _ => None
    }
  }

  fn default_label(
    &self
  ) -> &'static str {
    match self {
      | Self::Id => "ID",
      | Self::Uuid => "UUID",
      | Self::Status => "Status",
      | Self::Project => "Project",
      | Self::Tags => "Tags",
      | Self::Priority => "Pri",
      | Self::Due => "Due",
      | Self::Scheduled => "Scheduled",
      | Self::Wait => "Wait",
      | Self::Entry => "Entry",
      | Self::Modified => "Modified",
      | Self::End => "End",
      | Self::Start => "Start",
      | Self::Description => {
        "Description"
      }
      | Self::Urgency => "Urgency"
    }
  }
}

#[derive(Debug, Clone, Copy)]
struct SortSpec {
  column:     ReportColumn,
  descending: bool
}

#[derive(Debug, Clone)]
struct ReportSpec {
  name:         String,
  columns:      Vec<ReportColumn>,
  labels:       Vec<String>,
  sort:         Vec<SortSpec>,
  filter_terms: Vec<String>,
  limit:        Option<usize>
}

fn is_report_command(
  cfg: &Config,
  command: &str
) -> bool {
  cfg
    .get(&format!(
      "report.{command}.columns"
    ))
    .is_some()
}

fn load_report_spec(
  cfg: &Config,
  report_name: &str
) -> Option<ReportSpec> {
  let columns_raw =
    cfg.get(&format!(
      "report.{report_name}.columns"
    ))?;
  let columns: Vec<ReportColumn> =
    parse_config_list(&columns_raw)
      .into_iter()
      .filter_map(|token| {
        ReportColumn::parse(&token)
      })
      .collect();
  if columns.is_empty() {
    return None;
  }

  let labels_key = format!(
    "report.{report_name}.labels"
  );
  let mut labels = cfg
    .get(&labels_key)
    .map(|raw| parse_config_list(&raw))
    .unwrap_or_default();
  while labels.len() < columns.len() {
    labels.push(
      columns[labels.len()]
        .default_label()
        .to_string()
    );
  }
  labels.truncate(columns.len());

  let sort = parse_sort_specs(cfg.get(
    &format!(
      "report.{report_name}.sort"
    )
  ));
  let filter_terms = cfg
    .get(&format!(
      "report.{report_name}.filter"
    ))
    .map(|raw| {
      raw
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
    })
    .unwrap_or_default();
  let limit = cfg
    .get(&format!(
      "report.{report_name}.limit"
    ))
    .and_then(|raw| {
      raw.parse::<usize>().ok()
    })
    .filter(|value| *value > 0);

  Some(ReportSpec {
    name: report_name.to_string(),
    columns,
    labels,
    sort,
    filter_terms,
    limit
  })
}

fn parse_config_list(
  raw: &str
) -> Vec<String> {
  raw
    .split(',')
    .flat_map(str::split_whitespace)
    .map(str::trim)
    .filter(|token| !token.is_empty())
    .map(ToString::to_string)
    .collect()
}

fn parse_sort_specs(
  raw: Option<String>
) -> Vec<SortSpec> {
  let Some(raw) = raw else {
    return Vec::new();
  };

  parse_config_list(&raw)
    .into_iter()
    .filter_map(|token| {
      let (field, descending) =
        if let Some(field) =
          token.strip_suffix('-')
        {
          (field, true)
        } else if let Some(field) =
          token.strip_suffix('+')
        {
          (field, false)
        } else {
          (token.as_str(), false)
        };
      let column =
        ReportColumn::parse(field)?;
      Some(SortSpec {
        column,
        descending
      })
    })
    .collect()
}

fn compare_tasks_for_report(
  a: &Task,
  b: &Task,
  sort_specs: &[SortSpec],
  now: chrono::DateTime<Utc>
) -> Ordering {
  for sort_spec in sort_specs {
    let ordering =
      compare_tasks_on_column(
        a,
        b,
        sort_spec.column,
        now
      );
    if ordering != Ordering::Equal {
      return if sort_spec.descending {
        ordering.reverse()
      } else {
        ordering
      };
    }
  }

  a.id
    .unwrap_or(u64::MAX)
    .cmp(&b.id.unwrap_or(u64::MAX))
    .then_with(|| a.uuid.cmp(&b.uuid))
}

fn compare_tasks_on_column(
  a: &Task,
  b: &Task,
  column: ReportColumn,
  now: chrono::DateTime<Utc>
) -> Ordering {
  match column {
    | ReportColumn::Id => {
      cmp_optional(
        a.id.as_ref(),
        b.id.as_ref()
      )
    }
    | ReportColumn::Uuid => {
      a.uuid.cmp(&b.uuid)
    }
    | ReportColumn::Status => {
      display_status(a, now)
        .cmp(display_status(b, now))
    }
    | ReportColumn::Project => {
      cmp_optional(
        a.project.as_ref(),
        b.project.as_ref()
      )
    }
    | ReportColumn::Tags => {
      a.tags
        .join(" ")
        .cmp(&b.tags.join(" "))
    }
    | ReportColumn::Priority => {
      cmp_optional(
        a.priority.as_ref(),
        b.priority.as_ref()
      )
    }
    | ReportColumn::Due => {
      cmp_optional(
        a.due.as_ref(),
        b.due.as_ref()
      )
    }
    | ReportColumn::Scheduled => {
      cmp_optional(
        a.scheduled.as_ref(),
        b.scheduled.as_ref()
      )
    }
    | ReportColumn::Wait => {
      cmp_optional(
        a.wait.as_ref(),
        b.wait.as_ref()
      )
    }
    | ReportColumn::Entry => {
      a.entry.cmp(&b.entry)
    }
    | ReportColumn::Modified => {
      a.modified.cmp(&b.modified)
    }
    | ReportColumn::End => {
      cmp_optional(
        a.end.as_ref(),
        b.end.as_ref()
      )
    }
    | ReportColumn::Start => {
      cmp_optional(
        a.start.as_ref(),
        b.start.as_ref()
      )
    }
    | ReportColumn::Description => {
      a.description
        .to_ascii_lowercase()
        .cmp(
          &b.description
            .to_ascii_lowercase()
        )
    }
    | ReportColumn::Urgency => {
      task_urgency(a, now)
        .partial_cmp(&task_urgency(
          b, now
        ))
        .unwrap_or(Ordering::Equal)
    }
  }
}

fn cmp_optional<T: Ord>(
  left: Option<&T>,
  right: Option<&T>
) -> Ordering {
  match (left, right) {
    | (Some(a), Some(b)) => a.cmp(b),
    | (Some(_), None) => Ordering::Less,
    | (None, Some(_)) => {
      Ordering::Greater
    }
    | (None, None) => Ordering::Equal
  }
}

fn format_report_cell(
  task: &Task,
  column: ReportColumn,
  now: chrono::DateTime<Utc>
) -> String {
  match column {
    | ReportColumn::Id => {
      task
        .id
        .map(|id| id.to_string())
        .unwrap_or_else(|| {
          "-".to_string()
        })
    }
    | ReportColumn::Uuid => {
      task.uuid.to_string()
    }
    | ReportColumn::Status => {
      display_status(task, now)
        .to_string()
    }
    | ReportColumn::Project => {
      task
        .project
        .clone()
        .unwrap_or_default()
    }
    | ReportColumn::Tags => {
      task
        .tags
        .iter()
        .map(|tag| format!("+{tag}"))
        .collect::<Vec<_>>()
        .join(" ")
    }
    | ReportColumn::Priority => {
      task
        .priority
        .clone()
        .unwrap_or_default()
    }
    | ReportColumn::Due => {
      format_report_date(task.due)
    }
    | ReportColumn::Scheduled => {
      format_report_date(task.scheduled)
    }
    | ReportColumn::Wait => {
      format_report_date(task.wait)
    }
    | ReportColumn::Entry => {
      format_project_date(task.entry)
    }
    | ReportColumn::Modified => {
      format_project_date(task.modified)
    }
    | ReportColumn::End => {
      format_report_date(task.end)
    }
    | ReportColumn::Start => {
      format_report_date(task.start)
    }
    | ReportColumn::Description => {
      task.description.clone()
    }
    | ReportColumn::Urgency => {
      format!(
        "{:.3}",
        task_urgency(task, now)
      )
    }
  }
}

fn format_report_date(
  date: Option<chrono::DateTime<Utc>>
) -> String {
  date
    .map(format_project_date)
    .unwrap_or_default()
}

fn display_status(
  task: &Task,
  now: chrono::DateTime<Utc>
) -> &'static str {
  if task.status == Status::Pending
    && task.is_waiting(now)
  {
    "waiting"
  } else {
    match task.status {
      | Status::Pending => "pending",
      | Status::Completed => {
        "completed"
      }
      | Status::Deleted => "deleted",
      | Status::Waiting => "waiting"
    }
  }
}

fn task_urgency(
  task: &Task,
  now: chrono::DateTime<Utc>
) -> f64 {
  if matches!(
    task.status,
    Status::Completed | Status::Deleted
  ) {
    return 0.0;
  }

  let mut urgency = 0.0;

  urgency +=
    task.tags.len() as f64 * 0.8;

  if let Some(priority) =
    task.priority.as_deref()
  {
    urgency += match priority
      .to_ascii_uppercase()
      .as_str()
    {
      | "H" => 6.0,
      | "M" => 3.9,
      | "L" => 1.8,
      | _ => 0.0
    };
  }

  if task.start.is_some()
    && !task.is_waiting(now)
  {
    urgency += 4.0;
  }
  if task.is_waiting(now) {
    urgency -= 3.0;
  }
  if !task.depends.is_empty() {
    urgency -= 5.0;
  }

  if let Some(due) = task.due {
    let delta = due - now;
    let days = delta.num_minutes()
      as f64
      / (60.0 * 24.0);
    urgency += if days <= -1.0 {
      9.7
    } else if days <= 0.0 {
      9.3
    } else if days <= 1.0 {
      8.8
    } else if days <= 2.0 {
      8.4
    } else if days <= 7.0 {
      6.0
    } else {
      3.0
    };
  }

  urgency
}

