use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Why a frame rate or time value was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimeError {
    #[error("frame rate has a zero numerator or denominator")]
    ZeroRate,
    #[error("cannot parse frame rate {0:?}")]
    BadRate(String),
}

/// A frame rate as an exact fraction, e.g. `30000/1001` rather than `29.97`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawFps")]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

#[derive(Deserialize)]
struct RawFps {
    num: u32,
    den: u32,
}

impl TryFrom<RawFps> for Fps {
    type Error = TimeError;

    fn try_from(raw: RawFps) -> Result<Self, Self::Error> {
        Fps::new(raw.num, raw.den)
    }
}

impl Fps {
    pub fn new(num: u32, den: u32) -> Result<Self, TimeError> {
        if num == 0 || den == 0 {
            return Err(TimeError::ZeroRate);
        }
        Ok(Fps { num, den })
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

/// Parses the `r_frame_rate` / `avg_frame_rate` strings ffprobe prints:
/// `"30000/1001"` or a bare integer like `"30"`.
impl FromStr for Fps {
    type Err = TimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || TimeError::BadRate(s.to_string());
        let (num, den) = match s.split_once('/') {
            Some((n, d)) => (
                n.trim().parse().map_err(|_| bad())?,
                d.trim().parse().map_err(|_| bad())?,
            ),
            None => (s.trim().parse().map_err(|_| bad())?, 1),
        };
        Fps::new(num, den)
    }
}

impl fmt::Display for Fps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

/// A point in time as a whole frame count at a rate, like OTIO `RationalTime`.
///
/// Frames are signed so an audio stream that starts before video frame 0 can
/// carry a negative offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RationalTime {
    pub value: i64,
    pub rate: Fps,
}

impl RationalTime {
    pub fn from_frames(value: i64, rate: Fps) -> Self {
        RationalTime { value, rate }
    }

    /// Rounds to the nearest frame.
    pub fn from_seconds(seconds: f64, rate: Fps) -> Self {
        RationalTime {
            value: (seconds * rate.as_f64()).round() as i64,
            rate,
        }
    }

    pub fn to_seconds(self) -> f64 {
        self.value as f64 * f64::from(self.rate.den) / f64::from(self.rate.num)
    }
}

/// A start plus a duration at one rate, end exclusive, like OTIO `TimeRange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: RationalTime,
    pub duration: RationalTime,
}

impl TimeRange {
    pub fn from_frames(start: i64, duration: i64, rate: Fps) -> Self {
        TimeRange {
            start: RationalTime::from_frames(start, rate),
            duration: RationalTime::from_frames(duration, rate),
        }
    }

    /// First frame after the range.
    pub fn end_frame(&self) -> i64 {
        self.start.value + self.duration.value
    }

    pub fn contains_frame(&self, frame: i64) -> bool {
        frame >= self.start.value && frame < self.end_frame()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NTSC: Fps = Fps {
        num: 30000,
        den: 1001,
    };

    #[test]
    fn frames_to_seconds_uses_the_rational_rate() {
        let t = RationalTime::from_frames(54, NTSC);
        assert!((t.to_seconds() - 1.8018).abs() < 1e-6);
    }

    #[test]
    fn serializes_as_value_and_rational_rate() {
        let t = RationalTime::from_frames(54, NTSC);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#"{"value":54,"rate":{"num":30000,"den":1001}}"#);
        let back: RationalTime = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn time_range_end_is_exclusive() {
        let r = TimeRange::from_frames(0, 54, NTSC);
        assert!(r.contains_frame(0));
        assert!(r.contains_frame(53));
        assert!(!r.contains_frame(54));
        assert_eq!(r.end_frame(), 54);
    }

    #[test]
    fn fps_rejects_zero_terms() {
        assert_eq!(Fps::new(0, 1), Err(TimeError::ZeroRate));
        assert_eq!(Fps::new(30, 0), Err(TimeError::ZeroRate));
        assert_eq!(Fps::new(30, 1).unwrap().as_f64(), 30.0);
    }

    #[test]
    fn deserializing_a_zero_rate_fails() {
        let err = serde_json::from_str::<RationalTime>(r#"{"value":1,"rate":{"num":0,"den":1}}"#);
        assert!(err.is_err());
    }

    #[test]
    fn fps_parses_ffprobe_rate_strings() {
        assert_eq!("30000/1001".parse::<Fps>(), Ok(NTSC));
        assert_eq!("30".parse::<Fps>(), Ok(Fps { num: 30, den: 1 }));
        assert_eq!("0/0".parse::<Fps>(), Err(TimeError::ZeroRate));
        assert!(matches!("abc".parse::<Fps>(), Err(TimeError::BadRate(_))));
    }

    #[test]
    fn seconds_round_to_the_nearest_frame() {
        let fps = Fps::new(30, 1).unwrap();
        assert_eq!(RationalTime::from_seconds(1.8, fps).value, 54);
        assert_eq!(RationalTime::from_seconds(1.816, fps).value, 54);
        assert_eq!(RationalTime::from_seconds(1.817, fps).value, 55);
    }
}
