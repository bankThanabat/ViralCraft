use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn viralcraft(args: &[&str], models_dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args(args)
        .env("VIRALCRAFT_MODELS", models_dir)
        .stdin(Stdio::null())
        .output()
        .expect("the test binary is built by cargo before tests run")
}

fn viralcraft_answering(args: &[&str], models_dir: &Path, answers: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args(args)
        .env("VIRALCRAFT_MODELS", models_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the test binary is built by cargo before tests run");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answers.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn single_json_line(stdout: &[u8]) -> serde_json::Value {
    let stdout = String::from_utf8(stdout.to_vec()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout was: {stdout:?}");
    serde_json::from_str(lines[0]).unwrap()
}

#[test]
fn doctor_reports_missing_models_and_exits_1() {
    let empty = tempfile::tempdir().unwrap();
    let out = viralcraft(&["doctor"], empty.path());

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["command"], "doctor");
    assert_eq!(summary["ok"], false);

    let whisper = summary["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "ggml-tiny.en.bin")
        .unwrap();
    assert_eq!(whisper["state"], "missing");
    assert!(whisper["fix"].as_str().unwrap().starts_with("curl "));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ggml-tiny.en.bin"));
}

#[test]
fn doctor_points_at_setup_and_no_longer_installs() {
    let empty = tempfile::tempdir().unwrap();
    let doctor = viralcraft(&["doctor"], empty.path());
    assert!(
        String::from_utf8_lossy(&doctor.stderr)
            .contains("Run `npm run setup` (or `viralcraft setup`)"),
        "doctor tells the user how to install"
    );

    let install = viralcraft(&["doctor", "--install"], empty.path());
    assert_eq!(install.status.code(), Some(2), "--install is gone");
    assert_eq!(single_json_line(&install.stdout)["ok"], false);
}

#[test]
fn setup_with_no_one_at_the_terminal_installs_nothing() {
    let empty = tempfile::tempdir().unwrap();
    let out = viralcraft(&["setup"], empty.path());

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["command"], "setup");
    assert_eq!(summary["ok"], false);
    assert!(summary["filmcraft"].is_boolean(), "{summary}");
    assert!(
        summary["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "ggml-tiny.en.bin" && i["state"] == "missing"),
        "{summary}"
    );
    assert_eq!(
        std::fs::read_dir(empty.path()).unwrap().count(),
        0,
        "closed stdin is a no, so nothing is downloaded"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Not ready yet"), "{stderr}");
    assert!(
        stderr.contains("ggml-tiny.en.bin: curl "),
        "the missing list keeps each command for running by hand: {stderr}"
    );
}

#[test]
fn setup_asks_once_for_the_required_models_in_plain_words() {
    let empty = tempfile::tempdir().unwrap();
    let out = viralcraft_answering(&["setup"], empty.path(), &"n\n".repeat(5));

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(single_json_line(&out.stdout)["ok"], false);
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);

    let stderr = String::from_utf8_lossy(&out.stderr);
    let questions: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("speech and text models") && l.contains('?'))
        .collect();
    assert_eq!(
        questions,
        [format!(
            "Download the speech and text models (90 MB) to {}? [Y/n] ",
            empty.path().display()
        )],
        "one question for all three required models: {stderr}"
    );
    assert!(
        stderr
            .lines()
            .filter(|l| l.contains('?'))
            .all(|l| !l.contains("curl")),
        "questions never show a download command: {stderr}"
    );
}

#[test]
fn ingest_with_an_empty_inbox_asks_then_fails_with_a_json_error() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("inbox");
    std::fs::create_dir(&inbox).unwrap();
    let out = viralcraft(
        &[
            "ingest",
            "--inbox",
            inbox.to_str().unwrap(),
            "--out",
            dir.path().join("out").to_str().unwrap(),
        ],
        dir.path(),
    );

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["command"], "ingest");
    assert_eq!(summary["ok"], false);
    assert!(summary["error"]
        .as_str()
        .unwrap()
        .contains("no input video"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("No video found"));
}

#[test]
fn shots_on_a_missing_run_dir_reports_probe_json_in_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    let out = viralcraft(&["shots", missing.to_str().unwrap()], dir.path());

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["command"], "shots");
    assert!(
        summary["error"].as_str().unwrap().contains("probe.json"),
        "{summary}"
    );
}

fn run_dir_with(files: &[&str]) -> tempfile::TempDir {
    let data =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/editspec/tests/data/cuts-3");
    let dir = tempfile::tempdir().unwrap();
    for f in files {
        std::fs::copy(data.join(f), dir.path().join(f)).unwrap();
        // Fixed times, judgement last, as when an agent writes it after
        // analyze; copying may keep the source files' own times.
        let at = if *f == "judgement.json" {
            1_000_060
        } else {
            1_000_000
        };
        set_modified(&dir.path().join(f), at);
    }
    dir
}

fn set_modified(path: &Path, seconds: u64) {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

const MEASURED: [&str; 4] = ["probe.json", "shots.json", "audio.json", "inspect.json"];

#[test]
fn finalize_without_a_judgement_says_what_to_write() {
    let dir = run_dir_with(&MEASURED);
    let out = viralcraft(&["finalize", dir.path().to_str().unwrap()], dir.path());
    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    assert!(
        summary["error"]
            .as_str()
            .unwrap()
            .contains("judgement.json"),
        "{summary}"
    );
}

#[test]
fn finalize_lists_what_an_incomplete_judgement_is_missing() {
    let dir = run_dir_with(&MEASURED);
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/editspec/tests/data/cuts-3/judgement.json");
    let mut j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(full).unwrap()).unwrap();
    j.as_object_mut().unwrap().remove("format_name");
    j["captions"][0]["font_substitute"] = serde_json::Value::Null;
    std::fs::write(dir.path().join("judgement.json"), j.to_string()).unwrap();

    let out = viralcraft(&["finalize", dir.path().to_str().unwrap()], dir.path());

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    let missing: Vec<&str> = summary["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    assert_eq!(missing, ["captions[c1].font_substitute", "format_name"]);
}

#[test]
fn finalize_with_a_complete_judgement_writes_every_file() {
    let mut files = MEASURED.to_vec();
    files.push("judgement.json");
    let dir = run_dir_with(&files);

    let out = viralcraft(&["finalize", dir.path().to_str().unwrap()], dir.path());

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let spec = std::fs::read_to_string(dir.path().join("EDIT-SPEC.md")).unwrap();
    assert!(!spec.contains("TODO"));
    for f in [
        "captions.vtt",
        "captions.ass",
        "timeline.otio",
        "TEMPLATE.md",
    ] {
        assert!(dir.path().join(f).exists(), "{f}");
    }
}

#[test]
fn finalize_refuses_a_judgement_older_than_the_measurements() {
    // Re-running inspect renumbers captions; an old judgement would then
    // relabel the wrong ones without any error.
    let mut files = MEASURED.to_vec();
    files.push("judgement.json");
    let dir = run_dir_with(&files);
    set_modified(&dir.path().join("inspect.json"), 1_000_120);

    let out = viralcraft(&["finalize", dir.path().to_str().unwrap()], dir.path());

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    let error = summary["error"].as_str().unwrap();
    assert!(error.contains("inspect.json"), "{error}");
    assert!(error.contains("judgement.json"), "{error}");
    assert!(!dir.path().join("TEMPLATE.md").exists());
}

#[test]
fn finalize_keeps_what_the_agent_wrote_in_the_template() {
    let mut files = MEASURED.to_vec();
    files.push("judgement.json");
    let dir = run_dir_with(&files);
    let finalize = || viralcraft(&["finalize", dir.path().to_str().unwrap()], dir.path());
    assert!(finalize().status.success());

    let path = dir.path().join("TEMPLATE.md");
    let draft = std::fs::read_to_string(&path).unwrap();
    let rules_at = draft.find("TODO: the rules").unwrap();
    let written = format!(
        "{}- Cut on the beat.\n",
        draft[..rules_at].replacen("| TODO |", "| the first item |", 1)
    );
    std::fs::write(&path, &written).unwrap();

    assert!(finalize().status.success());
    let kept = std::fs::read_to_string(&path).unwrap();
    assert!(kept.contains("| the first item |"), "{kept}");
    assert!(kept.ends_with("- Cut on the beat.\n"), "{kept}");
}

#[test]
fn analyze_stops_before_touching_the_video_when_a_model_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("out");
    let video = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cuts-3.mp4");

    let out = viralcraft(
        &[
            "analyze",
            video.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ],
        dir.path(),
    );

    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["command"], "analyze");
    assert!(
        summary["error"]
            .as_str()
            .unwrap()
            .contains("viralcraft setup"),
        "the error points at the wizard: {summary}"
    );
    assert!(
        !out_dir.exists(),
        "no run directory before the tools check passes"
    );
}

#[test]
fn clean_removes_frames_and_the_normalized_video_only() {
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path();
    std::fs::create_dir(run.join("frames")).unwrap();
    std::fs::write(run.join("frames").join("f00000.jpg"), b"x").unwrap();
    std::fs::write(run.join("source.cfr.mp4"), b"x").unwrap();
    for keep in ["probe.json", "EDIT-SPEC.md", "timeline.otio"] {
        std::fs::write(run.join(keep), b"x").unwrap();
    }

    let out = viralcraft(&["clean", run.to_str().unwrap()], run);

    assert!(out.status.success());
    assert!(!run.join("frames").exists());
    assert!(!run.join("source.cfr.mp4").exists());
    for keep in ["probe.json", "EDIT-SPEC.md", "timeline.otio"] {
        assert!(run.join(keep).exists(), "{keep} must stay");
    }
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["removed"].as_array().unwrap().len(), 2);
}

#[test]
fn there_is_no_preview_command_and_doctor_offers_no_ffmpeg_full() {
    let dir = tempfile::tempdir().unwrap();
    let preview = viralcraft(&["preview", dir.path().to_str().unwrap()], dir.path());
    assert!(!preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stderr).contains("unrecognized subcommand"));

    let doctor = viralcraft(&["doctor"], dir.path());
    let summary = single_json_line(&doctor.stdout);
    let names: Vec<&str> = summary["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert!(!names.iter().any(|n| n.contains("libass")), "{names:?}");
    assert!(!String::from_utf8_lossy(&doctor.stderr).contains("ffmpeg-full"));
}

#[test]
fn a_mistyped_command_still_prints_one_json_line() {
    let dir = tempfile::tempdir().unwrap();
    let out = viralcraft(&["voicover", "x"], dir.path());
    assert_eq!(out.status.code(), Some(2));
    let summary = single_json_line(&out.stdout);
    assert_eq!(summary["ok"], false);
    assert!(
        summary["error"].as_str().unwrap().contains("voicover"),
        "{summary}"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage"));
}

#[test]
fn help_is_still_plain_text() {
    let dir = tempfile::tempdir().unwrap();
    let out = viralcraft(&["--help"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Turn a viral"));
}

#[test]
fn a_missing_voice_key_with_no_one_to_ask_says_to_run_it_in_a_terminal() {
    let dir = run_dir_with(&[
        "probe.json",
        "shots.json",
        "audio.json",
        "inspect.json",
        "judgement.json",
    ]);
    let script = dir.path().join("script.txt");
    std::fs::write(&script, "One\nTwo\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args([
            "voiceover",
            dir.path().to_str().unwrap(),
            "--engine",
            "elevenlabs",
            "--script",
        ])
        .arg(&script)
        .env_remove("ELEVENLABS_API_KEY")
        // Hide any signed-in `elevenlabs` tool on this machine, so the test
        // means the same everywhere.
        .env("PATH", "/nonexistent")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let summary = single_json_line(&out.stdout);
    let error = summary["error"].as_str().unwrap();
    assert!(error.contains("ELEVENLABS_API_KEY"), "{error}");
    assert!(error.contains("your own terminal"), "{error}");
    assert!(
        !dir.path().join("rebuild/voiceover").exists(),
        "nothing spoken without a key"
    );
}
