use clap::{Args, Parser, Subcommand};
use fountain_eval::{catalog, config::Configuration, evidence, execution, runner, util::Result};
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
    Report { run: PathBuf },
    /// Compare common measured cases and report coverage differences.
    Compare { left: PathBuf, right: PathBuf },
}
fn emit(value: &impl serde::Serialize) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value).map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())
}
fn outcome(directory: PathBuf, record: evidence::Record) -> Result<u8> {
    emit(&json!({"run_dir":directory,"result":record}))?;
    Ok(match record.status.as_str() {
        "prepared" | "succeeded" => 0,
        "unsupported" => 3,
        "interrupted" => 130,
        _ => 4,
    })
}
fn execute(command: Command) -> Result<u8> {
    match command {
        Command::List => {
            emit(&catalog::list())?;
            Ok(0)
        }
        Command::Inspect(input) => {
            let configuration = Configuration::load(&input.config)?;
            let report = runner::inspect(configuration.request.clone());
            emit(&json!({"configuration":configuration,"inspection":report}))?;
            Ok(if report.has_errors() { 2 } else { 0 })
        }
        Command::Prepare(input) => {
            let (path, record) = execution::start(Configuration::load(&input.config)?, true)?;
            outcome(path, record)
        }
        Command::Run { config, prepared } => {
            let (path, record) = if let Some(path) = prepared {
                execution::from_prepared(&path)?
            } else {
                execution::start(
                    Configuration::load(&config.unwrap_or_else(|| "fountain-eval.toml".into()))?,
                    false,
                )?
            };
            outcome(path, record)
        }
        Command::Report { run } => {
            emit(&evidence::load(&run)?)?;
            Ok(0)
        }
        Command::Compare { left, right } => {
            emit(&evidence::compare(
                &evidence::load(&left)?,
                &evidence::load(&right)?,
            )?)?;
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
