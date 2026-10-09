//! Apparent camera motion per shot: scale and translation of each frame
//! against the shot's first frame, fitted to Lottie-style keyframes.
//!
//! For each frame, candidate scales of the first frame are compared with
//! the frame by phase correlation; the best peak gives the scale and the
//! peak's offset gives the translation. This measures what the viewer sees,
//! not how the editor produced it (zoom keyframes or a punch-in crop).

use std::sync::Arc;

use model::{BoxN, Ease, Keyframe, Transform};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Below this median peak the transform is reported as unknown.
pub const MIN_CONFIDENCE: f64 = 0.3;
/// Scale steps tried around the previous frame's scale.
const STEP: f64 = 0.01;
const STEPS: i32 = 3;
/// Changes smaller than this (percent scale, normalized position) are noise.
const STATIC_SCALE: f64 = 1.0;
/// Scale movement (percent) below this that returns is estimate noise.
const WOBBLE: f64 = 3.0;
const STATIC_POSITION: f64 = 0.005;

/// A grayscale image as floats.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl Plane {
    pub fn from_gray(img: &image::GrayImage) -> Self {
        Plane {
            w: img.width() as usize,
            h: img.height() as usize,
            data: img.as_raw().iter().map(|&v| f32::from(v)).collect(),
        }
    }

    /// Bilinear sample at pixel coordinates, clamped at the edges.
    pub fn sample(&self, x: f64, y: f64) -> f32 {
        let x = x.clamp(0.0, (self.w - 1) as f64);
        let y = y.clamp(0.0, (self.h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let at = |x: usize, y: usize| self.data[y * self.w + x];
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    /// This plane magnified by `s` about its centre.
    fn scaled(&self, s: f64) -> Plane {
        let (cx, cy) = (self.w as f64 / 2.0, self.h as f64 / 2.0);
        let mut data = Vec::with_capacity(self.w * self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let sx = cx + (x as f64 + 0.5 - cx) / s - 0.5;
                let sy = cy + (y as f64 + 0.5 - cy) / s - 0.5;
                data.push(self.sample(sx, sy));
            }
        }
        Plane {
            w: self.w,
            h: self.h,
            data,
        }
    }
}

/// `y` on CSS `cubic-bezier(x1, y1, x2, y2)` at time `p`.
pub fn cubic_bezier(p: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let coord = |t: f64, a: f64, b: f64| {
        3.0 * a * t * (1.0 - t).powi(2) + 3.0 * b * t * t * (1.0 - t) + t.powi(3)
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..50 {
        let mid = (lo + hi) / 2.0;
        if coord(mid, x1, x2) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    coord((lo + hi) / 2.0, y1, y2)
}

struct Correlator {
    w: usize,
    h: usize,
    row: Arc<dyn Fft<f32>>,
    col: Arc<dyn Fft<f32>>,
    row_inv: Arc<dyn Fft<f32>>,
    col_inv: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    mask: Vec<bool>,
}

impl Correlator {
    fn new(w: usize, h: usize, masks: &[BoxN]) -> Self {
        let mut planner = FftPlanner::new();
        let hann = |i: usize, n: usize| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos()
        };
        let mut window = Vec::with_capacity(w * h);
        let mut mask = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                window.push(hann(x, w) * hann(y, h));
                let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
                mask.push(
                    masks.iter().any(|b| {
                        u >= b.left() && u <= b.right() && v >= b.top() && v <= b.bottom()
                    }),
                );
            }
        }
        Correlator {
            w,
            h,
            row: planner.plan_fft_forward(w),
            col: planner.plan_fft_forward(h),
            row_inv: planner.plan_fft_inverse(w),
            col_inv: planner.plan_fft_inverse(h),
            window,
            mask,
        }
    }

    fn fft2(&self, data: &mut [Complex<f32>], inverse: bool) {
        let (row, col) = if inverse {
            (&self.row_inv, &self.col_inv)
        } else {
            (&self.row, &self.col)
        };
        for r in data.chunks_mut(self.w) {
            row.process(r);
        }
        let mut column = vec![Complex::new(0.0, 0.0); self.h];
        for x in 0..self.w {
            for y in 0..self.h {
                column[y] = data[y * self.w + x];
            }
            col.process(&mut column);
            for y in 0..self.h {
                data[y * self.w + x] = column[y];
            }
        }
    }

    /// Mean-removed, text-masked, windowed spectrum.
    fn spectrum(&self, p: &Plane) -> Vec<Complex<f32>> {
        let unmasked: Vec<f32> = p
            .data
            .iter()
            .zip(&self.mask)
            .filter(|(_, &m)| !m)
            .map(|(v, _)| *v)
            .collect();
        let mean = unmasked.iter().sum::<f32>() / unmasked.len().max(1) as f32;
        let mut data: Vec<Complex<f32>> = p
            .data
            .iter()
            .zip(&self.window)
            .zip(&self.mask)
            .map(|((v, w), &m)| Complex::new(if m { 0.0 } else { (v - mean) * w }, 0.0))
            .collect();
        self.fft2(&mut data, false);
        data
    }

    /// Shift `(dx, dy)` such that `b(x) ≈ a(x - d)`, and the peak height.
    fn correlate(&self, a: &[Complex<f32>], b: &[Complex<f32>]) -> (f64, f64, f64) {
        let mut r: Vec<Complex<f32>> = a
            .iter()
            .zip(b)
            .map(|(fa, fb)| {
                let c = fb * fa.conj();
                c / (c.norm() + 1e-6)
            })
            .collect();
        self.fft2(&mut r, true);
        let n = (self.w * self.h) as f32;
        let values: Vec<f32> = r.iter().map(|c| c.re / n).collect();
        let (best, peak) = values
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, v)| (i, *v))
            .unwrap_or((0, 0.0));
        let (bx, by) = (best % self.w, best / self.w);
        let at = |x: isize, y: isize| {
            let x = x.rem_euclid(self.w as isize) as usize;
            let y = y.rem_euclid(self.h as isize) as usize;
            f64::from(values[y * self.w + x])
        };
        let refine = |m: f64, c: f64, p: f64| {
            let d = m - 2.0 * c + p;
            if d.abs() < 1e-12 {
                0.0
            } else {
                (0.5 * (m - p) / d).clamp(-0.5, 0.5)
            }
        };
        let (ix, iy) = (bx as isize, by as isize);
        let fx = refine(at(ix - 1, iy), at(ix, iy), at(ix + 1, iy));
        let fy = refine(at(ix, iy - 1), at(ix, iy), at(ix, iy + 1));
        let wrap = |i: usize, n: usize| {
            if i > n / 2 {
                i as f64 - n as f64
            } else {
                i as f64
            }
        };
        (
            wrap(bx, self.w) + fx,
            wrap(by, self.h) + fy,
            f64::from(peak),
        )
    }
}

/// Per-frame measurement against the first frame of the shot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameFit {
    pub scale: f64,
    /// Translation in normalized frame units.
    pub dx: f64,
    pub dy: f64,
    pub peak: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub fits: Vec<FrameFit>,
    pub confidence: f64,
    pub transform: Option<Transform>,
}

pub fn estimate_shot(frames: &[Plane], start_frame: i64) -> Estimate {
    estimate_shot_masked(frames, start_frame, &[])
}

/// `masks` are regions to ignore, such as captions, which move with the
/// screen rather than the scene.
pub fn estimate_shot_masked(frames: &[Plane], start_frame: i64, masks: &[BoxN]) -> Estimate {
    let Some(first) = frames.first() else {
        return Estimate {
            fits: Vec::new(),
            confidence: 0.0,
            transform: None,
        };
    };
    let corr = Correlator::new(first.w, first.h, masks);
    let mut fits = vec![FrameFit {
        scale: 1.0,
        dx: 0.0,
        dy: 0.0,
        peak: 1.0,
    }];
    let mut guess = 1.0;
    for frame in &frames[1..] {
        let target = corr.spectrum(frame);
        let try_scale = |s: f64| {
            let (dx, dy, peak) = corr.correlate(&corr.spectrum(&first.scaled(s)), &target);
            FrameFit {
                scale: s,
                dx: dx / first.w as f64,
                dy: dy / first.h as f64,
                peak,
            }
        };
        let mut centre = guess;
        let mut best;
        let mut row;
        let mut moves = 0;
        loop {
            row = (-STEPS..=STEPS)
                .map(|k| try_scale(centre * (1.0 + STEP * f64::from(k))))
                .collect::<Vec<_>>();
            let k = row
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.peak.total_cmp(&b.1.peak))
                .map_or(0, |(i, _)| i);
            best = k;
            let at_edge = k == 0 || k == row.len() - 1;
            if !at_edge || moves >= 10 {
                break;
            }
            centre = row[k].scale;
            moves += 1;
        }
        let refined = if best > 0 && best < row.len() - 1 {
            let (m, c, p) = (row[best - 1].peak, row[best].peak, row[best + 1].peak);
            let d = m - 2.0 * c + p;
            let shift = if d.abs() < 1e-12 {
                0.0
            } else {
                (0.5 * (m - p) / d).clamp(-0.5, 0.5)
            };
            try_scale(row[best].scale * (1.0 + STEP * shift))
        } else {
            row[best]
        };
        let fit = if refined.peak >= row[best].peak {
            refined
        } else {
            row[best]
        };
        guess = fit.scale;
        fits.push(fit);
    }

    let mut peaks: Vec<f64> = fits[1..].iter().map(|f| f.peak).collect();
    peaks.sort_by(f64::total_cmp);
    let confidence = if peaks.is_empty() {
        1.0
    } else {
        peaks[peaks.len() / 2].clamp(0.0, 1.0)
    };
    let transform = (confidence >= MIN_CONFIDENCE).then(|| keyframes(&fits, start_frame));
    Estimate {
        fits,
        confidence,
        transform,
    }
}

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

/// Best `cubic-bezier(x1, 0, x2, 1)` through the progress samples.
fn fit_ease(progress: &[(f64, f64)]) -> (Ease, Ease) {
    let mut best = (f64::MAX, 0.0, 1.0);
    for a in 0..=20 {
        for b in 0..=20 {
            let (x1, x2) = (f64::from(a) / 20.0, f64::from(b) / 20.0);
            let err: f64 = progress
                .iter()
                .map(|&(u, p)| (cubic_bezier(u, x1, 0.0, x2, 1.0) - p).powi(2))
                .sum();
            if err < best.0 {
                best = (err, x1, x2);
            }
        }
    }
    (Ease { x: best.1, y: 0.0 }, Ease { x: best.2, y: 1.0 })
}

fn progress_of(values: &[f64]) -> Vec<(f64, f64)> {
    let (first, last) = (values[0], values[values.len() - 1]);
    let n = (values.len() - 1).max(1) as f64;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                i as f64 / n,
                if (last - first).abs() < 1e-12 {
                    0.0
                } else {
                    (v - first) / (last - first)
                },
            )
        })
        .collect()
}

/// Scale curve to keyframes. Under `WOBBLE` of movement that ends where it
/// began is one keyframe (estimate noise from moving subjects); a swing
/// that comes back gets a keyframe at its peak; otherwise first and last.
fn scale_keyframes(scales: &[f64], start: i64) -> Vec<Keyframe<f64>> {
    let last = scales.len() - 1;
    let first = scales[0];
    let (peak, peak_dev) = scales
        .iter()
        .enumerate()
        .map(|(i, s)| (i, (s - first).abs()))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0));
    let end_dev = (scales[last] - first).abs();

    let still = |t: i64, s: f64| Keyframe {
        t,
        s: round(s, 1),
        o: None,
        i: None,
    };
    if peak_dev < WOBBLE && end_dev < STATIC_SCALE {
        return vec![still(start, first)];
    }
    let segment = |from: usize, to: usize| {
        let (o, i) = fit_ease(&progress_of(&scales[from..=to]));
        Keyframe {
            t: start + from as i64,
            s: round(scales[from], 1),
            o: Some(o),
            i: Some(i),
        }
    };
    let returns = peak > 0 && peak < last && (scales[peak] - scales[last]).abs() >= WOBBLE;
    if returns && peak_dev >= WOBBLE {
        vec![
            segment(0, peak),
            segment(peak, last),
            still(start + last as i64, scales[last]),
        ]
    } else {
        vec![segment(0, last), still(start + last as i64, scales[last])]
    }
}

fn keyframes(fits: &[FrameFit], start: i64) -> Transform {
    let last = (fits.len() - 1) as i64;
    let scales: Vec<f64> = fits.iter().map(|f| f.scale * 100.0).collect();
    let scale = scale_keyframes(&scales, start);

    let positions: Vec<[f64; 2]> = fits.iter().map(|f| [0.5 + f.dx, 0.5 + f.dy]).collect();
    let end = positions[last as usize];
    let moved = (end[0] - 0.5).hypot(end[1] - 0.5);
    let position = if moved < STATIC_POSITION {
        vec![Keyframe {
            t: start,
            s: [0.5, 0.5],
            o: None,
            i: None,
        }]
    } else {
        let along: Vec<f64> = positions
            .iter()
            .map(|p| (p[0] - 0.5).hypot(p[1] - 0.5))
            .collect();
        let (o, i) = fit_ease(&progress_of(&along));
        vec![
            Keyframe {
                t: start,
                s: [0.5, 0.5],
                o: Some(o),
                i: Some(i),
            },
            Keyframe {
                t: start + last,
                s: [round(end[0], 4), round(end[1], 4)],
                o: None,
                i: None,
            },
        ]
    };
    Transform {
        anchor: [0.5, 0.5],
        scale,
        position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 64;
    const H: usize = 112;

    /// A busy, deterministic texture at 2x the frame size.
    fn texture() -> Plane {
        let (w, h) = (W * 2, H * 2);
        let mut data = vec![60.0f32; w * h];
        let mut seed = 7u64;
        let mut next = |m: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize % m
        };
        for _ in 0..120 {
            let (x, y, r, v) = (next(w), next(h), 3 + next(10), 80 + next(170));
            for yy in y.saturating_sub(r)..(y + r).min(h) {
                for xx in x.saturating_sub(r)..(x + r).min(w) {
                    data[yy * w + xx] = v as f32;
                }
            }
        }
        Plane { w, h, data }
    }

    /// The frame a camera sees when zoomed by `s` (1.0 = whole texture).
    fn view(tex: &Plane, s: f64, shift: (f64, f64)) -> Plane {
        let mut out = Plane {
            w: W,
            h: H,
            data: vec![0.0; W * H],
        };
        for y in 0..H {
            for x in 0..W {
                let u = (x as f64 + 0.5) / W as f64 - 0.5;
                let v = (y as f64 + 0.5) / H as f64 - 0.5;
                let tx = (0.5 + u / s + shift.0) * tex.w as f64;
                let ty = (0.5 + v / s + shift.1) * tex.h as f64;
                out.data[y * W + x] = tex.sample(tx - 0.5, ty - 0.5);
            }
        }
        out
    }

    fn ease_out(p: f64) -> f64 {
        cubic_bezier(p, 0.0, 0.0, 0.58, 1.0)
    }

    #[test]
    fn an_eased_zoom_becomes_two_scale_keyframes_with_ease_out() {
        let tex = texture();
        let frames: Vec<Plane> = (0..30)
            .map(|f| view(&tex, 1.0 + 0.2 * ease_out(f as f64 / 29.0), (0.0, 0.0)))
            .collect();
        let m = estimate_shot(&frames, 0);
        let t = m.transform.expect("confident on a textured zoom");

        assert_eq!(t.scale.len(), 2);
        assert_eq!(t.scale[0].t, 0);
        assert!((t.scale[0].s - 100.0).abs() < 0.5);
        assert_eq!(t.scale[1].t, 29);
        assert!(
            (t.scale[1].s - 120.0).abs() <= 3.0,
            "end scale {}",
            t.scale[1].s
        );
        let ease = t.scale[0].i.expect("eased");
        assert!((0.45..=0.7).contains(&ease.x), "i.x {}", ease.x);
    }

    #[test]
    fn a_static_shot_is_scale_100_with_high_confidence() {
        let tex = texture();
        let frames: Vec<Plane> = (0..8).map(|_| view(&tex, 1.0, (0.0, 0.0))).collect();
        let m = estimate_shot(&frames, 40);
        let t = m.transform.expect("confident");
        assert_eq!(t.scale.len(), 1, "no animation");
        assert_eq!(t.scale[0].t, 40);
        assert!((t.scale[0].s - 100.0).abs() <= 1.0);
        assert!(m.confidence > 0.9, "confidence {}", m.confidence);
    }

    #[test]
    fn a_pan_is_reported_as_position_change() {
        let tex = texture();
        let frames: Vec<Plane> = (0..12)
            .map(|f| view(&tex, 1.25, (0.006 * f as f64, 0.0)))
            .collect();
        let t = estimate_shot(&frames, 0).transform.expect("confident");
        let (first, last) = (t.position.first().unwrap(), t.position.last().unwrap());
        // The camera moved right, so the picture moved left.
        assert!(
            last.s[0] - first.s[0] < -0.03,
            "dx {}",
            last.s[0] - first.s[0]
        );
        assert!((last.s[1] - first.s[1]).abs() < 0.01);
    }

    #[test]
    fn unrelated_noise_frames_are_unknown() {
        let mut seed = 99u64;
        let frames: Vec<Plane> = (0..8)
            .map(|_| Plane {
                w: W,
                h: H,
                data: (0..W * H)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        ((seed >> 33) % 256) as f32
                    })
                    .collect(),
            })
            .collect();
        let m = estimate_shot(&frames, 0);
        assert!(m.confidence < 0.3, "confidence {}", m.confidence);
        assert!(m.transform.is_none());
    }

    fn fits(scales: &[f64]) -> Vec<FrameFit> {
        scales
            .iter()
            .map(|&scale| FrameFit {
                scale,
                dx: 0.0,
                dy: 0.0,
                peak: 0.9,
            })
            .collect()
    }

    #[test]
    fn a_shot_that_wobbles_but_ends_where_it_started_is_one_keyframe() {
        // An animated character nudges the estimate by a percent or two.
        let scales: Vec<f64> = (0..60)
            .map(|f| 1.0 + 0.018 * ((f as f64) / 6.0).sin())
            .chain([1.0])
            .collect();
        let t = keyframes(&fits(&scales), 34);
        assert_eq!(t.scale.len(), 1, "{:?}", t.scale);
        assert_eq!(t.scale[0].t, 34);
    }

    #[test]
    fn a_zoom_in_and_back_out_keeps_its_peak() {
        let scales: Vec<f64> = (0..=60)
            .map(|f| 1.0 + 0.15 * (std::f64::consts::PI * f as f64 / 60.0).sin())
            .collect();
        let t = keyframes(&fits(&scales), 0);
        let values: Vec<(i64, f64)> = t.scale.iter().map(|k| (k.t, k.s)).collect();
        assert_eq!(values.len(), 3, "{values:?}");
        assert_eq!(values[1].0, 30);
        assert!(
            (values[1].1 - 115.0).abs() < 0.5 && (values[2].1 - 100.0).abs() < 0.5,
            "{values:?}"
        );
    }

    #[test]
    fn bezier_easing_matches_known_points() {
        assert!((cubic_bezier(0.0, 0.0, 0.0, 0.58, 1.0)).abs() < 1e-9);
        assert!((cubic_bezier(1.0, 0.0, 0.0, 0.58, 1.0) - 1.0).abs() < 1e-9);
        assert!(
            (cubic_bezier(0.5, 0.0, 0.0, 1.0, 1.0) - 0.5).abs() < 1e-6,
            "linear"
        );
        assert!(
            cubic_bezier(0.5, 0.0, 0.0, 0.58, 1.0) > 0.6,
            "ease-out is ahead at the midpoint"
        );
    }
}
