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
    /// Colour of a solid box behind the text (ASS BorderStyle 3).
    pub box_color: Option<String>,
}

/// Whether `c` lies on the line between `bg` and `fill`, i.e. is a mix of
/// the two rather than a colour of its own.
fn is_blend(c: [u8; 3], fill: [u8; 3], bg: [f64; 3]) -> bool {
    let d: Vec<f64> = (0..3).map(|i| f64::from(fill[i]) - bg[i]).collect();
    let v: Vec<f64> = (0..3).map(|i| f64::from(c[i]) - bg[i]).collect();
    let len2: f64 = d.iter().map(|x| x * x).sum();
    if len2 < 1.0 {
        return false;
    }
    let t = v.iter().zip(&d).map(|(a, b)| a * b).sum::<f64>() / len2;
    let off_line = (0..3)
        .map(|i| (v[i] - t * d[i]).powi(2))
        .sum::<f64>()
        .sqrt();
    (0.05..0.95).contains(&t) && off_line < 30.0
}

/// A colour this far (Euclidean RGB) from the surround ends a box.
const BOX_EDGE: f64 = 60.0;

/// Whether the colour around the text stops within reach on at least three
/// sides, as a box behind the text does. Open scenery continues instead.
fn on_a_box(img: &RgbImage, bx: PixelBox, surround: [f64; 3]) -> bool {
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let reach = i64::from((bx.h * 2).max(24));
    let (x0, y0) = (i64::from(bx.x), i64::from(bx.y));
    let (x1, y1) = (x0 + i64::from(bx.w), y0 + i64::from(bx.h));
    let edge_within = |sx: i64, sy: i64, dx: i64, dy: i64| {
        (5..reach).any(|step| {
            let (x, y) = (sx + dx * step, sy + dy * step);
            x >= 0
                && y >= 0
                && x < iw
                && y < ih
                && dist(img.get_pixel(x as u32, y as u32).0, surround) > BOX_EDGE
        })
    };
    let side = |points: [(i64, i64); 3], dx: i64, dy: i64| {
        points
            .iter()
            .filter(|(x, y)| edge_within(*x, *y, dx, dy))
            .count()
            >= 2
    };
    let (qx, qy) = ((x1 - x0) / 4, (y1 - y0) / 4);
    let closed = [
        side([(x0, y0 + qy), (x0, (y0 + y1) / 2), (x0, y1 - qy)], -1, 0),
        side([(x1, y0 + qy), (x1, (y0 + y1) / 2), (x1, y1 - qy)], 1, 0),
        side([(x0 + qx, y0), ((x0 + x1) / 2, y0), (x1 - qx, y0)], 0, -1),
        side([(x0 + qx, y1), ((x0 + x1) / 2, y1), (x1 - qx, y1)], 0, 1),
    ];
    closed.iter().filter(|&&c| c).count() >= 3
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

/// `#RRGGBB` back to a colour.
pub fn rgb(hex: &str) -> Option<[u8; 3]> {
    let h = hex.strip_prefix('#').filter(|h| h.len() == 6)?;
    let part = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

/// Summed RGB difference under which a pixel counts as caption fill.
const INK_DISTANCE: u32 = 90;
/// Pixels from an outline-coloured pixel that fill can be and still be text.
const OUTLINE_REACH: usize = 2;
/// Above this share of the box, the "ink" is background of the text colour.
const INK_MAX_SHARE: f64 = 0.5;

/// How much of the caption's "ink" inside `bx` sits in the same place on
/// both frames, with 1 px of slack: 0..1, the worse of the two directions.
/// Rendered text keeps its shape while the scene behind it moves, so this
/// tells whether a caption stayed the same without reading it. Ink is
/// fill-coloured pixels; with an outline, only those within 2 px of it, so
/// a pale object passing behind white text is not ink. `None` when either
/// frame has no ink or ink covers most of the box.
pub fn ink_match(
    a: &RgbImage,
    b: &RgbImage,
    bx: PixelBox,
    fill: [u8; 3],
    outline: Option<[u8; 3]>,
) -> Option<f64> {
    let w = (bx.x + bx.w)
        .min(a.width())
        .min(b.width())
        .saturating_sub(bx.x);
    let h = (bx.y + bx.h)
        .min(a.height())
        .min(b.height())
        .saturating_sub(bx.y);
    let bx = PixelBox { w, h, ..bx };
    let ma = ink_mask(a, bx, fill, outline)?;
    let mb = ink_mask(b, bx, fill, outline)?;
    Some(covered(&ma, &mb, bx).min(covered(&mb, &ma, bx)))
}

fn ink_mask(
    img: &RgbImage,
    bx: PixelBox,
    fill: [u8; 3],
    outline: Option<[u8; 3]>,
) -> Option<Vec<bool>> {
    let near = |c: [u8; 3]| {
        move |x: u32, y: u32| {
            let p = img.get_pixel(x, y).0;
            (0..3).map(|i| u32::from(p[i].abs_diff(c[i]))).sum::<u32>() < INK_DISTANCE
        }
    };
    let is_fill = near(fill);
    let (w, h) = (bx.w as usize, bx.h as usize);
    let mut mask = Vec::with_capacity(w * h);
    for y in bx.y..bx.y + bx.h {
        for x in bx.x..bx.x + bx.w {
            mask.push(is_fill(x, y));
        }
    }
    if let Some(outline) = outline {
        let is_outline = near(outline);
        let rim: Vec<bool> = (bx.y..bx.y + bx.h)
            .flat_map(|y| (bx.x..bx.x + bx.w).map(move |x| (x, y)))
            .map(|(x, y)| is_outline(x, y))
            .collect();
        let r = OUTLINE_REACH;
        for y in 0..h {
            for x in 0..w {
                if mask[y * w + x] {
                    mask[y * w + x] = (y.saturating_sub(r)..(y + r + 1).min(h)).any(|ny| {
                        (x.saturating_sub(r)..(x + r + 1).min(w)).any(|nx| rim[ny * w + nx])
                    });
                }
            }
        }
    }
    let ink = mask.iter().filter(|&&m| m).count();
    (ink > 0 && (ink as f64) <= INK_MAX_SHARE * mask.len() as f64).then_some(mask)
}

/// Share of `m`'s ink that has ink in `other` within 1 px.
fn covered(m: &[bool], other: &[bool], bx: PixelBox) -> f64 {
    let (w, h) = (bx.w as usize, bx.h as usize);
    let near = |x: usize, y: usize| {
        (y.saturating_sub(1)..(y + 2).min(h))
            .any(|ny| (x.saturating_sub(1)..(x + 2).min(w)).any(|nx| other[ny * w + nx]))
    };
    let (mut ink, mut hit) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if m[y * w + x] {
                ink += 1;
                hit += usize::from(near(x, y));
            }
        }
    }
    hit as f64 / ink as f64
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
    let box_color = on_a_box(img, bx, bg).then(|| hex(median(&ring)));
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
            box_color,
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
            box_color,
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
    let (fill, outline) = (group_median(fill_k), group_median(outline_k));
    CaptionColors {
        fill: hex(fill),
        // Anti-aliasing between the text and a plain background makes a
        // cluster of in-between shades; a real outline is its own colour.
        outline: (!is_blend(outline, fill, bg)).then(|| hex(outline)),
        box_color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// White strokes with a 2 px black outline at the given x positions,
    /// over a noisy background whose shade depends on `seed`, like a scene
    /// moving behind a caption.
    fn strokes(xs: &[u32], seed: u32) -> RgbImage {
        let mut img = RgbImage::from_fn(200, 60, |x, y| {
            // Mid tones only, never mistaken for the black outline.
            let v = (60 + (x * 7 + y * 13 + seed * 31) % 120) as u8;
            Rgb([v, v / 2 + 20, 40])
        });
        for &x0 in xs {
            for y in 13..47 {
                for x in x0.saturating_sub(2)..x0 + 8 {
                    img.put_pixel(x, y, Rgb([0, 0, 0]));
                }
            }
            for y in 15..45 {
                for x in x0..x0 + 6 {
                    img.put_pixel(x, y, Rgb([255, 255, 255]));
                }
            }
        }
        img
    }

    const BLACK: Option<[u8; 3]> = Some([0, 0, 0]);

    const WHOLE: PixelBox = PixelBox {
        x: 0,
        y: 0,
        w: 200,
        h: 60,
    };

    #[test]
    fn the_same_text_over_a_changing_background_matches() {
        let a = strokes(&[20, 50, 90, 140], 1);
        let b = strokes(&[21, 50, 90, 140], 9);
        let m = ink_match(&a, &b, WHOLE, [255, 255, 255], BLACK).unwrap();
        assert!(m > 0.9, "{m}");
    }

    #[test]
    fn different_text_in_the_same_place_does_not_match() {
        let a = strokes(&[20, 50, 90, 140], 1);
        let b = strokes(&[35, 70, 110, 170], 1);
        let m = ink_match(&a, &b, WHOLE, [255, 255, 255], BLACK).unwrap();
        assert!(m < 0.5, "{m}");
    }

    #[test]
    fn a_pale_hand_behind_outlined_text_does_not_count_as_ink() {
        // On the Thai clip a hand passing behind white text added "ink"
        // and split one caption into many; text ink has its outline nearby.
        let a = strokes(&[20, 50, 90, 140], 1);
        let mut b = strokes(&[20, 50, 90, 140], 4);
        for y in 0..60 {
            for x in 60..85 {
                b.put_pixel(x, y, Rgb([250, 248, 240]));
            }
        }
        let m = ink_match(&a, &b, WHOLE, [255, 255, 255], BLACK).unwrap();
        assert!(m > 0.9, "{m}");
        let plain = ink_match(&a, &b, WHOLE, [255, 255, 255], None).unwrap();
        assert!(plain < m, "without the outline the hand counts: {plain}");
    }

    #[test]
    fn a_background_the_colour_of_the_text_tells_nothing() {
        let white = RgbImage::from_pixel(200, 60, Rgb([250, 250, 250]));
        let a = strokes(&[20, 50], 1);
        assert_eq!(ink_match(&white, &a, WHOLE, [255, 255, 255], None), None);
        let bare = strokes(&[], 1);
        assert_eq!(ink_match(&bare, &a, WHOLE, [255, 255, 255], BLACK), None);
    }

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
    fn real_title_white_on_a_black_box_is_a_box_not_an_outline() {
        // The clip's persistent title, cropped from a 720x1280 frame.
        let s = real_crop(
            "title-box",
            PixelBox {
                x: 50,
                y: 44,
                w: 425,
                h: 29,
            },
        );
        assert!(close(&s.fill, "#FFFFFF", 16), "fill {}", s.fill);
        assert!(
            close(s.box_color.as_deref().unwrap_or("#808080"), "#000000", 16),
            "box {:?}",
            s.box_color
        );
        assert_eq!(s.outline, None, "fill {} box {:?}", s.fill, s.box_color);
    }

    #[test]
    fn text_on_open_scenery_has_no_box() {
        let navy = real_crop(
            "one-navy",
            PixelBox {
                x: 12,
                y: 12,
                w: 107,
                h: 39,
            },
        );
        assert_eq!(navy.box_color, None);
        let (img, bx) = caption_image([0x20, 0x20, 0x20], [255, 230, 0], None);
        assert_eq!(caption_colors(&img, bx).box_color, None);
    }

    #[test]
    fn hex_is_uppercase_rrggbb() {
        assert_eq!(hex([255, 230, 0]), "#FFE600");
    }

    #[test]
    fn hex_reads_back() {
        assert_eq!(rgb(&hex([255, 8, 171])), Some([255, 8, 171]));
        assert_eq!(rgb("white"), None);
    }
}
