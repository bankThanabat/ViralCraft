//! Overlays (stickers, emoji, inset images): regions that appear on screen
//! during a shot and stay put while the camera moves.
//!
//! Each frame is predicted from the shot's first frame, warped by that
//! frame's measured camera transform. Pixels that differ, outside captions,
//! form regions; regions tracked for a while at a steady size and place are
//! overlays. Moving subjects change shape or position and drop out. An
//! overlay already on the first frame looks like scene and is not found.

use model::BoxN;

use crate::captions;
use crate::motion::{FrameFit, Plane};

/// Grey-level difference from the prediction that marks a pixel as new.
const DIFF: f32 = 40.0;
/// Smallest region, as a fraction of the frame.
const MIN_AREA: f64 = 0.002;
const MIN_FRAMES: usize = 6;
/// How much a region's size and centre may wander and still be one overlay.
const SIZE_WOBBLE: f64 = 0.25;
const CENTRE_WOBBLE: f64 = 0.03;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySpan {
    pub start: u64,
    /// Exclusive.
    pub end: u64,
    pub bounds: BoxN,
}

fn blur(p: &Plane) -> Plane {
    let mut data = vec![0.0; p.w * p.h];
    for y in 0..p.h {
        for x in 0..p.w {
            let mut sum = 0.0;
            let mut n = 0.0;
            for yy in y.saturating_sub(1)..(y + 2).min(p.h) {
                for xx in x.saturating_sub(1)..(x + 2).min(p.w) {
                    sum += p.data[yy * p.w + xx];
                    n += 1.0;
                }
            }
            data[y * p.w + x] = sum / n;
        }
    }
    Plane {
        w: p.w,
        h: p.h,
        data,
    }
}

/// The first frame as the camera would show it at `fit`.
fn predict(reference: &Plane, fit: &FrameFit) -> Plane {
    let (cx, cy) = (reference.w as f64 / 2.0, reference.h as f64 / 2.0);
    let (tx, ty) = (fit.dx * reference.w as f64, fit.dy * reference.h as f64);
    let mut data = Vec::with_capacity(reference.w * reference.h);
    for y in 0..reference.h {
        for x in 0..reference.w {
            let sx = cx + (x as f64 + 0.5 - cx - tx) / fit.scale - 0.5;
            let sy = cy + (y as f64 + 0.5 - cy - ty) / fit.scale - 0.5;
            data.push(reference.sample(sx, sy));
        }
    }
    Plane {
        w: reference.w,
        h: reference.h,
        data,
    }
}

/// Connected regions (4-neighbour) of `mask`, as normalized boxes.
fn regions(mask: &[bool], w: usize, h: usize) -> Vec<BoxN> {
    let min_area = ((w * h) as f64 * MIN_AREA).max(12.0) as usize;
    let mut seen = vec![false; mask.len()];
    let mut out = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1, mut area) = (w, h, 0, 0, 0usize);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            area += 1;
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            let mut push = |j: usize| {
                if mask[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x + 1 < w {
                push(i + 1);
            }
            if y > 0 {
                push(i - w);
            }
            if y + 1 < h {
                push(i + w);
            }
        }
        if area >= min_area {
            out.push(BoxN::from_edges(
                x0 as f64 / w as f64,
                y0 as f64 / h as f64,
                (x1 + 1) as f64 / w as f64,
                (y1 + 1) as f64 / h as f64,
            ));
        }
    }
    out
}

fn steady(boxes: &[BoxN]) -> Option<BoxN> {
    if boxes.len() < MIN_FRAMES {
        return None;
    }
    let mid = |f: fn(&BoxN) -> f64| {
        let mut v: Vec<f64> = boxes.iter().map(f).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let median = BoxN {
        cx: mid(|b| b.cx),
        cy: mid(|b| b.cy),
        w: mid(|b| b.w),
        h: mid(|b| b.h),
    };
    boxes
        .iter()
        .all(|b| {
            (b.w / median.w - 1.0).abs() <= SIZE_WOBBLE
                && (b.h / median.h - 1.0).abs() <= SIZE_WOBBLE
                && (b.cx - median.cx).hypot(b.cy - median.cy) <= CENTRE_WOBBLE
        })
        .then_some(median)
}

/// Overlays in one shot. `fits[i]` is frame `i`'s camera transform against
/// frame 0; `masks` (captions) are ignored; `start` numbers the frames.
pub fn detect(frames: &[Plane], fits: &[FrameFit], start: i64, masks: &[BoxN]) -> Vec<OverlaySpan> {
    let Some(first) = frames.first() else {
        return Vec::new();
    };
    let (w, h) = (first.w, first.h);
    let reference = blur(first);
    let masked: Vec<bool> = (0..w * h)
        .map(|i| {
            let (u, v) = (
                ((i % w) as f64 + 0.5) / w as f64,
                ((i / w) as f64 + 0.5) / h as f64,
            );
            let border = i % w < 2 || i % w >= w - 2 || i / w < 2 || i / w >= h - 2;
            border
                || masks
                    .iter()
                    .any(|b| u >= b.left() && u <= b.right() && v >= b.top() && v <= b.bottom())
        })
        .collect();

    let mut per_frame = vec![Vec::new()];
    for (frame, fit) in frames.iter().zip(fits).skip(1) {
        let predicted = predict(&reference, fit);
        let current = blur(frame);
        let changed: Vec<bool> = (0..w * h)
            .map(|i| !masked[i] && (current.data[i] - predicted.data[i]).abs() > DIFF)
            .collect();
        per_frame.push(regions(&changed, w, h));
    }

    captions::track(&per_frame)
        .into_iter()
        .filter_map(|t| {
            steady(&t.boxes).map(|bounds| OverlaySpan {
                start: (start + t.start as i64) as u64,
                end: (start + t.end() as i64) as u64,
                bounds,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::{estimate_shot, Plane};

    const W: usize = 64;
    const H: usize = 112;

    fn texture() -> Plane {
        let (w, h) = (W * 2, H * 2);
        let mut data = vec![60.0f32; w * h];
        let mut seed = 11u64;
        let mut next = |m: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize % m
        };
        for _ in 0..120 {
            let (x, y, r, v) = (next(w), next(h), 3 + next(10), 80 + next(140));
            for yy in y.saturating_sub(r)..(y + r).min(h) {
                for xx in x.saturating_sub(r)..(x + r).min(w) {
                    data[yy * w + xx] = v as f32;
                }
            }
        }
        Plane { w, h, data }
    }

    fn view(tex: &Plane, s: f64) -> Plane {
        let mut out = Plane {
            w: W,
            h: H,
            data: vec![0.0; W * H],
        };
        for y in 0..H {
            for x in 0..W {
                let u = (x as f64 + 0.5) / W as f64 - 0.5;
                let v = (y as f64 + 0.5) / H as f64 - 0.5;
                out.data[y * W + x] = tex.sample(
                    (0.5 + u / s) * tex.w as f64 - 0.5,
                    (0.5 + v / s) * tex.h as f64 - 0.5,
                );
            }
        }
        out
    }

    /// A bright 0.12 x 0.07 sticker centred at (0.75, 0.30).
    fn stamp(p: &mut Plane) {
        let (x0, x1) = ((0.69 * W as f64) as usize, (0.81 * W as f64) as usize);
        let (y0, y1) = ((0.265 * H as f64) as usize, (0.335 * H as f64) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                p.data[y * W + x] = 250.0;
            }
        }
    }

    fn shot(zoom_to: f64) -> Vec<Plane> {
        let tex = texture();
        (0..60)
            .map(|f| {
                let mut p = view(&tex, 1.0 + (zoom_to - 1.0) * f as f64 / 59.0);
                if (12..54).contains(&f) {
                    stamp(&mut p);
                }
                p
            })
            .collect()
    }

    fn assert_one_sticker(events: &[OverlaySpan]) {
        assert_eq!(events.len(), 1, "{events:?}");
        let e = &events[0];
        assert!(
            e.start.abs_diff(12) <= 1 && e.end.abs_diff(54) <= 1,
            "f{}..f{}",
            e.start,
            e.end
        );
        assert!(
            (e.bounds.cx - 0.75).abs() <= 0.02 && (e.bounds.cy - 0.30).abs() <= 0.02,
            "{:?}",
            e.bounds
        );
        assert!(
            (e.bounds.w - 0.12).abs() <= 0.03 && (e.bounds.h - 0.07).abs() <= 0.03,
            "{:?}",
            e.bounds
        );
    }

    #[test]
    fn a_sticker_on_a_static_shot_is_one_overlay() {
        let frames = shot(1.0);
        let fits = estimate_shot(&frames, 0).fits;
        assert_one_sticker(&detect(&frames, &fits, 0, &[]));
    }

    #[test]
    fn a_sticker_on_a_zooming_shot_is_one_overlay_and_the_zoom_is_not() {
        let frames = shot(1.2);
        let fits = estimate_shot(&frames, 0).fits;
        assert_one_sticker(&detect(&frames, &fits, 0, &[]));
    }

    #[test]
    fn a_masked_region_is_never_an_overlay() {
        let frames = shot(1.0);
        let fits = estimate_shot(&frames, 0).fits;
        let caption = BoxN {
            cx: 0.75,
            cy: 0.30,
            w: 0.2,
            h: 0.12,
        };
        assert!(detect(&frames, &fits, 0, &[caption]).is_empty());
    }

    #[test]
    fn frame_numbers_are_offset_by_the_shot_start() {
        let frames = shot(1.0);
        let fits = estimate_shot(&frames, 0).fits;
        let events = detect(&frames, &fits, 100, &[]);
        assert!(events[0].start.abs_diff(112) <= 1);
    }
}
