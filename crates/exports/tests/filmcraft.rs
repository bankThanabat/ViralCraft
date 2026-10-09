//! Imports the cuts-3 timeline into FilmCraft, the reference editor for v1.
//! Set `FILMCRAFT_CLI` to a built `filmcraft-cli`; skips without it.
//! `cargo test -p exports -- --ignored`.

use std::path::Path;
use std::process::Command;

#[test]
#[ignore = "needs a built filmcraft-cli (set FILMCRAFT_CLI)"]
fn filmcraft_imports_the_timeline_vertical_with_placeholders_disabled() {
    let Ok(cli) = std::env::var("FILMCRAFT_CLI") else {
        eprintln!("skipping: FILMCRAFT_CLI not set");
        return;
    };
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../editspec/tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in ["probe.json", "shots.json", "audio.json", "inspect.json"] {
        std::fs::copy(data.join(f), dir.path().join(f)).unwrap();
    }
    let video = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cuts-3.mp4");
    std::fs::copy(video, dir.path().join("source.cfr.mp4")).unwrap();
    exports::run(dir.path()).unwrap();

    let run = |args: &[&str]| {
        let out = Command::new(&cli).current_dir(dir.path()).args(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
    };
    let import = run(&["--save-as", "p.fcproj", "--compact", "import", "timeline.otio"]);
    assert_eq!(import["errors"], serde_json::json!([]));
    assert_eq!(import["documents"][0]["linkedMedia"], 1);

    let seq = run(&["--project", "p.fcproj", "--compact", "inspect", "sequence"]);
    assert_eq!((seq["settings"]["width"].as_u64(), seq["settings"]["height"].as_u64()), (Some(1080), Some(1920)));
    assert_eq!(seq["durationFrames"], 180);
    let video = seq["video"].as_array().unwrap();
    assert_eq!(video[0]["items"].as_array().unwrap().len(), 3, "three shots");
    let captions = video[1]["items"].as_array().unwrap();
    assert_eq!(captions.len(), 3);
    assert!(captions.iter().all(|c| c["enabled"] == false), "placeholders never render");
}
