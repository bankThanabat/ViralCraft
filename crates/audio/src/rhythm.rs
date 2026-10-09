//! Onsets, tempo and beats from mono samples. Output follows the JAMS
//! `onset`, `tempo` and `beat` namespaces: times in seconds, confidence 0..1.

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

/// Envelope frames per second.
const FRAME_RATE: u32 = 100;
const WINDOW_S: f64 = 0.032;
const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;
/// Centre of the tempo prior, as in librosa: half/double ambiguity resolves
/// towards the tempo nearest 120 bpm.
const PRIOR_BPM: f64 = 120.0;
/// Below this tempo confidence no beat grid is reported.
const MIN_BEAT_CONFIDENCE: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub time: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    pub bpm: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rhythm {
    pub tempo: Tempo,
    pub beats: Vec<Event>,
    pub onsets: Vec<Event>,
}

pub fn analyze(samples: &[f32], sample_rate: u32) -> Rhythm {
    let hop = (sample_rate / FRAME_RATE) as usize;
    let window = ((WINDOW_S * f64::from(sample_rate)) as usize).next_power_of_two();
    let none = Rhythm {
        tempo: Tempo {
            bpm: 0.0,
            confidence: 0.0,
        },
        beats: Vec::new(),
        onsets: Vec::new(),
    };
    if samples.len() < window * 2 {
        return none;
    }

    let flux = spectral_flux(samples, hop, window);
    let onsets = pick_onsets(&flux, samples, sample_rate, hop, window);
    let tempo = estimate_tempo(&flux);
    if tempo.confidence < MIN_BEAT_CONFIDENCE {
        return Rhythm {
            tempo,
            beats: Vec::new(),
            onsets,
        };
    }
    let beats = track_beats(
        &flux,
        &onsets,
        tempo,
        samples.len() as f64 / f64::from(sample_rate),
    );
    Rhythm {
        tempo,
        beats,
        onsets,
    }
}

/// Positive change in log-compressed magnitude per centred frame.
fn spectral_flux(samples: &[f32], hop: usize, window: usize) -> Vec<f64> {
    let fft = FftPlanner::<f32>::new().plan_fft_forward(window);
    let hann: Vec<f32> = (0..window)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / window as f32).cos())
        .collect();
    let frames = samples.len() / hop;
    let bins = window / 2 + 1;
    let mut previous = vec![0.0f32; bins];
    let mut flux = Vec::with_capacity(frames);
    let mut buf = vec![Complex::new(0.0f32, 0.0); window];
    for n in 0..frames {
        let centre = (n * hop) as isize;
        for (i, slot) in buf.iter_mut().enumerate() {
            let idx = centre - (window / 2) as isize + i as isize;
            let x = if idx >= 0 {
                samples.get(idx as usize).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            *slot = Complex::new(x * hann[i], 0.0);
        }
        fft.process(&mut buf);
        let mut sum = 0.0f64;
        for (k, prev) in previous.iter_mut().enumerate() {
            let mag = (100.0 * buf[k].norm()).ln_1p();
            if n > 0 {
                sum += f64::from((mag - *prev).max(0.0));
            }
            *prev = mag;
        }
        flux.push(sum);
    }
    flux
}

fn pick_onsets(flux: &[f64], samples: &[f32], sr: u32, hop: usize, window: usize) -> Vec<Event> {
    let max = flux.iter().copied().fold(0.0, f64::max);
    if max <= 1e-9 {
        return Vec::new();
    }
    let local = 15usize;
    let mut onsets: Vec<Event> = Vec::new();
    let mut last_frame: Option<usize> = None;
    for n in 1..flux.len().saturating_sub(1) {
        let lo = n.saturating_sub(3);
        let hi = (n + 4).min(flux.len());
        if flux[n] < flux[lo..hi].iter().copied().fold(0.0, f64::max) {
            continue;
        }
        let (a, b) = (n.saturating_sub(local), (n + local + 1).min(flux.len()));
        let mean = flux[a..b].iter().sum::<f64>() / (b - a) as f64;
        if flux[n] < 1.5 * mean || flux[n] < 0.1 * max {
            continue;
        }
        if last_frame.is_some_and(|l| n - l < 5) {
            continue;
        }
        last_frame = Some(n);
        onsets.push(Event {
            time: refine_onset(samples, sr, n * hop, window, hop),
            confidence: (flux[n] / max).min(1.0),
        });
    }
    onsets
}

/// The flux peak only says "somewhere in this window". Find where the
/// sample envelope first rises halfway from its floor to its peak.
fn refine_onset(samples: &[f32], sr: u32, centre: usize, window: usize, hop: usize) -> f64 {
    let start = centre.saturating_sub(window / 2 + hop);
    let end = (centre + window / 2 + hop).min(samples.len());
    let step = (sr / 1000).max(1) as usize;
    let env: Vec<f32> = samples[start..end]
        .chunks(step)
        .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())))
        .collect();
    let peak = env.iter().copied().fold(0.0f32, f32::max);
    let mut sorted = env.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let level = floor + 0.5 * (peak - floor);
    let i = env.iter().position(|&e| e >= level).unwrap_or(0);
    (start + i * step) as f64 / f64::from(sr)
}

/// Autocorrelation of the mean-removed envelope, weighted by a log-normal
/// prior around 120 bpm, refined to a fractional lag.
fn estimate_tempo(flux: &[f64]) -> Tempo {
    let fr = f64::from(FRAME_RATE);
    let mean = flux.iter().sum::<f64>() / flux.len() as f64;
    let x: Vec<f64> = flux.iter().map(|f| f - mean).collect();
    let acf = |lag: usize| -> f64 {
        if lag >= x.len() {
            return 0.0;
        }
        x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f64>() / (x.len() - lag) as f64
    };
    let zero = acf(0);
    let none = Tempo {
        bpm: 0.0,
        confidence: 0.0,
    };
    if zero <= 1e-12 {
        return none;
    }
    let min_lag = (60.0 * fr / MAX_BPM).floor() as usize;
    let max_lag = (60.0 * fr / MIN_BPM).ceil() as usize;
    let mut best: Option<(usize, f64)> = None;
    for lag in min_lag..=max_lag {
        let r = acf(lag);
        if r <= 0.0 {
            continue;
        }
        let bpm = 60.0 * fr / lag as f64;
        let weight = (-0.5 * (bpm / PRIOR_BPM).log2().powi(2)).exp();
        if best.map_or(true, |(_, s)| r * weight > s) {
            best = Some((lag, r * weight));
        }
    }
    let Some((lag, _)) = best else { return none };
    let (a, b, c) = (acf(lag - 1), acf(lag), acf(lag + 1));
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-12 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Tempo {
        bpm: 60.0 * fr / (lag as f64 + shift),
        confidence: (b / zero).clamp(0.0, 1.0),
    }
}

/// Picks the grid phase with the most envelope energy, then moves each grid
/// beat to the nearest onset within a tenth of a period.
fn track_beats(flux: &[f64], onsets: &[Event], tempo: Tempo, duration: f64) -> Vec<Event> {
    let period = 60.0 / tempo.bpm;
    let fr = f64::from(FRAME_RATE);
    let at = |t: f64| flux.get((t * fr).round() as usize).copied().unwrap_or(0.0);
    let steps = (period * fr).ceil() as usize;
    let phase = (0..steps)
        .map(|i| i as f64 / fr)
        .max_by(|&a, &b| {
            let score = |p: f64| {
                let mut t = p;
                let mut sum = 0.0;
                while t < duration {
                    sum += at(t);
                    t += period;
                }
                sum
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(0.0);

    let tolerance = period * 0.1;
    let mut beats = Vec::new();
    let mut t = phase;
    while t < duration {
        let nearest = onsets
            .iter()
            .filter(|o| (o.time - t).abs() <= tolerance)
            .min_by(|a, b| (a.time - t).abs().total_cmp(&(b.time - t).abs()));
        beats.push(match nearest {
            Some(o) => Event {
                time: o.time,
                confidence: o.confidence * tempo.confidence,
            },
            None => Event {
                time: t,
                confidence: 0.3 * tempo.confidence,
            },
        });
        t += period;
    }
    beats
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    /// Deterministic noise so tests are repeatable.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    fn clicks(seconds: f64, bpm: f64, first: f64) -> Vec<f32> {
        let mut out = vec![0.0f32; (seconds * f64::from(SR)) as usize];
        let mut t = first;
        while t < seconds {
            let start = (t * f64::from(SR)) as usize;
            for i in 0..(SR as usize / 100) {
                if let Some(s) = out.get_mut(start + i) {
                    let env = (-(i as f32) / (0.002 * SR as f32)).exp();
                    *s += 0.6
                        * env
                        * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / SR as f32).sin();
                }
            }
            t += 60.0 / bpm;
        }
        out
    }

    fn assert_beats_on_grid(r: &Rhythm, period: f64, first: f64, tol: f64) {
        assert!(r.beats.len() >= 8, "only {} beats", r.beats.len());
        for b in &r.beats {
            let k = ((b.time - first) / period).round();
            let err = (b.time - (first + k * period)).abs();
            assert!(
                err <= tol,
                "beat at {:.3}s is {:.0} ms off the grid",
                b.time,
                err * 1000.0
            );
        }
    }

    #[test]
    fn a_120_bpm_click_track_is_120_bpm_with_beats_on_the_clicks() {
        let r = analyze(&clicks(10.0, 120.0, 0.25), SR);
        assert!((r.tempo.bpm - 120.0).abs() <= 1.0, "bpm {}", r.tempo.bpm);
        assert!(
            r.tempo.confidence > 0.5,
            "confidence {}",
            r.tempo.confidence
        );
        assert_beats_on_grid(&r, 0.5, 0.25, 0.020);
    }

    #[test]
    fn a_noisy_click_track_still_finds_the_tempo() {
        let mut samples = clicks(10.0, 120.0, 0.0);
        let mut rng = Lcg(42);
        // Noise at -10 dB relative to the click peak.
        for s in &mut samples {
            *s += 0.6 * 0.316 * rng.next();
        }
        let r = analyze(&samples, SR);
        assert!((r.tempo.bpm - 120.0).abs() <= 2.0, "bpm {}", r.tempo.bpm);
    }

    #[test]
    fn other_tempos_are_not_pulled_to_120() {
        for bpm in [90.0, 150.0] {
            let r = analyze(&clicks(12.0, bpm, 0.1), SR);
            assert!(
                (r.tempo.bpm - bpm).abs() <= 1.5,
                "{bpm} bpm detected as {}",
                r.tempo.bpm
            );
        }
    }

    #[test]
    fn silence_has_no_confident_tempo_and_no_beats() {
        let r = analyze(&vec![0.0; 10 * SR as usize], SR);
        assert!(r.tempo.confidence < 0.2);
        assert!(r.beats.is_empty());
        assert!(r.onsets.is_empty());
    }

    #[test]
    fn every_click_is_an_onset() {
        let r = analyze(&clicks(4.0, 120.0, 0.25), SR);
        let times: Vec<f64> = r.onsets.iter().map(|o| o.time).collect();
        assert_eq!(times.len(), 8, "{times:?}");
        for (i, t) in times.iter().enumerate() {
            assert!((t - (0.25 + 0.5 * i as f64)).abs() <= 0.020, "onset {t}");
        }
    }

    #[test]
    fn very_short_input_does_not_panic() {
        let r = analyze(&[0.1; 100], SR);
        assert!(r.beats.is_empty());
    }
}
