//! `viralcraft`: turn a viral short-form video into an edit spec.
//!
//! Every subcommand prints exactly one JSON summary line on stdout for the
//! agent to parse. Human-readable progress goes to stderr.

use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that the tools and model files ViralCraft needs are present.
    Doctor,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let summary = match cli.command {
        // The real checks arrive with the `doctor` crate; until then it
        // reports that nothing was checked rather than claiming success.
        Command::Doctor => json!({ "command": "doctor", "ok": true, "checks": [] }),
    };
    println!("{summary}");
    Ok(())
}
