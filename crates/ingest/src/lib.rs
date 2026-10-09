//! `ingest`: find the input, probe it, normalize it to constant frame rate.

pub mod probe;
pub mod resolve;

use std::fs;
use std::path::{Path, PathBuf};

use model::tools::{self, ToolError};
use model::{Fps, Probe, SafeZone};

/// The normalized video every later stage reads, inside the run directory.
pub const SOURCE_FILE: &str = "source.cfr.mp4";
pub const PROBE_FILE: &str = "probe.json";

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("unknown safe-zone profile {0:?}")]
    UnknownSafeZone(String),
    #[error("{path}")]
    Probe {
        path: PathBuf,
        source: probe::ProbeError,
    },
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("{path}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Normalizes `input` into `out_root/<input stem>/` and returns that run
/// directory. Writes `source.cfr.mp4` and `probe.json`.
pub fn run(input: &Path, out_root: &Path, safe_zone: &str) -> Result<PathBuf, IngestError> {
    let safe_zone = SafeZone::profile(safe_zone)
        .ok_or_else(|| IngestError::UnknownSafeZone(safe_zone.to_string()))?;

    let original = ffprobe(input)?;
    let timing = probe::packet_timing(&packet_pts(input)?);
    let vfr = original.vfr || timing.as_ref().is_some_and(|t| t.irregular(original.rate));
    let rate = match (&timing, vfr) {
        (Some(t), true) => probe::snap_rate(1.0 / t.median_interval),
        _ => original.rate,
    };

    let run_dir = out_root.join(input.file_stem().unwrap_or_default());
    fs::create_dir_all(&run_dir).map_err(|source| IngestError::Io {
        path: run_dir.clone(),
        source,
    })?;
    let dest = run_dir.join(SOURCE_FILE);

    if vfr || copy(input, &dest).is_err() {
        encode_cfr(input, &dest, rate)?;
    }

    let normalized = ffprobe(&dest)?;
    let probe = Probe {
        version: 1,
        original: input.display().to_string(),
        file: SOURCE_FILE.to_string(),
        rate: normalized.rate,
        frames: normalized.frames,
        width: normalized.width,
        height: normalized.height,
        aspect: normalized.aspect,
        was_vfr: vfr,
        audio: normalized.audio,
        safe_zone,
    };
    let probe_path = run_dir.join(PROBE_FILE);
    let json = serde_json::to_string_pretty(&probe).expect("Probe always serializes");
    fs::write(&probe_path, json).map_err(|source| IngestError::Io {
        path: probe_path,
        source,
    })?;
    Ok(run_dir)
}

fn ffprobe(path: &Path) -> Result<probe::Parsed, IngestError> {
    let json = tools::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
            path_str(path),
        ],
    )?;
    probe::parse_ffprobe(&json).map_err(|source| IngestError::Probe {
        path: path.to_path_buf(),
        source,
    })
}

fn packet_pts(path: &Path) -> Result<String, IngestError> {
    Ok(tools::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
            path_str(path),
        ],
    )?)
}

/// Re-muxes without re-encoding. Fails for codecs MP4 cannot hold, in which
/// case the caller encodes instead.
fn copy(input: &Path, dest: &Path) -> Result<(), ToolError> {
    tools::run(
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-i",
            path_str(input),
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            path_str(dest),
        ],
    )
    .map(drop)
}

/// Resamples video to exactly `rate` and stretches audio to stay in sync.
fn encode_cfr(input: &Path, dest: &Path, rate: Fps) -> Result<(), ToolError> {
    let fps = format!("fps={rate}");
    tools::run(
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-i",
            path_str(input),
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-vf",
            &fps,
            "-fps_mode",
            "cfr",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "16",
            "-pix_fmt",
            "yuv420p",
            "-af",
            "aresample=async=1",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            path_str(dest),
        ],
    )
    .map(drop)
}

fn path_str(path: &Path) -> &str {
    // Paths come from the user's filesystem; non-UTF-8 names are rare enough
    // that a lossy fallback is not worth carrying through every call.
    path.to_str().unwrap_or_default()
}
