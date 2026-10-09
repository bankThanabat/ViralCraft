//! Runs real ffmpeg on the committed fixtures. `cargo test -p shots -- --ignored`.

use std::path::{Path, PathBuf};

use model::Shots;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn analyze(name: &str, out: &Path) -> (PathBuf, Shots) {
    let run_dir = ingest::run(&fixture(name), out, model::probe::DEFAULT_SAFE_ZONE).unwrap();
    shots::run(&run_dir).unwrap();
    let json = std::fs::read_to_string(run_dir.join(shots::SHOTS_FILE)).unwrap();
    (run_dir, serde_json::from_str(&json).unwrap())
}

fn cut_frames(shots: &Shots) -> Vec<i64> {
    shots
        .segments
        .iter()
        .skip(1)
        .map(|s| s.range.start.value)
        .collect()
}

#[test]
#[ignore = "needs ffmpeg"]
fn cuts_3_has_three_shots_and_every_frame_on_disk() {
    let out = tempfile::tempdir().unwrap();
    let (run_dir, shots) = analyze("cuts-3.mp4", out.path());

    assert_eq!(shots.segments.len(), 3);
    for (got, want) in cut_frames(&shots).iter().zip([60, 120]) {
        assert!((got - want).abs() <= 1, "cut at {got}, expected {want} ± 1");
    }
    assert!(shots.transitions.is_empty(), "{:?}", shots.transitions);

    let frames = std::fs::read_dir(run_dir.join(shots::FRAMES_DIR))
        .unwrap()
        .count();
    assert_eq!(frames, 180);
    assert!(run_dir.join(shots::frame_path(0)).exists());
    assert!(run_dir.join(shots::frame_path(179)).exists());
    let first = image_size(&run_dir.join(shots::frame_path(0)));
    assert_eq!(first, (540, 960), "frames are half resolution");
}

#[test]
#[ignore = "needs ffmpeg"]
fn the_vfr_variant_gives_the_same_shots() {
    let out = tempfile::tempdir().unwrap();
    let (_, cfr) = analyze("cuts-3.mp4", out.path());
    let (_, vfr) = analyze("cuts-3-vfr.mp4", out.path());
    assert_eq!(cfr.segments, vfr.segments);
}

/// Width and height from a baseline JPEG's SOF0 marker.
fn image_size(path: &Path) -> (u32, u32) {
    let bytes = std::fs::read(path).unwrap();
    let sof = bytes
        .windows(2)
        .position(|w| w == [0xFF, 0xC0])
        .expect("baseline JPEG");
    let h = u32::from(bytes[sof + 5]) << 8 | u32::from(bytes[sof + 6]);
    let w = u32::from(bytes[sof + 7]) << 8 | u32::from(bytes[sof + 8]);
    (w, h)
}
