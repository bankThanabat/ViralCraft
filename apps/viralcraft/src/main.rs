//! `viralcraft`: turn a viral short-form video into an edit spec.
//!
//! Every subcommand prints exactly one JSON summary line on stdout for the
//! agent to parse. Human-readable progress goes to stderr.

mod doctor_cmd;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    let (summary, ok) = match cli.command {
        Command::Doctor { install } => doctor_cmd::run(install)?,
    };
    println!("{summary}");
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
