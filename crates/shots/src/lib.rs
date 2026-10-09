//! `shots`: find cuts, dissolves and flashes, and extract every frame.

pub mod detect;

use std::fs;
use std::path::{Path, PathBuf};

use model::tools::{self, ToolError};
use model::{Probe, Shots};

pub const SHOTS_FILE: &str = "shots.json";
pub const FRAMES_DIR: &str = "frames";
/// scdet's own default on its 0..100 scale.
pub const DEFAULT_THRESHOLD: f64 = 10.0;

/// Where frame `n` of the normalized source is stored, relative to the run
/// directory. Frames are half resolution, numbered from 0.
pub fn frame_path(n: u64) -> PathBuf {
    Path::new(FRAMES_DIR).join(format!("f{n:05}.jpg"))
}

#[derive(Debug, thiserror::Error)]
pub enum ShotsError {
    #[error("{path}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("scdet output")]
    Parse(#[from] detect::ParseError),
}

/// Reads `probe.json`, decodes the source once, writes `frames/` and
/// `shots.json`. Returns the shots.
pub fn run(run_dir: &Path) -> Result<Shots, ShotsError> {
    run_with_threshold(run_dir, DEFAULT_THRESHOLD)
}

pub fn run_with_threshold(run_dir: &Path, threshold: f64) -> Result<Shots, ShotsError> {
    let probe: Probe = read_json(&run_dir.join(ingest::PROBE_FILE))?;

    let frames_dir = run_dir.join(FRAMES_DIR);
    if frames_dir.exists() {
        fs::remove_dir_all(&frames_dir).map_err(|source| io(&frames_dir, source))?;
    }
    fs::create_dir_all(&frames_dir).map_err(|source| io(&frames_dir, source))?;

    const STATS: &str = "scdet.txt";
    let pattern = Path::new(FRAMES_DIR).join("f%05d.jpg");
    tools::run_in(
        run_dir,
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-i",
            &probe.file,
            "-an",
            "-vf",
            &format!("signalstats,scdet=threshold=0,metadata=print:file={STATS},scale=iw/2:ih/2"),
            "-fps_mode",
            "passthrough",
            "-q:v",
            "3",
            "-start_number",
            "0",
            pattern.to_str().unwrap_or_default(),
        ],
    )?;

    let stats_path = run_dir.join(STATS);
    let text = fs::read_to_string(&stats_path).map_err(|source| io(&stats_path, source))?;
    let shots = detect::detect(&detect::parse_metadata(&text)?, probe.rate, threshold);
    // The raw score dump is fully represented in shots.json.
    let _ = fs::remove_file(&stats_path);

    let out = run_dir.join(SHOTS_FILE);
    let json = serde_json::to_string_pretty(&shots).expect("Shots always serializes");
    fs::write(&out, json).map_err(|source| io(&out, source))?;
    Ok(shots)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ShotsError> {
    let text = fs::read_to_string(path).map_err(|source| io(path, source))?;
    serde_json::from_str(&text).map_err(|source| ShotsError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn io(path: &Path, source: std::io::Error) -> ShotsError {
    ShotsError::Io {
        path: path.to_path_buf(),
        source,
    }
}
