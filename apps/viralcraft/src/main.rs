//! `viralcraft`: turn a viral short-form video into an edit spec.
//!
//! Every subcommand prints exactly one JSON summary line on stdout for the
//! agent to parse, including on failure. Human-readable progress goes to
//! stderr.

mod doctor_cmd;
mod prompt;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that the tools and model files ViralCraft needs are present.
    Doctor {
        /// Offer to run each missing item's install command, asking first.
        #[arg(long)]
        install: bool,
    },
    /// Normalize a video to constant frame rate and write probe.json.
    Ingest {
        /// The video. Defaults to the only video in --inbox; asks otherwise.
        file: Option<PathBuf>,
        #[arg(long, default_value = "inbox")]
        inbox: PathBuf,
        #[arg(long, default_value = "out")]
        out: PathBuf,
        #[arg(long, default_value = model::probe::DEFAULT_SAFE_ZONE)]
        safe_zone: String,
    },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Doctor { .. } => "doctor",
            Command::Ingest { .. } => "ingest",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let name = cli.command.name();
    let (summary, ok) = match run(cli.command) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("error: {e:#}");
            (json!({ "ok": false, "error": format!("{e:#}") }), false)
        }
    };
    let mut summary = summary;
    summary["command"] = json!(name);
    println!("{summary}");
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(command: Command) -> anyhow::Result<(Value, bool)> {
    match command {
        Command::Doctor { install } => doctor_cmd::run(install),
        Command::Ingest {
            file,
            inbox,
            out,
            safe_zone,
        } => {
            let input = ingest::resolve::resolve(file.as_deref(), &inbox, prompt::ask_path)?;
            eprintln!("ingest: {}", input.display());
            let run_dir = ingest::run(&input, &out, &safe_zone)?;
            Ok((
                json!({
                    "ok": true,
                    "run_dir": run_dir,
                    "files": [
                        run_dir.join(ingest::SOURCE_FILE),
                        run_dir.join(ingest::PROBE_FILE),
                    ],
                }),
                true,
            ))
        }
    }
}
