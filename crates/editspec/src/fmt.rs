//! Small formatting rules shared by every section.

use model::{RationalTime, Transform};

/// `1.800s (f54)`: seconds for people, the frame for tools.
pub fn time(t: RationalTime) -> String {
    format!("{:.3}s (f{})", t.to_seconds(), t.value)
}

pub fn seconds(t: RationalTime) -> String {
    format!("{:.3}s", t.to_seconds())
}

/// One decimal at most, no trailing `.0`.
pub fn num(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// A normalized coordinate or fraction, two decimals.
pub fn norm(v: f64) -> String {
    format!("{v:.2}")
}

/// Text safe to put in a table cell.
pub fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " / ")
}

/// The camera transform of a shot in one line.
pub fn transform_summary(t: Option<&Transform>) -> String {
    let Some(t) = t else {
        return "unknown".to_string();
    };
    if t.scale.len() < 2 {
        let s = t.scale.first().map_or(100.0, |k| k.s);
        return format!("static, scale {}%", num(s));
    }
    let scales: Vec<String> = t.scale.iter().map(|k| format!("{}%", num(k.s))).collect();
    let frames: Vec<String> = t.scale.iter().map(|k| format!("f{}", k.t)).collect();
    let eases: Vec<String> = t
        .scale
        .iter()
        .filter_map(|k| Some((k.o?, k.i?)))
        .map(|(o, i)| {
            format!(
                "cubic-bezier({:.2}, {:.2}, {:.2}, {:.2})",
                o.x, o.y, i.x, i.y
            )
        })
        .collect();
    format!(
        "scale {} ({}), {}",
        scales.join(" → "),
        frames.join(" → "),
        eases.join(" then ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{Ease, Fps, Keyframe, RationalTime, Transform};

    const FPS30: Fps = Fps { num: 30, den: 1 };

    #[test]
    fn times_show_seconds_and_the_frame() {
        assert_eq!(time(RationalTime::from_frames(54, FPS30)), "1.800s (f54)");
        assert_eq!(time(RationalTime::from_frames(0, FPS30)), "0.000s (f0)");
    }

    #[test]
    fn durations_are_seconds_only() {
        assert_eq!(seconds(RationalTime::from_frames(45, FPS30)), "1.500s");
    }

    fn key(t: i64, s: f64, ease: bool) -> Keyframe<f64> {
        Keyframe {
            t,
            s,
            o: ease.then_some(Ease { x: 0.0, y: 0.0 }),
            i: ease.then_some(Ease { x: 0.58, y: 1.0 }),
        }
    }

    fn transform(scale: Vec<Keyframe<f64>>) -> Transform {
        Transform {
            anchor: [0.5, 0.5],
            scale,
            position: vec![Keyframe {
                t: 0,
                s: [0.5, 0.5],
                o: None,
                i: None,
            }],
        }
    }

    #[test]
    fn a_missing_transform_is_unknown() {
        assert_eq!(transform_summary(None), "unknown");
    }

    #[test]
    fn a_single_keyframe_is_static() {
        assert_eq!(
            transform_summary(Some(&transform(vec![key(0, 100.0, false)]))),
            "static, scale 100%"
        );
    }

    #[test]
    fn a_zoom_names_its_scales_frames_and_easing() {
        let t = transform(vec![key(0, 100.0, true), key(89, 119.9, false)]);
        assert_eq!(
            transform_summary(Some(&t)),
            "scale 100% → 119.9% (f0 → f89), cubic-bezier(0.00, 0.00, 0.58, 1.00)"
        );
    }

    #[test]
    fn a_zoom_and_return_lists_every_keyframe() {
        let t = transform(vec![
            key(0, 100.0, true),
            key(30, 115.0, true),
            key(60, 100.0, false),
        ]);
        assert!(
            transform_summary(Some(&t)).starts_with("scale 100% → 115% → 100% (f0 → f30 → f60)")
        );
    }

    #[test]
    fn numbers_drop_needless_zeros() {
        assert_eq!(num(100.0), "100");
        assert_eq!(num(119.94), "119.9");
        assert_eq!(norm(0.5), "0.50");
    }
}
