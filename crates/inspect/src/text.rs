//! The ocrs models: line detection on every frame, recognition on demand.

use std::path::Path;

use image::RgbImage;
use model::BoxN;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use rten::Model;

pub const DETECTION_MODEL: &str = "text-detection.rten";
pub const RECOGNITION_MODEL: &str = "text-recognition.rten";

pub struct Ocr {
    engine: OcrEngine,
}

impl Ocr {
    pub fn load(models_dir: &Path) -> Result<Self, String> {
        let load = |name: &str| {
            Model::load_file(models_dir.join(name))
                .map_err(|e| format!("{name}: {e}; run `viralcraft doctor`"))
        };
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(load(DETECTION_MODEL)?),
            recognition_model: Some(load(RECOGNITION_MODEL)?),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Ocr { engine })
    }

    /// Text-line boxes, normalized to the image size. No recognition.
    pub fn detect_lines(&self, img: &RgbImage) -> Result<Vec<BoxN>, String> {
        let (w, h) = img.dimensions();
        let input = self
            .engine
            .prepare_input(
                ImageSource::from_bytes(img.as_raw(), (w, h)).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let words = self
            .engine
            .detect_words(&input)
            .map_err(|e| e.to_string())?;
        let lines = self.engine.find_text_lines(&input, &words);
        let mut boxes = Vec::with_capacity(lines.len());
        for line in &lines {
            let (mut left, mut top, mut right, mut bottom) =
                (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for corner in line.iter().flat_map(|word| word.corners()) {
                left = left.min(corner.x);
                right = right.max(corner.x);
                top = top.min(corner.y);
                bottom = bottom.max(corner.y);
            }
            if right > left && bottom > top {
                boxes.push(BoxN::from_edges(
                    f64::from(left.max(0.0)) / f64::from(w),
                    f64::from(top.max(0.0)) / f64::from(h),
                    f64::from(right.min(w as f32)) / f64::from(w),
                    f64::from(bottom.min(h as f32)) / f64::from(h),
                ));
            }
        }
        Ok(boxes)
    }

    /// Recognized lines with their normalized boxes.
    pub fn read_lines(&self, img: &RgbImage) -> Result<Vec<(BoxN, String)>, String> {
        let (w, h) = img.dimensions();
        let input = self
            .engine
            .prepare_input(
                ImageSource::from_bytes(img.as_raw(), (w, h)).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let words = self
            .engine
            .detect_words(&input)
            .map_err(|e| e.to_string())?;
        let lines = self.engine.find_text_lines(&input, &words);
        let texts = self
            .engine
            .recognize_text(&input, &lines)
            .map_err(|e| e.to_string())?;
        Ok(texts
            .iter()
            .flatten()
            .map(|line| {
                let r = line.bounding_rect();
                let b = BoxN::from_edges(
                    f64::from(r.left()) / f64::from(w),
                    f64::from(r.top()) / f64::from(h),
                    f64::from(r.right()) / f64::from(w),
                    f64::from(r.bottom()) / f64::from(h),
                );
                (b, line.to_string().trim().to_string())
            })
            .collect())
    }
}
