//! Runs ffmpeg and the ocrs models on the committed fixtures.
//! `cargo test -p inspect --release -- --ignored`.

use std::path::Path;

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures"))
}

fn near(got: &str, want: &str, tol: u8) -> bool {
    let p = |s: &str| [1, 3, 5].map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap());
    p(got)
        .iter()
        .zip(p(want))
        .all(|(a, b)| a.abs_diff(b) <= tol)
}

#[test]
#[ignore = "needs ffmpeg and the ocrs models"]
fn cuts_3_captions_match_the_fixture() {
    let out = tempfile::tempdir().unwrap();
    let run_dir = ingest::run(
        &fixtures().join("cuts-3.mp4"),
        out.path(),
        model::probe::DEFAULT_SAFE_ZONE,
    )
    .unwrap();
    shots::run(&run_dir).unwrap();
    let result = inspect::run(&run_dir).unwrap();

    let truth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("fixtures.json")).unwrap())
            .unwrap();
    let truth = truth["cuts"]["captions"].as_array().unwrap();

    let texts: Vec<&str> = result.captions.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["ONE", "TWO", "THREE"]);
    for (c, t) in result.captions.iter().zip(truth) {
        let (start, end) = (c.range.start.value, c.range.end_frame());
        assert!(
            (start - t["start_frame"].as_i64().unwrap()).abs() <= 1,
            "{} starts f{start}",
            c.text
        );
        assert!(
            (end - t["end_frame"].as_i64().unwrap()).abs() <= 1,
            "{} ends f{end}",
            c.text
        );
        let (cx, cy) = (
            t["center"][0].as_f64().unwrap(),
            t["center"][1].as_f64().unwrap(),
        );
        assert!(
            (c.bounds.cx - cx).abs() <= 0.05 && (c.bounds.cy - cy).abs() <= 0.05,
            "{} at {:?}",
            c.text,
            c.bounds
        );
        assert!(near(&c.fill, "#FFFFFF", 16), "{} fill {}", c.text, c.fill);
        assert!(
            near(c.outline.as_deref().unwrap_or("#808080"), "#000000", 16),
            "{} outline {:?}",
            c.text,
            c.outline
        );
        assert!(c.uppercase);
        assert!(c.animation.is_none());
    }
    eprintln!(
        "detection: {:.1} ms per frame",
        result.timing.detect_ms_per_frame
    );
}
