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
    /// Stretches with a steady beat. v1 reports one span from the first to
    /// the last beat when tempo confidence is at least 0.5.
    pub music: Vec<TimeRange>,
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
