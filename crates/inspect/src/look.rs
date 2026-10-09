//! Colour and brightness statistics ("look") and safe-zone clearance.

use std::collections::HashMap;

use model::{BoxN, Clearance, Look, LumaStats, SafeZone, Swatch};

use crate::style::hex;

/// Swatches whose colours are closer than this (Euclidean RGB) merge.
const MERGE_DISTANCE: f64 = 24.0;

fn luma(p: [u8; 3]) -> f64 {
    (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])) / 255.0
}

fn saturation(p: [u8; 3]) -> f64 {
    let max = f64::from(*p.iter().max().expect("three channels"));
    let min = f64::from(*p.iter().min().expect("three channels"));
    if max == 0.0 {
        0.0
    } else {
        (max - min) / max
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Look statistics for a set of sampled pixels.
pub fn measure(pixels: &[[u8; 3]]) -> Look {
    if pixels.is_empty() {
        return Look {
            dominant: Vec::new(),
            luma: LumaStats {
                mean: 0.0,
                min: 0.0,
                max: 0.0,
            },
            saturation: 0.0,
            warmth: 0.0,
        };
    }
    let n = pixels.len() as f64;
    let mut lumas: Vec<f64> = pixels.iter().map(|&p| luma(p)).collect();
    let mean = lumas.iter().sum::<f64>() / n;
    lumas.sort_by(f64::total_cmp);
    let pct = |q: f64| lumas[((lumas.len() - 1) as f64 * q).round() as usize];

    Look {
        dominant: dominant(pixels),
        luma: LumaStats {
            mean: round(mean),
            min: round(pct(0.01)),
            max: round(pct(0.99)),
        },
        saturation: round(pixels.iter().map(|&p| saturation(p)).sum::<f64>() / n),
        warmth: round(
            pixels
                .iter()
                .map(|p| (f64::from(p[0]) - f64::from(p[2])) / 255.0)
                .sum::<f64>()
                / n,
        ),
    }
}

/// Bins colours on a 16-level grid per channel, takes the fullest bins,
/// merges near-identical ones, and returns the top three by share.
fn dominant(pixels: &[[u8; 3]]) -> Vec<Swatch> {
    let mut bins: HashMap<[u8; 3], ([u64; 3], u64)> = HashMap::new();
    for p in pixels {
        let e = bins.entry(p.map(|c| c >> 4)).or_insert(([0; 3], 0));
        for (sum, &channel) in e.0.iter_mut().zip(p) {
            *sum += u64::from(channel);
        }
        e.1 += 1;
    }
    let mut ranked: Vec<([f64; 3], u64)> = bins
        .into_values()
        .map(|(sum, count)| (sum.map(|s| s as f64 / count as f64), count))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0[0].total_cmp(&b.0[0])));

    let mut merged: Vec<([f64; 3], u64)> = Vec::new();
    for (color, count) in ranked.into_iter().take(12) {
        let near = merged.iter_mut().find(|(c, _)| {
            c.iter()
                .zip(color)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt()
                < MERGE_DISTANCE
        });
        match near {
            Some((c, n)) => {
                let total = (*n + count) as f64;
                for i in 0..3 {
                    c[i] = (c[i] * *n as f64 + color[i] * count as f64) / total;
                }
                *n += count;
            }
            None => merged.push((color, count)),
        }
    }
    merged.sort_by_key(|m| std::cmp::Reverse(m.1));
    let total = pixels.len() as f64;
    merged
        .into_iter()
        .take(3)
        .map(|(c, count)| Swatch {
            hex: hex(c.map(|v| v.round() as u8)),
            share: round(count as f64 / total),
        })
        .collect()
}

/// How far `b` keeps from each UI band of `zone`.
pub fn clearance(b: &BoxN, zone: &SafeZone) -> Clearance {
    let top = b.top() - zone.top;
    let bottom = (1.0 - zone.bottom) - b.bottom();
    let left = b.left() - zone.left;
    let right = (1.0 - zone.right) - b.right();
    Clearance {
        top: round(top),
        right: round(right),
        bottom: round(bottom),
        left: round(left),
        inside: top >= 0.0 && bottom >= 0.0 && left >= 0.0 && right >= 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{BoxN, SafeZone};

    fn flat(color: [u8; 3], n: usize) -> Vec<[u8; 3]> {
        vec![color; n]
    }

    fn close(got: &str, want: &str, tol: u8) -> bool {
        let p = |s: &str| [1, 3, 5].map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap());
        p(got)
            .iter()
            .zip(p(want))
            .all(|(a, b)| a.abs_diff(b) <= tol)
    }

    #[test]
    fn three_flat_colours_are_the_three_dominant_colours_by_share() {
        let mut px = flat([0x1E, 0x2A, 0x38], 500);
        px.extend(flat([0xC9, 0xA9, 0x6E], 300));
        px.extend(flat([0x2E, 0x7D, 0x32], 200));
        let look = measure(&px);
        let hexes: Vec<&str> = look.dominant.iter().map(|s| s.hex.as_str()).collect();
        for (got, want) in hexes.iter().zip(["#1E2A38", "#C9A96E", "#2E7D32"]) {
            assert!(close(got, want, 2), "{got} vs {want}");
        }
        assert!((look.dominant[0].share - 0.5).abs() < 0.01);
    }

    #[test]
    fn near_identical_shades_are_one_dominant_colour() {
        let mut px = flat([200, 100, 50], 400);
        px.extend(flat([202, 101, 49], 400));
        px.extend(flat([10, 10, 10], 200));
        let look = measure(&px);
        assert!(
            (look.dominant[0].share - 0.8).abs() < 0.01,
            "{:?}",
            look.dominant
        );
    }

    #[test]
    fn luma_saturation_and_warmth() {
        let px = flat([255, 255, 255], 10);
        let white = measure(&px);
        assert!((white.luma.mean - 1.0).abs() < 1e-6);
        assert!(white.saturation.abs() < 1e-6);
        assert!(white.warmth.abs() < 1e-6);

        let orange = measure(&flat([255, 128, 0], 10));
        assert!((orange.saturation - 1.0).abs() < 1e-6);
        assert!((orange.warmth - 1.0).abs() < 1e-6, "R minus B over 255");

        let mut ramp: Vec<[u8; 3]> = (0..=255u8).map(|v| [v, v, v]).collect();
        ramp.extend(flat([0, 0, 0], 0));
        let r = measure(&ramp);
        assert!(r.luma.min < 0.02 && r.luma.max > 0.98);
    }

    #[test]
    fn empty_input_is_all_zero_not_a_panic() {
        let look = measure(&[]);
        assert!(look.dominant.is_empty());
        assert_eq!(look.luma.mean, 0.0);
    }

    fn zone() -> SafeZone {
        SafeZone::profile(model::probe::DEFAULT_SAFE_ZONE).unwrap()
    }

    #[test]
    fn a_caption_in_the_bottom_band_is_outside_the_safe_zone() {
        let c = clearance(
            &BoxN {
                cx: 0.5,
                cy: 0.90,
                w: 0.4,
                h: 0.05,
            },
            &zone(),
        );
        assert!(!c.inside);
        assert!(c.bottom < 0.0, "bottom clearance {}", c.bottom);
    }

    #[test]
    fn a_centred_caption_is_inside_with_positive_clearance_on_all_sides() {
        let c = clearance(
            &BoxN {
                cx: 0.45,
                cy: 0.50,
                w: 0.4,
                h: 0.05,
            },
            &zone(),
        );
        assert!(c.inside);
        // Right band is 165/1080 = 0.153; box right edge 0.65.
        assert!((c.right - (1.0 - 0.152_777 - 0.65)).abs() < 1e-3);
        assert!(c.top > 0.0 && c.bottom > 0.0 && c.left > 0.0);
    }
}
