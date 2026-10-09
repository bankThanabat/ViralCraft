//! Burns our captions.ass onto text-free video and reads them back, to
//! check the caption file puts text where the original had it. A
//! development check: needs an ffmpeg with libass (on macOS:
//! `brew install ffmpeg-full`) and the ocrs models.
//! `cargo test -p viralcraft --release -- --ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/editspec/tests/data/cuts-3")
}

fn read<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_str(&std::fs::read_to_string(data().join(name)).unwrap()).unwrap()
}

/// An ffmpeg with the libass `ass` filter: the one on PATH, or Homebrew's
/// keg-only ffmpeg-full. A development tool only; users never need it.
fn libass_ffmpeg() -> Option<PathBuf> {
    let has_ass = |filters: &str| {
        filters
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some("ass"))
    };
    [
        "ffmpeg",
        "/opt/homebrew/opt/ffmpeg-full/bin/ffmpeg",
        "/usr/local/opt/ffmpeg-full/bin/ffmpeg",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| {
        Command::new(p)
            .args(["-hide_banner", "-filters"])
            .output()
            .map(|o| has_ass(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or(false)
    })
}

#[test]
#[ignore = "needs ffmpeg with libass and the ocrs models"]
fn burned_captions_land_where_the_originals_were() {
    let Some(ffmpeg) = libass_ffmpeg() else {
        eprintln!("skipping: no ffmpeg with libass");
        return;
    };
    let probe: model::Probe = read("probe.json");
    let inspect: model::Inspect = read("inspect.json");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("captions.ass"),
        exports::ass::write(&probe, &inspect),
    )
    .unwrap();

    // The three cuts-3 scene colours with no text, 2 s each.
    let blank = "color=c=0x1E2A38:s=1080x1920:r=30:d=2[a];color=c=0xC9A96E:s=1080x1920:r=30:d=2[b];\
                 color=c=0x2E7D32:s=1080x1920:r=30:d=2[c];[a][b][c]concat=n=3:v=1:a=0,ass=captions.ass";
    let status = Command::new(&ffmpeg)
        .current_dir(dir.path())
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=16x16:d=0.1",
            "-filter_complex",
            blank,
        ])
        .args(["-frames:v", "180", "burned_%03d.png", "-start_number", "0"])
        .status()
        .unwrap();
    assert!(status.success());

    let ocr = inspect::text::Ocr::load(&doctor::default_models_dir()).unwrap();
    for (caption, frame) in inspect.captions.iter().zip([30, 90, 150]) {
        let img = image::open(dir.path().join(format!("burned_{:03}.png", frame + 1)))
            .unwrap()
            .into_rgb8();
        let lines = ocr.read_lines(&img).unwrap();
        let (b, text) = lines
            .iter()
            .find(|(_, t)| t.contains(&caption.text))
            .unwrap_or_else(|| {
                panic!(
                    "{} not read back from frame {frame}: {lines:?}",
                    caption.text
                )
            });
        let (dx, dy) = (b.cx - caption.bounds.cx, b.cy - caption.bounds.cy);
        eprintln!("{text}: off by ({dx:+.3}, {dy:+.3}) of the frame");
        assert!(
            dx.abs() <= 0.02 && dy.abs() <= 0.02,
            "{text} lands ({dx:+.3}, {dy:+.3}) away"
        );
    }
}
