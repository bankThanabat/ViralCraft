//! `exports`: the files other tools open. WebVTT and ASS captions, and the
//! OpenTimelineIO timeline.

pub mod ass;
pub mod otio;
pub mod vtt;

use std::fs;
use std::path::{Path, PathBuf};

use model::{Audio, Inspect, Judgement, Probe, Shots};

pub const VTT_FILE: &str = "captions.vtt";
pub const ASS_FILE: &str = "captions.ass";
pub const OTIO_FILE: &str = "timeline.otio";

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

/// Writes `captions.vtt`, `captions.ass` and `timeline.otio` from the
/// run's measurements.
pub fn run(run_dir: &Path) -> Result<Vec<PathBuf>, ExportError> {
    let probe: Probe = read_json(&run_dir.join("probe.json"))?;
    let shots: Shots = read_json(&run_dir.join("shots.json"))?;
    let audio: Audio = read_json(&run_dir.join("audio.json"))?;
    let mut inspect: Inspect = read_json(&run_dir.join("inspect.json"))?;
    let judgement_path = run_dir.join("judgement.json");
    let judgement: Option<Judgement> = if judgement_path.exists() {
        Some(read_json(&judgement_path)?)
    } else {
        None
    };
    if let Some(j) = &judgement {
        j.apply_to_captions(&mut inspect.captions);
        inspect.overlays.retain(|o| j.keeps_overlay(&o.id));
    }
    let timeline = otio::write_judged(&probe, &shots, &audio, &inspect, judgement.as_ref());
    let files = [
        (run_dir.join(VTT_FILE), vtt::write(&probe, &audio, &inspect)),
        (run_dir.join(ASS_FILE), ass::write(&probe, &inspect)),
        (
            run_dir.join(OTIO_FILE),
            serde_json::to_string_pretty(&timeline).expect("the timeline is plain JSON"),
        ),
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
