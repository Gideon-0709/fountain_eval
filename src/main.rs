use clap::{Args, Parser, Subcommand};
use fountain_eval::{
    catalog, evidence,
    execution::{self, Options},
    protocol::EvaluationRequest,
    runner,
    util::Result,
};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
#[derive(Parser)]
#[command(
    name = "fountain-eval",
    version,
    about = "Reproducible Fountain component evaluation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Offline reference and local declaration inspection.
    Inspect(Inputs),
    /// List immutable interface and acceptance-test registrations.
    List,
    /// Resolve, freeze, build and probe without running measurements.
    Prepare {
        #[command(flatten)]
        inputs: Inputs,
        #[command(flatten)]
        storage: Storage,
    },
    /// Validate and measure a component pair, or execute a prepared run.
    Run {
        #[arg(long,required_unless_present="prepared",requires_all=["operator","test"])]
        engine: Option<String>,
        #[arg(long,requires_all=["engine","test"])]
        operator: Option<String>,
        #[arg(long,requires_all=["engine","operator"])]
        test: Option<String>,
        #[arg(long,conflicts_with_all=["engine","operator","test","cache_dir"])]
        prepared: Option<PathBuf>,
        #[command(flatten)]
        storage: Storage,
    },
    /// Verify and display a sealed run record without needing its build cache.
    Report { run: PathBuf },
    /// Check comparability and compare acceptance results.
    Compare { left: PathBuf, right: PathBuf },
}
#[derive(Args)]
struct Inputs {
    #[arg(long)]
    engine: String,
    #[arg(long)]
    operator: String,
    #[arg(long)]
    test: String,
}
impl Inputs {
    fn request(&self) -> Result<EvaluationRequest> {
        EvaluationRequest::parse(&self.engine, &self.operator, &self.test)
            .map_err(|e| e.to_string())
    }
}
#[derive(Args)]
struct Storage {
    #[arg(long, default_value = ".fountain-eval")]
    cache_dir: PathBuf,
    #[arg(long, default_value = "runs")]
    output: PathBuf,
}
impl Storage {
    fn options(&self) -> Options {
        Options {
            cache: self.cache_dir.clone(),
            output: self.output.clone(),
        }
    }
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
        Command::Inspect(inputs) => {
            let report = runner::inspect(inputs.request()?);
            emit(&report)?;
            Ok(if report.has_errors() { 2 } else { 0 })
        }
        Command::Prepare { inputs, storage } => {
            let (directory, record) =
                execution::start(inputs.request()?, &storage.options(), true)?;
            outcome(directory, record)
        }
        Command::Run {
            engine,
            operator,
            test,
            prepared,
            storage,
        } => {
            let (directory, record) = if let Some(path) = prepared {
                execution::from_prepared(&path, &storage.output)?
            } else {
                let request = EvaluationRequest::parse(
                    &engine.ok_or("engine required")?,
                    &operator.ok_or("operator required")?,
                    &test.ok_or("test required")?,
                )
                .map_err(|e| e.to_string())?;
                execution::start(request, &storage.options(), false)?
            };
            outcome(directory, record)
        }
        Command::Report { run } => {
            let record = evidence::load(&run)?;
            emit(&record)?;
            Ok(0)
        }
        Command::Compare { left, right } => {
            let comparison: Value =
                evidence::compare(&evidence::load(&left)?, &evidence::load(&right)?)?;
            emit(&comparison)?;
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
