//! Runs real ffmpeg. `cargo test -p ingest -- --ignored`.

use std::path::Path;

use model::{tools, Fps, Probe};

fn make_clip(path: &Path, extra_vf: &str) {
    let vf = format!("format=yuv420p{extra_vf}");
    tools::run(
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x568:r=30:d=2",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=2",
            "-vf",
            &vf,
            "-fps_mode",
            "passthrough",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
            path.to_str().unwrap(),
        ],
    )
    .unwrap();
}

fn read_probe(run_dir: &Path) -> Probe {
    serde_json::from_str(&std::fs::read_to_string(run_dir.join("probe.json")).unwrap()).unwrap()
}

#[test]
#[ignore = "needs ffmpeg"]
fn a_cfr_clip_is_copied_and_probed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("cfr.mp4");
    make_clip(&input, "");

    let run_dir = ingest::run(
        &input,
        &dir.path().join("out"),
        model::probe::DEFAULT_SAFE_ZONE,
    )
    .unwrap();

    assert_eq!(run_dir, dir.path().join("out").join("cfr"));
    assert!(run_dir.join("source.cfr.mp4").exists());
    let probe = read_probe(&run_dir);
    assert_eq!(probe.rate, Fps::new(30, 1).unwrap());
    assert_eq!(probe.frames, 60);
    assert_eq!(
        (probe.width, probe.height, probe.aspect.as_str()),
        (320, 568, "40:71")
    );
    assert!(!probe.was_vfr);
    assert_eq!(probe.audio.unwrap().sample_rate, 44100);
    assert_eq!(probe.safe_zone.profile, "tiktok-2026-conservative");
}

#[test]
#[ignore = "needs ffmpeg"]
fn a_jittered_clip_is_re_encoded_to_constant_rate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("vfr.mp4");
    make_clip(&input, ",setpts='(N/30+0.012*sin(N))/TB'");

    let run_dir = ingest::run(
        &input,
        &dir.path().join("out"),
        model::probe::DEFAULT_SAFE_ZONE,
    )
    .unwrap();

    let probe = read_probe(&run_dir);
    assert!(probe.was_vfr);
    assert_eq!(probe.rate, Fps::new(30, 1).unwrap());
    assert!((59..=61).contains(&probe.frames), "frames {}", probe.frames);

    let pts = tools::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
            run_dir.join("source.cfr.mp4").to_str().unwrap(),
        ],
    )
    .unwrap();
    let timing = ingest::probe::packet_timing(&pts).unwrap();
    assert!(
        !timing.irregular(probe.rate),
        "normalized file must be regular"
    );
}

#[test]
#[ignore = "needs ffmpeg"]
fn an_audio_only_file_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("speech.wav");
    tools::run(
        "ffmpeg",
        &[
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=d=1",
            input.to_str().unwrap(),
        ],
    )
    .unwrap();

    let err = ingest::run(
        &input,
        &dir.path().join("out"),
        model::probe::DEFAULT_SAFE_ZONE,
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ingest::IngestError::Probe {
                source: ingest::probe::ProbeError::NoVideoStream,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(
        !dir.path().join("out").exists(),
        "no run dir for a rejected input"
    );
}
