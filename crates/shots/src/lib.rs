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

/// Short side of extracted frames. At half size a 720p source left 13 px
/// dialogue text, too small to read reliably; 720 keeps it legible.
pub const FRAME_SHORT_SIDE: u32 = 720;

/// Extracted frame size for a `width` x `height` source: the short side
/// becomes 720 px, never upscaled, both sides even.
pub fn frame_size(width: u32, height: u32) -> (u32, u32) {
    let short = width.min(height).max(1);
    let factor = (f64::from(FRAME_SHORT_SIDE) / f64::from(short)).min(1.0);
    let even = |v: u32| ((f64::from(v) * factor / 2.0).round() as u32 * 2).max(2);
    (even(width), even(height))
}

/// Where frame `n` of the normalized source is stored, relative to the run
/// directory. Numbered from 0; sized by [`frame_size`].
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
    let (fw, fh) = frame_size(probe.width, probe.height);
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
            &format!("signalstats,scdet=threshold=0,metadata=print:file={STATS},scale={fw}:{fh}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_get_720_px_on_the_short_side() {
        assert_eq!(frame_size(1080, 1920), (720, 1280));
        assert_eq!(frame_size(1920, 1080), (1280, 720));
        assert_eq!(frame_size(720, 1280), (720, 1280));
    }

    #[test]
    fn small_sources_are_never_upscaled() {
        assert_eq!(frame_size(540, 960), (540, 960));
    }

    #[test]
    fn sizes_are_even_for_the_encoder() {
        let (w, h) = frame_size(1081, 1921);
        assert_eq!((w % 2, h % 2), (0, 0));
    }
}
