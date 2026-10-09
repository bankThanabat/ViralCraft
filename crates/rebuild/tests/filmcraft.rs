//! Builds a small rebuild of cuts-3 in FilmCraft with the commands in
//! `skill/viralcraft-filmcraft/references/filmcraft-commands.md`, then checks
//! the export. Needs FilmCraft's CLI:
//! `FILMCRAFT_CLI=/path/to/filmcraft-cli cargo test -p rebuild -- --ignored`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

const TICKS_PER_SECOND: i64 = 254_016_000_000;
const FPS: i64 = 30;
const WIDTH: f64 = 1080.0;
const HEIGHT: f64 = 1920.0;

fn ticks(frames: i64) -> i64 {
    frames * TICKS_PER_SECOND / FPS
}

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures"))
}

struct FilmCraft {
    cli: PathBuf,
    dir: PathBuf,
}

impl FilmCraft {
    fn call(&self, args: &[&str]) -> String {
        let out = Command::new(&self.cli)
            .current_dir(&self.dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// Runs commands as one `run` batch and returns each line's result.
    fn run(&self, steps: &[Value]) -> Vec<Value> {
        let script: String = steps.iter().map(|s| format!("{s}\n")).collect();
        fs::write(self.dir.join("steps.jsonl"), script).unwrap();
        let out = self.call(&[
            "--project",
            "p.fcproj",
            "--save",
            "--compact",
            "run",
            "steps.jsonl",
        ]);
        out.lines()
            .map(|l| {
                let v: Value = serde_json::from_str(l).unwrap();
                assert_eq!(v["ok"], true, "{l}");
                v["result"].clone()
            })
            .collect()
    }
}

/// A caption from the prompt: text and its pixel centre.
struct Caption {
    text: &'static str,
    from: i64,
    to: i64,
    centre: (f64, f64),
    size: f64,
}

#[test]
#[ignore = "needs FILMCRAFT_CLI"]
fn a_rebuild_of_cuts_3_exports_with_the_cuts_and_captions_in_place() {
    let Some(cli) = std::env::var_os("FILMCRAFT_CLI").map(PathBuf::from) else {
        eprintln!("skipping: set FILMCRAFT_CLI");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    fs::copy(fixtures().join("zoom-1.mp4"), tmp.path().join("zoom-1.mp4")).unwrap();
    // Plain cards without text, so the new captions read back cleanly
    // (cuts-3 has its own words burned in where the captions go).
    for (name, colour) in [("navy.mp4", "0x1E2A38"), ("gold.mp4", "0xC9A96E")] {
        let status = Command::new("ffmpeg")
            .current_dir(tmp.path())
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!("color=c={colour}:s=1080x1920:r=30:d=3"))
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", name])
            .status()
            .unwrap();
        assert!(status.success());
    }
    let fc = FilmCraft {
        cli,
        dir: tmp.path().to_path_buf(),
    };

    let imported: Value = serde_json::from_str(&fc.call(&[
        "--save-as",
        "p.fcproj",
        "--compact",
        "import",
        "navy.mp4",
        "gold.mp4",
        "zoom-1.mp4",
    ]))
    .unwrap();
    let item = |i: usize| imported["items"][i].clone();
    let (navy, gold, confetti) = (item(0), item(1), item(2));

    // Three 2 s shots like cuts-3: navy and gold cards, then the confetti
    // clip, which is half size and must be scaled to fill the frame.
    let placed = fc.run(&[
        json!({"id":"file.newSequence","params":{"name":"rebuild","width":1080,"height":1920,"fps":30,"video":3,"audio":3}}),
        json!({"id":"timeline.place","params":{"item":navy,"track":"V1","frame":0,"duration":ticks(60)}}),
        json!({"id":"timeline.place","params":{"item":gold,"track":"V1","frame":60,"sourceIn":ticks(15),"duration":ticks(60)}}),
        json!({"id":"timeline.place","params":{"item":confetti,"track":"V1","frame":120,"sourceIn":ticks(20),"duration":ticks(60)}}),
    ]);
    let small = [placed[3]["clips"][0].clone()];
    fc.run(&[json!({"id":"clip.fillFrame","params":{"clips":small}})]);

    // c1 and c2 of cuts-3, with new text.
    let captions = [
        Caption {
            text: "FIRST",
            from: 0,
            to: 60,
            centre: (0.4986 * WIDTH, 0.1781 * HEIGHT),
            size: 104.0,
        },
        Caption {
            text: "SECOND",
            from: 60,
            to: 120,
            centre: (0.4977 * WIDTH, 0.4975 * HEIGHT),
            size: 118.0,
        },
    ];
    for c in &captions {
        let made = fc.run(&[json!({"id":"graphics.newText","params":{
            "text": c.text,
            "position": [c.centre.0, c.centre.1],
            "size": c.size,
            "font": "Arial Black",
            "seconds": (c.to - c.from) as f64 / FPS as f64,
            "track": 2,
            "time": ticks(c.from),
            "newClip": true
        }})]);
        let clip = made[0]["clip"].clone();
        // Style first: alignment moves the text, so measure after it.
        fc.run(&[
            json!({"id":"graphics.set","params":{"clip":clip,"layer":0,"props":{
                "align": "center",
                "fill_color": "#FFFFFF",
                "stroke": true,
                "stroke_width": 8,
                "stroke_color": "#000000"
            }}}),
        ]);
        let listed = fc.run(&[json!({"id":"graphics.list","params":{"clip":clip}})]);
        let b = &listed[0]["layers"][0]["localBounds"];
        let mid = |i: usize, j: usize| (b[i].as_f64().unwrap() + b[j].as_f64().unwrap()) / 2.0;
        let position = [c.centre.0 - mid(0, 2), c.centre.1 - mid(1, 3)];
        fc.run(&[json!({"id":"graphics.set","params":{"clip":clip,"layer":0,"props":{"position":position}}})]);
    }

    fc.call(&[
        "--project",
        "p.fcproj",
        "--compact",
        "export",
        "rebuild.mp4",
        "--format",
        "h264",
    ]);

    // Size, rate and length.
    let probe_dir = tmp.path().join("check");
    let run_dir = ingest::run(
        &tmp.path().join("rebuild.mp4"),
        &probe_dir,
        "tiktok-2026-conservative",
    )
    .unwrap();
    let probe: model::Probe =
        serde_json::from_str(&fs::read_to_string(run_dir.join("probe.json")).unwrap()).unwrap();
    assert_eq!((probe.width, probe.height, probe.frames), (1080, 1920, 180));
    assert_eq!(probe.rate.as_f64(), 30.0);

    // Cuts where the prompt put them.
    let shots = shots::run(&run_dir).unwrap();
    let starts: Vec<i64> = shots.segments.iter().map(|s| s.range.start.value).collect();
    assert_eq!(starts.len(), 3, "{starts:?}");
    for (got, want) in starts.iter().zip([0, 60, 120]) {
        assert!((got - want).abs() <= 1, "{starts:?}");
    }

    // Each caption reads back at its place.
    let ocr = inspect::text::Ocr::load(&doctor::default_models_dir()).unwrap();
    for c in &captions {
        let png = format!("{}.png", c.text);
        let seconds = (c.from + c.to) as f64 / 2.0 / FPS as f64;
        fc.call(&[
            "--project",
            "p.fcproj",
            "render",
            "--seconds",
            &seconds.to_string(),
            "--out",
            &png,
        ]);
        let img = image::open(tmp.path().join(&png)).unwrap().into_rgb8();
        let lines = ocr.read_lines(&img).unwrap();
        let (b, _) = lines
            .iter()
            .find(|(_, t)| t.contains(c.text))
            .unwrap_or_else(|| panic!("{} not found in {lines:?}", c.text));
        // OCR finds the line; its box is padded, so measure the white fill
        // inside it (the cards are plain, so white is only the text).
        let (w, h) = (f64::from(img.width()), f64::from(img.height()));
        let (x0, x1) = (
            (b.left() * w) as u32,
            ((b.right() * w) as u32).min(img.width()),
        );
        let (y0, y1) = (
            (b.top() * h) as u32,
            ((b.bottom() * h) as u32).min(img.height()),
        );
        let white: Vec<(u32, u32)> = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| img.get_pixel(x, y).0.iter().all(|&v| v > 245))
            .collect();
        let span = |v: Vec<u32>| {
            (f64::from(*v.iter().min().unwrap()) + f64::from(*v.iter().max().unwrap())) / 2.0
        };
        let cx = span(white.iter().map(|p| p.0).collect()) / w;
        let cy = span(white.iter().map(|p| p.1).collect()) / h;
        assert!((cx - c.centre.0 / WIDTH).abs() < 0.01, "{} x {cx}", c.text);
        assert!((cy - c.centre.1 / HEIGHT).abs() < 0.01, "{} y {cy}", c.text);
    }
}
