//! Turns per-frame scene-change scores and luma into shots and transitions.

use model::{Candidate, Fps, Segment, Shots, TimeRange, Transition, TransitionKind};

/// One line group from `signalstats,scdet,metadata=print`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStat {
    pub frame: u64,
    /// scdet score, 0..100.
    pub score: f64,
    /// Mean luma, 0..255.
    pub luma: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("frame {0} has no lavfi.scd.score line")]
    NoScore(u64),
    #[error("cannot parse line {0:?}")]
    Line(String),
}

/// Frames whose score reaches this are listed as candidates.
const CANDIDATE_MIN: f64 = 1.0;
/// Per-frame score range that counts towards a dissolve.
const DISSOLVE_MIN: f64 = 2.0;
const DISSOLVE_MIN_FRAMES: usize = 4;
/// A flash frame is this much brighter than both neighbours.
const FLASH_DELTA: f64 = 60.0;
/// Neighbours of a flash within this luma are the same scene.
const SAME_SCENE_LUMA: f64 = 15.0;

pub fn parse_metadata(text: &str) -> Result<Vec<FrameStat>, ParseError> {
    let mut out: Vec<FrameStat> = Vec::new();
    let mut current: Option<(u64, Option<f64>, f64)> = None;
    let finish = |c: Option<(u64, Option<f64>, f64)>, out: &mut Vec<FrameStat>| {
        if let Some((frame, score, luma)) = c {
            let score = score.ok_or(ParseError::NoScore(frame))?;
            out.push(FrameStat { frame, score, luma });
        }
        Ok::<(), ParseError>(())
    };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix("frame:") {
            finish(current.take(), &mut out)?;
            let n = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| ParseError::Line(line.to_string()))?;
            current = Some((n, None, 0.0));
        } else if let Some((key, value)) = line.split_once('=') {
            let Some(c) = current.as_mut() else { continue };
            let parse = || {
                value
                    .parse::<f64>()
                    .map_err(|_| ParseError::Line(line.to_string()))
            };
            match key {
                "lavfi.scd.score" => c.1 = Some(parse()?),
                "lavfi.signalstats.YAVG" => c.2 = parse()?,
                _ => {}
            }
        }
    }
    finish(current, &mut out)?;
    Ok(out)
}

pub fn detect(stats: &[FrameStat], rate: Fps, threshold: f64) -> Shots {
    let total = stats.len();
    let mut boundaries: Vec<u64> = Vec::new();
    let mut transitions: Vec<Transition> = Vec::new();
    // Frames whose score is explained by a flash and must not become cuts.
    let mut consumed = vec![false; total];

    for n in 1..total.saturating_sub(1) {
        let (before, here, after) = (stats[n - 1].luma, stats[n].luma, stats[n + 1].luma);
        if here - before > FLASH_DELTA && here - after > FLASH_DELTA {
            transitions.push(Transition {
                kind: TransitionKind::Flash,
                range: TimeRange::from_frames(n as i64, 1, rate),
            });
            consumed[n] = true;
            consumed[n + 1] = true;
            if (after - before).abs() > SAME_SCENE_LUMA {
                boundaries.push(n as u64);
            }
        }
    }

    let mut run_start = None;
    for n in 0..=total {
        let in_run = n < total
            && !consumed[n]
            && stats[n].score >= DISSOLVE_MIN
            && stats[n].score < threshold;
        match (in_run, run_start) {
            (true, None) => run_start = Some(n),
            (false, Some(start)) => {
                let len = n - start;
                let sum: f64 = stats[start..n].iter().map(|s| s.score).sum();
                if len >= DISSOLVE_MIN_FRAMES && sum >= threshold {
                    transitions.push(Transition {
                        kind: TransitionKind::Dissolve,
                        range: TimeRange::from_frames(start as i64, len as i64, rate),
                    });
                    boundaries.push((start + len / 2) as u64);
                }
                run_start = None;
            }
            _ => {}
        }
    }

    let mut candidates = Vec::new();
    for (n, stat) in stats.iter().enumerate().skip(1) {
        if stat.score < CANDIDATE_MIN {
            continue;
        }
        let accepted = !consumed[n] && stat.score >= threshold;
        if accepted {
            boundaries.push(stat.frame);
        }
        candidates.push(Candidate {
            frame: stat.frame,
            score: stat.score,
            luma: stat.luma,
            accepted,
        });
    }

    boundaries.sort_unstable();
    boundaries.dedup();
    transitions.sort_by_key(|t| t.range.start.value);

    let mut edges = vec![0u64];
    edges.extend(
        boundaries
            .into_iter()
            .filter(|&b| b > 0 && b < total as u64),
    );
    edges.push(total as u64);
    let segments = edges
        .windows(2)
        .enumerate()
        .map(|(i, w)| Segment {
            n: i as u32 + 1,
            range: TimeRange::from_frames(w[0] as i64, (w[1] - w[0]) as i64, rate),
        })
        .collect();

    Shots {
        version: 1,
        rate,
        frames: total as i64,
        threshold,
        segments,
        transitions,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{Fps, TransitionKind};

    const FPS30: Fps = Fps { num: 30, den: 1 };

    /// Frames with (frame, score, luma) events. A score applies to its frame
    /// only; a luma holds until the next event, as a scene's brightness does.
    fn stats(total: u64, events: &[(u64, f64, f64)]) -> Vec<FrameStat> {
        let mut luma = 40.0;
        (0..total)
            .map(|frame| {
                let event = events.iter().find(|e| e.0 == frame);
                if let Some(e) = event {
                    luma = e.2;
                }
                FrameStat {
                    frame,
                    score: event.map_or(0.0, |e| e.1),
                    luma,
                }
            })
            .collect()
    }

    #[test]
    fn parses_metadata_print_output() {
        let text = "frame:0    pts:0       pts_time:0\n\
                    lavfi.signalstats.YMIN=16\n\
                    lavfi.signalstats.YAVG=40.5\n\
                    lavfi.scd.mafd=0.000\n\
                    lavfi.scd.score=0.000\n\
                    frame:1    pts:512     pts_time:0.0333333\n\
                    lavfi.signalstats.YAVG=164.007\n\
                    lavfi.scd.score=44.524\n";
        let parsed = parse_metadata(text).unwrap();
        assert_eq!(
            parsed,
            [
                FrameStat {
                    frame: 0,
                    score: 0.0,
                    luma: 40.5
                },
                FrameStat {
                    frame: 1,
                    score: 44.524,
                    luma: 164.007
                },
            ]
        );
    }

    #[test]
    fn a_frame_without_a_score_is_an_error() {
        assert!(parse_metadata("frame:0 pts:0\nlavfi.signalstats.YAVG=1\n").is_err());
    }

    #[test]
    fn scores_over_the_threshold_split_shots() {
        let s = stats(180, &[(60, 44.5, 160.0), (120, 30.0, 90.0)]);
        let shots = detect(&s, FPS30, 10.0);

        let ranges: Vec<(i64, i64)> = shots
            .segments
            .iter()
            .map(|seg| (seg.range.start.value, seg.range.end_frame()))
            .collect();
        assert_eq!(ranges, [(0, 60), (60, 120), (120, 180)]);
        assert_eq!(
            shots.segments.iter().map(|s| s.n).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(shots.transitions.is_empty());
    }

    #[test]
    fn every_candidate_keeps_its_score_even_below_threshold() {
        let s = stats(100, &[(40, 6.0, 40.0), (70, 25.0, 120.0)]);
        let shots = detect(&s, FPS30, 10.0);
        let kept: Vec<(u64, bool)> = shots
            .candidates
            .iter()
            .map(|c| (c.frame, c.accepted))
            .collect();
        assert_eq!(kept, [(40, false), (70, true)]);
        assert_eq!(shots.segments.len(), 2);
    }

    #[test]
    fn a_run_of_moderate_scores_is_one_dissolve_with_one_boundary() {
        let run: Vec<(u64, f64, f64)> = (100..108)
            .map(|f| (f, 3.0, 40.0 + (f - 100) as f64 * 10.0))
            .collect();
        let shots = detect(&stats(200, &run), FPS30, 10.0);

        assert_eq!(shots.segments.len(), 2);
        assert_eq!(
            shots.segments[1].range.start.value, 104,
            "boundary at the dissolve middle"
        );
        assert_eq!(shots.transitions.len(), 1);
        let t = &shots.transitions[0];
        assert_eq!(t.kind, TransitionKind::Dissolve);
        assert_eq!((t.range.start.value, t.range.duration.value), (100, 8));
    }

    #[test]
    fn a_one_frame_luma_spike_is_a_flash_not_two_cuts() {
        // Frame 50 flashes white; the change in and the change out both score high.
        let s = stats(100, &[(50, 60.0, 235.0), (51, 60.0, 40.0)]);
        let shots = detect(&s, FPS30, 10.0);

        assert_eq!(shots.segments.len(), 1, "a flash does not start a new shot");
        assert_eq!(shots.transitions.len(), 1);
        assert_eq!(shots.transitions[0].kind, TransitionKind::Flash);
        assert_eq!(shots.transitions[0].range.start.value, 50);
    }

    #[test]
    fn a_flash_into_a_new_scene_is_a_cut_with_a_flash() {
        // Frame 50 is white, frame 51 starts a different scene (luma 120).
        let s = stats(100, &[(50, 60.0, 235.0), (51, 60.0, 120.0)]);
        let shots = detect(&s, FPS30, 10.0);

        assert_eq!(shots.segments.len(), 2);
        assert_eq!(shots.segments[1].range.start.value, 50);
        assert_eq!(shots.transitions[0].kind, TransitionKind::Flash);
    }
}
