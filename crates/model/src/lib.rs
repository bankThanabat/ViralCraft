//! Shared types for every file in a ViralCraft run directory.
//!
//! Times follow OpenTimelineIO: a [`RationalTime`] is a frame count at a
//! rational [`Fps`], and a [`TimeRange`] is a start plus a duration with an
//! exclusive end.

pub mod probe;
pub mod time;
pub mod tools;

pub use probe::{AudioInfo, Probe, SafeZone};
pub use time::{Fps, RationalTime, TimeError, TimeRange};
