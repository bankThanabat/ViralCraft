use std::io;
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use doctor::{Env, Item, Os, Report, State, MODELS};
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

pub fn run(install: bool) -> anyhow::Result<(Value, bool)> {
    let models_dir = doctor::default_models_dir();
    let env = Env {
        os: Os::current(),
        models_dir: &models_dir,
        find_tool: Box::new(|name| which::which(name).ok()),
    };

    let mut report = doctor::check(&env, &MODELS);
    print_report(&report);

    if install && report.has_offers() {
        doctor::install(&report, ask, choose, run_fix)?;
        report = doctor::check(&env, &MODELS);
        eprintln!();
        print_report(&report);
    } else if !report.ok {
        eprintln!(
            "\nRun `viralcraft doctor --install` to install, or run the commands above yourself."
        );
    } else if report.has_offers() {
        eprintln!("\nOptional features are off. `viralcraft doctor --install` offers them.");
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
                eprintln!("off      {group}  (optional: small, medium or large; `viralcraft doctor --install`)");
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

/// One question for a group of interchangeable models, e.g. the three
/// multilingual speech sizes.
fn choose(group: &str, choices: &[&Item]) -> Option<usize> {
    let mut question =
        format!("Optional: do you want {group} (Thai, Japanese, Spanish...)? [n] no");
    for c in choices {
        let size = c.purpose.rsplit(": ").next().unwrap_or(&c.purpose);
        let key = size.chars().next().unwrap_or('?');
        question.push_str(&format!(", [{key}] {size}"));
    }
    let answer = crate::prompt::answer(&question)?;
    choices.iter().position(|c| {
        let size = c.purpose.rsplit(": ").next().unwrap_or(&c.purpose);
        size.starts_with(answer.as_str()) && !answer.is_empty()
    })
}

fn ask(item: &Item) -> bool {
    let fix = item.fix.as_deref().unwrap_or_default();
    if item.optional && item.state == State::Missing {
        return crate::prompt::confirm(&format!(
            "Optional: do you want {}? It downloads {} with `{fix}`. [y/N]",
            item.purpose, item.name
        ));
    }
    crate::prompt::confirm(&format!("Install {} with `{fix}`? [y/N]", item.name))
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
