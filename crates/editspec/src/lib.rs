//! `editspec`: the Markdown edit spec an editor rebuilds the video from.
//!
//! Everything measured is written as numbers; anything not measured is
//! `unknown`; anything that needs the agent's judgement is a `TODO:` marked
//! `<!-- judgement -->` until `judgement.json` fills it.

pub mod fmt;
pub mod render;
pub mod sync;

pub use render::{render, Inputs};

use std::fs;
use std::path::{Path, PathBuf};

pub const EDIT_SPEC_FILE: &str = "EDIT-SPEC.md";

#[derive(Debug, thiserror::Error)]
pub enum EditSpecError {
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
}

/// Reads the four measurement files from `run_dir` and writes
/// `EDIT-SPEC.md` next to them.
pub fn run(run_dir: &Path) -> Result<PathBuf, EditSpecError> {
    let inputs = load(run_dir)?;
    let out = run_dir.join(EDIT_SPEC_FILE);
    fs::write(&out, render(&inputs)).map_err(|source| EditSpecError::Io {
        path: out.clone(),
        source,
    })?;
    Ok(out)
}

pub fn load(run_dir: &Path) -> Result<Inputs, EditSpecError> {
    Ok(Inputs {
        probe: read_json(&run_dir.join(ingest::PROBE_FILE))?,
        shots: read_json(&run_dir.join(shots::SHOTS_FILE))?,
        audio: read_json(&run_dir.join(audio::AUDIO_FILE))?,
        inspect: read_json(&run_dir.join(inspect::INSPECT_FILE))?,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, EditSpecError> {
    let text = fs::read_to_string(path).map_err(|source| EditSpecError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| EditSpecError::Json {
        path: path.to_path_buf(),
        source,
    })
}
