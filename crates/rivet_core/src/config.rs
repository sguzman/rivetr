use std::collections::HashMap;
use std::fs;
use std::path::{
  Path,
  PathBuf
};

use anyhow::{
  Context,
  anyhow
};
use tracing::{
  debug,
  info,
  trace,
  warn
};

#[derive(Debug, Clone)]
pub struct Config {
  map: HashMap<String, String>,
  pub loaded_files: Vec<PathBuf>
}

impl Config {
  #[tracing::instrument(skip(
    taskrc_override
  ))]
  pub fn load(
    taskrc_override: Option<&Path>
  ) -> anyhow::Result<Self> {
    let mut cfg = Config {
      map:          HashMap::new(),
      loaded_files: vec![]
    };

    cfg.map.insert(
      "data.location".to_string(),
      "~/.task".to_string()
    );
    cfg.map.insert(
      "default.command".to_string(),
      "next".to_string()
    );
    cfg.map.insert(
      "color".to_string(),
      "on".to_string()
    );

    let taskrc = resolve_taskrc_path(
      taskrc_override
    )?;
    if let Some(path) = taskrc {
      info!(taskrc = %path.display(), "loading taskrc");
      cfg.load_file(&path)?;
    } else {
      warn!(
        "no taskrc found; using \
         defaults"
      );
    }

    Ok(cfg)
  }

  #[tracing::instrument(skip(
    self, overrides
  ))]
  pub fn apply_overrides<I>(
    &mut self,
    overrides: I
  ) where
    I: IntoIterator<
      Item = (String, String)
    >
  {
    for (k, v) in overrides {
      let key = k
        .strip_prefix("rc.")
        .unwrap_or(&k)
        .to_string();
      debug!(key = %key, value = %v, "applying override");
      self.map.insert(key, v);
    }
  }

  pub fn get(
    &self,
    key: &str
  ) -> Option<String> {
    self.map.get(key).cloned()
  }

  pub fn get_bool(
    &self,
    key: &str
  ) -> Option<bool> {
    self
      .map
      .get(key)
      .map(|v| parse_bool(v))
  }

  pub fn iter(
    &self
  ) -> impl Iterator<Item = (&String, &String)>
  {
    self.map.iter()
  }

  #[tracing::instrument(skip(self))]
  fn load_file(
    &mut self,
    path: &Path
  ) -> anyhow::Result<()> {
    let path = expand_tilde(path);
    let text =
      fs::read_to_string(&path)
        .with_context(|| {
          format!(
            "failed to read {}",
            path.display()
          )
        })?;

    self
      .loaded_files
      .push(path.clone());

    let base_dir = path
      .parent()
      .map(|p| p.to_path_buf())
      .unwrap_or_else(|| {
        PathBuf::from(".")
      });

    for (line_num, raw_line) in
      text.lines().enumerate()
    {
      let mut line = raw_line.trim();
      if line.is_empty()
        || line.starts_with('#')
      {
        continue;
      }

      if let Some((before, _)) =
        line.split_once('#')
      {
        line = before.trim();
      }

      if line.is_empty() {
        continue;
      }

      if let Some(include_rest) =
        line.strip_prefix("include ")
      {
        let include_path =
          resolve_include_path(
            &base_dir,
            include_rest.trim()
          )?;
        debug!(
            file = %path.display(),
            include = %include_path.display(),
            line = line_num + 1,
            "processing include"
        );

        if include_path.exists() {
          self
            .load_file(&include_path)?;
        } else {
          warn!(include = %include_path.display(), "include file does not exist; skipping");
        }
        continue;
      }

      let (k, v) = line
        .split_once('=')
        .ok_or_else(|| {
          anyhow!(
            "invalid config line \
             {}:{}: {}",
            path.display(),
            line_num + 1,
            raw_line
          )
        })?;

      let key = k.trim().to_string();
      let value = v.trim().to_string();
      trace!(key = %key, value = %value, "loaded config key");
      self.map.insert(key, value);
    }

    Ok(())
  }
}

#[tracing::instrument(skip(
  cfg,
  override_dir
))]
pub fn resolve_data_dir(
  cfg: &Config,
  override_dir: Option<&Path>
) -> anyhow::Result<PathBuf> {
  let dir = if let Some(path) =
    override_dir
  {
    path.to_path_buf()
  } else if let Some(cfg_value) =
    cfg.get("data.location")
  {
    expand_tilde(Path::new(&cfg_value))
  } else {
    default_data_dir()?
  };

  if !dir.exists() {
    info!(dir = %dir.display(), "creating data directory");
    fs::create_dir_all(&dir)
      .with_context(|| {
        format!(
          "failed to create {}",
          dir.display()
        )
      })?;
  }

  Ok(dir)
}

#[tracing::instrument(skip(
  override_path
))]
fn resolve_taskrc_path(
  override_path: Option<&Path>
) -> anyhow::Result<Option<PathBuf>> {
  if let Some(path) = override_path {
    return Ok(Some(path.to_path_buf()));
  }

  if let Ok(taskrc_env) =
    std::env::var("TASKRC")
  {
    if taskrc_env == "/dev/null" {
      return Ok(None);
    }
    return Ok(Some(PathBuf::from(
      taskrc_env
    )));
  }

  let home = dirs::home_dir()
    .ok_or_else(|| {
      anyhow!(
        "cannot determine home \
         directory"
      )
    })?;
  let candidate = home.join(".taskrc");
  if candidate.exists() {
    return Ok(Some(candidate));
  }

  Ok(None)
}

fn default_data_dir()
-> anyhow::Result<PathBuf> {
  let home = dirs::home_dir()
    .ok_or_else(|| {
      anyhow!(
        "cannot determine home \
         directory"
      )
    })?;
  Ok(home.join(".task"))
}

fn resolve_include_path(
  base_dir: &Path,
  include: &str
) -> anyhow::Result<PathBuf> {
  if include.trim().is_empty() {
    return Err(anyhow!(
      "include path cannot be empty"
    ));
  }

  let raw = PathBuf::from(include);
  let expanded = expand_tilde(&raw);
  if expanded.is_absolute() {
    Ok(expanded)
  } else {
    Ok(base_dir.join(expanded))
  }
}

fn expand_tilde(
  path: &Path
) -> PathBuf {
  let text = path.to_string_lossy();
  if let Some(rest) =
    text.strip_prefix("~/")
    && let Some(home) = dirs::home_dir()
  {
    return home.join(rest);
  }
  path.to_path_buf()
}

fn parse_bool(s: &str) -> bool {
  matches!(
    s.trim()
      .to_ascii_lowercase()
      .as_str(),
    "1" | "y" | "yes" | "on" | "true"
  )
}
