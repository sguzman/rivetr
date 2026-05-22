use std::cmp::Ordering;
use std::collections::{
  BTreeMap,
  BTreeSet
};
use std::io::{
  self,
  Read
};

use anyhow::{
  Context,
  anyhow
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use tracing::{
  debug,
  info,
  instrument,
  warn
};

use crate::cli::Invocation;
use crate::config::Config;
use crate::datastore::DataStore;
use crate::datetime::{
  format_project_date,
  parse_date_expr
};
use crate::filter::Filter;
use crate::hooks::HookRunner;
use crate::render::Renderer;
use crate::task::{
  Annotation,
  Status,
  Task
};

pub fn known_command_names()
-> Vec<&'static str> {
  vec![
    "add",
    "append",
    "prepend",
    "list",
    "next",
    "info",
    "modify",
    "start",
    "stop",
    "annotate",
    "denotate",
    "duplicate",
    "log",
    "done",
    "delete",
    "undo",
    "export",
    "import",
    "projects",
    "tags",
    "context",
    "contexts",
    "_commands",
    "_show",
    "_unique",
    "help",
    "version",
  ]
}

pub fn expand_command_abbrev<'a>(
  token: &'a str,
  known: &[&'a str]
) -> Option<&'a str> {
  if known.contains(&token) {
    return Some(token);
  }

  let mut matches =
    known.iter().copied().filter(
      |name| name.starts_with(token)
    );
  let first = matches.next()?;
  if matches.next().is_some() {
    None
  } else {
    Some(first)
  }
}

#[instrument(skip(
  store, cfg, renderer, inv
))]
pub fn dispatch(
  store: &mut DataStore,
  cfg: &Config,
  renderer: &mut Renderer,
  inv: Invocation
) -> anyhow::Result<()> {
  let now = Utc::now();
  let hooks = HookRunner::new(
    cfg,
    &store.data_dir
  );
  hooks.run_on_launch()?;
  let command = inv.command.as_str();
  let effective_filters =
    resolve_effective_filter_terms(
      store,
      cfg,
      command,
      &inv.filter_terms
    )?;

  debug!(
      command,
      filter = ?inv.filter_terms,
      args = ?inv.command_args,
      "dispatching command"
  );

  match command {
    | "add" => {
      cmd_add(
        store,
        &hooks,
        cfg,
        renderer,
        &inv.command_args,
        now
      )
    }
    | "append" => {
      cmd_append(
        store,
        &hooks,
        &effective_filters,
        &inv.command_args,
        now
      )
    }
    | "prepend" => {
      cmd_prepend(
        store,
        &hooks,
        &effective_filters,
        &inv.command_args,
        now
      )
    }
    | "list" | "next" => {
      cmd_list(
        store,
        cfg,
        renderer,
        command,
        &effective_filters,
        now
      )
    }
    | "info" => {
      cmd_info(
        store,
        renderer,
        &effective_filters,
        now
      )
    }
    | "modify" => {
      cmd_modify(
        store,
        &hooks,
        &effective_filters,
        &inv.command_args,
        now
      )
    }
    | "start" => {
      cmd_start(
        store,
        &hooks,
        &effective_filters,
        now
      )
    }
    | "stop" => {
      cmd_stop(
        store,
        &hooks,
        &effective_filters,
        now
      )
    }
    | "annotate" => {
      cmd_annotate(
        store,
        &hooks,
        &effective_filters,
        &inv.command_args,
        now
      )
    }
    | "denotate" => {
      cmd_denotate(
        store,
        &hooks,
        &effective_filters,
        &inv.command_args,
        now
      )
    }
    | "duplicate" => {
      cmd_duplicate(
        store,
        &hooks,
        &effective_filters,
        now
      )
    }
    | "log" => {
      cmd_log(
        store,
        &hooks,
        &inv.command_args,
        now
      )
    }
    | "done" => {
      cmd_done(
        store,
        &hooks,
        &effective_filters,
        now
      )
    }
    | "delete" => {
      cmd_delete(
        store,
        &hooks,
        &effective_filters,
        now
      )
    }
    | "undo" => cmd_undo(store),
    | "export" => {
      cmd_export(
        store,
        &effective_filters,
        now
      )
    }
    | "import" => {
      cmd_import(store, &hooks)
    }
    | "projects" => cmd_projects(store),
    | "tags" => cmd_tags(store),
    | "context" | "contexts" => {
      cmd_context(
        store,
        cfg,
        &inv.command_args
      )
    }
    | "_commands" => cmd_commands(),
    | "_show" => cmd_show(cfg),
    | "_unique" => {
      cmd_unique(
        store,
        &inv.command_args
      )
    }
    | "help" => cmd_help(),
    | "version" => {
      println!(
        "{}",
        env!("CARGO_PKG_VERSION")
      );
      Ok(())
    }
    | other => {
      if is_report_command(cfg, other) {
        cmd_report(
          store,
          cfg,
          renderer,
          other,
          &effective_filters,
          now
        )
      } else {
        Err(anyhow!(
          "unknown command: {other}"
        ))
      }
    }
  }
}

