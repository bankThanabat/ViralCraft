//! `audio.json`: words, speech spans, and (from task 7) beats and tempo.

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
