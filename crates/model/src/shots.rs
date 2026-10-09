//! `shots.json`: shot boundaries and transition candidates.

use serde::{Deserialize, Serialize};

use crate::{Fps, TimeRange};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shots {
    pub version: u32,
    pub rate: Fps,
    pub frames: i64,
    /// scdet threshold on its 0..100 scale that a hard cut must reach.
    pub threshold: f64,
    pub segments: Vec<Segment>,
    pub transitions: Vec<Transition>,
    /// Every frame whose score reached 1.0, so a reviewer can promote a
    /// missed jump cut or demote a false one.
    pub candidates: Vec<Candidate>,
}

/// One shot. `n` is 1-based and stable across every file in the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub n: u32,
    pub range: TimeRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionKind {
    Dissolve,
    Flash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    pub range: TimeRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub frame: u64,
    pub score: f64,
    pub luma: f64,
    /// True when this frame became a hard-cut boundary.
    pub accepted: bool,
}
