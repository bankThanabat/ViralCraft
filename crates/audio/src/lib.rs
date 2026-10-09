//! `audio`: words with timing from whisper, plus beats, tempo and onsets.

pub mod words;

use std::fs;
use std::path::{Path, PathBuf};

use model::tools::{self, ToolError};
use model::{Audio, Fps, Probe, RationalTime, Word};
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

pub const AUDIO_FILE: &str = "audio.json";
pub const WHISPER_MODEL: &str = "ggml-tiny.en.bin";
/// Words closer than this belong to one speech span.
const SPEECH_GAP_S: f64 = 0.5;

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
    #[error(transparent)]
    Tool(#[from] ToolError),
}

/// Reads `probe.json`, transcribes the source's audio, writes `audio.json`.
pub fn run(run_dir: &Path) -> Result<Audio, AudioError> {
    let probe: Probe = read_json(&run_dir.join(ingest::PROBE_FILE))?;
    let words = match &probe.audio {
        None => Vec::new(),
        Some(info) => {
            let samples = extract_16k_mono(run_dir, &probe.file)?;
            transcribe(&samples, probe.rate, info.offset)?
        }
    };
    let audio = Audio {
        version: 1,
        rate: probe.rate,
        speech: words::speech_spans(&words, SPEECH_GAP_S),
        words,
    };
    let out = run_dir.join(AUDIO_FILE);
    let json = serde_json::to_string_pretty(&audio).expect("Audio always serializes");
    fs::write(&out, json).map_err(|source| AudioError::Io { path: out, source })?;
    Ok(audio)
}

/// Transcribes 16 kHz mono samples. Word times land on the video clock at
/// `rate`, shifted by the audio `offset` from `probe.json`.
pub fn transcribe(
    samples: &[f32],
    rate: Fps,
    offset: RationalTime,
) -> Result<Vec<Word>, AudioError> {
    whisper_rs::install_logging_hooks();
    let model_path = doctor::default_models_dir().join(WHISPER_MODEL);
    let model = model_path
        .to_str()
        .filter(|_| model_path.exists())
        .ok_or_else(|| AudioError::Model(model_path.clone()))?;

    let mut ctx_params = WhisperContextParameters::default();
    ctx_params.dtw_parameters(DtwParameters {
        mode: DtwMode::ModelPreset {
            model_preset: DtwModelPreset::TinyEn,
        },
        ..DtwParameters::default()
    });
    let ctx = WhisperContext::new_with_params(model, ctx_params)?;
    let eot = ctx.token_eot();
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_token_timestamps(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    state.full(params, samples)?;

    let mut tokens = Vec::new();
    for segment in state.as_iter() {
        for i in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(i) else {
                continue;
            };
            let data = token.token_data();
            tokens.push(words::RawToken {
                text: token.to_str_lossy()?.into_owned(),
                t0: data.t0,
                t1: data.t1,
                t_dtw: data.t_dtw,
                p: data.p,
                special: token.token_id() >= eot,
            });
        }
    }
    Ok(words::tokens_to_words(&tokens, rate, offset))
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

/// Decodes the source's first audio stream to 16 kHz mono via a temporary
/// WAV in the run directory.
fn extract_16k_mono(run_dir: &Path, source: &str) -> Result<Vec<f32>, AudioError> {
    const TMP: &str = "audio16k.wav";
    tools::run_in(
        run_dir,
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
    let path = run_dir.join(TMP);
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
