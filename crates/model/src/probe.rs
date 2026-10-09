//! `probe.json`: what `ingest` learned about the normalized source.

use serde::{Deserialize, Serialize};

use crate::{Fps, RationalTime};

/// Contents of `probe.json`. Describes `source.cfr.mp4`, not the original.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Probe {
    pub version: u32,
    /// The file the user supplied.
    pub original: String,
    /// The normalized file every later stage reads, relative to the run dir.
    pub file: String,
    pub rate: Fps,
    pub frames: i64,
    pub width: u32,
    pub height: u32,
    /// Reduced ratio such as `9:16`.
    pub aspect: String,
    /// Whether the original was variable frame rate and got re-encoded.
    pub was_vfr: bool,
    pub audio: Option<AudioInfo>,
    pub safe_zone: SafeZone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u32,
    /// Audio start relative to video frame 0; negative when audio leads.
    pub offset: RationalTime,
}

/// Bands the platform UI covers, as fractions of frame width or height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafeZone {
    pub profile: String,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Deserialize)]
struct PixelProfile {
    name: String,
    width: f64,
    height: f64,
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
}

/// Measured from platform overlay files; TikTok publishes no numbers.
const PROFILES: &str = include_str!("../safe-zones.json");

pub const DEFAULT_SAFE_ZONE: &str = "tiktok-2026-conservative";

impl SafeZone {
    pub fn profile(name: &str) -> Option<SafeZone> {
        let profiles: Vec<PixelProfile> =
            serde_json::from_str(PROFILES).expect("safe-zones.json is checked by a unit test");
        profiles
            .into_iter()
            .find(|p| p.name == name)
            .map(|p| SafeZone {
                profile: p.name,
                top: p.top / p.height,
                right: p.right / p.width,
                bottom: p.bottom / p.height,
                left: p.left / p.width,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_safe_zone_profile_is_normalized_from_1080x1920_pixels() {
        let z = SafeZone::profile("tiktok-2026-conservative").unwrap();
        assert!((z.top - 150.0 / 1920.0).abs() < 1e-9);
        assert!((z.right - 165.0 / 1080.0).abs() < 1e-9);
        assert!((z.bottom - 320.0 / 1920.0).abs() < 1e-9);
        assert!((z.left - 60.0 / 1080.0).abs() < 1e-9);
        assert_eq!(z.profile, "tiktok-2026-conservative");
    }

    #[test]
    fn an_unknown_profile_is_none() {
        assert!(SafeZone::profile("myspace-2006").is_none());
    }
}
