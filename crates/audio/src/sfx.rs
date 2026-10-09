//! Names non-speech sounds from YAMNet's per-window class scores.
//!
//! YAMNet scores 521 AudioSet classes for each 0.96 s window, every 0.48 s.
//! Speech, singing, music and room tone are covered elsewhere and ignored;
//! what is left (typing, whoosh, pop, sigh, thud...) becomes a named sound.

const CLASS_MAP: &str = include_str!("../data/yamnet_class_map.csv");
/// YAMNet's analysis window; windows start every hop.
pub const WINDOW_S: f64 = 0.96;
pub const HOP_S: f64 = 0.48;
/// A class needs this score to be named.
const MIN_SCORE: f32 = 0.3;

/// Class indices covered by other measurements: speech (0-5, 12),
/// singing (24-31), music and instruments (132-276), silence (494) and room
/// tone or noise (500-520).
fn ignored(class: usize) -> bool {
    matches!(class, 0..=5 | 12 | 24..=31 | 132..=276 | 494 | 500..=520)
}

/// A named sound, in seconds from the start of the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedSound {
    pub label: String,
    pub start: f64,
    pub end: f64,
    pub score: f32,
}

/// The 521 display names, in model output order.
pub fn class_names() -> Vec<String> {
    CLASS_MAP
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.splitn(3, ',');
            parts.next()?;
            parts.next()?;
            Some(parts.next()?.trim().trim_matches('"').to_string())
        })
        .collect()
}

/// `Whoosh, swoosh, swish` -> `whoosh`.
pub fn short_label(display: &str) -> String {
    display
        .split(',')
        .next()
        .unwrap_or(display)
        .trim()
        .to_lowercase()
}

/// Turns per-window scores into named sounds. A sound continues while its
/// class still scores, even if a similar class edges ahead for a window.
pub fn name_sounds(scores: &[Vec<f32>], names: &[String], hop_s: f64) -> Vec<NamedSound> {
    let mut out: Vec<NamedSound> = Vec::new();
    let mut current: Option<(usize, usize, usize, f32)> = None;
    let close = |c: (usize, usize, usize, f32), out: &mut Vec<NamedSound>| {
        out.push(NamedSound {
            label: short_label(&names[c.0]),
            start: c.1 as f64 * hop_s,
            end: c.2 as f64 * hop_s + WINDOW_S,
            score: c.3,
        });
    };
    for (f, row) in scores.iter().enumerate() {
        if let Some(c) = current.as_mut() {
            if row[c.0] >= MIN_SCORE {
                c.2 = f;
                c.3 = c.3.max(row[c.0]);
                continue;
            }
        }
        if let Some(c) = current.take() {
            close(c, &mut out);
        }
        let best = row
            .iter()
            .enumerate()
            .filter(|(i, &v)| !ignored(*i) && v >= MIN_SCORE)
            .max_by(|a, b| a.1.total_cmp(b.1));
        if let Some((class, &score)) = best {
            current = Some((class, f, f, score));
        }
    }
    if let Some(c) = current {
        close(c, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        class_names()
    }

    fn idx(name: &str) -> usize {
        names()
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("no class {name}"))
    }

    /// `frames` windows of zeros with `(frame, class, score)` set.
    fn scores(frames: usize, set: &[(usize, &str, f32)]) -> Vec<Vec<f32>> {
        let mut s = vec![vec![0.0; 521]; frames];
        for (f, name, v) in set {
            s[*f][idx(name)] = *v;
        }
        s
    }

    #[test]
    fn the_class_map_has_521_names_with_quotes_removed() {
        let n = names();
        assert_eq!(n.len(), 521);
        assert_eq!(n[0], "Speech");
        assert_eq!(n[453], "Whoosh, swoosh, swish");
    }

    #[test]
    fn a_label_is_the_first_name_in_lower_case() {
        assert_eq!(short_label("Whoosh, swoosh, swish"), "whoosh");
        assert_eq!(short_label("Computer keyboard"), "computer keyboard");
        assert_eq!(short_label("Sigh"), "sigh");
    }

    #[test]
    fn speech_music_and_room_tone_are_never_named() {
        let s = scores(
            4,
            &[
                (0, "Speech", 0.99),
                (1, "Music", 0.9),
                (2, "Silence", 1.0),
                (3, "Inside, small room", 0.8),
            ],
        );
        assert!(name_sounds(&s, &names(), 0.48).is_empty());
    }

    #[test]
    fn a_whoosh_then_a_thud_are_two_sounds_with_window_times() {
        let s = scores(
            6,
            &[
                (2, "Music", 0.9),
                (3, "Whoosh, swoosh, swish", 0.66),
                (4, "Whoosh, swoosh, swish", 0.55),
                (4, "Arrow", 0.58),
                (5, "Thump, thud", 0.6),
            ],
        );
        let got = name_sounds(&s, &names(), 0.48);
        let labels: Vec<&str> = got.iter().map(|g| g.label.as_str()).collect();
        // The whoosh stays one sound while it still scores, even when a
        // similar class edges ahead for a window.
        assert_eq!(labels, ["whoosh", "thump"]);
        assert!((got[0].start - 3.0 * 0.48).abs() < 1e-9);
        assert!((got[0].end - (4.0 * 0.48 + 0.96)).abs() < 1e-9);
    }

    #[test]
    fn weak_scores_are_ignored() {
        let s = scores(3, &[(1, "Boing", 0.13)]);
        assert!(name_sounds(&s, &names(), 0.48).is_empty());
    }

    #[test]
    fn a_sigh_is_kept_because_it_is_not_speech() {
        let s = scores(2, &[(0, "Sigh", 0.5)]);
        assert_eq!(name_sounds(&s, &names(), 0.48)[0].label, "sigh");
    }
}
