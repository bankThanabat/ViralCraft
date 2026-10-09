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
    pub timing: InspectTiming,
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
    /// Line height as a fraction of frame height.
    pub line_height: f64,
    /// Font size estimate in source pixels, from line height.
    pub font_px_estimate: f64,
    pub uppercase: bool,
    pub animation: Option<Animation>,
}
