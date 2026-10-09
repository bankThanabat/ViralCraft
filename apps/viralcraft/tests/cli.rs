use std::path::Path;
use std::process::Command;

fn viralcraft(args: &[&str], models_dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args(args)
        .env("VIRALCRAFT_MODELS", models_dir)
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
