//! Shared types for every file in a ViralCraft run directory.
//!
//! Times follow OpenTimelineIO: a [`RationalTime`] is a frame count at a
//! rational [`Fps`], and a [`TimeRange`] is a start plus a duration with an
//! exclusive end.

pub mod audio;
pub mod inspect;
pub mod judgement;
pub mod probe;
pub mod rebuild;
pub mod shots;
pub mod time;
pub mod tools;

pub use audio::{Audio, AudioEvent, MusicSpan, SoundEvent, Tempo, Word};
pub use inspect::{
    Animation, BoxN, CaptionEvent, Clearance, Ease, Inspect, InspectTiming, Keyframe, Look,
    LumaStats, OverlayEvent, ShotLook, ShotMotion, Swatch, Transform,
};
pub use judgement::{Judgement, Needs};
pub use probe::{AudioInfo, Probe, SafeZone};
pub use rebuild::{Asset, Assets, VoiceLine, Voiceover};
pub use shots::{Candidate, Segment, Shots, Transition, TransitionKind};
pub use time::{Fps, RationalTime, TimeError, TimeRange};
