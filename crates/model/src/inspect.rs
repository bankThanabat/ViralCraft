//! `inspect.json`: caption events and other per-frame measurements.

use serde::{Deserialize, Serialize};

use crate::{Fps, TimeRange};

/// A box in normalized frame coordinates (0..1, origin top-left), by centre.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxN {
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
}

impl BoxN {
    pub fn left(&self) -> f64 {
        self.cx - self.w / 2.0
    }
    pub fn right(&self) -> f64 {
        self.cx + self.w / 2.0
    }
    pub fn top(&self) -> f64 {
        self.cy - self.h / 2.0
    }
    pub fn bottom(&self) -> f64 {
        self.cy + self.h / 2.0
    }

    pub fn from_edges(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        BoxN {
            cx: (left + right) / 2.0,
            cy: (top + bottom) / 2.0,
            w: right - left,
            h: bottom - top,
        }
    }

    pub fn iou(&self, other: &BoxN) -> f64 {
        let w = (self.right().min(other.right()) - self.left().max(other.left())).max(0.0);
        let h = (self.bottom().min(other.bottom()) - self.top().max(other.top())).max(0.0);
        let inter = w * h;
        let union = self.w * self.h + other.w * other.h - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }

    pub fn union(&self, other: &BoxN) -> BoxN {
        BoxN::from_edges(
            self.left().min(other.left()),
            self.top().min(other.top()),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Animation {
    /// Scales up from `from_scale` percent to 100 over `frames` frames.
    PopIn { frames: u64, from_scale: f64 },
    /// Moves in from an offset (normalized) over `frames` frames.
    Slide { frames: u64, dx: f64, dy: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inspect {
    pub version: u32,
    pub rate: Fps,
    pub captions: Vec<CaptionEvent>,
    /// One entry per shot, in shot order.
    pub motion: Vec<ShotMotion>,
    /// Stickers, emoji and inset images that appear mid-shot.
    pub overlays: Vec<OverlayEvent>,
    /// Look of the whole video.
    pub look: Look,
    /// Look of each shot, in shot order.
    pub shot_looks: Vec<ShotLook>,
    pub timing: InspectTiming,
}

/// Lottie bezier handle, both coordinates 0..1 in time and progress.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ease {
    pub x: f64,
    pub y: f64,
}

/// A Lottie keyframe: value `s` at frame `t`. `o` and `i` shape the segment
/// from this keyframe to the next; absent on the last keyframe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T> {
    pub t: i64,
    pub s: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub o: Option<Ease>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i: Option<Ease>,
}

/// Apparent camera transform over a shot in Lottie terms: `a` anchor and
/// `p` position in normalized frame units, `s` scale in percent. Rotation
/// is not measured in v1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub anchor: [f64; 2],
    pub scale: Vec<Keyframe<f64>>,
    pub position: Vec<Keyframe<[f64; 2]>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotMotion {
    pub shot: u32,
    /// Median phase-correlation peak, 0..1.
    pub confidence: f64,
    /// `None` (written `unknown`) when confidence is too low to trust.
    pub transform: Option<Transform>,
}

/// How long text detection took, so the per-frame budget can be checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectTiming {
    pub frames: u64,
    pub detect_ms_per_frame: f64,
}

/// One on-screen caption, possibly several stacked lines. Style fields use
/// the ASS vocabulary; anything OCR cannot see is `None` (written `unknown`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionEvent {
    /// `c1`, `c2`, ... in time order.
    pub id: String,
    pub range: TimeRange,
    /// Lines joined with `\n`.
    pub text: String,
    pub lines: Vec<String>,
    pub bounds: BoxN,
    pub line_boxes: Vec<BoxN>,
    /// ASS numpad alignment, `an1` to `an9`.
    pub anchor: String,
    pub fill: String,
    pub outline: Option<String>,
    /// Solid box behind the text (ASS BorderStyle 3), if any.
    pub box_color: Option<String>,
    /// Line height as a fraction of frame height.
    pub line_height: f64,
    /// Font size estimate in source pixels, from line height.
    pub font_px_estimate: f64,
    pub uppercase: bool,
    pub animation: Option<Animation>,
    /// Distance from each platform UI band, from `probe.json`'s profile.
    pub safe_zone: Clearance,
}

/// A region that appears during a shot and holds still while the camera
/// moves. What it is (emoji, sticker, image) is a judgement call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayEvent {
    /// `o1`, `o2`, ... in time order.
    pub id: String,
    pub range: TimeRange,
    pub bounds: BoxN,
    /// ASS numpad alignment, as for captions.
    pub anchor: String,
    pub safe_zone: Clearance,
}

/// Distance from a box edge to the matching safe-zone band, in normalized
/// units. Negative means the box reaches under the platform UI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Clearance {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
    pub inside: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    /// `#RRGGBB`, sRGB.
    pub hex: String,
    /// Fraction of sampled pixels, 0..1.
    pub share: f64,
}

/// Rec.709 luma, 0..1. `min` and `max` are the 1st and 99th percentiles so
/// a few stray pixels do not set them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LumaStats {
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

/// Measured output appearance. Not the grade the editor applied; see the
/// `look` section of the edit spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Look {
    /// Up to three, largest share first.
    pub dominant: Vec<Swatch>,
    pub luma: LumaStats,
    /// Mean HSV saturation, 0..1.
    pub saturation: f64,
    /// Mean of (R - B) / 255; positive is warm.
    pub warmth: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotLook {
    pub shot: u32,
    pub look: Look,
}
