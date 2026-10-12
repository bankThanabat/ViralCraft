use doctor::setup::{self, Choice, Mark, Step};
use doctor::Os;
use serde_json::{json, Value};

use crate::doctor_cmd::{self, run_fix};
use crate::prompt;

const FILMCRAFT_URL: &str = "https://getartcraft.com/apps/filmcraft";

pub fn run() -> anyhow::Result<(Value, bool)> {
    let report = doctor_cmd::check();
    let filmcraft = which::which("filmcraft-cli").is_ok();

    eprintln!("ViralCraft setup. Here is what is installed:\n");
    for line in setup::status(&report) {
        print_line(&line.label, line.mark, line.optional);
    }
    print_line(
        "FilmCraft",
        if filmcraft { Mark::Ready } else { Mark::Off },
        true,
    );

    let has_brew = which::which("brew").is_ok();
    for step in setup::plan(&report, Os::current(), has_brew) {
        eprintln!();
        walk(&step);
    }

    eprintln!();
    if filmcraft {
        eprintln!(
            "FilmCraft is installed, so your agent can rebuild the video from your own clips."
        );
    } else {
        eprintln!(
            "FilmCraft is optional: it rebuilds the video from your own clips. Get it at {FILMCRAFT_URL}"
        );
    }

    let report = doctor_cmd::check();
    eprintln!();
    if report.ok {
        eprintln!(
            "Ready. Put a viral video in `inbox/` and your clips in `assets/`, then ask your agent: \"copy this video with my clips\""
        );
    } else {
        eprintln!("Not ready yet. Still missing, with the command that installs each:");
        for item in report.items.iter().filter(|i| i.fails()) {
            eprintln!("  {}: {}", item.name, item.fix.as_deref().unwrap_or("?"));
        }
    }

    let summary = json!({ "ok": report.ok, "items": report.items, "filmcraft": filmcraft });
    Ok((summary, report.ok))
}

fn print_line(label: &str, mark: Mark, optional: bool) {
    let mark = match mark {
        Mark::Ready => "ready  ",
        Mark::Missing => "missing",
        Mark::Broken => "broken ",
        Mark::Off => "off    ",
    };
    let optional = if optional { " (optional)" } else { "" };
    eprintln!("  {mark}  {label}{optional}");
}

fn walk(step: &Step) {
    const FFMPEG: &str =
        "ffmpeg is missing. ViralCraft uses it to read the video's frames and sound.";
    match step {
        Step::NeedsBrew => {
            eprintln!("{FFMPEG}");
            eprintln!("On a Mac, ffmpeg installs through Homebrew, which is not installed.");
            eprintln!("Install Homebrew from https://brew.sh, then run setup again.");
        }
        Step::Tools { fix } => {
            eprintln!("{FFMPEG}");
            if prompt::confirm_default_yes(&format!("Install ffmpeg with `{fix}`? [Y/n]")) {
                install(std::slice::from_ref(fix));
            }
        }
        Step::Models { fixes, bytes, dir } => {
            eprintln!("ViralCraft needs models to hear speech and read the text on screen.");
            let question = format!(
                "Download the speech and text models ({}) to {}? [Y/n]",
                megabytes(*bytes),
                dir.display()
            );
            if prompt::confirm_default_yes(&question) {
                install(fixes);
            }
        }
        Step::Extra {
            purpose,
            fix,
            bytes,
            broken: false,
        } => {
            let question = format!(
                "Optional: do you want {purpose}? It downloads {}. [y/N]",
                megabytes(*bytes)
            );
            if prompt::confirm(&question) {
                install(std::slice::from_ref(fix));
            }
        }
        Step::Extra {
            purpose,
            fix,
            bytes,
            broken: true,
        } => {
            let question = format!(
                "The file for {purpose} is damaged. Download it again ({})? [Y/n]",
                megabytes(*bytes)
            );
            if prompt::confirm_default_yes(&question) {
                install(std::slice::from_ref(fix));
            }
        }
        Step::Choice { group, options } => {
            if let Some(pick) = choose(group, options) {
                install(std::slice::from_ref(&pick.fix));
            }
        }
    }
}

fn choose<'a>(group: &str, options: &'a [Choice]) -> Option<&'a Choice> {
    let mut question =
        format!("Optional: do you want {group} (Thai, Japanese, Spanish...)? [n] no");
    for option in options {
        let key = option.label.chars().next().unwrap_or('?');
        question.push_str(&format!(", [{key}] {}", option.label));
    }
    let answer = prompt::answer(&question)?;
    options
        .iter()
        .find(|o| o.label.starts_with(answer.as_str()))
}

fn install(fixes: &[String]) {
    for fix in fixes {
        if let Err(e) = run_fix(fix) {
            eprintln!("error: {e:#}");
        }
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", (bytes + 500_000) / 1_000_000)
}
