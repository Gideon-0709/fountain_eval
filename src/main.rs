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
    name = "feval",
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
        /// Export to `OUTPUT/<run-id>/report.md` instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Replace the existing report for this exact run after verification.
        #[arg(long, requires = "output")]
        replace: bool,
        /// Remove this raw batch after a verified durable report is published.
        #[arg(long, requires = "output")]
        clean: bool,
        /// Read the compact result index instead of Markdown.
        #[arg(long, conflicts_with = "output")]
        json: bool,
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
fn outcome(report: PathBuf, record: batch::Record, machine: bool) -> Result<u8> {
    let code = batch::exit_code(&record);
    if machine {
        emit(&json!({"report":report,"batch":record}))?;
    } else {
        emit_text(&fountain_eval::progress::summary(&record, &report))?;
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
        Command::Report {
            run,
            output,
            replace,
            clean,
            json,
        } => {
            let run = run.canonicalize().map_err(|e| e.to_string())?;
            let (body, saved, raw) = if run.is_file() || run.join("report.md").is_file() {
                let (text, saved) = fountain_eval::artifact::load(&run)?;
                (
                    fountain_eval::artifact::visible_text(&text)?.to_owned(),
                    saved,
                    false,
                )
            } else if run.join("batch.json").is_file() {
                let saved = fountain_eval::artifact::Snapshot::capture(&run)?;
                let mut body = fountain_eval::report::batch_markdown(&run, &saved.batch)?;
                fountain_eval::report::failure_logs(&mut body, &saved);
                (body, saved, true)
            } else {
                if clean || replace || json {
                    return Err(
                        "use the complete batch to publish, clean or inspect a stored result"
                            .into(),
                    );
                }
                let r = evidence::load(&run)?;
                let text = fountain_eval::report::markdown(&r);
                if let Some(root) = output {
                    let dir = fountain_eval::report::reserve(&root, &r.run_id)?;
                    let path = fountain_eval::report::publish(&dir, &text)?;
                    emit_text(&format!("{}\n", path.display()))?;
                } else {
                    emit_text(&text)?;
                }
                return Ok(0);
            };
            if json {
                emit(&saved)?;
            } else if let Some(root) = output {
                let directory = if replace {
                    root.join(&saved.batch.run_id)
                } else {
                    fountain_eval::report::reserve(&root, &saved.batch.run_id)?
                };
                let report = fountain_eval::artifact::publish(&directory, &body, &saved, replace)?;
                if clean && raw {
                    fountain_eval::artifact::cleanup(&run, &report)?;
                }
                emit_text(&format!("{}\n", report.display()))?;
            } else if raw {
                emit_text(&body)?;
            } else {
                emit_text(&fountain_eval::artifact::load(&run)?.0)?;
            }
            Ok(0)
        }
        Command::Compare { left, right } => {
            let r = evidence::compare_summaries(
                &fountain_eval::artifact::select(&left)?,
                &fountain_eval::artifact::select(&right)?,
            )?;
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
