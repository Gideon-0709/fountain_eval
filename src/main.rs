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
    Prepare {
        #[command(flatten)]
        input: Input,
        /// Emit the machine-readable result on stdout; progress remains on stderr.
        #[arg(long)]
        json: bool,
    },
    /// Execute every applicable check; prepared execution consumes its frozen configuration.
    Run {
        #[arg(long, conflicts_with = "prepared")]
        config: Option<PathBuf>,
        #[arg(long)]
        prepared: Option<PathBuf>,
        /// Emit the machine-readable result on stdout; progress remains on stderr.
        #[arg(long)]
        json: bool,
    },
    /// Read a sealed result including warnings and complete coverage.
    Report {
        run: PathBuf,
        /// Export to OUTPUT/<run-id>/report.md instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Compare common measured cases and report coverage differences.
    Compare { left: PathBuf, right: PathBuf },
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
fn outcome(directory: PathBuf, record: batch::Record, machine: bool) -> Result<u8> {
    let code = batch::exit_code(&record);
    let report = record
        .configuration
        .report_root()
        .join(&record.run_id)
        .join("report.md");
    if machine {
        emit(&json!({"run_dir":directory,"report":report,"batch":record}))?;
    } else {
        emit_text(&fountain_eval::progress::summary(&record, &directory))?;
    }
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
        Command::Prepare { input, json } => {
            fountain_eval::progress::set_enabled(true);
            let (path, record) = batch::run(Configuration::load(&input.config)?, true)?;
            outcome(path, record, json)
        }
        Command::Run {
            config,
            prepared,
            json,
        } => {
            fountain_eval::progress::set_enabled(true);
            let (path, record) = if let Some(path) = prepared {
                batch::from_prepared(&path)?
            } else {
                batch::run(
                    Configuration::load(&config.unwrap_or_else(|| "fountain-eval.toml".into()))?,
                    false,
                )?
            };
            outcome(path, record, json)
        }
        Command::Report { run, output } => {
            let run = run.canonicalize().map_err(|e| e.to_string())?;
            let (id, text) = if run.join("batch.json").exists() {
                let r = batch::load(&run)?;
                let text = fountain_eval::report::batch_markdown(&run, &r)?;
                (r.run_id, text)
            } else {
                let r = evidence::load(&run)?;
                let text = fountain_eval::report::markdown(&r);
                (r.run_id, text)
            };
            if let Some(root) = output {
                let directory = fountain_eval::report::reserve(&root, &id)?;
                let report = fountain_eval::report::publish(&directory, &text)?;
                emit_text(&format!("{}\n", report.display()))?;
            } else {
                emit_text(&text)?;
            }
            Ok(0)
        }
        Command::Compare { left, right } => {
            if left.join("batch.json").exists() || right.join("batch.json").exists() {
                return Err("compare requires two per-C evidence directories; choose the evidence paths listed in the report".into());
            }
            let r = evidence::compare(&evidence::load(&left)?, &evidence::load(&right)?)?;
            emit_text(&fountain_eval::report::comparison(&r))?;
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
