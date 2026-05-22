pub mod cli;
pub mod commands;
pub mod config;
pub mod datastore;
pub mod datetime;
pub mod filter;
pub mod hooks;
pub mod render;
pub mod task;

use std::ffi::OsString;

use anyhow::Context;
use clap::Parser;
use tracing::{
  debug,
  info
};

#[tracing::instrument(skip_all)]
pub fn run(
  raw_args: Vec<OsString>
) -> anyhow::Result<()> {
  let pre =
    cli::preprocess_args(&raw_args)?;
  let cli = cli::GlobalCli::parse_from(
    pre.cleaned_args
  );

  cli::init_tracing(
    cli.verbose,
    cli.quiet
  )?;

  info!(
    verbose = cli.verbose,
    quiet = cli.quiet,
    "starting rivet CLI"
  );
  debug!(?pre.rc_overrides, "preprocessed rc overrides");

  let mut cfg = config::Config::load(
    cli.taskrc.as_deref()
  )?;
  cfg.apply_overrides(
    pre.rc_overrides.into_iter().chain(
      cli
        .rc_overrides
        .into_iter()
        .map(|kv| (kv.key, kv.value))
    )
  );

  let data_dir =
    config::resolve_data_dir(
      &cfg,
      cli.data.as_deref()
    )
    .context(
      "failed to resolve data \
       directory"
    )?;

  let mut store =
    datastore::DataStore::open(
      &data_dir
    )
    .with_context(|| {
      format!(
        "failed to open datastore at \
         {}",
        data_dir.display()
      )
    })?;

  let mut renderer =
    render::Renderer::new(&cfg)?;
  let inv = cli::Invocation::parse(
    &cfg, cli.rest
  )?;

  commands::dispatch(
    &mut store,
    &cfg,
    &mut renderer,
    inv
  )?;

  info!("done");
  Ok(())
}
