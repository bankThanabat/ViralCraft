//! The whole pipeline on a committed fixture. Needs ffmpeg and the models
//! (`viralcraft doctor`). `cargo test -p viralcraft --release -- --ignored`.

use std::path::Path;
use std::process::Command;

#[test]
#[ignore = "needs ffmpeg and the models"]
fn analyze_cuts_3_writes_every_file_with_no_empty_field() {
    let dir = tempfile::tempdir().unwrap();
    let video = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cuts-3.mp4");
    let out = Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args([
            "analyze",
            video.to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let run = dir.path().join("cuts-3");
    assert_eq!(summary["run_dir"], run.to_str().unwrap());
    for f in [
        "probe.json",
        "source.cfr.mp4",
        "shots.json",
        "frames",
        "audio.json",
        "inspect.json",
        "EDIT-SPEC.md",
        "captions.vtt",
        "captions.ass",
        "timeline.otio",
    ] {
        assert!(run.join(f).exists(), "{f} missing");
        assert!(
            summary["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p.as_str().unwrap().ends_with(f)),
            "{f} not in summary"
        );
    }
    let spec = std::fs::read_to_string(run.join("EDIT-SPEC.md")).unwrap();
    for line in spec.lines() {
        if let Some((key, value)) = line.split_once(": ") {
            if !key.contains('|') && !key.starts_with('#') {
                assert!(!value.trim().is_empty(), "empty field: {line}");
            }
        }
    }
    assert!(summary["next"].as_str().unwrap().contains("finalize"));
}
