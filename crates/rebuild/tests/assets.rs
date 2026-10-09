//! Reads real clips with ffmpeg and whisper. `cargo test -p rebuild -- --ignored`.
//! Needs `viralcraft doctor` to pass.

use std::fs;
use std::path::Path;
use std::process::Command;

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures"))
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| {
            let bytes = fs::read(&p).unwrap();
            (p.file_name().unwrap().to_string_lossy().into_owned(), bytes)
        })
        .collect();
    files.sort();
    files
}

#[test]
#[ignore = "needs ffmpeg and the whisper model"]
fn clips_are_probed_transcribed_and_thumbnailed_and_left_untouched() {
    // A relative folder, like the default `assets`: cargo runs this test
    // from the package root, so a temp dir inside it can be named relatively.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir_in(root).unwrap();
    let assets = tmp.path().strip_prefix(root).unwrap().join("assets");
    assert!(assets.is_relative());
    let run_dir = tmp.path().join("run");
    fs::create_dir_all(&assets).unwrap();
    fs::create_dir_all(&run_dir).unwrap();
    for f in ["cuts-3.mp4", "zoom-1.mp4"] {
        fs::copy(fixtures().join(f), assets.join(f)).unwrap();
    }
    fs::copy(fixtures().join("zoom-1.mp4"), assets.join("zoom-copy.mp4")).unwrap();
    let speech = assets.join("talk.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:s=360x640:r=30",
        ])
        .arg("-i")
        .arg(fixtures().join("speech.wav"))
        .args([
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(&speech)
        .status()
        .unwrap();
    assert!(status.success());
    let before = snapshot(&assets);

    let read = rebuild::run(&run_dir, &assets).unwrap();

    assert_eq!(
        snapshot(&assets),
        before,
        "the user's clips are never changed"
    );
    let names: Vec<&str> = read.clips.iter().map(|c| c.file.as_str()).collect();
    assert_eq!(
        names,
        ["cuts-3.mp4", "talk.mp4", "zoom-1.mp4", "zoom-copy.mp4"]
    );

    let cuts = &read.clips[0];
    assert_eq!((cuts.width, cuts.height, cuts.frames), (1080, 1920, 180));
    assert!((cuts.seconds - 6.0).abs() < 0.01, "{}", cuts.seconds);
    let zoom = &read.clips[2];
    assert_eq!((zoom.width, zoom.height, zoom.frames), (540, 960, 90));
    assert_eq!(read.clips[3].duplicate_of.as_deref(), Some("zoom-1.mp4"));

    let talk = &read.clips[1];
    assert!(talk.has_audio);
    assert_eq!(talk.language, "en");
    let text = talk.transcript.to_lowercase();
    for word in ["stop", "scrolling", "trick"] {
        assert!(text.contains(word), "{text}");
    }

    for clip in read.clips.iter().filter(|c| c.duplicate_of.is_none()) {
        let thumb = run_dir.join(&clip.thumbnail);
        assert!(thumb.is_file(), "{}", thumb.display());
    }
    assert!(read.clips[3].thumbnail.is_empty());
    assert!(run_dir.join("rebuild/assets.json").is_file());
}
