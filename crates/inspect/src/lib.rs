//! `inspect`: measurements from the extracted frames.

pub mod captions;
pub mod style;
pub mod text;

use std::collections::hash_map::{Entry, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use image::{imageops, GrayImage, RgbImage};
use model::{BoxN, CaptionEvent, Inspect, InspectTiming, Probe, Shots, TimeRange};

pub const INSPECT_FILE: &str = "inspect.json";

/// Size of the grayscale thumbnails kept for change detection.
const THUMB: (u32, u32) = (135, 240);
/// Mean absolute grey-level difference inside a box that counts as new
/// content (0..255). JPEG noise on a still caption stays under 3.
const CONTENT_CHANGE: f64 = 12.0;
/// OCR line height to font size; caps and x-height sit inside the em box.
const LINE_TO_FONT: f64 = 0.75;

#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    #[error("{path}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{path}")]
    Image {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("ocr: {0}")]
    Ocr(String),
}

/// Reads `probe.json`, `shots.json` and `frames/`; writes `inspect.json`.
pub fn run(run_dir: &Path) -> Result<Inspect, InspectError> {
    let probe: Probe = read_json(&run_dir.join(ingest::PROBE_FILE))?;
    let shots: Shots = read_json(&run_dir.join(shots::SHOTS_FILE))?;
    let ocr = text::Ocr::load(&doctor::default_models_dir()).map_err(InspectError::Ocr)?;
    let frames = shots.frames.max(0) as u64;
    let load = |n: u64| load_frame(run_dir, n);

    let mut detected = Vec::with_capacity(frames as usize);
    let mut thumbs = Vec::with_capacity(frames as usize);
    let mut detect_time = 0.0f64;
    for n in 0..frames {
        let img = load(n)?;
        let t = Instant::now();
        detected.push(ocr.detect_lines(&img).map_err(InspectError::Ocr)?);
        detect_time += t.elapsed().as_secs_f64();
        thumbs.push(imageops::grayscale(&imageops::thumbnail(
            &img, THUMB.0, THUMB.1,
        )));
    }

    let mut read_cache: HashMap<u64, Vec<(BoxN, String)>> = HashMap::new();
    let mut labeled = Vec::new();
    for track in captions::track(&detected) {
        let changed = |f: u64| {
            let i = (f - track.start) as usize;
            f > 0
                && box_diff(
                    &thumbs[f as usize - 1],
                    &thumbs[f as usize],
                    &track.boxes[i],
                ) > CONTENT_CHANGE
        };
        let spans = captions::split_track(&track, changed);
        let mut failure = None;
        let spans = captions::label(spans, |span| {
            let sample = (span.start + span.end) / 2;
            let lines = match read_cache.entry(sample) {
                Entry::Occupied(e) => e.into_mut(),
                Entry::Vacant(v) => {
                    match load(sample)
                        .and_then(|img| ocr.read_lines(&img).map_err(InspectError::Ocr))
                    {
                        Ok(lines) => v.insert(lines),
                        Err(e) => {
                            failure = Some(e);
                            return String::new();
                        }
                    }
                }
            };
            lines
                .iter()
                .map(|(b, t)| (b.iou(&span.settled), t))
                .filter(|(iou, _)| *iou > 0.3)
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, t)| t.clone())
                .unwrap_or_default()
        });
        if let Some(e) = failure {
            return Err(e);
        }
        labeled.extend(spans.into_iter().filter(|s| !s.text.is_empty()));
    }

    let mut events = Vec::new();
    for (i, c) in captions::group_lines(labeled).into_iter().enumerate() {
        let img = load((c.start + c.end) / 2)?;
        let colors = style::caption_colors(&img, pixel_box(&c.bounds, &img, 2));
        let line_height = c.line_boxes.iter().map(|b| b.h).sum::<f64>() / c.line_boxes.len() as f64;
        let text = c.lines.join("\n");
        let letters: Vec<char> = text.chars().filter(|ch| ch.is_alphabetic()).collect();
        events.push(CaptionEvent {
            id: format!("c{}", i + 1),
            range: TimeRange::from_frames(c.start as i64, (c.end - c.start) as i64, probe.rate),
            uppercase: !letters.is_empty() && letters.iter().all(|ch| ch.is_uppercase()),
            text,
            lines: c.lines,
            anchor: captions::anchor(&c.bounds).to_string(),
            bounds: c.bounds,
            line_boxes: c.line_boxes,
            fill: colors.fill,
            outline: colors.outline,
            line_height,
            font_px_estimate: (line_height * f64::from(probe.height) / LINE_TO_FONT).round(),
            animation: c.animation,
        });
    }

    let inspect = Inspect {
        version: 1,
        rate: probe.rate,
        captions: events,
        timing: InspectTiming {
            frames,
            detect_ms_per_frame: if frames == 0 {
                0.0
            } else {
                (detect_time * 1000.0 / frames as f64 * 10.0).round() / 10.0
            },
        },
    };
    let out = run_dir.join(INSPECT_FILE);
    let json = serde_json::to_string_pretty(&inspect).expect("Inspect always serializes");
    fs::write(&out, json).map_err(|source| InspectError::Io { path: out, source })?;
    Ok(inspect)
}

fn load_frame(run_dir: &Path, n: u64) -> Result<RgbImage, InspectError> {
    let path = run_dir.join(shots::frame_path(n));
    Ok(image::open(&path)
        .map_err(|source| InspectError::Image { path, source })?
        .into_rgb8())
}

/// Normalized box to pixels on `img`, grown by `pad` pixels and clipped.
fn pixel_box(b: &BoxN, img: &RgbImage, pad: u32) -> style::PixelBox {
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let x0 = ((b.left() * w).floor().max(0.0) as u32).saturating_sub(pad);
    let y0 = ((b.top() * h).floor().max(0.0) as u32).saturating_sub(pad);
    let x1 = (((b.right() * w).ceil() as u32) + pad).min(img.width());
    let y1 = (((b.bottom() * h).ceil() as u32) + pad).min(img.height());
    style::PixelBox {
        x: x0,
        y: y0,
        w: x1.saturating_sub(x0),
        h: y1.saturating_sub(y0),
    }
}

/// Mean absolute grey difference between two thumbnails inside `b`.
fn box_diff(a: &GrayImage, b: &GrayImage, bx: &BoxN) -> f64 {
    let (w, h) = (f64::from(a.width()), f64::from(a.height()));
    let x0 = (bx.left() * w).max(0.0) as u32;
    let y0 = (bx.top() * h).max(0.0) as u32;
    let x1 = ((bx.right() * w).ceil() as u32).min(a.width());
    let y1 = ((bx.bottom() * h).ceil() as u32).min(a.height());
    let mut sum = 0u64;
    let mut count = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            sum += u64::from(a.get_pixel(x, y)[0].abs_diff(b.get_pixel(x, y)[0]));
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum as f64 / count as f64
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, InspectError> {
    let text = fs::read_to_string(path).map_err(|source| InspectError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| InspectError::Json {
        path: path.to_path_buf(),
        source,
    })
}
