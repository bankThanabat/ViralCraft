//! Runs the real whisper model. `cargo test -p audio -- --ignored`.
//! Needs `viralcraft doctor` to pass (the model file must be present).

use std::path::Path;

use model::{Fps, RationalTime};

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures"))
}

#[test]
#[ignore = "needs the whisper model"]
fn speech_fixture_words_start_within_3_frames_of_ground_truth() {
    let rate = Fps::new(30, 1).unwrap();
    let samples = audio::read_wav_16k_mono(&fixtures().join("speech.wav")).unwrap();
    let words = audio::transcribe(&samples, rate, RationalTime::from_frames(0, rate)).unwrap();

    let truth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("speech.json")).unwrap())
            .unwrap();
    let truth = truth["words"].as_array().unwrap();

    let normalize = |s: &str| {
        s.trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    };
    let got: Vec<String> = words.iter().map(|w| normalize(&w.text)).collect();
    let want: Vec<String> = truth
        .iter()
        .map(|w| normalize(w["text"].as_str().unwrap()))
        .collect();
    assert_eq!(got, want);

    // Compared on the 30 fps frame grid the words live on: 3 frames is
    // 100 ms. Whisper tiny's word timing is at about that limit on this
    // fixture ("scrolling" lands 3 frames early), so a regression shows here.
    let mut worst = 0;
    for (w, t) in words.iter().zip(truth) {
        let truth_frame = RationalTime::from_seconds(t["start"].as_f64().unwrap(), rate).value;
        let err = (w.start.value - truth_frame).abs();
        worst = worst.max(err);
        assert!(
            err <= 3,
            "{:?} starts at f{}, truth f{truth_frame}",
            w.text,
            w.start.value
        );
    }
    eprintln!("worst word start error: {worst} frames");
}
