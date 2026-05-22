use std::io::{
  self,
  IsTerminal,
  Write
};

use anyhow::anyhow;
use chrono::{
  DateTime,
  Utc
};
use unicode_width::UnicodeWidthStr;

use crate::config::Config;
use crate::datetime::format_project_date;
use crate::task::Task;

#[derive(Debug, Clone)]
pub struct Renderer {
  color: bool
}

impl Renderer {
  pub fn new(
    cfg: &Config
  ) -> anyhow::Result<Self> {
    let color_cfg =
      cfg.get("color").unwrap_or_else(
        || "on".to_string()
      );
    let color = match color_cfg
      .to_ascii_lowercase()
      .as_str()
    {
      | "on" | "yes" | "true" | "1" => {
        true
      }
      | "off" | "no" | "false"
      | "0" => false,
      | other => {
        return Err(anyhow!(
          "invalid color setting: \
           {other}"
        ))
      }
    };

    Ok(Self {
      color
    })
  }

  #[tracing::instrument(skip(
    self, tasks, now
  ))]
  pub fn print_task_table(
    &mut self,
    tasks: &[Task],
    now: DateTime<Utc>
  ) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();

    let headers = vec![
      "ID".to_string(),
      "Due".to_string(),
      "Project".to_string(),
      "Description".to_string(),
      "Tags".to_string(),
    ];

    let mut rows =
      Vec::with_capacity(tasks.len());

    for task in tasks {
      let id = task
        .id
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
          "-".to_string()
        });

      let due = task
        .due
        .map(format_project_date)
        .unwrap_or_default();

      let due = if let Some(task_due) =
        task.due
      {
        if task_due < now {
          self.paint(&due, "31")
        } else {
          due
        }
      } else {
        due
      };

      let id = self.paint(&id, "33");
      let project = task
        .project
        .clone()
        .unwrap_or_default();
      let tags = task
        .tags
        .iter()
        .map(|tag| format!("+{tag}"))
        .collect::<Vec<_>>()
        .join(" ");

      rows.push(vec![
        id,
        due,
        project,
        task.description.clone(),
        tags,
      ]);
    }

    write_table(
      &mut out, &headers, &rows
    )?;
    Ok(())
  }

  #[tracing::instrument(skip(
    self, headers, rows
  ))]
  pub fn print_report_table(
    &mut self,
    headers: &[String],
    rows: &[Vec<String>]
  ) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    write_table(
      &mut out, headers, rows
    )?;
    Ok(())
  }

  #[tracing::instrument(skip(
    self, task
  ))]
  pub fn print_task_info(
    &mut self,
    task: &Task
  ) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();

    writeln!(
      out,
      "id        {}",
      task
        .id
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
          "-".to_string()
        })
    )?;
    writeln!(
      out,
      "uuid      {}",
      task.uuid
    )?;
    writeln!(
      out,
      "status    {:?}",
      task.status
    )?;
    writeln!(
      out,
      "desc      {}",
      task.description
    )?;
    writeln!(
      out,
      "project   {}",
      task
        .project
        .clone()
        .unwrap_or_default()
    )?;
    writeln!(
      out,
      "priority  {}",
      task
        .priority
        .clone()
        .unwrap_or_default()
    )?;
    writeln!(
      out,
      "tags      {}",
      task.tags.join(", ")
    )?;
    writeln!(
      out,
      "entry     {}",
      task
        .entry
        .format("%Y%m%dT%H%M%SZ")
    )?;
    writeln!(
      out,
      "modified  {}",
      task
        .modified
        .format("%Y%m%dT%H%M%SZ")
    )?;

    if let Some(end) = task.end {
      writeln!(
        out,
        "end       {}",
        end.format("%Y%m%dT%H%M%SZ")
      )?;
    }
    if let Some(start) = task.start {
      writeln!(
        out,
        "start     {}",
        start.format("%Y%m%dT%H%M%SZ")
      )?;
    }
    if let Some(due) = task.due {
      writeln!(
        out,
        "due       {}",
        due.format("%Y%m%dT%H%M%SZ")
      )?;
    }
    if let Some(scheduled) =
      task.scheduled
    {
      writeln!(
        out,
        "scheduled {}",
        scheduled
          .format("%Y%m%dT%H%M%SZ")
      )?;
    }
    if let Some(wait) = task.wait {
      writeln!(
        out,
        "wait      {}",
        wait.format("%Y%m%dT%H%M%SZ")
      )?;
    }

    Ok(())
  }

  fn paint(
    &self,
    text: &str,
    code: &str
  ) -> String {
    if !self.color
      || !io::stdout().is_terminal()
    {
      return text.to_string();
    }
    format!("\x1b[{code}m{text}\x1b[0m")
  }
}

fn write_table<W: Write>(
  mut writer: W,
  headers: &[String],
  rows: &[Vec<String>]
) -> anyhow::Result<()> {
  let column_count = headers.len();
  let mut widths =
    vec![0usize; column_count];

  for (idx, header) in
    headers.iter().enumerate()
  {
    widths[idx] = widths[idx].max(
      UnicodeWidthStr::width(
        header.as_str()
      )
    );
  }

  for row in rows {
    for (idx, cell) in
      row.iter().enumerate()
    {
      widths[idx] = widths[idx].max(
        UnicodeWidthStr::width(
          strip_ansi(cell).as_str()
        )
      );
    }
  }

  for idx in 0..column_count {
    write!(
      writer,
      "{:width$} ",
      headers[idx],
      width = widths[idx]
    )?;
  }
  writeln!(writer)?;

  for idx in 0..column_count {
    write!(
      writer,
      "{:-<width$} ",
      "",
      width = widths[idx]
    )?;
  }
  writeln!(writer)?;

  for row in rows {
    for idx in 0..column_count {
      let cell = &row[idx];
      let visible_width =
        UnicodeWidthStr::width(
          strip_ansi(cell).as_str()
        );
      let padding = widths[idx]
        .saturating_sub(visible_width);
      write!(
        writer,
        "{}{} ",
        cell,
        " ".repeat(padding)
      )?;
    }
    writeln!(writer)?;
  }

  Ok(())
}

fn strip_ansi(s: &str) -> String {
  let mut out =
    String::with_capacity(s.len());
  let mut escaped = false;

  for ch in s.chars() {
    if escaped {
      if ch == 'm' {
        escaped = false;
      }
      continue;
    }

    if ch == '\x1b' {
      escaped = true;
      continue;
    }

    out.push(ch);
  }

  out
}
