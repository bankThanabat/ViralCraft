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
    /// Transcribe speech with word timing; detect beats and tempo.
    Audio { run_dir: PathBuf },
    /// Measure captions (and later motion, look, overlays) from the frames.
    Inspect { run_dir: PathBuf },
    /// Write captions.vtt and captions.ass from the measurements.
    Exports { run_dir: PathBuf },
    /// Write EDIT-SPEC.md from the measurements in a run directory.
    Editspec { run_dir: PathBuf },
    /// Detect cuts, dissolves and flashes; extract every frame (720 px short side).
    Shots {
        run_dir: PathBuf,
        /// scdet score (0..100) a hard cut must reach.
        #[arg(long, default_value_t = shots::DEFAULT_THRESHOLD)]
        threshold: f64,
    },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Doctor { .. } => "doctor",
            Command::Ingest { .. } => "ingest",
            Command::Shots { .. } => "shots",
            Command::Audio { .. } => "audio",
            Command::Inspect { .. } => "inspect",
            Command::Editspec { .. } => "editspec",
            Command::Exports { .. } => "exports",
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
        Command::Audio { run_dir } => {
            eprintln!("audio: {}", run_dir.display());
            let audio = audio::run(&run_dir)?;
            Ok((
                json!({
                    "ok": true,
                    "words": audio.words.len(),
                    "speech_spans": audio.speech.len(),
                    "bpm": (audio.tempo.bpm * 10.0).round() / 10.0,
                    "tempo_confidence": (audio.tempo.confidence * 100.0).round() / 100.0,
                    "beats": audio.beats.len(),
                    "music_spans": audio.music.len(),
                    "sounds": audio.sounds.len(),
                    "files": [run_dir.join(audio::AUDIO_FILE)],
                }),
                true,
            ))
        }
        Command::Inspect { run_dir } => {
            eprintln!("inspect: {}", run_dir.display());
            let inspect = inspect::run(&run_dir)?;
            Ok((
                json!({
                    "ok": true,
                    "captions": inspect.captions.len(),
                    "overlays": inspect.overlays.len(),
                    "shots_with_motion": inspect.motion.iter().filter(|m| m.transform.is_some()).count(),
                    "detect_ms_per_frame": inspect.timing.detect_ms_per_frame,
                    "files": [run_dir.join(inspect::INSPECT_FILE)],
                }),
                true,
            ))
        }
        Command::Exports { run_dir } => {
            let files = exports::run(&run_dir)?;
            Ok((json!({ "ok": true, "files": files }), true))
        }
        Command::Editspec { run_dir } => {
            let path = editspec::run(&run_dir)?;
            let todos = std::fs::read_to_string(&path)
                .map(|d| d.matches("TODO:").count())
                .unwrap_or(0);
            Ok((
                json!({ "ok": true, "todo_slots": todos, "files": [path] }),
                true,
            ))
        }
        Command::Shots { run_dir, threshold } => {
            eprintln!("shots: {}", run_dir.display());
            let shots = shots::run_with_threshold(&run_dir, threshold)?;
            Ok((
                json!({
                    "ok": true,
                    "segments": shots.segments.len(),
                    "transitions": shots.transitions.len(),
                    "frames": shots.frames,
                    "files": [run_dir.join(shots::SHOTS_FILE), run_dir.join(shots::FRAMES_DIR)],
                }),
                true,
            ))
        }
    }
}
