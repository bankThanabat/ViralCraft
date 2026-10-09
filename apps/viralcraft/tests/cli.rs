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
fn unknown_subcommand_fails_with_usage_on_stderr() {
    let empty = tempfile::tempdir().unwrap();
    let out = viralcraft(&["nope"], empty.path());
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage"));
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
    }
    dir
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
    for f in ["captions.vtt", "captions.ass", "timeline.otio"] {
        assert!(dir.path().join(f).exists(), "{f}");
    }
}
