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
    let (words, _) = audio::transcribe(&samples, rate, RationalTime::from_frames(0, rate)).unwrap();

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

#[test]
#[ignore = "needs ffmpeg and the whisper model"]
fn cuts_3_click_track_gives_120_bpm_and_beats_every_15_frames() {
    let out = tempfile::tempdir().unwrap();
    let run_dir = ingest::run(
        &fixtures().join("cuts-3.mp4"),
        out.path(),
        model::probe::DEFAULT_SAFE_ZONE,
    )
    .unwrap();
    let a = audio::run(&run_dir).unwrap();

    assert!((a.tempo.bpm - 120.0).abs() <= 1.0, "bpm {}", a.tempo.bpm);
    assert!(a.beats.len() >= 10, "{} beats", a.beats.len());
    for b in &a.beats {
        let off = (b.time.value as f64 / 15.0).round() * 15.0 - b.time.value as f64;
        assert!(
            off.abs() <= 2.0,
            "beat at f{} is {off} frames off the 0.5 s grid",
            b.time.value
        );
    }
    assert_eq!(a.music.len(), 1, "one music span over the click track");
    assert!(a.music[0].beat, "the click track has a beat");
    if doctor::default_models_dir().join("yamnet.onnx").exists() {
        assert!(
            a.sounds
                .iter()
                .any(|s| s.source == "yamnet" && s.label == "tick"),
            "YAMNet hears the clicks as ticks: {:?}",
            a.sounds
        );
        assert!(a.transcript.contains("[tick]"), "{}", a.transcript);
    }
    assert!(a.words.is_empty());
}

#[test]
#[ignore = "needs a multilingual whisper model (viralcraft doctor --install)"]
fn thai_speech_is_detected_and_transcribed_in_thai() {
    let models = doctor::default_models_dir();
    if !["ggml-small.bin", "ggml-medium.bin", "ggml-large-v3.bin"]
        .iter()
        .any(|m| models.join(m).exists())
    {
        eprintln!("skipping: no multilingual model installed");
        return;
    }
    let rate = Fps::new(30, 1).unwrap();
    let samples = audio::read_wav_16k_mono(&fixtures().join("speech-th.wav")).unwrap();
    let (words, language) =
        audio::transcribe(&samples, rate, RationalTime::from_frames(0, rate)).unwrap();
    let text: String = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("{language}: {text}");
    assert_eq!(language, "th");
    let thai = text
        .chars()
        .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
        .count();
    assert!(thai >= 15, "expected Thai script, got {text:?}");
    assert!(text.contains("สวัสดี"), "{text}");
    assert!(!text.contains('\u{FFFD}'), "broken characters in {text}");
}
