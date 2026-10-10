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
const AUDIO_EXTENSIONS: [&str; 9] = [
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "aiff", "aif",
];
/// A sound at least this long is music unless it is mostly speech.
const MUSIC_MIN_SECONDS: f64 = 15.0;
/// Tempo confidence that counts as a steady beat (the same bar as `audio`).
const STEADY_BEAT: f64 = 0.5;
/// A tempo means nothing in less audio than this: a beat has to repeat.
const MIN_BEAT_SECONDS: f64 = 4.0;
/// Words that make a recording a voice rather than an effect.
const VOICE_MIN_WORDS: usize = 3;
const MAX_LABELS: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum AssetsError {
    #[error("no video clips in {0}")]
    NoClips(PathBuf),
    #[error("no music or sound effect files in {0}")]
    NoSounds(PathBuf),
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

fn has_ext(p: &Path, exts: &[&str]) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default();
    !name.starts_with('.') && exts.contains(&ext.to_lowercase().as_str())
}

/// Audio files in `dir` and in every subfolder below it (so effects can be
/// sorted like `assets/sounds/memes/`), skipping hidden files and folders,
/// with their path inside `dir`, ordered by that path.
pub fn find_sounds(dir: &Path) -> Result<Vec<(PathBuf, String)>, AssetsError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(d) = folders.pop() {
        for p in fs::read_dir(&d)
            .map_err(io_err(&d))?
            .filter_map(Result::ok)
            .map(|e| e.path())
        {
            let hidden = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            if hidden {
                continue;
            }
            if p.is_dir() {
                folders.push(p);
            } else if p.is_file() && has_ext(&p, &AUDIO_EXTENSIONS) {
                found.push(p);
            }
        }
    }
    let mut out: Vec<(PathBuf, String)> = found
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(dir)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (p, rel)
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

/// A first guess at what a sound is for. The agent decides how to use it.
pub fn guess_role(seconds: f64, beat_confidence: f64, words: usize) -> &'static str {
    let steady = beat_confidence >= STEADY_BEAT && seconds >= MIN_BEAT_SECONDS;
    if seconds >= MUSIC_MIN_SECONDS && steady {
        "music"
    } else if words >= VOICE_MIN_WORDS {
        "voice"
    } else if seconds >= MUSIC_MIN_SECONDS || steady {
        "music"
    } else {
        "effect"
    }
}

/// Sound names heard most often first, each once, at most five.
pub fn top_labels(sounds: &[model::SoundEvent]) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for s in sounds {
        match counts.iter_mut().find(|(l, _)| *l == s.label) {
            Some((_, n)) => *n += 1,
            None => counts.push((s.label.clone(), 1)),
        }
    }
    // Stable: equal counts keep the order they were first heard in.
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    counts
        .into_iter()
        .take(MAX_LABELS)
        .map(|(l, _)| l)
        .collect()
}

/// Mean power in dBFS; `None` for silence.
pub fn level_db(samples: &[f32]) -> Option<f64> {
    let power =
        samples.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / samples.len().max(1) as f64;
    (power > 1e-10).then(|| 10.0 * power.log10())
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

/// The extra folders to read sounds from: `given` when the user named
/// some; none when the assets folder has sounds of its own; otherwise asks
/// once. No answer (Enter, or nobody there to answer) means no sounds,
/// which is fine; a named folder without audio files is an error.
pub fn resolve_sound_dirs(
    folder: &Path,
    given: &[PathBuf],
    ask: impl FnOnce(&str) -> Option<PathBuf>,
) -> Result<Vec<PathBuf>, AssetsError> {
    if !given.is_empty() {
        return Ok(given.to_vec());
    }
    if !find_sounds(folder)?.is_empty() {
        return Ok(Vec::new());
    }
    let question = format!(
        "No music or sound effects in {}. Which folder has them? (Enter to skip)",
        folder.display()
    );
    match ask(&question) {
        Some(dir) if !find_sounds(&dir)?.is_empty() => Ok(vec![dir]),
        Some(dir) => Err(AssetsError::NoSounds(dir)),
        None => Ok(Vec::new()),
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
/// Sounds can also live outside the assets folder: `sound_dirs` are read
/// the same way, never changed.
pub fn run(run_dir: &Path, folder: &Path, sound_dirs: &[PathBuf]) -> Result<Assets, AssetsError> {
    // Absolute, because ffmpeg runs inside the run directory for the audio.
    let folder = &fs::canonicalize(folder).map_err(io_err(folder))?;
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

    let sounds = read_sounds(&dir, folder, sound_dirs)?;
    let assets = Assets {
        version: 1,
        folder: folder.display().to_string(),
        clips: out,
        sounds,
    };
    let file = dir.join(ASSETS_FILE);
    let json = serde_json::to_string_pretty(&assets).expect("Assets always serialize");
    fs::write(&file, json).map_err(io_err(&file))?;
    Ok(assets)
}

/// Measures every audio file the user added; see [`model::SoundAsset`].
/// Every sound to read: the assets folder's, named by their path inside
/// it, then each extra folder's, named by their full path so an editor can
/// import them from where they are.
pub fn sound_sources(
    folder: &Path,
    sound_dirs: &[PathBuf],
) -> Result<Vec<(PathBuf, String)>, AssetsError> {
    let mut all = find_sounds(folder)?;
    for d in sound_dirs {
        let d = fs::canonicalize(d).map_err(io_err(d))?;
        all.extend(
            find_sounds(&d)?
                .into_iter()
                .map(|(p, _)| (p.clone(), p.display().to_string())),
        );
    }
    Ok(all)
}

fn read_sounds(
    work_dir: &Path,
    folder: &Path,
    sound_dirs: &[PathBuf],
) -> Result<Vec<model::SoundAsset>, AssetsError> {
    let found = sound_sources(folder, sound_dirs)?;
    let named: Vec<(String, String)> = found
        .iter()
        .map(|(p, rel)| sha256(p).map(|h| (rel.clone(), h)))
        .collect::<Result<_, _>>()?;
    let dupes = duplicates(&named);
    let rate = model::Fps::new(30, 1).expect("30 fps is valid");
    let zero = model::RationalTime::from_frames(0, rate);
    let mut out = Vec::with_capacity(found.len());
    for (((path, _), (rel, hash)), duplicate_of) in found.iter().zip(named).zip(dupes) {
        let mut sound = model::SoundAsset {
            file: rel.clone(),
            sha256: hash,
            duplicate_of,
            seconds: 0.0,
            role: String::new(),
            bpm: None,
            level_db: None,
            labels: Vec::new(),
            language: String::new(),
            transcript: String::new(),
        };
        if sound.duplicate_of.is_some() {
            out.push(sound);
            continue;
        }
        eprintln!("assets: {rel}");
        let mut samples = audio::extract_16k_mono(work_dir, path_str(path))?;
        sound.seconds = samples.len() as f64 / 16_000.0;
        sound.level_db = level_db(&samples);
        let rhythm = audio::rhythm::analyze(&samples, 16_000);
        if rhythm.tempo.confidence >= STEADY_BEAT && sound.seconds >= MIN_BEAT_SECONDS {
            sound.bpm = Some((rhythm.tempo.bpm * 10.0).round() / 10.0);
        }
        let (words, language) = audio::transcribe(&samples, rate, zero)?;
        let (words, tags) = audio::words::split_sound_tags(words);
        // The sound model needs a second of audio; pad short effects.
        if samples.len() < 16_000 {
            samples.resize(16_000, 0.0);
        }
        let mut heard = tags;
        heard.extend(audio::name_sounds(&samples, rate, zero)?);
        sound.labels = top_labels(&heard);
        sound.role = guess_role(sound.seconds, rhythm.tempo.confidence, words.len()).to_string();
        if words.len() >= VOICE_MIN_WORDS {
            sound.transcript = audio::words::transcript(&words, &[]);
            sound.language = language;
        }
        out.push(sound);
    }
    Ok(out)
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
    fn audio_files_are_found_in_every_subfolder() {
        // Users sort effects by category: assets/sounds/memes/bruh.mp3.
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::create_dir_all(d.join("sounds/memes")).unwrap();
        fs::create_dir_all(d.join(".cache/x")).unwrap();
        fs::create_dir_all(d.join("music")).unwrap();
        for f in [
            "song.MP3",
            "clip.mp4",
            ".hidden.wav",
            "sounds/whoosh.wav",
            "sounds/notes.txt",
            "sounds/memes/bruh.mp3",
            ".cache/x/skip.wav",
            "music/bed.m4a",
        ] {
            fs::write(d.join(f), b"x").unwrap();
        }
        let got: Vec<String> = find_sounds(d)
            .unwrap()
            .into_iter()
            .map(|(_, rel)| rel)
            .collect();
        assert_eq!(
            got,
            [
                "music/bed.m4a",
                "song.MP3",
                "sounds/memes/bruh.mp3",
                "sounds/whoosh.wav"
            ]
        );
    }

    #[test]
    fn sounds_elsewhere_are_listed_by_full_path_after_the_assets_folder() {
        let assets = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(assets.path().join("song.mp3"), b"x").unwrap();
        fs::write(elsewhere.path().join("whoosh.wav"), b"x").unwrap();
        let got: Vec<String> = sound_sources(assets.path(), &[elsewhere.path().to_path_buf()])
            .unwrap()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        // The real path: on macOS the temp folder sits under /private.
        let far = fs::canonicalize(elsewhere.path())
            .unwrap()
            .join("whoosh.wav")
            .display()
            .to_string();
        assert_eq!(got, ["song.mp3".to_string(), far]);
    }

    #[test]
    fn sounds_in_the_assets_folder_need_no_question() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("song.mp3"), b"x").unwrap();
        let got = resolve_sound_dirs(dir.path(), &[], |_| panic!("asked")).unwrap();
        assert!(got.is_empty(), "the assets folder is always read");
    }

    #[test]
    fn folders_the_user_named_need_no_question() {
        let dir = tempfile::tempdir().unwrap();
        let named = vec![PathBuf::from("/somewhere/sfx")];
        let got = resolve_sound_dirs(dir.path(), &named, |_| panic!("asked")).unwrap();
        assert_eq!(got, named);
    }

    #[test]
    fn with_no_sounds_found_the_user_is_asked_once() {
        let assets = tempfile::tempdir().unwrap();
        let sfx = tempfile::tempdir().unwrap();
        fs::write(sfx.path().join("whoosh.wav"), b"x").unwrap();
        let mut asked = String::new();
        let got = resolve_sound_dirs(assets.path(), &[], |q| {
            asked = q.to_string();
            Some(sfx.path().to_path_buf())
        })
        .unwrap();
        assert_eq!(got, [sfx.path().to_path_buf()]);
        assert!(asked.contains("No music or sound effects"), "{asked}");
        assert!(asked.contains("Enter to skip"), "{asked}");
    }

    #[test]
    fn no_answer_means_no_sounds_and_no_error() {
        let assets = tempfile::tempdir().unwrap();
        let got = resolve_sound_dirs(assets.path(), &[], |_| None).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn an_answer_with_no_sounds_in_it_is_an_error() {
        let assets = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let err = resolve_sound_dirs(assets.path(), &[], |_| Some(empty.path().to_path_buf()))
            .unwrap_err();
        assert!(matches!(err, AssetsError::NoSounds(_)), "{err}");
    }

    #[test]
    fn a_sound_is_first_guessed_as_voice_music_or_effect() {
        assert_eq!(guess_role(12.0, 0.1, 9), "voice", "speech wins");
        assert_eq!(guess_role(95.0, 0.1, 0), "music", "long beds are music");
        assert_eq!(guess_role(8.0, 0.8, 0), "music", "a steady beat is music");
        assert_eq!(guess_role(1.2, 0.0, 0), "effect");
        assert_eq!(
            guess_role(6.0, 0.2, 1),
            "effect",
            "one stray word is not a voice"
        );
        assert_eq!(
            guess_role(180.0, 0.7, 40),
            "music",
            "a song with lyrics is music"
        );
        // A 0.6 s noise burst had a "confident" tempo: too short to mean it.
        assert_eq!(
            guess_role(0.6, 0.9, 0),
            "effect",
            "a beat needs time to repeat"
        );
    }

    #[test]
    fn the_most_heard_labels_come_first_once_each() {
        let r = model::Fps::new(30, 1).unwrap();
        let s = |label: &str| model::SoundEvent {
            label: label.into(),
            start: model::RationalTime::from_frames(0, r),
            duration: model::RationalTime::from_frames(1, r),
            source: "yamnet".into(),
        };
        let got = top_labels(&[
            s("whoosh"),
            s("thunk"),
            s("whoosh"),
            s("bell"),
            s("thunk"),
            s("whoosh"),
        ]);
        assert_eq!(got, ["whoosh", "thunk", "bell"]);
    }

    #[test]
    fn loudness_is_mean_power_in_dbfs() {
        let full: Vec<f32> = (0..1600)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert!((level_db(&full).unwrap() - 0.0).abs() < 0.01);
        let half: Vec<f32> = full.iter().map(|v| v * 0.5).collect();
        assert!((level_db(&half).unwrap() + 6.02).abs() < 0.05);
        assert_eq!(level_db(&vec![0.0; 100]), None, "silence has no level");
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
