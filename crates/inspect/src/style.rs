//! Caption colours from the pixels inside and around a text box.

use image::RgbImage;

/// A box in pixels on the frame image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptionColors {
    pub fill: String,
    pub outline: Option<String>,
}

/// Pixels this close (Euclidean RGB) to the background are background.
const BG_DISTANCE: f64 = 48.0;
/// Two ink clusters closer than this are one colour.
const SAME_COLOR: f64 = 60.0;

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn dist(a: [u8; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, y)| (f64::from(x) - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn median(pixels: &[[u8; 3]]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (c, slot) in out.iter_mut().enumerate() {
        let mut v: Vec<u8> = pixels.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        *slot = v.get(v.len() / 2).copied().unwrap_or(0);
    }
    out
}

fn as_f(c: [u8; 3]) -> [f64; 3] {
    c.map(f64::from)
}

/// Background is the median of a 4 px ring just outside the box. Inside, the
/// non-background "ink" splits into at most two colours; the one that more
/// often touches background is the outline.
pub fn caption_colors(img: &RgbImage, bx: PixelBox) -> CaptionColors {
    let (iw, ih) = img.dimensions();
    let x1 = (bx.x + bx.w).min(iw);
    let y1 = (bx.y + bx.h).min(ih);
    let px = |x: u32, y: u32| img.get_pixel(x, y).0;

    let mut ring = Vec::new();
    for y in bx.y.saturating_sub(4)..(y1 + 4).min(ih) {
        for x in bx.x.saturating_sub(4)..(x1 + 4).min(iw) {
            if x < bx.x || x >= x1 || y < bx.y || y >= y1 {
                ring.push(px(x, y));
            }
        }
    }
    let bg = as_f(median(&ring));
    let is_bg = |x: u32, y: u32| dist(px(x, y), bg) < BG_DISTANCE;

    let mut ink: Vec<(u32, u32)> = Vec::new();
    for y in bx.y..y1 {
        for x in bx.x..x1 {
            if !is_bg(x, y) {
                ink.push((x, y));
            }
        }
    }
    if ink.is_empty() {
        return CaptionColors {
            fill: hex(median(&ring)),
            outline: None,
        };
    }

    // Two-means, seeded with the lightest and darkest ink pixels.
    let luma =
        |p: [u8; 3]| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
    let colors: Vec<[u8; 3]> = ink.iter().map(|&(x, y)| px(x, y)).collect();
    let lightest = *colors
        .iter()
        .max_by(|a, b| luma(**a).total_cmp(&luma(**b)))
        .expect("ink is not empty");
    let darkest = *colors
        .iter()
        .min_by(|a, b| luma(**a).total_cmp(&luma(**b)))
        .expect("ink is not empty");
    let mut centres = [as_f(lightest), as_f(darkest)];
    let mut assign = vec![0usize; colors.len()];
    for _ in 0..10 {
        for (a, c) in assign.iter_mut().zip(&colors) {
            *a = usize::from(dist(*c, centres[1]) < dist(*c, centres[0]));
        }
        for (k, centre) in centres.iter_mut().enumerate() {
            let members: Vec<[u8; 3]> = colors
                .iter()
                .zip(&assign)
                .filter(|(_, &a)| a == k)
                .map(|(c, _)| *c)
                .collect();
            if !members.is_empty() {
                *centre = as_f(median(&members));
            }
        }
    }
    let groups: [Vec<usize>; 2] =
        [0, 1].map(|k| (0..colors.len()).filter(|&i| assign[i] == k).collect());
    let small = groups[0].len().min(groups[1].len()) as f64 / colors.len() as f64;
    if dist(centres[0].map(|v| v as u8), centres[1]) < SAME_COLOR || small < 0.08 {
        return CaptionColors {
            fill: hex(median(&colors)),
            outline: None,
        };
    }

    let touches_bg = |&(x, y): &(u32, u32)| {
        [(0i64, 1i64), (0, -1), (1, 0), (-1, 0)]
            .iter()
            .any(|(dx, dy)| {
                let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                nx >= 0
                    && ny >= 0
                    && (nx as u32) < iw
                    && (ny as u32) < ih
                    && is_bg(nx as u32, ny as u32)
            })
    };
    let edge_share = |k: usize| {
        let g = &groups[k];
        g.iter().filter(|&&i| touches_bg(&ink[i])).count() as f64 / g.len() as f64
    };
    let (fill_k, outline_k) = if edge_share(0) > edge_share(1) {
        (1, 0)
    } else {
        (0, 1)
    };
    // Strokes are thin and anti-aliased into both the background and the
    // fill, so a cluster's median is a blend. Its true colour is the end
    // farthest from the other ink colour: the darkest of a dark stroke
    // around light text, the lightest of the fill. Measured on real frames.
    let group_median = |k: usize| {
        let other = centres[1 - k];
        let mut members: Vec<[u8; 3]> = groups[k].iter().map(|&i| colors[i]).collect();
        members.sort_by(|a, b| dist(*b, other).total_cmp(&dist(*a, other)));
        let keep = (members.len() * 3 / 10).max(1);
        median(&members[..keep])
    };
    CaptionColors {
        fill: hex(group_median(fill_k)),
        outline: Some(hex(group_median(outline_k))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// Background, then a block of "text": fill inside a 3 px outline.
    fn caption_image(bg: [u8; 3], fill: [u8; 3], outline: Option<[u8; 3]>) -> (RgbImage, PixelBox) {
        let mut img = RgbImage::from_pixel(200, 100, Rgb(bg));
        // Three "letters" so there is background between strokes.
        for letter in 0..3 {
            let x0 = 40 + letter * 45;
            for y in 30..70 {
                for x in x0..x0 + 30 {
                    let edge = !(x0 + 3..x0 + 27).contains(&x) || !(33..67).contains(&y);
                    let color = match (edge, outline) {
                        (true, Some(o)) => o,
                        _ => fill,
                    };
                    img.put_pixel(x, y, Rgb(color));
                }
            }
        }
        (
            img,
            PixelBox {
                x: 38,
                y: 28,
                w: 139,
                h: 44,
            },
        )
    }

    fn close(got: &str, want: &str, tol: u8) -> bool {
        let p = |s: &str| [1, 3, 5].map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap());
        p(got)
            .iter()
            .zip(p(want))
            .all(|(a, b)| a.abs_diff(b) <= tol)
    }

    #[test]
    fn white_text_with_a_black_outline_on_navy() {
        let (img, bx) = caption_image([0x1E, 0x2A, 0x38], [255, 255, 255], Some([0, 0, 0]));
        let s = caption_colors(&img, bx);
        assert!(close(&s.fill, "#FFFFFF", 4), "fill {}", s.fill);
        assert!(
            close(s.outline.as_deref().unwrap(), "#000000", 4),
            "outline {:?}",
            s.outline
        );
    }

    #[test]
    fn yellow_text_without_an_outline() {
        let (img, bx) = caption_image([0x20, 0x20, 0x20], [255, 230, 0], None);
        let s = caption_colors(&img, bx);
        assert!(close(&s.fill, "#FFE600", 4), "fill {}", s.fill);
        assert_eq!(s.outline, None);
    }

    /// Half-resolution frames turn a 3 px stroke into ~1.5 px, so most
    /// outline pixels are blends of the outline and the background.
    fn blended_stroke(bg: [u8; 3]) -> CaptionColors {
        let (mut img, bx) = caption_image(bg, [255, 255, 255], Some([0, 0, 0]));
        let blend = |t: f32| bg.map(|c| (f32::from(c) * t) as u8);
        for letter in 0..3u32 {
            let x0 = 40 + letter * 45;
            for y in 30..70 {
                for x in x0..x0 + 30 {
                    // Distance into the stroke from the outer edge: 0, 1, 2.
                    let depth = (x - x0).min(x0 + 29 - x).min(y - 30).min(69 - y);
                    match depth {
                        0 => img.put_pixel(x, y, image::Rgb(blend(0.6))),
                        1 => img.put_pixel(x, y, image::Rgb(blend(0.3))),
                        // Anti-aliasing between the stroke and the fill.
                        3 => img.put_pixel(x, y, image::Rgb([150, 150, 150])),
                        _ => {}
                    }
                }
            }
        }
        caption_colors(&img, bx)
    }

    #[test]
    fn a_thin_outline_blended_into_a_light_background_is_still_black() {
        let s = blended_stroke([0xC9, 0xA9, 0x6E]);
        assert!(
            close(s.outline.as_deref().unwrap(), "#000000", 24),
            "outline {:?}",
            s.outline
        );
        assert!(close(&s.fill, "#FFFFFF", 4), "fill {}", s.fill);
    }

    #[test]
    fn a_thin_outline_blended_into_a_dark_background_is_still_black() {
        let s = blended_stroke([0x1E, 0x2A, 0x38]);
        assert!(
            close(s.outline.as_deref().unwrap(), "#000000", 24),
            "outline {:?}",
            s.outline
        );
        assert!(close(&s.fill, "#FFFFFF", 4), "fill {}", s.fill);
    }

    /// Real pixels: caption crops from the half-resolution JPEG frames that
    /// `shots` extracted from cuts-3.mp4 (white fill, 3 px black stroke).
    fn real_crop(name: &str, bx: PixelBox) -> CaptionColors {
        let path = format!(
            "{}/tests/data/caption-{name}.png",
            env!("CARGO_MANIFEST_DIR")
        );
        caption_colors(&image::open(path).unwrap().into_rgb8(), bx)
    }

    #[test]
    fn real_frame_white_on_navy_with_black_stroke() {
        let s = real_crop(
            "one-navy",
            PixelBox {
                x: 12,
                y: 12,
                w: 107,
                h: 39,
            },
        );
        assert!(close(&s.fill, "#FFFFFF", 16), "fill {}", s.fill);
        assert!(
            close(s.outline.as_deref().unwrap(), "#000000", 16),
            "outline {:?}",
            s.outline
        );
    }

    #[test]
    fn real_frame_white_on_gold_with_black_stroke() {
        let s = real_crop(
            "two-gold",
            PixelBox {
                x: 12,
                y: 12,
                w: 116,
                h: 39,
            },
        );
        assert!(close(&s.fill, "#FFFFFF", 16), "fill {}", s.fill);
        assert!(
            close(s.outline.as_deref().unwrap(), "#000000", 16),
            "outline {:?}",
            s.outline
        );
    }

    #[test]
    fn hex_is_uppercase_rrggbb() {
        assert_eq!(hex([255, 230, 0]), "#FFE600");
    }
}
