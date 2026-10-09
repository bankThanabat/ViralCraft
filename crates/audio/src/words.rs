//! Turns whisper tokens into timed words. Kept free of whisper-rs types so
//! it can be tested with hand-built tokens.

use model::{Fps, RationalTime, SoundEvent, TimeRange, Word};

/// One whisper token. Times are in whisper's centiseconds; `t_dtw` is -1
/// when DTW alignment produced no time.
#[derive(Debug, Clone, PartialEq)]
pub struct RawToken {
    pub text: String,
    pub t0: i64,
    pub t1: i64,
    pub t_dtw: i64,
    pub p: f32,
    /// Control tokens such as `[_BEG_]` or end-of-text.
    pub special: bool,
}

/// Confidence given to a word whose start came from coarse token stamps.
const FALLBACK_CONFIDENCE: f32 = 0.3;

struct Building {
    text: String,
    /// First token's coarse start.
    t0: i64,
    /// Last token's coarse end.
    t1: i64,
    /// Latest DTW time among the word's tokens.
    dtw_end: Option<i64>,
    probs: Vec<f32>,
}

/// Groups tokens into words. A token starting with a space opens a new word;
/// anything else (subword pieces, punctuation) extends the current one.
/// `offset` is the audio start on the video clock, from `probe.json`.
///
/// Timing rule, measured against ground truth on `speech.wav`: a token's
/// DTW time marks where the word ends, and `t0` is a fair start that can
/// drift early into the previous word. So a word ends at its latest DTW
/// time and starts at `t0`, but never before the previous word's end.
pub fn tokens_to_words(tokens: &[RawToken], rate: Fps, offset: RationalTime) -> Vec<Word> {
    let mut built: Vec<Building> = Vec::new();
    for t in tokens.iter().filter(|t| !t.special) {
        let dtw = (t.t_dtw >= 0).then_some(t.t_dtw);
        let opens_word = t.text.starts_with(' ') || built.is_empty();
        if opens_word && !t.text.trim().is_empty() {
            built.push(Building {
                text: t.text.trim().to_string(),
                t0: t.t0,
                t1: t.t1,
                dtw_end: dtw,
                probs: vec![t.p],
            });
        } else if let Some(w) = built.last_mut() {
            w.text.push_str(t.text.trim());
            w.t1 = w.t1.max(t.t1);
            w.dtw_end = w.dtw_end.max(dtw);
            w.probs.push(t.p);
        }
    }

    let offset_s = offset.to_seconds();
    let at = |cs: i64| RationalTime::from_seconds(cs as f64 / 100.0 + offset_s, rate);
    let mut previous_end = i64::MIN;
    built
        .into_iter()
        .map(|w| {
            let end_cs = w.dtw_end.unwrap_or(w.t1);
            let start_cs = w.t0.max(previous_end);
            previous_end = end_cs;
            let (start, end) = (at(start_cs), at(end_cs));
            let confidence = if w.dtw_end.is_some() {
                w.probs.iter().sum::<f32>() / w.probs.len() as f32
            } else {
                FALLBACK_CONFIDENCE
            };
            Word {
                text: w.text,
                start,
                duration: RationalTime::from_frames((end.value - start.value).max(1), rate),
                confidence,
            }
        })
        .collect()
}

/// Moves whisper's non-speech tags, such as `[sighs]` or `(laughs)`, out of
/// the words and into labelled sounds.
pub fn split_sound_tags(words: Vec<Word>) -> (Vec<Word>, Vec<SoundEvent>) {
    let mut kept = Vec::new();
    let mut sounds = Vec::new();
    for w in words {
        let t = w.text.trim();
        let tagged =
            (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'));
        if tagged && t.len() > 2 {
            sounds.push(SoundEvent {
                label: t[1..t.len() - 1].trim().to_lowercase(),
                start: w.start,
                duration: w.duration,
                source: "whisper".to_string(),
            });
        } else {
            kept.push(w);
        }
    }
    (kept, sounds)
}

/// Merges words into speech spans, joining words separated by less than
/// `max_gap_s` seconds.
pub fn speech_spans(words: &[Word], max_gap_s: f64) -> Vec<TimeRange> {
    let mut spans: Vec<TimeRange> = Vec::new();
    for w in words {
        let rate = w.start.rate;
        let (start, end) = (w.start.value, w.start.value + w.duration.value);
        let max_gap = RationalTime::from_seconds(max_gap_s, rate).value;
        match spans.last_mut() {
            Some(last) if start - last.end_frame() < max_gap => {
                let new_end = end.max(last.end_frame());
                last.duration = RationalTime::from_frames(new_end - last.start.value, rate);
            }
            _ => spans.push(TimeRange::from_frames(start, end - start, rate)),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::Fps;

    const FPS30: Fps = Fps { num: 30, den: 1 };

    fn tok(text: &str, t0: i64, t1: i64, t_dtw: i64) -> RawToken {
        RawToken {
            text: text.to_string(),
            t0,
            t1,
            t_dtw,
            p: 0.9,
            special: false,
        }
    }

    fn zero() -> RationalTime {
        RationalTime::from_frames(0, FPS30)
    }

    #[test]
    fn subword_pieces_and_punctuation_join_the_previous_word() {
        let tokens = [
            tok(" Stop", 20, 70, 60),
            tok(" scroll", 40, 100, 84),
            tok("ing", 100, 140, 120),
            tok(".", 140, 150, -1),
        ];
        let words = tokens_to_words(&tokens, FPS30, zero());

        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["Stop", "scrolling."]);
        assert!((words[0].confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn a_word_ends_at_its_dtw_time_and_starts_no_earlier_than_the_previous_end() {
        // Measured on speech.wav: t_dtw marks a word's end, and t0 can drift
        // early into the previous word.
        let tokens = [
            tok(" Stop", 20, 70, 60),
            tok(" scroll", 40, 100, 84),
            tok("ing", 100, 140, 120),
        ];
        let words = tokens_to_words(&tokens, FPS30, zero());
        let span = |w: &Word| (w.start.value, w.start.value + w.duration.value);
        // Stop: t0 0.20 s → f6, DTW end 0.60 s → f18.
        assert_eq!(span(&words[0]), (6, 18));
        // scrolling: t0 0.40 s is before Stop ends, so it starts at f18;
        // its last DTW time 1.20 s → f36.
        assert_eq!(span(&words[1]), (18, 36));
    }

    #[test]
    fn special_tokens_are_skipped() {
        let mut beg = tok("[_BEG_]", 0, 0, 0);
        beg.special = true;
        let mut eot = tok("<|endoftext|>", 150, 150, 150);
        eot.special = true;
        let words = tokens_to_words(&[beg, tok(" Hi", 10, 40, 12), eot], FPS30, zero());
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hi");
    }

    #[test]
    fn a_missing_dtw_time_falls_back_to_t0_with_low_confidence() {
        let words = tokens_to_words(&[tok(" Hi", 50, 80, -1)], FPS30, zero());
        assert_eq!(words[0].start.value, 15);
        assert!((words[0].confidence - 0.3).abs() < 1e-6);
    }

    #[test]
    fn the_audio_offset_moves_words_onto_the_video_clock() {
        let offset = RationalTime::from_frames(2, FPS30);
        let words = tokens_to_words(&[tok(" Hi", 0, 30, 0)], FPS30, offset);
        assert_eq!(words[0].start.value, 2);
    }

    #[test]
    fn a_word_never_has_negative_duration() {
        // DTW can place a word's end before its t0.
        let words = tokens_to_words(&[tok(" Hi", 50, 60, 40)], FPS30, zero());
        assert!(words[0].duration.value >= 1);
    }

    #[test]
    fn bracketed_tags_are_sounds_not_words() {
        let w = |text: &str, start: i64| Word {
            text: text.into(),
            start: RationalTime::from_frames(start, FPS30),
            duration: RationalTime::from_frames(10, FPS30),
            confidence: 0.8,
        };
        let (words, sounds) = split_sound_tags(vec![
            w("everything.", 0),
            w("[sighs]", 20),
            w("Okay,", 40),
            w("(laughs)", 60),
        ]);
        assert_eq!(
            words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            ["everything.", "Okay,"]
        );
        let labels: Vec<(&str, i64)> = sounds
            .iter()
            .map(|s| (s.label.as_str(), s.start.value))
            .collect();
        assert_eq!(labels, [("sighs", 20), ("laughs", 60)]);
    }

    #[test]
    fn speech_spans_merge_words_closer_than_the_gap() {
        let w = |start: i64, dur: i64| Word {
            text: "w".into(),
            start: RationalTime::from_frames(start, FPS30),
            duration: RationalTime::from_frames(dur, FPS30),
            confidence: 1.0,
        };
        // Gaps: 5 frames (0.17 s, merge), 30 frames (1 s, split).
        let spans = speech_spans(&[w(0, 10), w(15, 10), w(55, 5)], 0.5);
        let ranges: Vec<(i64, i64)> = spans
            .iter()
            .map(|r| (r.start.value, r.end_frame()))
            .collect();
        assert_eq!(ranges, [(0, 25), (55, 60)]);
    }
}
