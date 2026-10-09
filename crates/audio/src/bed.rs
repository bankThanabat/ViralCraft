//! Background beds: music or ambience that keeps playing under speech.
//!
//! Voice alone falls to near silence between syllables; a bed fills those
//! gaps. So a stretch has a bed when its quietest moments stay loud.

/// A stretch with continuous background sound, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bed {
    pub start: f64,
    pub end: f64,
    /// Typical level of the bed alone, dBFS.
    pub level_db: f64,
}

const FRAME_S: f64 = 0.05;
const WINDOW_S: f64 = 2.0;
const HOP_S: f64 = 0.5;
/// The quietest 10% of a window must stay above this for a bed. A voice
/// recording's room tone sits near -50 to -60 dB; the first real clip's
/// music bed never fell below -33 dB.
const FLOOR_DB: f64 = -40.0;

pub fn bed_spans(samples: &[f32], sample_rate: u32) -> Vec<Bed> {
    let frame = (FRAME_S * f64::from(sample_rate)) as usize;
    let levels: Vec<f64> = samples
        .chunks_exact(frame.max(1))
        .map(|c| {
            let rms =
                (c.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / c.len() as f64).sqrt();
            20.0 * (rms + 1e-9).log10()
        })
        .collect();
    let per_window = (WINDOW_S / FRAME_S) as usize;
    let per_hop = (HOP_S / FRAME_S) as usize;
    if levels.len() < per_window {
        return Vec::new();
    }

    let mut spans: Vec<Bed> = Vec::new();
    let mut floors: Vec<f64> = Vec::new();
    let mut start = 0;
    while start + per_window <= levels.len() {
        let mut window = levels[start..start + per_window].to_vec();
        window.sort_by(f64::total_cmp);
        let floor = window[window.len() / 10];
        let (t0, t1) = (
            start as f64 * FRAME_S,
            (start + per_window) as f64 * FRAME_S,
        );
        if floor > FLOOR_DB {
            match spans.last_mut() {
                Some(last) if t0 <= last.end => {
                    last.end = t1;
                    floors.push(floor);
                    last.level_db = floors.iter().sum::<f64>() / floors.len() as f64;
                }
                _ => {
                    floors = vec![floor];
                    spans.push(Bed {
                        start: t0,
                        end: t1,
                        level_db: floor,
                    });
                }
            }
        }
        start += per_hop;
    }
    for b in &mut spans {
        b.level_db = (b.level_db * 10.0).round() / 10.0;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    fn tone(f: f32, i: usize) -> f32 {
        (2.0 * std::f32::consts::PI * f * i as f32 / SR as f32).sin()
    }

    /// Speech-like: 200 ms voiced bursts, 150 ms pauses, a -60 dB room floor.
    fn speech(seconds: f32) -> Vec<f32> {
        let n = (seconds * SR as f32) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let voiced = (t % 0.35) < 0.2;
                let room = 0.001 * tone(97.0, i);
                if voiced {
                    0.3 * tone(180.0, i) + room
                } else {
                    room
                }
            })
            .collect()
    }

    /// A soft chord at roughly -30 dB.
    fn chord(i: usize) -> f32 {
        0.012 * (tone(220.0, i) + tone(277.2, i) + tone(329.6, i))
    }

    #[test]
    fn voice_alone_has_no_bed() {
        assert!(bed_spans(&speech(10.0), SR).is_empty());
    }

    #[test]
    fn voice_over_a_chord_is_one_bed_across_the_whole_clip() {
        let mix: Vec<f32> = speech(10.0)
            .iter()
            .enumerate()
            .map(|(i, s)| s + chord(i))
            .collect();
        let spans = bed_spans(&mix, SR);
        assert_eq!(spans.len(), 1, "{spans:?}");
        assert!(spans[0].start <= 0.5 && spans[0].end >= 9.5, "{spans:?}");
        assert!(
            spans[0].level_db > -40.0 && spans[0].level_db < -20.0,
            "{spans:?}"
        );
    }

    #[test]
    fn a_bed_that_starts_late_is_found_where_it_plays() {
        let mix: Vec<f32> = speech(12.0)
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let t = i as f32 / SR as f32;
                s + if (4.0..10.0).contains(&t) {
                    chord(i)
                } else {
                    0.0
                }
            })
            .collect();
        let spans = bed_spans(&mix, SR);
        assert_eq!(spans.len(), 1, "{spans:?}");
        assert!(
            (spans[0].start - 4.0).abs() <= 0.75 && (spans[0].end - 10.0).abs() <= 0.75,
            "{spans:?}"
        );
    }

    #[test]
    fn silence_and_tiny_input_have_no_bed() {
        assert!(bed_spans(&vec![0.0; 5 * SR as usize], SR).is_empty());
        assert!(bed_spans(&[0.5; 10], SR).is_empty());
    }
}
