use clap::{Args, Parser, Subcommand};
use fountain_eval::{batch, catalog, config::Configuration, evidence, runner, util::Result};
use serde_json::json;
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
#[derive(Parser)]
#[command(
    name = "fountain-eval",
    version,
    about = "Configuration-driven Fountain evaluation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Args)]
struct Input {
    #[arg(long, default_value = "fountain-eval.toml")]
    config: PathBuf,
}
#[derive(Subcommand)]
enum Command {
    /// Offline configuration and local declaration checks.
    Inspect(Input),
    /// List the full test content and interface registrations.
    List,
    /// Freeze, adapt, build and check capabilities from a configuration.
    Prepare(Input),
    /// Execute every applicable check; prepared execution consumes its frozen configuration.
    Run {
        #[arg(long, conflicts_with = "prepared")]
        config: Option<PathBuf>,
        #[arg(long)]
        prepared: Option<PathBuf>,
    },
    /// Read a sealed result including warnings and complete coverage.
    Report {
        run: PathBuf,
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
    },
    /// Compare common measured cases and report coverage differences.
    Compare {
        left: PathBuf,
        right: PathBuf,
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
    },
}
#[derive(Clone, clap::ValueEnum)]
enum Format {
    Json,
    Markdown,
    Html,
}
fn emit_text(text: &str) -> Result<()> {
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())
}
fn emit(value: &impl serde::Serialize) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value).map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())
}
fn outcome(directory: PathBuf, record: batch::Record) -> Result<u8> {
    let code = batch::exit_code(&record);
    emit(&json!({"run_dir":directory,"batch":record}))?;
    Ok(code)
}
fn execute(command: Command) -> Result<u8> {
    match command {
        Command::List => {
            emit(&catalog::list())?;
            Ok(0)
        }
        Command::Inspect(input) => {
            let configuration = Configuration::load(&input.config)?;
            let reports: Vec<_> = configuration
                .selections
                .iter()
                .map(|s| (s.id, runner::inspect(s.request.clone())))
                .collect();
            let failed = reports.iter().any(|(_, r)| r.has_errors());
            emit(
                &json!({"configuration":configuration,"inspections":reports.iter().map(|(id,r)|json!({"component_id":id,"inspection":r})).collect::<Vec<_>>()}),
            )?;
            Ok(if failed { 2 } else { 0 })
        }
        Command::Prepare(input) => {
            let (path, record) = batch::run(Configuration::load(&input.config)?, true)?;
            outcome(path, record)
        }
        Command::Run { config, prepared } => {
            let (path, record) = if let Some(path) = prepared {
                batch::from_prepared(&path)?
            } else {
                batch::run(
                    Configuration::load(&config.unwrap_or_else(|| "fountain-eval.toml".into()))?,
                    false,
                )?
            };
            outcome(path, record)
        }
        Command::Report { run, format } => {
            if run.join("batch.json").exists() {
                let r = batch::load(&run)?;
                match format {
                    Format::Json => emit(&r)?,
                    Format::Markdown => emit_text(&fountain_eval::report::batch_markdown(&r))?,
                    Format::Html => emit_text(&fountain_eval::report::batch_html(&r))?,
                }
            } else {
                let r = evidence::load(&run)?;
                match format {
                    Format::Json => emit(&r)?,
                    Format::Markdown => emit_text(&fountain_eval::report::markdown(&r))?,
                    Format::Html => emit_text(&fountain_eval::report::html(&r))?,
                };
            }
            Ok(0)
        }
        Command::Compare {
            left,
            right,
            format,
        } => {
            if left.join("batch.json").exists() || right.join("batch.json").exists() {
                return Err("compare requires two per-C result directories; choose the detailed result paths listed in each batch report".into());
            }
            let r = evidence::compare(&evidence::load(&left)?, &evidence::load(&right)?)?;
            match format {
                Format::Json => emit(&r)?,
                Format::Markdown => emit_text(&fountain_eval::report::comparison(&r))?,
                Format::Html => emit_text(&fountain_eval::report::comparison_html(&r))?,
            };
            Ok(0)
        }
    }
}
fn main() -> ExitCode {
    match execute(Cli::parse().command) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
