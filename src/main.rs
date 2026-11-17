use clap::{Args, Parser, Subcommand};
use fountain_eval::{
    artifact, batch, catalog, evidence, progress, runner, util::Result, workspace::Workspace,
};
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
    /// Configuration inside the active workspace. Defaults to fountain-eval.toml.
    #[arg(long)]
    config: Option<PathBuf>,
}
#[derive(Subcommand)]
enum Command {
    /// Create and activate a workspace.
    Init {
        /// Empty this existing directory before initializing it. Omit to create ./feval_workspace.
        directory: Option<PathBuf>,
    },
    /// Select the workspace for subsequent commands.
    Activate { directory: PathBuf },
    /// Print the active workspace path.
    Workspace,
    /// Clear results and temporary data from the active workspace.
    Clean,
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
        #[command(flatten)]
        input: Input,
        #[arg(long, conflicts_with = "config")]
        prepared: Option<PathBuf>,
        /// Emit the machine-readable result on stdout; progress remains on stderr.
        #[arg(long)]
        json: bool,
    },
    /// Generate a Markdown report from a sealed result, or read a published report.
    Report {
        result: PathBuf,
        /// Keep the raw result after publishing its report.
        #[arg(long)]
        keep: bool,
        /// Emit the published report path or the stored report index as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Compare common measured cases and report coverage differences.
    Compare {
        left: PathBuf,
        right: PathBuf,
        #[arg(long)]
        json: bool,
    },
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
fn outcome(path: PathBuf, record: batch::Record, machine: bool) -> Result<u8> {
    let code = batch::exit_code(&record);
    if machine {
        if record.action == "prepare" {
            emit(&json!({"prepared":path,"batch":record}))?;
        } else {
            emit(&json!({"result":path,"batch":record}))?;
        }
    } else {
        emit_text(&progress::summary(&record, &path))?;
    }
    Ok(code)
}
fn execute(command: Command) -> Result<u8> {
    match command {
        Command::Init { directory } => {
            let workspace = Workspace::initialize(directory.as_deref())?;
            emit_text(&format!(
                "Initialized and activated {}\n",
                workspace.root().display()
            ))?;
            Ok(0)
        }
        Command::Activate { directory } => {
            let workspace = Workspace::activate(&directory)?;
            emit_text(&format!("Activated {}\n", workspace.root().display()))?;
            Ok(0)
        }
        Command::Workspace => {
            let workspace = Workspace::active()?;
            let _guard = workspace.lock()?;
            emit_text(&format!("{}\n", workspace.root().display()))?;
            Ok(0)
        }
        Command::Clean => {
            let workspace = Workspace::active()?;
            workspace.clean()?;
            emit_text(&format!(
                "Cleared results and temporary data in {}\n",
                workspace.root().display()
            ))?;
            Ok(0)
        }
        Command::List => {
            emit(&catalog::list())?;
            Ok(0)
        }
        Command::Inspect(input) => {
            let workspace = Workspace::active()?;
            let _guard = workspace.lock()?;
            let configuration = workspace.configuration(input.config.as_deref())?;
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
            let workspace = Workspace::active()?;
            let _guard = workspace.lock()?;
            progress::set_enabled(true);
            progress::line(format!("Workspace: {}", workspace.root().display()));
            let (path, record) =
                batch::run(workspace.configuration(input.config.as_deref())?, true)?;
            outcome(path, record, json)
        }
        Command::Run {
            input,
            prepared,
            json,
        } => {
            let workspace = Workspace::active()?;
            let _guard = workspace.lock()?;
            progress::set_enabled(true);
            progress::line(format!("Workspace: {}", workspace.root().display()));
            let (path, record) = if let Some(path) = prepared {
                batch::from_prepared_in(&workspace.prepared_input(&path)?, Some(workspace.root()))?
            } else {
                batch::run(workspace.configuration(input.config.as_deref())?, false)?
            };
            outcome(path, record, json)
        }
        Command::Report { result, keep, json } => {
            let workspace = Workspace::active()?;
            let _guard = workspace.lock()?;
            let path = workspace.root().join(&result);
            if path.extension().is_some_and(|extension| extension == "md")
                || path.join("report.md").is_file()
            {
                if keep {
                    return Err("--keep applies when generating a report from a run result".into());
                }
                let (body, saved) = artifact::load(&path)?;
                if json {
                    emit(&saved)?;
                } else {
                    emit_text(&body)?;
                }
            } else {
                let raw = workspace.result_input(&result)?;
                let report = artifact::generate(&raw, &workspace.reports(), keep)?;
                if json {
                    emit(&json!({"report":report,"result_id":raw.file_name()}))?;
                } else {
                    emit_text(&format!("Report: {}\n", report.display()))?;
                }
            }
            Ok(0)
        }
        Command::Compare { left, right, json } => {
            let workspace = Workspace::active()?;
            let _guard = workspace.lock()?;
            let r = evidence::compare_summaries(
                &artifact::select(&workspace.root().join(left))?,
                &artifact::select(&workspace.root().join(right))?,
            )?;
            if json {
                emit(&r)?;
            } else {
                emit_text(&fountain_eval::report::comparison(&r))?;
            }
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
