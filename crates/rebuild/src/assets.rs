//! Probes, transcribes and thumbnails each clip in the assets folder and
//! writes `rebuild/assets.json`.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use model::{Asset, Assets};
use sha2::{Digest, Sha256};

pub const REBUILD_DIR: &str = "rebuild";
pub const ASSETS_FILE: &str = "assets.json";
const THUMBS_DIR: &str = "thumbs";
/// Frames in each clip's thumbnail strip.
const THUMB_FRAMES: u32 = 4;

const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mov", "m4v", "webm", "mkv", "avi"];

#[derive(Debug, thiserror::Error)]
pub enum AssetsError {
    #[error("no video clips in {0}")]
    NoClips(PathBuf),
    #[error("{path}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Ingest(#[from] ingest::IngestError),
    #[error(transparent)]
    Audio(#[from] audio::AudioError),
    #[error(transparent)]
    Tool(#[from] model::tools::ToolError),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> AssetsError + '_ {
    move |source| AssetsError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Video files directly inside `dir`, by name. Hidden files are skipped.
/// A missing folder has no clips.
pub fn find_clips(dir: &Path) -> Result<Vec<PathBuf>, AssetsError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut clips: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(io_err(dir))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default();
            !name.starts_with('.') && VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
        })
        .collect();
    clips.sort();
    Ok(clips)
}

/// `given` when it holds clips; otherwise asks for a folder once. No
/// answer, or a folder without clips, is an error.
pub fn resolve_folder(
    given: &Path,
    ask: impl FnOnce(&str) -> Option<PathBuf>,
) -> Result<PathBuf, AssetsError> {
    if !find_clips(given)?.is_empty() {
        return Ok(given.to_path_buf());
    }
    let question = format!(
        "No video clips in {}. Which folder has your clips?",
        given.display()
    );
    match ask(&question) {
        Some(dir) if !find_clips(&dir)?.is_empty() => Ok(dir),
        Some(dir) => Err(AssetsError::NoClips(dir)),
        None => Err(AssetsError::NoClips(given.to_path_buf())),
    }
}

/// For each `(name, hash)`, the earlier name with the same hash, if any.
pub fn duplicates(files: &[(String, String)]) -> Vec<Option<String>> {
    let mut first: HashMap<&str, &str> = HashMap::new();
    files
        .iter()
        .map(|(name, hash)| match first.get(hash.as_str()) {
            Some(original) => Some(original.to_string()),
            None => {
                first.insert(hash, name);
                None
            }
        })
        .collect()
}

/// ffmpeg filter that tiles frames spread evenly over `seconds` into one
/// strip, each frame 240 px wide.
pub fn thumbnail_filter(seconds: f64) -> String {
    let every = (seconds / f64::from(THUMB_FRAMES)).max(0.04);
    format!("fps=1/{every:.3},scale=240:-2,tile={THUMB_FRAMES}x1")
}

fn sha256(path: &Path) -> Result<String, AssetsError> {
    let mut file = File::open(path).map_err(io_err(path))?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(io_err(path))?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

/// Reads every clip in `folder` into `<run_dir>/rebuild/assets.json`.
/// Writes only inside `run_dir`.
pub fn run(run_dir: &Path, folder: &Path) -> Result<Assets, AssetsError> {
    let clips = find_clips(folder)?;
    if clips.is_empty() {
        return Err(AssetsError::NoClips(folder.to_path_buf()));
    }
    let dir = run_dir.join(REBUILD_DIR);
    let thumbs = dir.join(THUMBS_DIR);
    fs::create_dir_all(&thumbs).map_err(io_err(&thumbs))?;

    let named: Vec<(String, String)> = clips
        .iter()
        .map(|p| {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            sha256(p).map(|h| (name, h))
        })
        .collect::<Result<_, _>>()?;
    let dupes = duplicates(&named);

    let mut out = Vec::with_capacity(clips.len());
    for ((path, (name, hash)), duplicate_of) in clips.iter().zip(named).zip(dupes) {
        let probe = ingest::ffprobe(path)?;
        let mut asset = Asset {
            file: name.clone(),
            sha256: hash,
            duplicate_of,
            seconds: probe.frames as f64 / probe.rate.as_f64(),
            rate: probe.rate,
            frames: probe.frames,
            width: probe.width,
            height: probe.height,
            aspect: probe.aspect,
            has_audio: probe.audio.is_some(),
            language: String::new(),
            transcript: String::new(),
            words: Vec::new(),
            sounds: Vec::new(),
            music: Vec::new(),
            thumbnail: String::new(),
        };
        if asset.duplicate_of.is_some() {
            out.push(asset);
            continue;
        }
        eprintln!("assets: {name}");

        let stem = Path::new(&name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let thumb = Path::new(REBUILD_DIR)
            .join(THUMBS_DIR)
            .join(format!("{stem}.jpg"));
        model::tools::run(
            "ffmpeg",
            &[
                "-y",
                "-v",
                "error",
                "-i",
                path_str(path),
                "-vf",
                &thumbnail_filter(asset.seconds),
                "-frames:v",
                "1",
                "-q:v",
                "3",
                path_str(&run_dir.join(&thumb)),
            ],
        )?;
        asset.thumbnail = thumb.to_string_lossy().into_owned();

        if let Some(info) = &probe.audio {
            let samples = audio::extract_16k_mono(&dir, path_str(path))?;
            let (words, language) = audio::transcribe(&samples, probe.rate, info.offset)?;
            let (words, mut sounds) = audio::words::split_sound_tags(words);
            sounds.extend(audio::name_sounds(&samples, probe.rate, info.offset)?);
            sounds.sort_by_key(|s| s.start.value);
            asset.transcript = audio::words::transcript(&words, &sounds);
            asset.language = if words.is_empty() {
                String::new()
            } else {
                language
            };
            asset.music = audio::music_beds(&samples, probe.rate, info.offset, probe.frames);
            asset.words = words;
            asset.sounds = sounds;
        }
        out.push(asset);
    }

    let assets = Assets {
        version: 1,
        folder: folder.display().to_string(),
        clips: out,
    };
    let file = dir.join(ASSETS_FILE);
    let json = serde_json::to_string_pretty(&assets).expect("Assets always serialize");
    fs::write(&file, json).map_err(io_err(&file))?;
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_visible_video_files_are_clips_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["b.MOV", "a.mp4", "notes.txt", ".hidden.mp4", "c.webm"] {
            fs::write(dir.path().join(f), b"x").unwrap();
        }
        fs::create_dir(dir.path().join("sub.mp4")).unwrap();
        let names: Vec<String> = find_clips(dir.path())
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.mp4", "b.MOV", "c.webm"]);
    }

    #[test]
    fn a_missing_folder_has_no_clips() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_clips(&dir.path().join("assets")).unwrap().is_empty());
    }

    #[test]
    fn a_folder_with_clips_is_used_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.mp4"), b"x").unwrap();
        let got = resolve_folder(dir.path(), |_| panic!("asked")).unwrap();
        assert_eq!(got, dir.path());
    }

    #[test]
    fn an_empty_folder_asks_and_uses_the_answer() {
        let empty = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("a.mov"), b"x").unwrap();
        let mut asked = String::new();
        let got = resolve_folder(empty.path(), |q| {
            asked = q.to_string();
            Some(other.path().to_path_buf())
        })
        .unwrap();
        assert_eq!(got, other.path());
        assert!(asked.contains("No video clips"), "{asked}");
    }

    #[test]
    fn no_answer_is_an_error() {
        let empty = tempfile::tempdir().unwrap();
        let err = resolve_folder(&empty.path().join("assets"), |_| None).unwrap_err();
        assert!(matches!(err, AssetsError::NoClips(_)), "{err}");
    }

    #[test]
    fn a_copy_of_an_earlier_clip_points_back_to_it() {
        let f = |n: &str, h: &str| (n.to_string(), h.to_string());
        let got = duplicates(&[f("a.mp4", "1"), f("b.mp4", "2"), f("c.mp4", "1")]);
        assert_eq!(got, [None, None, Some("a.mp4".to_string())]);
    }

    #[test]
    fn the_thumbnail_strip_spreads_four_frames_over_the_clip() {
        assert_eq!(thumbnail_filter(28.0), "fps=1/7.000,scale=240:-2,tile=4x1");
        assert!(thumbnail_filter(0.0).starts_with("fps=1/0.040"));
    }
}
