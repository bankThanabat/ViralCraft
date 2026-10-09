//! `exports`: the files other tools open. WebVTT and ASS captions, and the
//! OpenTimelineIO timeline.

pub mod ass;
pub mod vtt;

use std::fs;
use std::path::{Path, PathBuf};

use model::{Audio, Inspect, Probe};

pub const VTT_FILE: &str = "captions.vtt";
pub const ASS_FILE: &str = "captions.ass";

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
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

/// Writes `captions.vtt` and `captions.ass` from the run's measurements.
pub fn run(run_dir: &Path) -> Result<Vec<PathBuf>, ExportError> {
    let probe: Probe = read_json(&run_dir.join("probe.json"))?;
    let audio: Audio = read_json(&run_dir.join("audio.json"))?;
    let inspect: Inspect = read_json(&run_dir.join("inspect.json"))?;
    let files = [
        (run_dir.join(VTT_FILE), vtt::write(&probe, &audio, &inspect)),
        (run_dir.join(ASS_FILE), ass::write(&probe, &inspect)),
    ];
    let mut written = Vec::new();
    for (path, text) in files {
        fs::write(&path, text).map_err(|source| ExportError::Io {
            path: path.clone(),
            source,
        })?;
        written.push(path);
    }
    Ok(written)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ExportError> {
    let text = fs::read_to_string(path).map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ExportError::Json {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
pub(crate) mod testdata {
    use std::path::Path;

    use model::{Audio, Inspect, Probe};

    fn read<T: serde::de::DeserializeOwned>(name: &str) -> T {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../editspec/tests/data/cuts-3"
        ));
        serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap()
    }

    pub fn cuts3() -> (Probe, Audio, Inspect) {
        (read("probe.json"), read("audio.json"), read("inspect.json"))
    }
}
