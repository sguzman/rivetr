use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::anyhow;
use clap::{
  ArgAction,
  Parser
};
use tracing::{
  debug,
  warn
};
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct PreprocessedArgs {
  pub cleaned_args: Vec<OsString>,
  pub rc_overrides:
    Vec<(String, String)>
}

#[derive(Debug, Clone)]
pub struct KeyVal {
  pub key:   String,
  pub value: String
}

impl std::str::FromStr for KeyVal {
  type Err = anyhow::Error;

  fn from_str(
    s: &str
  ) -> Result<Self, Self::Err> {
    let (k, v) = s
      .split_once('=')
      .ok_or_else(|| {
        anyhow!(
          "expected KEY=VALUE, got: \
           {s}"
        )
      })?;
    Ok(Self {
      key:   k.trim().to_string(),
      value: v.trim().to_string()
    })
  }
}

#[derive(Parser, Debug, Clone)]
#[command(
  name = "task",
  version,
  about = "Rivet: Taskwarrior-compatible Rust CLI",
  disable_help_subcommand = true,
  arg_required_else_help = false
)]
pub struct GlobalCli {
  #[arg(short = 'v', long = "verbose", action = ArgAction::Count)]
  pub verbose: u8,

  #[arg(short = 'q', long = "quiet", action = ArgAction::Count)]
  pub quiet: u8,

  #[arg(
        long = "rc",
        value_parser = clap::builder::ValueParser::new(|s: &str| s.parse::<KeyVal>()),
        action = ArgAction::Append
    )]
  pub rc_overrides: Vec<KeyVal>,

  #[arg(long = "taskrc")]
  pub taskrc: Option<PathBuf>,

  #[arg(long = "data")]
  pub data: Option<PathBuf>,

  #[arg(
    trailing_var_arg = true,
    allow_hyphen_values = true
  )]
  pub rest: Vec<OsString>
}

pub fn init_tracing(
  verbose: u8,
  quiet: u8
) -> anyhow::Result<()> {
  let default_level = if quiet >= 2 {
    "error"
  } else if quiet == 1 {
    "warn"
  } else if verbose >= 3 {
    "trace"
  } else if verbose == 2 {
    "debug"
  } else if verbose == 1 {
    "info"
  } else {
    "warn"
  };

  let env_filter =
    EnvFilter::try_from_default_env()
      .or_else(|_| {
        EnvFilter::try_new(
          default_level
        )
      })
      .map_err(|e| {
        anyhow!(
          "invalid RUST_LOG / log \
           filter: {e}"
        )
      })?;

  let init_result =
    tracing_subscriber::fmt()
      .with_env_filter(env_filter)
      .with_target(true)
      .with_level(true)
      .with_thread_ids(true)
      .with_ansi(
        std::io::stderr().is_terminal()
      )
      .try_init();

  if let Err(err) = init_result {
    debug!(error = %err, "tracing subscriber already set, continuing");
  }

  Ok(())
}

#[tracing::instrument(skip_all)]
pub fn preprocess_args(
  raw: &[OsString]
) -> anyhow::Result<PreprocessedArgs> {
  let mut cleaned =
    Vec::with_capacity(raw.len());
  let mut overrides: Vec<(
    String,
    String
  )> = Vec::new();

  let mut iter = raw.iter().cloned();
  if let Some(bin) = iter.next() {
    cleaned.push(bin);
  }

  for arg in iter {
    let s = arg.to_string_lossy();
    if let Some(rest) =
      s.strip_prefix("rc.")
    {
      let parsed = if let Some((k, v)) =
        rest.split_once('=')
      {
        Some((
          format!("rc.{k}"),
          v.to_string()
        ))
      } else if let Some((k, v)) =
        rest.split_once(':')
      {
        Some((
          format!("rc.{k}"),
          v.to_string()
        ))
      } else {
        None
      };

      if let Some((k, v)) = parsed {
        debug!(key = %k, value = %v, "captured positional rc override");
        overrides.push((k, v));
        continue;
      }
    }

    cleaned.push(arg);
  }

  Ok(PreprocessedArgs {
    cleaned_args: cleaned,
    rc_overrides: overrides
  })
}

#[derive(Debug, Clone)]
pub struct Invocation {
  pub filter_terms: Vec<String>,
  pub command:      String,
  pub command_args: Vec<String>
}

impl Invocation {
  #[tracing::instrument(skip(
    cfg, rest
  ))]
  pub fn parse(
    cfg: &Config,
    rest: Vec<OsString>
  ) -> anyhow::Result<Self> {
    let tokens: Vec<String> = rest
      .into_iter()
      .map(|arg| {
        arg
          .to_string_lossy()
          .to_string()
      })
      .collect();

    if tokens.is_empty() {
      let cmd = cfg
        .get("default.command")
        .unwrap_or_else(|| {
          "next".to_string()
        });
      debug!(command = %cmd, "no explicit command, using default");
      return Ok(Self {
        filter_terms: vec![],
        command:      cmd,
        command_args: vec![]
      });
    }

    if tokens.len() == 1
      && tokens[0]
        .parse::<u64>()
        .is_ok()
    {
      debug!(token = %tokens[0], "single numeric token interpreted as task info query");
      return Ok(Self {
        filter_terms: vec![
          tokens[0].clone(),
        ],
        command:      "info"
          .to_string(),
        command_args: vec![]
      });
    }

    let (
      filter_terms,
      command,
      command_args
    ) = split_filter_command(
      cfg, &tokens
    );

    let report_commands =
      report_command_names(cfg);

    if command == "next"
            && !tokens.is_empty()
            && !tokens.iter().any(|tok| {
                crate::commands::expand_command_abbrev(tok, &crate::commands::known_command_names())
                    .is_some()
                    || expand_report_abbrev(tok, &report_commands).is_some()
            })
            && !report_commands.is_empty()
        {
            warn!("no command detected, treated all terms as filter for default 'next'");
        } else if command == "next"
            && !tokens.is_empty()
            && !tokens.iter().any(|tok| {
                crate::commands::expand_command_abbrev(tok, &crate::commands::known_command_names())
                    .is_some()
            })
        {
            warn!("no command detected, treated all terms as filter for default 'next'");
        }

    Ok(Self {
      filter_terms,
      command,
      command_args
    })
  }
}

fn split_filter_command(
  cfg: &Config,
  tokens: &[String]
) -> (Vec<String>, String, Vec<String>)
{
  let known = crate::commands::known_command_names();
  let report_commands =
    report_command_names(cfg);

  for i in 0..tokens.len() {
    let token = tokens[i].as_str();
    if let Some(full) = crate::commands::expand_command_abbrev(token, &known) {
            debug!(
                token = %token,
                expanded = %full,
                split_index = i,
                "resolved command token"
            );
            return (
                tokens[..i].to_vec(),
                full.to_string(),
                tokens[i + 1..].to_vec(),
            );
        }

    if let Some(full) =
      expand_report_abbrev(
        token,
        &report_commands
      )
    {
      debug!(
          token = %token,
          expanded = %full,
          split_index = i,
          "resolved report token"
      );
      return (
        tokens[..i].to_vec(),
        full,
        tokens[i + 1..].to_vec()
      );
    }
  }

  (
    tokens.to_vec(),
    "next".to_string(),
    vec![]
  )
}

fn report_command_names(
  cfg: &Config
) -> Vec<String> {
  let mut names = BTreeSet::new();
  for (key, _) in cfg.iter() {
    let Some(rest) =
      key.strip_prefix("report.")
    else {
      continue;
    };
    let Some((name, _)) =
      rest.split_once('.')
    else {
      continue;
    };
    if !name.is_empty() {
      names.insert(name.to_string());
    }
  }
  names.into_iter().collect()
}

fn expand_report_abbrev(
  token: &str,
  report_commands: &[String]
) -> Option<String> {
  if report_commands
    .iter()
    .any(|name| name == token)
  {
    return Some(token.to_string());
  }

  let mut matches = report_commands
    .iter()
    .filter(|name| {
      name.starts_with(token)
    })
    .cloned();
  let first = matches.next()?;
  if matches.next().is_some() {
    None
  } else {
    Some(first)
  }
}
