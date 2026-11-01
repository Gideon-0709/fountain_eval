use std::{io, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use fountain_eval::{
    protocol::{EvaluationRequest, FORMAT_VERSION},
    runner,
};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "fountain-eval",
    version,
    about = "Fountain evaluation scaffold: engine + operator + test version"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse references and inspect local manifests; performs no evaluation.
    Inspect(Inputs),
    /// Request evaluation. Currently exits 3 because no test/driver is registered.
    Run(Inputs),
    /// List registered interfaces and test versions (currently empty).
    List,
}

#[derive(Args)]
struct Inputs {
    /// Engine: path:<crate>, registry:<crate>@<version>, or pinned git+ URL.
    #[arg(long)]
    engine: String,
    /// Operator reference; an optional #implementation selects a crate implementation.
    #[arg(long)]
    operator: String,
    /// Test content identity in name@exact-version form.
    #[arg(long = "test")]
    test_version: String,
}

fn emit(value: &impl Serialize) -> Result<(), String> {
    use std::io::Write;
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value).map_err(|e| e.to_string())?;
    writeln!(stdout).map_err(|e| e.to_string())
}

fn execute(command: Command) -> Result<u8, String> {
    let (inputs, run) = match command {
        Command::List => {
            emit(&serde_json::json!({
                "format_version": FORMAT_VERSION,
                "status": "scaffold",
                "interfaces": [],
                "tests": []
            }))?;
            return Ok(0);
        }
        Command::Inspect(inputs) => (inputs, false),
        Command::Run(inputs) => (inputs, true),
    };
    let request =
        match EvaluationRequest::parse(&inputs.engine, &inputs.operator, &inputs.test_version) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("invalid request: {error}");
                return Ok(2);
            }
        };
    let inspection = runner::inspect(request);
    emit(&inspection)?;
    if inspection.has_errors() {
        return Ok(2);
    }
    if run {
        eprintln!(
            "evaluation unavailable: no test versions or interface implementations are registered; use inspect for declaration checks"
        );
        return Ok(3);
    }
    Ok(0)
}

fn main() -> ExitCode {
    match execute(Cli::parse().command) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("output error: {error}");
            ExitCode::FAILURE
        }
    }
}
