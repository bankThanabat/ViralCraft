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
