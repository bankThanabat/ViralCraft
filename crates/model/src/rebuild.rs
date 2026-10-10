//! `rebuild/assets.json`: the user's own clips, read for the optional
//! rebuild step. Times are on each clip's own clock.

use serde::{Deserialize, Serialize};

use crate::{Fps, MusicSpan, SoundEvent, Word};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assets {
    pub version: u32,
    /// The folder the clips were read from.
    pub folder: String,
    pub clips: Vec<Asset>,
    /// Music, sound effects and recordings the user added, in the folder
    /// or any subfolder of it. Which to use where is the agent's choice.
    #[serde(default)]
    pub sounds: Vec<SoundAsset>,
}

/// One audio file the user added. Every field is measured; `role` is only
/// a first guess from length, beat and speech.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundAsset {
    /// Path inside the assets folder, e.g. `sounds/whoosh.wav`.
    pub file: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
    pub seconds: f64,
    /// `music`, `effect` or `voice`.
    pub role: String,
    /// Beats per minute when a steady beat was found.
    pub bpm: Option<f64>,
    /// Mean loudness in dBFS.
    pub level_db: Option<f64>,
    /// What the sound-effect model hears, most frequent first.
    pub labels: Vec<String>,
    pub language: String,
    /// What is said, for a recording with speech.
    pub transcript: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// File name inside `folder`.
    pub file: String,
    pub sha256: String,
    /// Set when this file has the same bytes as an earlier clip, which is
    /// then the one to use. Nothing else is measured for a duplicate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
    pub seconds: f64,
    pub rate: Fps,
    pub frames: i64,
    pub width: u32,
    pub height: u32,
    pub aspect: String,
    pub has_audio: bool,
    /// Spoken language, empty when there is no speech or no audio.
    pub language: String,
    /// Words and named sounds in time order, sounds in brackets.
    pub transcript: String,
    pub words: Vec<Word>,
    pub sounds: Vec<SoundEvent>,
    /// Stretches with music or another continuous background bed.
    pub music: Vec<MusicSpan>,
    /// A strip of four frames across the clip, relative to the run directory.
    pub thumbnail: String,
}

/// `rebuild/voiceover.json`: an AI voice reading the user's approved
/// script, one audio file per caption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Voiceover {
    pub version: u32,
    /// `say` (the system's own voices), `openai` or `elevenlabs`.
    pub engine: String,
    pub voice: String,
    /// The provider's model; empty for `say`.
    pub model: String,
    pub lines: Vec<VoiceLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceLine {
    /// The caption this line is spoken under.
    pub caption: String,
    pub text: String,
    /// Relative to the run directory.
    pub file: String,
    /// Frame the line starts at: its caption's start.
    pub start: i64,
    pub seconds: f64,
    /// Frames until the next caption starts (or the video ends).
    pub room: i64,
    /// Whether the line ends before the next caption starts.
    pub fits: bool,
}
