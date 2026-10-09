//! `viralcraft`: turn a viral short-form video into an edit spec.
//!
//! Every subcommand prints exactly one JSON summary line on stdout for the
//! agent to parse, including on failure. Human-readable progress goes to
//! stderr.

mod doctor_cmd;
mod finalize;
mod pipeline;
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
    /// Run every measuring stage on a video and write the draft edit spec,
    /// captions and timeline.
    Analyze {
        /// The video. Defaults to the only video in --inbox; asks otherwise.
        file: Option<PathBuf>,
        #[arg(long, default_value = "inbox")]
        inbox: PathBuf,
        #[arg(long, default_value = "out")]
        out: PathBuf,
        #[arg(long, default_value = model::probe::DEFAULT_SAFE_ZONE)]
        safe_zone: String,
        /// scdet score (0..100) a hard cut must reach.
        #[arg(long, default_value_t = shots::DEFAULT_THRESHOLD)]
        threshold: f64,
    },
    /// Delete frames/ and source.cfr.mp4 from a finished run.
    Clean { run_dir: PathBuf },
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
    /// Measure captions, camera motion, look and overlays from the frames.
    Inspect { run_dir: PathBuf },
    /// After judgement.json is written: check it, then write the finished
    /// EDIT-SPEC.md, captions and timeline.
    Finalize { run_dir: PathBuf },
    /// Write captions.vtt, captions.ass and timeline.otio from the measurements.
    Exports { run_dir: PathBuf },
    /// Optional rebuild step: read the user's own clips (probe, transcript,
    /// thumbnail) and any music, effects or recordings they added (in the
    /// folder or a subfolder) into rebuild/assets.json. Never changes them.
    Assets {
        run_dir: PathBuf,
        /// Folder with the clips; asks for one if it has none.
        #[arg(long, default_value = "assets")]
        assets: PathBuf,
        /// Another folder with music or sound effects to use (repeatable).
        #[arg(long)]
        sounds: Vec<PathBuf>,
    },
    /// Optional rebuild step: write rebuild/REBUILD-PROMPT.md, steps any
    /// editor agent can follow to remake the format with the user's clips.
    Prompt {
        run_dir: PathBuf,
        /// The user's caption lines, one per line, used in order.
        #[arg(long)]
        script: Option<PathBuf>,
        /// Rebuild without captions.
        #[arg(long)]
        no_captions: bool,
    },
    /// Optional rebuild step for clips without speech: an AI voice reads
    /// the approved script, one line per caption, into rebuild/voiceover/.
    Voiceover {
        run_dir: PathBuf,
        /// The approved lines, one per caption, in order.
        #[arg(long)]
        script: PathBuf,
        /// say (free, the system's own voices); or openai, elevenlabs,
        /// minimax, qwen or bytedance with your own key in OPENAI_API_KEY,
        /// ELEVENLABS_API_KEY, MINIMAX_API_KEY, DASHSCOPE_API_KEY or
        /// VOLCENGINE_API_KEY.
        #[arg(long, default_value = "say")]
        engine: String,
        #[arg(long)]
        voice: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
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
            Command::Analyze { .. } => "analyze",
            Command::Assets { .. } => "assets",
            Command::Prompt { .. } => "prompt",
            Command::Voiceover { .. } => "voiceover",
            Command::Clean { .. } => "clean",
            Command::Doctor { .. } => "doctor",
            Command::Ingest { .. } => "ingest",
            Command::Shots { .. } => "shots",
            Command::Audio { .. } => "audio",
            Command::Inspect { .. } => "inspect",
            Command::Editspec { .. } => "editspec",
            Command::Exports { .. } => "exports",
            Command::Finalize { .. } => "finalize",
        }
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                e.exit();
            }
            // Usage errors keep the contract too: one JSON line on stdout.
            let text = e.to_string();
            eprint!("{text}");
            let first = text
                .lines()
                .next()
                .unwrap_or_default()
                .trim_start_matches("error: ");
            println!(
                "{}",
                json!({ "command": null, "ok": false, "error": first })
            );
            return ExitCode::from(2);
        }
    };
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
        Command::Analyze {
            file,
            inbox,
            out,
            safe_zone,
            threshold,
        } => pipeline::analyze(pipeline::AnalyzeArgs {
            file,
            inbox,
            out,
            safe_zone,
            threshold,
        }),
        Command::Assets {
            run_dir,
            assets,
            sounds: sound_dirs,
        } => {
            let folder = rebuild::resolve_folder(&assets, prompt::ask_path)?;
            eprintln!("assets: {}", folder.display());
            let read = rebuild::run(&run_dir, &folder, &sound_dirs)?;
            let clips: Vec<Value> = read
                .clips
                .iter()
                .map(|c| {
                    json!({
                        "file": c.file,
                        "seconds": (c.seconds * 10.0).round() / 10.0,
                        "language": c.language,
                        "words": c.words.len(),
                        "duplicate_of": c.duplicate_of,
                    })
                })
                .collect();
            let sounds: Vec<Value> = read
                .sounds
                .iter()
                .map(|s| {
                    json!({
                        "file": s.file,
                        "role": s.role,
                        "seconds": (s.seconds * 10.0).round() / 10.0,
                        "bpm": s.bpm,
                        "heard": s.labels,
                        "duplicate_of": s.duplicate_of,
                    })
                })
                .collect();
            Ok((
                json!({
                    "ok": true,
                    "folder": folder,
                    "clips": clips,
                    "sounds": sounds,
                    "files": [run_dir.join(rebuild::REBUILD_DIR).join(rebuild::ASSETS_FILE)],
                }),
                true,
            ))
        }
        Command::Prompt {
            run_dir,
            script,
            no_captions,
        } => {
            let path = rebuild::prompt::run(&run_dir, script.as_deref(), !no_captions)?;
            Ok((json!({ "ok": true, "files": [path] }), true))
        }
        Command::Voiceover {
            run_dir,
            script,
            engine,
            voice,
            model,
        } => {
            let opts = rebuild::voiceover::VoiceOptions {
                engine: rebuild::voiceover::Engine::parse(&engine)?,
                voice,
                model,
            };
            let v =
                rebuild::voiceover::run(&run_dir, &script, &opts, |var| std::env::var(var).ok())?;
            let too_long: Vec<&str> = v
                .lines
                .iter()
                .filter(|l| !l.fits)
                .map(|l| l.caption.as_str())
                .collect();
            Ok((
                json!({
                    "ok": true,
                    "engine": v.engine,
                    "voice": v.voice,
                    "lines": v.lines.len(),
                    "too_long": too_long,
                    "files": [run_dir.join(rebuild::REBUILD_DIR).join(rebuild::voiceover::VOICEOVER_FILE)],
                }),
                true,
            ))
        }
        Command::Clean { run_dir } => pipeline::clean(&run_dir),
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
        Command::Finalize { run_dir } => finalize::run(&run_dir),
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
