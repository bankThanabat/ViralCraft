use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use doctor::{Env, Item, Os, Report, State, MODELS};
use serde_json::{json, Value};

pub fn run(install: bool) -> anyhow::Result<(Value, bool)> {
    let models_dir = doctor::default_models_dir();
    let env = Env {
        os: Os::current(),
        models_dir: &models_dir,
        find_tool: Box::new(|name| which::which(name).ok()),
    };

    let mut report = doctor::check(&env, &MODELS);
    print_report(&report);

    if install && !report.ok {
        doctor::install(&report, ask, run_fix)?;
        report = doctor::check(&env, &MODELS);
        eprintln!();
        print_report(&report);
    } else if !report.ok {
        eprintln!(
            "\nRun `viralcraft doctor --install` to install, or run the commands above yourself."
        );
    }

    let summary = json!({ "command": "doctor", "ok": report.ok, "items": report.items });
    Ok((summary, report.ok))
}

fn print_report(report: &Report) {
    for item in &report.items {
        let mark = match item.state {
            State::Present => "ok     ",
            State::Missing => "missing",
            State::Corrupt => "corrupt",
        };
        match &item.fix {
            None => eprintln!("{mark}  {}", item.name),
            Some(fix) => eprintln!("{mark}  {}  →  {fix}", item.name),
        }
    }
}

fn ask(item: &Item) -> bool {
    let fix = item.fix.as_deref().unwrap_or_default();
    eprint!("Install {} with `{fix}`? [y/N] ", item.name);
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes")
}

/// Runs a fix through the platform shell. The child's stdout is sent to our
/// stderr so stdout stays a single JSON line.
fn run_fix(cmd: &str) -> anyhow::Result<()> {
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
