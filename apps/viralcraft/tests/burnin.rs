//! Burns our captions.ass onto text-free video and reads them back, to
//! check the caption file puts text where the original had it. Needs an
//! ffmpeg with libass (`viralcraft doctor --install` offers one) and the
//! ocrs models. `cargo test -p viralcraft --release -- --ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/editspec/tests/data/cuts-3")
}

fn read<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_str(&std::fs::read_to_string(data().join(name)).unwrap()).unwrap()
}

fn libass_ffmpeg() -> Option<PathBuf> {
    let candidates = std::iter::once(PathBuf::from("ffmpeg"))
        .chain(doctor::PREVIEW_CANDIDATES.iter().map(PathBuf::from));
    candidates.into_iter().find(|p| {
        Command::new(p)
            .args(["-hide_banner", "-filters"])
            .output()
            .map(|o| doctor::has_ass_filter(&String::from_utf8_lossy(&o.stdout)))
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

#[test]
#[ignore = "needs ffmpeg with libass"]
fn preview_writes_the_source_with_our_captions_on_top() {
    if libass_ffmpeg().is_none() {
        eprintln!("skipping: no ffmpeg with libass");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let video = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cuts-3.mp4");
    std::fs::copy(&video, dir.path().join("source.cfr.mp4")).unwrap();
    for f in ["probe.json", "audio.json", "inspect.json"] {
        std::fs::copy(data().join(f), dir.path().join(f)).unwrap();
    }
    let probe: model::Probe = read("probe.json");
    let inspect: model::Inspect = read("inspect.json");
    std::fs::write(
        dir.path().join("captions.ass"),
        exports::ass::write(&probe, &inspect),
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args(["preview", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let preview = dir.path().join("preview.mp4");
    let frames = model::tools::run(
        "ffprobe",
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
            preview.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(frames.trim(), "180");
}
