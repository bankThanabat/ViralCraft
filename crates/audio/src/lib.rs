//! `audio`: words with timing from whisper, plus beats, tempo and onsets.

pub mod bed;
pub mod rhythm;
pub mod sfx;
pub mod words;

use std::fs;
use std::path::{Path, PathBuf};

use model::tools::{self, ToolError};
use model::{
    Audio, AudioEvent, Fps, MusicSpan, Probe, RationalTime, SoundEvent, Tempo, TimeRange, Word,
};
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

pub const AUDIO_FILE: &str = "audio.json";
pub const WHISPER_MODEL: &str = "ggml-tiny.en.bin";
/// Optional sound-effect classifier; see `doctor::MODELS`.
pub const SFX_MODEL: &str = "yamnet.onnx";
/// Words closer than this belong to one speech span.
const SPEECH_GAP_S: f64 = 0.5;
/// Tempo confidence needed to call a stretch music.
const MUSIC_CONFIDENCE: f64 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
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
    #[error("{path}")]
    Wav { path: PathBuf, source: hound::Error },
    #[error("{0} must be 16 kHz mono 16-bit PCM")]
    WavFormat(PathBuf),
    #[error("whisper model {0} is missing or unreadable; run `viralcraft doctor`")]
    Model(PathBuf),
    #[error("whisper: {0}")]
    Whisper(#[from] whisper_rs::WhisperError),
    #[error("sound-effect model: {0}")]
    Sfx(String),
    #[error(transparent)]
    Tool(#[from] ToolError),
}

/// Reads `probe.json`, transcribes the source's audio, writes `audio.json`.
pub fn run(run_dir: &Path) -> Result<Audio, AudioError> {
    let probe: Probe = read_json(&run_dir.join(ingest::PROBE_FILE))?;
    let rate = probe.rate;
    let (words, language, rhythm, beds, named) = match &probe.audio {
        None => (
            Vec::new(),
            String::new(),
            rhythm::analyze(&[], 16_000),
            Vec::new(),
            Vec::new(),
        ),
        Some(info) => {
            let samples = extract_16k_mono(run_dir, &probe.file)?;
            let (words, language) = transcribe(&samples, rate, info.offset)?;
            (
                words,
                language,
                rhythm::analyze(&samples, 16_000),
                music_beds(&samples, rate, info.offset, probe.frames),
                name_sounds(&samples, rate, info.offset)?,
            )
        }
    };
    let (words, mut sounds) = words::split_sound_tags(words);
    sounds.extend(named);
    sounds.sort_by_key(|s| s.start.value);
    let offset_s = probe.audio.as_ref().map_or(0.0, |a| a.offset.to_seconds());
    let at = |s: f64| RationalTime::from_seconds(s + offset_s, rate);
    let event = |e: &rhythm::Event| AudioEvent {
        time: at(e.time),
        confidence: e.confidence,
    };
    let beats: Vec<AudioEvent> = rhythm.beats.iter().map(event).collect();

    let beat_span = match (beats.first(), beats.last()) {
        (Some(first), Some(last)) if rhythm.tempo.confidence >= MUSIC_CONFIDENCE => {
            let period = RationalTime::from_seconds(60.0 / rhythm.tempo.bpm, rate).value;
            Some((
                first.time.value,
                (last.time.value + period).min(probe.frames),
            ))
        }
        _ => None,
    };
    let mut music = beds;
    for m in &mut music {
        let (start, end) = (m.range.start.value, m.range.end_frame());
        m.beat = beat_span.is_some_and(|(s, e)| s < end && e > start);
    }
    if let Some((s, e)) = beat_span {
        if !music.iter().any(|m| m.beat) {
            music.push(MusicSpan {
                range: TimeRange::from_frames(s, e - s, rate),
                level_db: None,
                beat: true,
            });
        }
    }

    let audio = Audio {
        version: 1,
        rate,
        speech: words::speech_spans(&words, SPEECH_GAP_S),
        transcript: words::transcript(&words, &sounds),
        language,
        words,
        tempo: Tempo {
            bpm: rhythm.tempo.bpm,
            confidence: rhythm.tempo.confidence,
        },
        beats,
        onsets: rhythm.onsets.iter().map(event).collect(),
        music,
        sounds,
    };
    let out = run_dir.join(AUDIO_FILE);
    let json = serde_json::to_string_pretty(&audio).expect("Audio always serializes");
    fs::write(&out, json).map_err(|source| AudioError::Io { path: out, source })?;
    Ok(audio)
}

/// Multilingual whisper models, biggest first, with their DTW presets.
const MULTILINGUAL: [&str; 3] = ["ggml-large-v3.bin", "ggml-medium.bin", "ggml-small.bin"];

/// Which whisper model to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    pub file: &'static str,
    /// Multilingual models detect the language; the default is English only.
    pub multilingual: bool,
}

/// The biggest installed multilingual model, else the English-only one.
pub fn pick_model(models_dir: &Path) -> ModelChoice {
    MULTILINGUAL
        .iter()
        .find(|f| models_dir.join(f).exists())
        .map_or(
            ModelChoice {
                file: WHISPER_MODEL,
                multilingual: false,
            },
            |f| ModelChoice {
                file: f,
                multilingual: true,
            },
        )
}

fn dtw_preset(file: &str) -> DtwModelPreset {
    match file {
        "ggml-large-v3.bin" => DtwModelPreset::LargeV3,
        "ggml-medium.bin" => DtwModelPreset::Medium,
        "ggml-small.bin" => DtwModelPreset::Small,
        _ => DtwModelPreset::TinyEn,
    }
}

/// Whether to transcribe again with the English-only model. Its word
/// timing is calibrated against ground truth; the multilingual models start
/// words up to six frames early on the same English fixture.
pub fn rerun_in_english(choice: &ModelChoice, language: &str, english_installed: bool) -> bool {
    choice.multilingual && language == "en" && english_installed
}

/// Transcribes 16 kHz mono samples and returns the words and the language
/// (`en` with the English-only model; detected with a multilingual one).
pub fn transcribe(
    samples: &[f32],
    rate: Fps,
    offset: RationalTime,
) -> Result<(Vec<Word>, String), AudioError> {
    whisper_rs::install_logging_hooks();
    let models_dir = doctor::default_models_dir();
    let mut choice = pick_model(&models_dir);
    let (mut tokens, language) = whisper(samples, &models_dir, &choice)?;
    if rerun_in_english(&choice, &language, models_dir.join(WHISPER_MODEL).exists()) {
        choice = ModelChoice {
            file: WHISPER_MODEL,
            multilingual: false,
        };
        tokens = whisper(samples, &models_dir, &choice)?.0;
    }
    Ok((words::tokens_to_words(&tokens, rate, offset), language))
}

fn whisper(
    samples: &[f32],
    models_dir: &Path,
    choice: &ModelChoice,
) -> Result<(Vec<words::RawToken>, String), AudioError> {
    let model_path = models_dir.join(choice.file);
    let model = model_path
        .to_str()
        .filter(|_| model_path.exists())
        .ok_or_else(|| AudioError::Model(model_path.clone()))?;

    let mut ctx_params = WhisperContextParameters::default();
    ctx_params.dtw_parameters(DtwParameters {
        mode: DtwMode::ModelPreset {
            model_preset: dtw_preset(choice.file),
        },
        ..DtwParameters::default()
    });
    let ctx = WhisperContext::new_with_params(model, ctx_params)?;
    let eot = ctx.token_eot();
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(if choice.multilingual { "auto" } else { "en" }));
    params.set_token_timestamps(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    state.full(params, samples)?;

    // Whisper invents text over silence and noise; such segments are
    // dropped (see `words::heard`).
    let mut segments: Vec<(String, f32, Vec<words::RawToken>)> = Vec::new();
    for segment in state.as_iter() {
        let mut seg_tokens = Vec::new();
        for i in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(i) else {
                continue;
            };
            let data = token.token_data();
            seg_tokens.push(words::RawToken {
                bytes: token.to_bytes()?.to_vec(),
                t0: data.t0,
                t1: data.t1,
                t_dtw: data.t_dtw,
                p: data.p,
                special: token.token_id() >= eot,
            });
        }
        let mean_p = seg_tokens.iter().map(|t| t.p).sum::<f32>() / seg_tokens.len().max(1) as f32;
        let text = segment.to_str_lossy()?.into_owned();
        segments.push((text, mean_p, seg_tokens));
    }
    let keep = words::heard(
        &segments
            .iter()
            .map(|(t, p, _)| (t.clone(), *p))
            .collect::<Vec<_>>(),
    );
    let tokens: Vec<words::RawToken> = segments
        .into_iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .flat_map(|((_, _, t), _)| t)
        .collect();
    let language = if choice.multilingual {
        whisper_rs::get_lang_str(state.full_lang_id_from_state())
            .unwrap_or("unknown")
            .to_string()
    } else {
        "en".to_string()
    };
    Ok((tokens, language))
}

/// Names non-speech sounds with YAMNet when its optional model is
/// installed; returns nothing when it is not.
pub fn name_sounds(
    samples: &[f32],
    rate: Fps,
    offset: RationalTime,
) -> Result<Vec<SoundEvent>, AudioError> {
    let path = doctor::default_models_dir().join(SFX_MODEL);
    if !path.exists() || samples.len() < 16_000 {
        return Ok(Vec::new());
    }
    let model = rten::Model::load_file(&path).map_err(|e| AudioError::Sfx(e.to_string()))?;
    let input: Tensor<f32> = Tensor::from_data(&[samples.len()], samples.to_vec());
    let output = model
        .run_one(input.view().into(), None)
        .map_err(|e| AudioError::Sfx(e.to_string()))?;
    let scores: NdTensor<f32, 2> = output
        .try_into()
        .map_err(|e| AudioError::Sfx(format!("unexpected output: {e:?}")))?;
    let [frames, classes] = scores.shape();
    let rows: Vec<Vec<f32>> = (0..frames)
        .map(|f| (0..classes).map(|c| scores[[f, c]]).collect())
        .collect();

    let offset_s = offset.to_seconds();
    Ok(sfx::name_sounds(&rows, &sfx::class_names(), sfx::HOP_S)
        .into_iter()
        .map(|n| {
            let start = RationalTime::from_seconds(n.start + offset_s, rate);
            let end = RationalTime::from_seconds(n.end + offset_s, rate);
            SoundEvent {
                label: n.label,
                start,
                duration: RationalTime::from_frames(end.value - start.value, rate),
                source: "yamnet".to_string(),
            }
        })
        .collect())
}

/// Reads a 16 kHz mono 16-bit WAV into `[-1, 1]` samples.
pub fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>, AudioError> {
    let wav_err = |source| AudioError::Wav {
        path: path.to_path_buf(),
        source,
    };
    let reader = hound::WavReader::open(path).map_err(wav_err)?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err(AudioError::WavFormat(path.to_path_buf()));
    }
    reader
        .into_samples::<i16>()
        .map(|s| s.map(|v| f32::from(v) / f32::from(i16::MAX)))
        .collect::<Result<_, _>>()
        .map_err(wav_err)
}

/// Background beds (music or another continuous bed) on the video clock at
/// `rate`, cut at `frames`. Beat flags are left off.
pub fn music_beds(samples: &[f32], rate: Fps, offset: RationalTime, frames: i64) -> Vec<MusicSpan> {
    let at = |s: f64| RationalTime::from_seconds(s + offset.to_seconds(), rate);
    bed::bed_spans(samples, 16_000)
        .iter()
        .map(|b| {
            let (start, end) = (at(b.start).value, at(b.end).value.min(frames));
            MusicSpan {
                range: TimeRange::from_frames(start, end - start, rate),
                level_db: Some(b.level_db),
                beat: false,
            }
        })
        .collect()
}

/// Decodes the first audio stream of `source` (a path relative to
/// `work_dir`, or absolute) to 16 kHz mono via a temporary WAV there.
pub fn extract_16k_mono(work_dir: &Path, source: &str) -> Result<Vec<f32>, AudioError> {
    const TMP: &str = "audio16k.wav";
    tools::run_in(
        work_dir,
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-i",
            source,
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            TMP,
        ],
    )?;
    let path = work_dir.join(TMP);
    let samples = read_wav_16k_mono(&path);
    let _ = fs::remove_file(&path);
    samples
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AudioError> {
    let text = fs::read_to_string(path).map_err(|source| AudioError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| AudioError::Json {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for f in files {
            std::fs::write(dir.path().join(f), b"").unwrap();
        }
        dir
    }

    #[test]
    fn without_a_multilingual_model_english_is_used() {
        let dir = dir_with(&["ggml-tiny.en.bin"]);
        let choice = pick_model(dir.path());
        assert_eq!(choice.file, "ggml-tiny.en.bin");
        assert!(!choice.multilingual);
    }

    #[test]
    fn english_detected_by_a_multilingual_model_reruns_on_the_english_model() {
        let small = ModelChoice {
            file: "ggml-small.bin",
            multilingual: true,
        };
        let tiny = ModelChoice {
            file: WHISPER_MODEL,
            multilingual: false,
        };
        assert!(rerun_in_english(&small, "en", true));
        assert!(!rerun_in_english(&small, "th", true));
        assert!(
            !rerun_in_english(&small, "en", false),
            "nothing to rerun with"
        );
        assert!(!rerun_in_english(&tiny, "en", true), "already English-only");
    }

    #[test]
    fn the_biggest_multilingual_model_installed_wins() {
        let dir = dir_with(&["ggml-tiny.en.bin", "ggml-small.bin", "ggml-medium.bin"]);
        let choice = pick_model(dir.path());
        assert_eq!(choice.file, "ggml-medium.bin");
        assert!(choice.multilingual);
        let dir = dir_with(&["ggml-tiny.en.bin", "ggml-small.bin", "ggml-large-v3.bin"]);
        assert_eq!(pick_model(dir.path()).file, "ggml-large-v3.bin");
    }
}
