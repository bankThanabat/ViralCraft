use std::process::Command;

fn viralcraft(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_viralcraft"))
        .args(args)
        .output()
        .expect("the test binary is built by cargo before tests run")
}

#[test]
fn doctor_prints_one_json_summary_line() {
    let out = viralcraft(&["doctor"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout was: {stdout:?}");

    let summary: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(summary["command"], "doctor");
}

#[test]
fn unknown_subcommand_fails_with_usage_on_stderr() {
    let out = viralcraft(&["nope"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage"));
}
