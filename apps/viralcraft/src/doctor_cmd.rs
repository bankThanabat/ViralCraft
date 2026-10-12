use std::io;
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use doctor::{Env, Os, Report, State, MODELS};
use serde_json::{json, Value};

/// The tools-and-models check on this machine, without printing.
pub fn check() -> Report {
    let models_dir = doctor::default_models_dir();
    let env = Env {
        os: Os::current(),
        models_dir: &models_dir,
        find_tool: Box::new(|name| which::which(name).ok()),
    };
    doctor::check(&env, &MODELS)
}

pub fn run() -> anyhow::Result<(Value, bool)> {
    let report = check();
    print_report(&report);
    if !report.ok {
        eprintln!("\nRun `npm run setup` (or `viralcraft setup`) to install them.");
    } else if report.has_offers() {
        eprintln!("\nOptional features are off. `viralcraft setup` offers them.");
    }
    let summary = json!({ "ok": report.ok, "items": report.items });
    Ok((summary, report.ok))
}

pub fn print_report(report: &Report) {
    let mut groups_shown: Vec<String> = Vec::new();
    for item in &report.items {
        if let Some(group) = &item.group {
            if groups_shown.contains(group) {
                continue;
            }
            groups_shown.push(group.clone());
            let installed: Vec<&str> = report
                .items
                .iter()
                .filter(|i| i.group.as_ref() == Some(group) && i.state == State::Present)
                .map(|i| i.name.as_str())
                .collect();
            if installed.is_empty() {
                eprintln!(
                    "off      {group}  (optional: small, medium or large; `viralcraft setup`)"
                );
            } else {
                eprintln!("ok       {group}  ({})", installed.join(", "));
            }
            continue;
        }
        let mark = match (item.state, item.optional) {
            (State::Present, _) => "ok     ",
            (State::Missing, true) => "off    ",
            (State::Missing, false) => "missing",
            (State::Corrupt, _) => "corrupt",
        };
        if item.optional && item.state == State::Missing {
            eprintln!("{mark}  {}  (optional: {})", item.name, item.purpose);
            continue;
        }
        match &item.fix {
            None => eprintln!("{mark}  {}", item.name),
            Some(fix) => eprintln!("{mark}  {}  →  {fix}", item.name),
        }
    }
}

/// Runs a fix through the platform shell. The child's stdout is sent to our
/// stderr so stdout stays a single JSON line.
pub fn run_fix(cmd: &str) -> anyhow::Result<()> {
    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    command.stdout(stderr_as_stdio()?);
    let status = command
        .status()
        .with_context(|| format!("could not start `{cmd}`"))?;
    if !status.success() {
        bail!("`{cmd}` failed with {status}");
    }
    Ok(())
}

#[cfg(unix)]
fn stderr_as_stdio() -> io::Result<Stdio> {
    use std::os::fd::AsFd;
    Ok(Stdio::from(io::stderr().as_fd().try_clone_to_owned()?))
}

#[cfg(windows)]
fn stderr_as_stdio() -> io::Result<Stdio> {
    use std::os::windows::io::AsHandle;
    Ok(Stdio::from(io::stderr().as_handle().try_clone_to_owned()?))
}
