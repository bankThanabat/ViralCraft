//! Tracks text-line boxes across frames and splits them into caption events.
//! Pure: detection, recognition and pixel comparison are passed in.

pub use model::{Animation, BoxN};

/// One text line followed across consecutive frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub start: u64,
    /// `boxes[i]` is the box on frame `start + i`.
    pub boxes: Vec<BoxN>,
}

impl Track {
    pub fn end(&self) -> u64 {
        self.start + self.boxes.len() as u64
    }
}

/// A run of a track with steady geometry and content.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
    /// The box once any intro animation has finished.
    pub settled: BoxN,
    pub animation: Option<Animation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabeledSpan {
    pub start: u64,
    pub end: u64,
    pub text: String,
    pub settled: BoxN,
    pub animation: Option<Animation>,
}

/// One on-screen caption, possibly several stacked lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub start: u64,
    pub end: u64,
    /// Top line first.
    pub lines: Vec<String>,
    pub line_boxes: Vec<BoxN>,
    pub bounds: BoxN,
    pub animation: Option<Animation>,
}

const MATCH_IOU: f64 = 0.3;
/// Frames a track may go undetected before it ends.
const MAX_GAP: u64 = 1;
const MIN_TRACK: usize = 3;
const JUMP: f64 = 0.15;
const INTRO_MAX: usize = 8;

/// Links boxes on consecutive frames into tracks.
pub fn track(frames: &[Vec<BoxN>]) -> Vec<Track> {
    struct Open {
        track: Track,
        last_seen: u64,
    }
    let mut open: Vec<Open> = Vec::new();
    let mut done: Vec<Track> = Vec::new();

    for (n, boxes) in frames.iter().enumerate() {
        let n = n as u64;
        let mut taken = vec![false; open.len()];
        for bx in boxes {
            let best = open
                .iter()
                .enumerate()
                .filter(|(i, o)| !taken[*i] && n - o.last_seen <= MAX_GAP + 1)
                .map(|(i, o)| (i, o.track.boxes.last().map_or(0.0, |l| l.iou(bx))))
                .filter(|(_, iou)| *iou > MATCH_IOU)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((i, _)) => {
                    taken[i] = true;
                    let o = &mut open[i];
                    let last = *o.track.boxes.last().expect("open tracks are never empty");
                    for _ in o.last_seen + 1..n {
                        o.track.boxes.push(last);
                    }
                    o.track.boxes.push(*bx);
                    o.last_seen = n;
                }
                None => {
                    open.push(Open {
                        track: Track {
                            start: n,
                            boxes: vec![*bx],
                        },
                        last_seen: n,
                    });
                    taken.push(true);
                }
            }
        }
        let (still, closed): (Vec<Open>, Vec<Open>) =
            open.into_iter().partition(|o| n - o.last_seen <= MAX_GAP);
        done.extend(closed.into_iter().map(|o| o.track));
        open = still;
    }
    done.extend(open.into_iter().map(|o| o.track));
    done.retain(|t| t.boxes.len() >= MIN_TRACK);
    done.sort_by_key(|t| t.start);
    done
}

fn jumped(a: &BoxN, b: &BoxN) -> bool {
    (b.w / a.w - 1.0).abs() > JUMP
        || (b.h / a.h - 1.0).abs() > JUMP
        || ((b.cx - a.cx).powi(2) + (b.cy - a.cy).powi(2)).sqrt() > 0.5 * a.h
}

/// Recognizes a scale-up or slide over the first frames of a track.
/// Returns the animation and how many frames it took.
fn intro(boxes: &[BoxN]) -> Option<(Animation, usize)> {
    let reference = boxes[boxes.len().min(INTRO_MAX + 4) - 1];
    let ratio = |b: &BoxN| b.w / reference.w;
    let first = boxes[0];

    if ratio(&first) < 0.92 {
        let k = (1..boxes.len().min(INTRO_MAX + 1)).find(|&k| ratio(&boxes[k]) >= 0.97)?;
        let rising = boxes[..=k].windows(2).all(|w| w[1].w >= w[0].w);
        return rising.then(|| {
            (
                Animation::PopIn {
                    frames: k as u64,
                    from_scale: (ratio(&first) * 1000.0).round() / 10.0,
                },
                k,
            )
        });
    }
    let distance =
        |b: &BoxN| ((b.cx - reference.cx).powi(2) + (b.cy - reference.cy).powi(2)).sqrt();
    if distance(&first) > 0.5 * reference.h && (ratio(&first) - 1.0).abs() < 0.08 {
        let k = (1..boxes.len().min(INTRO_MAX + 1))
            .find(|&k| distance(&boxes[k]) < 0.1 * reference.h)?;
        return Some((
            Animation::Slide {
                frames: k as u64,
                dx: first.cx - reference.cx,
                dy: first.cy - reference.cy,
            },
            k,
        ));
    }
    None
}

/// Splits a track wherever its box jumps or `changed(frame)` reports that
/// the pixels inside the box differ from the previous frame.
pub fn split_track(track: &Track, changed: impl Fn(u64) -> bool) -> Vec<Span> {
    let boxes = &track.boxes;
    let (animation, intro_end) = match intro(boxes) {
        Some((a, k)) => (Some(a), k),
        None => (None, 0),
    };

    let mut cuts = vec![0usize];
    for i in intro_end + 1..boxes.len() {
        if jumped(&boxes[i - 1], &boxes[i]) || changed(track.start + i as u64) {
            cuts.push(i);
        }
    }
    cuts.push(boxes.len());

    cuts.windows(2)
        .enumerate()
        .map(|(n, w)| {
            let steady_from = if n == 0 { w[0].max(intro_end) } else { w[0] };
            let steady = &boxes[steady_from..w[1]];
            Span {
                start: track.start + w[0] as u64,
                end: track.start + w[1] as u64,
                settled: steady[steady.len() / 2],
                animation: if n == 0 { animation } else { None },
            }
        })
        .collect()
}

/// Attaches recognized text and merges neighbours that read the same.
pub fn label(spans: Vec<Span>, mut recognize: impl FnMut(&Span) -> String) -> Vec<LabeledSpan> {
    let mut out: Vec<LabeledSpan> = Vec::new();
    for span in spans {
        let text = recognize(&span).trim().to_string();
        match out.last_mut() {
            Some(prev) if prev.end == span.start && prev.text == text => prev.end = span.end,
            _ => out.push(LabeledSpan {
                start: span.start,
                end: span.end,
                text,
                settled: span.settled,
                animation: span.animation,
            }),
        }
    }
    out
}

/// Letters or digits a line needs to count as text. Cartoon line art and
/// set details read as one or two characters; real captions rarely do.
const MIN_TEXT_CHARS: usize = 3;
/// Captions on screen for fewer frames than this are detection flicker.
const MIN_CAPTION_FRAMES: u64 = 6;

pub fn is_text_line(text: &str) -> bool {
    text.chars().filter(|c| c.is_alphanumeric()).count() >= MIN_TEXT_CHARS
}

/// Drops lines that are not text, before they can join a real caption.
pub fn filter_lines(spans: Vec<LabeledSpan>) -> Vec<LabeledSpan> {
    spans
        .into_iter()
        .filter(|s| is_text_line(&s.text))
        .collect()
}

/// Drops captions too brief to be read.
pub fn keep_captions(captions: Vec<Caption>) -> Vec<Caption> {
    captions
        .into_iter()
        .filter(|c| c.end - c.start >= MIN_CAPTION_FRAMES)
        .collect()
}

/// Groups lines that appear and disappear together and sit directly above
/// one another into one caption.
pub fn group_lines(mut spans: Vec<LabeledSpan>) -> Vec<Caption> {
    spans.sort_by(|a, b| a.settled.cy.total_cmp(&b.settled.cy));
    let mut captions: Vec<Caption> = Vec::new();
    for s in spans {
        let joins = captions.iter_mut().find(|c| {
            let last = c.line_boxes.last().expect("captions have a line");
            c.start.abs_diff(s.start) <= 2
                && c.end.abs_diff(s.end) <= 2
                && s.settled.top() - last.bottom() < last.h.max(s.settled.h)
                && (s.settled.cx - last.cx).abs() < 0.25
        });
        match joins {
            Some(c) => {
                c.start = c.start.min(s.start);
                c.end = c.end.max(s.end);
                c.bounds = c.bounds.union(&s.settled);
                c.lines.push(s.text);
                c.line_boxes.push(s.settled);
            }
            None => captions.push(Caption {
                start: s.start,
                end: s.end,
                lines: vec![s.text],
                line_boxes: vec![s.settled],
                bounds: s.settled,
                animation: s.animation,
            }),
        }
    }
    captions.sort_by_key(|c| c.start);
    captions
}

/// ASS numpad alignment for a box: 7 8 9 top, 4 5 6 middle, 1 2 3 bottom.
pub fn anchor(b: &BoxN) -> &'static str {
    let col = if (b.cx - 0.5).abs() < 0.1 {
        1
    } else if b.cx < 0.5 {
        0
    } else {
        2
    };
    let row = if b.cy < 1.0 / 3.0 {
        0
    } else if b.cy > 2.0 / 3.0 {
        2
    } else {
        1
    };
    [
        ["an7", "an8", "an9"],
        ["an4", "an5", "an6"],
        ["an1", "an2", "an3"],
    ][row][col]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(cx: f64, cy: f64, w: f64, h: f64) -> BoxN {
        BoxN { cx, cy, w, h }
    }

    /// `n` frames, with `boxes(frame)` giving that frame's detections.
    fn frames(n: u64, boxes: impl Fn(u64) -> Vec<BoxN>) -> Vec<Vec<BoxN>> {
        (0..n).map(boxes).collect()
    }

    const BASE: BoxN = BoxN {
        cx: 0.5,
        cy: 0.18,
        w: 0.40,
        h: 0.05,
    };

    fn scaled(s: f64) -> BoxN {
        b(BASE.cx, BASE.cy, BASE.w * s, BASE.h * s)
    }

    #[test]
    fn a_box_present_on_consecutive_frames_is_one_track() {
        let f = frames(50, |n| {
            if (10..40).contains(&n) {
                vec![BASE]
            } else {
                vec![]
            }
        });
        let tracks = track(&f);
        assert_eq!(tracks.len(), 1);
        assert_eq!((tracks[0].start, tracks[0].end()), (10, 40));
    }

    #[test]
    fn a_single_missed_detection_does_not_break_a_track() {
        let f = frames(50, |n| {
            if (10..40).contains(&n) && n != 25 {
                vec![BASE]
            } else {
                vec![]
            }
        });
        assert_eq!(track(&f).len(), 1);
    }

    #[test]
    fn tracks_shorter_than_three_frames_are_noise() {
        let f = frames(20, |n| if n == 5 || n == 6 { vec![BASE] } else { vec![] });
        assert!(track(&f).is_empty());
    }

    #[test]
    fn two_boxes_far_apart_are_two_tracks() {
        let low = b(0.5, 0.80, 0.30, 0.05);
        let f = frames(30, |_| vec![BASE, low]);
        assert_eq!(track(&f).len(), 2);
    }

    #[test]
    fn a_scale_up_at_the_start_is_a_pop_in_not_a_split() {
        let scales = [0.80, 0.87, 0.93, 1.0];
        let f = frames(50, |n| match n {
            10..=13 => vec![scaled(scales[(n - 10) as usize])],
            14..=39 => vec![BASE],
            _ => vec![],
        });
        let tracks = track(&f);
        let spans = split_track(&tracks[0], |_| false);

        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end), (10, 40));
        assert_eq!(
            spans[0].animation,
            Some(Animation::PopIn {
                frames: 3,
                from_scale: 80.0
            })
        );
        assert_eq!(spans[0].settled, BASE);
    }

    #[test]
    fn changed_pixels_inside_a_steady_box_split_the_event() {
        let f = frames(40, |n| {
            if (10..30).contains(&n) {
                vec![BASE]
            } else {
                vec![]
            }
        });
        let tracks = track(&f);
        let spans = split_track(&tracks[0], |frame| frame == 20);
        let ranges: Vec<(u64, u64)> = spans.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(ranges, [(10, 20), (20, 30)]);
        assert!(spans.iter().all(|s| s.animation.is_none()));
    }

    #[test]
    fn a_width_jump_mid_track_splits_the_event() {
        // Same position, a longer phrase replaces a short one.
        let wide = b(BASE.cx, BASE.cy, 0.70, BASE.h);
        let f = frames(40, |n| match n {
            10..=19 => vec![BASE],
            20..=29 => vec![wide],
            _ => vec![],
        });
        let tracks = track(&f);
        assert_eq!(tracks.len(), 1, "same line, one track");
        let spans = split_track(&tracks[0], |_| false);
        assert_eq!(
            spans.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>(),
            [(10, 20), (20, 30)]
        );
    }

    #[test]
    fn neighbouring_events_with_the_same_text_merge() {
        let spans = vec![
            Span {
                start: 10,
                end: 20,
                settled: BASE,
                animation: None,
            },
            Span {
                start: 20,
                end: 25,
                settled: BASE,
                animation: None,
            },
            Span {
                start: 25,
                end: 30,
                settled: BASE,
                animation: None,
            },
        ];
        let events = label(spans, |s| match s.start {
            10 | 20 => "STOP".to_string(),
            _ => "SCROLLING".to_string(),
        });
        let got: Vec<(u64, u64, &str)> = events
            .iter()
            .map(|e| (e.start, e.end, e.text.as_str()))
            .collect();
        assert_eq!(got, [(10, 25, "STOP"), (25, 30, "SCROLLING")]);
    }

    #[test]
    fn stacked_lines_with_the_same_timing_become_one_caption() {
        let top = LabeledSpan {
            start: 10,
            end: 30,
            text: "STOP".into(),
            settled: b(0.5, 0.40, 0.3, 0.05),
            animation: None,
        };
        let bottom = LabeledSpan {
            start: 11,
            end: 30,
            text: "SCROLLING".into(),
            settled: b(0.5, 0.46, 0.5, 0.05),
            animation: None,
        };
        let far = LabeledSpan {
            start: 10,
            end: 30,
            text: "@handle".into(),
            settled: b(0.2, 0.90, 0.2, 0.03),
            animation: None,
        };
        let captions = group_lines(vec![bottom, top, far]);

        assert_eq!(captions.len(), 2);
        let stacked = captions.iter().find(|c| c.lines.len() == 2).unwrap();
        assert_eq!(stacked.lines, ["STOP", "SCROLLING"], "top line first");
        assert_eq!((stacked.start, stacked.end), (10, 30));
        assert!((stacked.bounds.cy - 0.43).abs() < 1e-9);
        assert!((stacked.bounds.h - 0.11).abs() < 1e-9);
    }

    #[test]
    fn line_art_read_as_a_letter_or_two_is_not_text() {
        for noise in ["T", "7", "w", "dC", "1", "?!", "  C  "] {
            assert!(!is_text_line(noise), "{noise:?} should be noise");
        }
        for real in [
            "BUT",
            "like",
            "you know?",
            "@theintrovertedattorney",
            "1... think that's everything!",
        ] {
            assert!(is_text_line(real), "{real:?} should be text");
        }
    }

    #[test]
    fn captions_shorter_than_six_frames_are_flicker() {
        let line = |start: u64, end: u64, text: &str| LabeledSpan {
            start,
            end,
            text: text.into(),
            settled: BASE,
            animation: None,
        };
        let spans = vec![
            line(10, 15, "flicker"),
            line(20, 26, "steady"),
            line(30, 60, "T"),
        ];
        let captions = keep_captions(group_lines(filter_lines(spans)));
        let texts: Vec<&str> = captions.iter().map(|c| c.lines[0].as_str()).collect();
        assert_eq!(texts, ["steady"]);
    }

    #[test]
    fn numpad_anchor_follows_position() {
        assert_eq!(anchor(&b(0.50, 0.14, 0.4, 0.05)), "an8");
        assert_eq!(anchor(&b(0.50, 0.50, 0.4, 0.05)), "an5");
        assert_eq!(anchor(&b(0.50, 0.85, 0.4, 0.05)), "an2");
        assert_eq!(anchor(&b(0.20, 0.85, 0.2, 0.05)), "an1");
        assert_eq!(anchor(&b(0.80, 0.10, 0.2, 0.05)), "an9");
    }
}
