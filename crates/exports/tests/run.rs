use std::fs;
use std::path::Path;

#[test]
fn run_writes_the_caption_files_and_the_timeline() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../editspec/tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in ["probe.json", "shots.json", "audio.json", "inspect.json"] {
        fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }

    let files = exports::run(dir.path()).unwrap();

    assert_eq!(
        files,
        [
            dir.path().join("captions.vtt"),
            dir.path().join("captions.ass"),
            dir.path().join("timeline.otio")
        ]
    );
    let timeline: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&files[2]).unwrap()).unwrap();
    assert_eq!(timeline["OTIO_SCHEMA"], "Timeline.1");
    exports::vtt::validate(&fs::read_to_string(&files[0]).unwrap()).unwrap();
    assert!(fs::read_to_string(&files[1])
        .unwrap()
        .starts_with("[Script Info]"));
}

#[test]
fn a_judgement_fixes_and_filters_every_export() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../editspec/tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in [
        "probe.json",
        "shots.json",
        "audio.json",
        "inspect.json",
        "judgement.json",
    ] {
        fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }
    let files = exports::run(dir.path()).unwrap();
    for f in &files {
        let text = fs::read_to_string(f).unwrap();
        assert!(
            !text.contains("THREE\n") && !text.contains("}THREE"),
            "scene text c3 left in {}",
            f.display()
        );
    }
    let vtt = fs::read_to_string(&files[0]).unwrap();
    assert_eq!(vtt.matches(" --> ").count(), 2);
}
