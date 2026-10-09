//! WebVTT captions: one cue per caption, placed with cue settings, with
//! inline word timestamps where the caption's words were spoken.

use std::fmt::Write;

use model::{Audio, Inspect, Probe, Word};

pub fn vtt_time(seconds: f64) -> String {
    let ms = (seconds * 1000.0).round().max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn parse_time(t: &str) -> Option<f64> {
    let (hms, ms) = t.split_once('.')?;
    let parts: Vec<f64> = hms
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let ms: f64 = ms.parse().ok()?;
    let secs = match parts.as_slice() {
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        [m, s] => m * 60.0 + s,
        _ => return None,
    };
    Some(secs + ms / 1000.0)
}

fn normalize(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Caption text with `<hh:mm:ss.mmm>` before each word whose spoken start
/// falls inside the cue. Captions nobody says out loud come back unchanged.
pub fn timed_text(text: &str, start: f64, end: f64, words: &[Word]) -> String {
    let spoken: Vec<(String, f64)> = words
        .iter()
        .map(|w| (normalize(&w.text), w.start.to_seconds()))
        .filter(|(_, t)| *t >= start - 0.5 && *t <= end)
        .collect();
    let mut next = 0;
    let mut last = start;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            line.split_whitespace()
                .map(|token| {
                    let key = normalize(token);
                    let found = (next..spoken.len().min(next + 4))
                        .find(|&j| !key.is_empty() && spoken[j].0 == key);
                    match found {
                        Some(j) => {
                            next = j + 1;
                            let t = spoken[j].1;
                            if t > last + 0.0005 && t < end {
                                last = t;
                                format!("<{}>{token}", vtt_time(t))
                            } else {
                                token.to_string()
                            }
                        }
                        None => token.to_string(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    lines.join("\n")
}

pub fn write(probe: &Probe, audio: &Audio, inspect: &Inspect) -> String {
    let mut out = String::from("WEBVTT\n\n");
    let secs = |f: i64| model::RationalTime::from_frames(f, probe.rate).to_seconds();
    for c in &inspect.captions {
        let (start, end) = (secs(c.range.start.value), secs(c.range.end_frame()));
        let _ = writeln!(out, "{}", c.id);
        let _ = writeln!(
            out,
            "{} --> {} line:{:.1}%,center position:{:.1}%,center size:{:.1}% align:center",
            vtt_time(start),
            vtt_time(end),
            c.bounds.cy * 100.0,
            c.bounds.cx * 100.0,
            (c.bounds.w * 100.0).clamp(1.0, 100.0)
        );
        let _ = writeln!(out, "{}\n", timed_text(&c.text, start, end, &audio.words));
    }
    out
}

/// Checks the subset of WebVTT this module writes: the header, cue timing
/// lines with start before end, and inline timestamps strictly inside the
/// cue and increasing.
pub fn validate(vtt: &str) -> Result<(), String> {
    let mut blocks = vtt.split("\n\n");
    let header = blocks.next().unwrap_or_default();
    if !header.starts_with("WEBVTT") {
        return Err("missing WEBVTT header".into());
    }
    let mut previous_start = 0.0;
    for block in blocks.filter(|b| !b.trim().is_empty()) {
        let mut lines = block.lines();
        let mut timing = lines.next().ok_or("empty cue")?;
        if !timing.contains("-->") {
            timing = lines.next().ok_or("cue without timing")?;
        }
        let (a, rest) = timing
            .split_once(" --> ")
            .ok_or_else(|| format!("bad timing line {timing:?}"))?;
        let b = rest.split_whitespace().next().unwrap_or_default();
        let (start, end) = (
            parse_time(a).ok_or_else(|| format!("bad time {a:?}"))?,
            parse_time(b).ok_or_else(|| format!("bad time {b:?}"))?,
        );
        if end <= start {
            return Err(format!("cue ends before it starts: {timing}"));
        }
        if start < previous_start {
            return Err(format!("cues out of order at {timing}"));
        }
        previous_start = start;
        let mut last = start;
        for text in lines {
            for piece in text.split('<').skip(1) {
                let stamp = piece.split('>').next().unwrap_or_default();
                if let Some(t) = parse_time(stamp) {
                    if t <= last || t >= end {
                        return Err(format!(
                            "inline time {stamp} outside or out of order in {timing}"
                        ));
                    }
                    last = t;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata::cuts3;
    use model::{Fps, RationalTime, Word};

    #[test]
    fn timestamps_are_hours_minutes_seconds_millis() {
        assert_eq!(vtt_time(1.8), "00:00:01.800");
        assert_eq!(vtt_time(3725.25), "01:02:05.250");
    }

    #[test]
    fn spoken_caption_words_get_inline_timestamps() {
        let fps = Fps::new(30, 1).unwrap();
        let w = |text: &str, f: i64| Word {
            text: text.into(),
            start: RationalTime::from_frames(f, fps),
            duration: RationalTime::from_frames(5, fps),
            confidence: 0.9,
        };
        let words = [w("Here", 30), w("it", 36), w("goes!", 42)];
        // The cue starts at f30 (1.0 s): the first word needs no timestamp,
        // and every inline timestamp must be after the cue start.
        assert_eq!(
            timed_text("here it goes!", 1.0, 2.0, &words),
            "here <00:00:01.200>it <00:00:01.400>goes!"
        );
    }

    #[test]
    fn unspoken_captions_have_no_inline_timestamps() {
        assert_eq!(
            timed_text("Me if I had a one wish willow:", 0.0, 56.0, &[]),
            "Me if I had a one wish willow:"
        );
    }

    #[test]
    fn cuts_3_output_is_valid_webvtt() {
        let (probe, audio, inspect) = cuts3();
        let out = write(&probe, &audio, &inspect);
        validate(&out).unwrap();
        assert_eq!(out.matches(" --> ").count(), 3);
    }

    #[test]
    fn the_validator_rejects_broken_files() {
        assert!(
            validate("WEBVTT\n\nc1\n00:00:02.000 --> 00:00:01.000\nx\n").is_err(),
            "end before start"
        );
        assert!(validate("NOTVTT\n").is_err());
        assert!(
            validate("WEBVTT\n\nc1\n00:00:01.000 --> 00:00:02.000\na <00:00:03.000>b\n").is_err(),
            "inline time past the end"
        );
    }

    #[test]
    fn cuts_3_vtt_snapshot() {
        let (probe, audio, inspect) = cuts3();
        insta::assert_snapshot!(write(&probe, &audio, &inspect));
    }
}
