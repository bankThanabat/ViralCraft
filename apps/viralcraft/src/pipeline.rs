//! `analyze` runs every measuring stage and writes the draft outputs;
//! `clean` removes the large working files once a run is done.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

use crate::{doctor_cmd, prompt};

pub struct AnalyzeArgs {
    pub file: Option<PathBuf>,
    pub inbox: PathBuf,
    pub out: PathBuf,
    pub safe_zone: String,
    pub threshold: f64,
}

/// Runs `step`, printing its name and how long it took on stderr.
fn timed<T>(name: &str, step: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
    let t = Instant::now();
    eprint!("{name}… ");
    let result = step();
    eprintln!("{:.1}s", t.elapsed().as_secs_f64());
    result
}

pub fn analyze(args: AnalyzeArgs) -> anyhow::Result<(Value, bool)> {
    let report = doctor_cmd::check();
    if !report.ok {
        doctor_cmd::print_report(&report);
        return Ok((
            json!({
                "ok": false,
                "error": "missing tools or models: run `viralcraft doctor --install`",
                "items": report.items,
            }),
            false,
        ));
    }

    let input = ingest::resolve::resolve(args.file.as_deref(), &args.inbox, prompt::ask_path)?;
    eprintln!("analyze: {}", input.display());
    let run_dir = timed("ingest", || {
        Ok(ingest::run(&input, &args.out, &args.safe_zone)?)
    })?;
    let shots = timed("shots", || {
        Ok(shots::run_with_threshold(&run_dir, args.threshold)?)
    })?;
    let audio = timed("audio", || Ok(audio::run(&run_dir)?))?;
    let inspect = timed("inspect", || Ok(inspect::run(&run_dir)?))?;
    let spec = timed("edit spec", || Ok(editspec::run(&run_dir)?))?;
    let exported = timed("exports", || Ok(exports::run(&run_dir)?))?;

    let mut files: Vec<PathBuf> = [
        ingest::PROBE_FILE,
        ingest::SOURCE_FILE,
        shots::SHOTS_FILE,
        shots::FRAMES_DIR,
        audio::AUDIO_FILE,
        inspect::INSPECT_FILE,
    ]
    .iter()
    .map(|f| run_dir.join(f))
    .collect();
    files.push(spec.clone());
    files.extend(exported);
    let todo_slots = fs::read_to_string(&spec)
        .map(|d| d.matches("TODO:").count())
        .unwrap_or(0);

    Ok((
        json!({
            "ok": true,
            "run_dir": run_dir,
            "files": files,
            "shots": shots.segments.len(),
            "captions": inspect.captions.len(),
            "overlays": inspect.overlays.len(),
            "words": audio.words.len(),
            "sounds": audio.sounds.len(),
            "todo_slots": todo_slots,
            "next": format!(
                "review EDIT-SPEC.md and the frames, write judgement.json, then run `viralcraft finalize {}`",
                run_dir.display()
            ),
        }),
        true,
    ))
}

/// Deletes `frames/` and `source.cfr.mp4`, the two large working files.
pub fn clean(run_dir: &Path) -> anyhow::Result<(Value, bool)> {
    let mut removed = Vec::new();
    let frames = run_dir.join(shots::FRAMES_DIR);
    if frames.is_dir() {
        fs::remove_dir_all(&frames)?;
        removed.push(frames);
    }
    let source = run_dir.join(ingest::SOURCE_FILE);
    if source.is_file() {
        fs::remove_file(&source)?;
        removed.push(source);
    }
    Ok((json!({ "ok": true, "removed": removed }), true))
}

pub const PREVIEW_FILE: &str = "preview.mp4";

/// Writes `preview.mp4`: the source with `captions.ass` burned on top, so
/// any mismatch with the original captions shows as doubled or offset
/// text. Development only; needs an ffmpeg with libass, which users are
/// not asked to install.
pub fn preview(run_dir: &Path) -> anyhow::Result<(Value, bool)> {
    let Some(ffmpeg) = doctor_cmd::find_preview_ffmpeg() else {
        anyhow::bail!(
            "preview is a development tool and needs an ffmpeg with libass (on macOS: brew install ffmpeg-full)"
        );
    };
    for needed in [ingest::SOURCE_FILE, exports::ASS_FILE] {
        if !run_dir.join(needed).exists() {
            anyhow::bail!(
                "{} not found; run `viralcraft analyze` first",
                run_dir.join(needed).display()
            );
        }
    }
    let program = ffmpeg.to_string_lossy().into_owned();
    timed("preview", || {
        Ok(model::tools::run_in(
            run_dir,
            &program,
            &[
                "-y",
                "-v",
                "error",
                "-i",
                ingest::SOURCE_FILE,
                "-vf",
                &format!("ass={}", exports::ASS_FILE),
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "20",
                "-c:a",
                "copy",
                PREVIEW_FILE,
            ],
        )?)
    })?;
    Ok((
        json!({ "ok": true, "files": [run_dir.join(PREVIEW_FILE)] }),
        true,
    ))
}
