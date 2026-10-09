//! Re-reads each final caption from the full-resolution source.
//!
//! Detection runs on small frames for speed, but reading text there caused
//! typos ("qoes", "Tike") that the full-size original does not have. Each
//! caption is cropped from the source at three moments and the most common
//! reading wins. No transcript is used: many videos have none, and creators
//! spell on purpose ("h@rming").

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use image::RgbImage;
use model::tools;
use model::{BoxN, CaptionEvent};

use crate::text::Ocr;
use crate::InspectError;

/// Padding around a caption crop, as a fraction of frame width.
const CROP_PAD: f64 = 0.015;
const FULLRES_DIR: &str = "fullres";

/// Up to three frames spread across `[start, end)`.
pub fn sample_frames(start: u64, end: u64) -> Vec<u64> {
    let len = end.saturating_sub(start);
    if len >= 4 {
        vec![start + len / 4, start + len / 2, start + 3 * len / 4]
    } else {
        (start..end).collect()
    }
}

/// The reading that occurs most often; with no repeat, the middle one.
pub fn vote(readings: &[String]) -> String {
    let real: Vec<&String> = readings.iter().filter(|r| !r.trim().is_empty()).collect();
    if real.is_empty() {
        return String::new();
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &real {
        *counts.entry(r.as_str()).or_default() += 1;
    }
    let (best, n) = counts
        .iter()
        .max_by_key(|(_, &n)| n)
        .map(|(s, &n)| (*s, n))
        .expect("readings are not empty");
    if n > 1 {
        best.to_string()
    } else {
        real[real.len() / 2].clone()
    }
}

/// Pixel crop `(x, y, w, h)` around `b` on a `w` x `h` frame, padded by
/// `pad` of the frame width on every side and clipped to the frame.
pub fn crop_rect(b: &BoxN, width: u32, height: u32, pad: f64) -> (u32, u32, u32, u32) {
    let (fw, fh) = (f64::from(width), f64::from(height));
    let p = pad * fw;
    let x0 = (b.left() * fw - p).max(0.0).round() as u32;
    let y0 = (b.top() * fh - p).max(0.0).round() as u32;
    let x1 = ((b.right() * fw + p).round() as u32).min(width);
    let y1 = ((b.bottom() * fh + p).round() as u32).min(height);
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

/// Replaces each caption's text with the majority reading of full-size
/// crops. Keeps the small-frame reading when every full-size read is empty.
pub fn reread(
    run_dir: &Path,
    source: &str,
    ocr: &Ocr,
    captions: &mut [CaptionEvent],
) -> Result<(), InspectError> {
    let wanted: Vec<Vec<u64>> = captions
        .iter()
        .map(|c| {
            let start = c.range.start.value.max(0) as u64;
            sample_frames(start, start + c.range.duration.value.max(0) as u64)
        })
        .collect();
    let mut frames: Vec<u64> = wanted.iter().flatten().copied().collect();
    frames.sort_unstable();
    frames.dedup();
    if frames.is_empty() {
        return Ok(());
    }

    let dir = run_dir.join(FULLRES_DIR);
    let io = |path: &Path, source: std::io::Error| InspectError::Io {
        path: path.to_path_buf(),
        source,
    };
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| io(&dir, e))?;
    }
    fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    let select = frames
        .iter()
        .map(|f| format!("eq(n\\,{f})"))
        .collect::<Vec<_>>()
        .join("+");
    let pattern = Path::new(FULLRES_DIR).join("%05d.jpg");
    tools::run_in(
        run_dir,
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-i",
            source,
            "-an",
            "-vf",
            &format!("select='{select}'"),
            "-fps_mode",
            "passthrough",
            "-q:v",
            "2",
            "-start_number",
            "0",
            pattern.to_str().unwrap_or_default(),
        ],
    )
    .map_err(|e| InspectError::Ocr(e.to_string()))?;

    // Output images are numbered in frame order.
    let index: BTreeMap<u64, usize> = frames.iter().enumerate().map(|(i, &f)| (f, i)).collect();
    let mut cache: BTreeMap<u64, RgbImage> = BTreeMap::new();
    for (caption, samples) in captions.iter_mut().zip(&wanted) {
        let mut readings = Vec::new();
        for f in samples {
            if !cache.contains_key(f) {
                let path = dir.join(format!("{:05}.jpg", index[f]));
                let img = image::open(&path)
                    .map_err(|source| InspectError::Image { path, source })?
                    .into_rgb8();
                cache.insert(*f, img);
            }
            let img = &cache[f];
            let (x, y, w, h) = crop_rect(&caption.bounds, img.width(), img.height(), CROP_PAD);
            if w == 0 || h == 0 {
                continue;
            }
            let crop = image::imageops::crop_imm(img, x, y, w, h).to_image();
            let mut lines = ocr.read_lines(&crop).map_err(InspectError::Ocr)?;
            lines.sort_by(|a, b| a.0.cy.total_cmp(&b.0.cy));
            readings.push(
                lines
                    .into_iter()
                    .map(|(_, t)| t)
                    .filter(|t| crate::captions::is_text_line(t))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let best = vote(&readings);
        if !best.is_empty() {
            caption.lines = best.split('\n').map(str::to_string).collect();
            caption.text = best;
        }
    }
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::BoxN;

    #[test]
    fn three_moments_spread_across_the_caption() {
        assert_eq!(sample_frames(100, 200), [125, 150, 175]);
    }

    #[test]
    fn a_short_caption_still_gets_distinct_frames() {
        assert_eq!(sample_frames(10, 12), [10, 11]);
        assert_eq!(sample_frames(10, 11), [10]);
    }

    #[test]
    fn the_most_common_reading_wins() {
        let r = |s: &str| s.to_string();
        assert_eq!(
            vote(&[r("here it goes!"), r("here it qoes!"), r("here it goes!")]),
            "here it goes!"
        );
    }

    #[test]
    fn with_no_majority_the_middle_reading_wins() {
        let r = |s: &str| s.to_string();
        assert_eq!(vote(&[r("a"), r("b"), r("c")]), "b");
    }

    #[test]
    fn empty_readings_are_ignored() {
        let r = |s: &str| s.to_string();
        assert_eq!(vote(&[r(""), r("like"), r("")]), "like");
        assert_eq!(vote(&[]), "");
    }

    #[test]
    fn crops_are_padded_and_clipped_to_the_frame() {
        let b = BoxN {
            cx: 0.5,
            cy: 0.5,
            w: 0.2,
            h: 0.1,
        };
        assert_eq!(crop_rect(&b, 1000, 2000, 0.02), (380, 880, 240, 240));
        let edge = BoxN {
            cx: 0.05,
            cy: 0.98,
            w: 0.2,
            h: 0.1,
        };
        let (x, y, w, h) = crop_rect(&edge, 1000, 2000, 0.02);
        assert_eq!((x, y), (0, 1840));
        assert!(x + w <= 1000 && y + h <= 2000);
    }
}
