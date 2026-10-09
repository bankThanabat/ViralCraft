//! `audio.json`: words and speech spans, plus tempo, beats, onsets and
//! music spans in the shape of the JAMS `tempo`, `beat` and `onset`
//! namespaces.

use serde::{Deserialize, Serialize};

use crate::{Fps, RationalTime, TimeRange};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub version: u32,
    pub rate: Fps,
    /// Transcribed words on the video clock.
    pub words: Vec<Word>,
    /// Runs of words with gaps under half a second.
    pub speech: Vec<TimeRange>,
    pub tempo: Tempo,
    /// Empty when tempo confidence is below 0.2.
    pub beats: Vec<AudioEvent>,
    pub onsets: Vec<AudioEvent>,
    /// Stretches with music or another continuous background bed.
    pub music: Vec<MusicSpan>,
    /// Labelled non-speech sounds, such as whisper's `[sighs]`.
    pub sounds: Vec<SoundEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MusicSpan {
    pub range: TimeRange,
    /// Level of the bed alone in dBFS; `None` when only a beat was found.
    pub level_db: Option<f64>,
    /// Whether a steady beat (tempo confidence at least 0.5) runs here.
    pub beat: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundEvent {
    /// What the sound is, e.g. `sighs`.
    pub label: String,
    pub start: RationalTime,
    pub duration: RationalTime,
    /// Which detector named it, e.g. `whisper`.
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tempo {
    pub bpm: f64,
    /// 0..1; normalized autocorrelation at the chosen period.
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioEvent {
    pub time: RationalTime,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    /// As spoken, with attached punctuation.
    pub text: String,
    pub start: RationalTime,
    pub duration: RationalTime,
    /// Mean token probability; 0.3 when timing fell back to coarse stamps.
    pub confidence: f32,
}
