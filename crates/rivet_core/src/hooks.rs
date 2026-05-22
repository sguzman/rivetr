use std::fs;
use std::io::Write;
use std::path::{
  Path,
  PathBuf
};
use std::process::{
  Command,
  Stdio
};

use anyhow::{
  Context,
  anyhow
};
use tracing::{
  debug,
  info,
  instrument,
  warn
};

use crate::config::Config;
use crate::task::Task;

#[derive(Debug, Clone)]
pub struct HookRunner {
  enabled:   bool,
  hooks_dir: PathBuf
}

impl HookRunner {
  pub fn new(
    cfg: &Config,
    data_dir: &Path
  ) -> Self {
    let enabled = cfg
      .get_bool("hooks")
      .unwrap_or(true);
    let hooks_dir =
      data_dir.join("hooks");
    debug!(
        enabled,
        hooks_dir = %hooks_dir.display(),
        "initialized hook runner"
    );
    Self {
      enabled,
      hooks_dir
    }
  }

  #[instrument(skip(self))]
  pub fn run_on_launch(
    &self
  ) -> anyhow::Result<()> {
    if !self.enabled {
      debug!(
        "hooks disabled; skipping \
         on-launch"
      );
      return Ok(());
    }
    let scripts =
      self.list_scripts("on-launch")?;
    debug!(
      count = scripts.len(),
      "running on-launch hooks"
    );
    for script in scripts {
      run_hook_noio(&script)?;
    }
    Ok(())
  }

  #[instrument(skip(self, task))]
  pub fn apply_on_add(
    &self,
    task: &Task
  ) -> anyhow::Result<Task> {
    if !self.enabled {
      debug!(
        "hooks disabled; skipping \
         on-add"
      );
      return Ok(task.clone());
    }

    let mut current = task.clone();
    let scripts =
      self.list_scripts("on-add")?;
    debug!(
      count = scripts.len(),
      "running on-add hooks"
    );
    for script in scripts {
      let payload =
        serialize_task_for_hook(
          &current
        );
      let response =
        run_hook_with_json_lines(
          &script,
          &[payload],
          1
        )?;
      let mut updated: Task =
        serde_json::from_str(
          &response[0]
        )
        .with_context(
          || {
            format!(
              "hook {} emitted \
               invalid task json",
              script.display()
            )
          }
        )?;
      if updated.id.is_none() {
        updated.id = current.id;
      }
      current = updated;
    }
    Ok(current)
  }

  #[instrument(skip(self, old, new))]
  pub fn apply_on_modify(
    &self,
    old: &Task,
    new: &Task
  ) -> anyhow::Result<Task> {
    if !self.enabled {
      debug!(
        "hooks disabled; skipping \
         on-modify"
      );
      return Ok(new.clone());
    }

    let mut current = new.clone();
    let scripts =
      self.list_scripts("on-modify")?;
    debug!(
      count = scripts.len(),
      "running on-modify hooks"
    );
    for script in scripts {
      let old_payload =
        serialize_task_for_hook(old);
      let new_payload =
        serialize_task_for_hook(
          &current
        );
      let response =
        run_hook_with_json_lines(
          &script,
          &[old_payload, new_payload],
          1
        )?;
      let mut updated: Task =
        serde_json::from_str(
          &response[0]
        )
        .with_context(
          || {
            format!(
              "hook {} emitted \
               invalid task json",
              script.display()
            )
          }
        )?;
      if updated.id.is_none() {
        updated.id = current.id;
      }
      current = updated;
    }
    Ok(current)
  }

  #[instrument(skip(self))]
  fn list_scripts(
    &self,
    event: &str
  ) -> anyhow::Result<Vec<PathBuf>> {
    if !self.hooks_dir.exists() {
      return Ok(Vec::new());
    }

    let mut scripts = Vec::new();
    for entry in
      fs::read_dir(&self.hooks_dir)
        .with_context(|| {
          format!(
            "failed to read hooks dir \
             {}",
            self.hooks_dir.display()
          )
        })?
    {
      let entry = entry?;
      let path = entry.path();
      if !path.is_file() {
        continue;
      }

      let Some(name) = path
        .file_name()
        .and_then(|name| name.to_str())
      else {
        continue;
      };
      if !name.starts_with(&format!(
        "{event}."
      )) {
        continue;
      }

      if !is_executable(&path)? {
        debug!(path = %path.display(), "skipping non-executable hook");
        continue;
      }

      debug!(event, path = %path.display(), "selected hook script");
      scripts.push(path);
    }

    scripts.sort();
    Ok(scripts)
  }
}

fn serialize_task_for_hook(
  task: &Task
) -> String {
  let mut task_for_hook = task.clone();
  task_for_hook.id = None;
  serde_json::to_string(&task_for_hook)
    .unwrap_or_default()
}

fn run_hook_noio(
  path: &Path
) -> anyhow::Result<()> {
  info!(hook = %path.display(), "running hook");
  let output = Command::new(path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .with_context(|| {
      format!(
        "failed to run hook {}",
        path.display()
      )
    })?;

  if !output.status.success() {
    return Err(anyhow!(
      "Hook Error: script {} failed \
       with status {}",
      path.display(),
      output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| {
          "unknown".to_string()
        })
    ));
  }

  let stderr = String::from_utf8_lossy(
    &output.stderr
  )
  .trim()
  .to_string();
  if !stderr.is_empty() {
    warn!(hook = %path.display(), stderr = %stderr, "hook wrote stderr");
  }

  Ok(())
}

fn run_hook_with_json_lines(
  path: &Path,
  input_lines: &[String],
  expected_output_lines: usize
) -> anyhow::Result<Vec<String>> {
  info!(hook = %path.display(), "running hook");
  let mut child = Command::new(path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .with_context(|| {
      format!(
        "failed to run hook {}",
        path.display()
      )
    })?;

  if let Some(mut stdin) =
    child.stdin.take()
  {
    for line in input_lines {
      writeln!(stdin, "{line}")?;
    }
  }

  let output = child
    .wait_with_output()
    .with_context(|| {
      format!(
        "failed to wait for hook {}",
        path.display()
      )
    })?;

  if !output.status.success() {
    let stderr =
      String::from_utf8_lossy(
        &output.stderr
      )
      .trim()
      .to_string();
    if !stderr.is_empty() {
      warn!(hook = %path.display(), stderr = %stderr, "hook failed");
    }
    return Err(anyhow!(
      "Hook Error: Expected feedback \
       from failing hook script: {}",
      path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
    ));
  }

  let stdout = String::from_utf8_lossy(
    &output.stdout
  );
  let lines: Vec<String> = stdout
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .map(ToString::to_string)
    .collect();

  if lines.len()
    != expected_output_lines
  {
    return Err(anyhow!(
      "Hook Error: Expected \
       {expected_output_lines} JSON \
       task(s), found {}, in hook \
       script: {}",
      lines.len(),
      path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
    ));
  }

  let stderr = String::from_utf8_lossy(
    &output.stderr
  )
  .trim()
  .to_string();
  if !stderr.is_empty() {
    warn!(hook = %path.display(), stderr = %stderr, "hook wrote stderr");
  }

  Ok(lines)
}

#[cfg(unix)]
fn is_executable(
  path: &Path
) -> anyhow::Result<bool> {
  use std::os::unix::fs::PermissionsExt;

  let mode = fs::metadata(path)?
    .permissions()
    .mode();
  Ok(mode & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(
  path: &Path
) -> anyhow::Result<bool> {
  Ok(path.is_file())
}
