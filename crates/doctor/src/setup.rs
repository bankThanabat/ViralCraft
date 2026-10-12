//! What the first-run wizard shows and asks, decided from a report alone so
//! the questions can be tested without a terminal.

use std::path::{Path, PathBuf};

use crate::{Item, Kind, Os, Report, State};

/// One status line of the wizard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub mark: Mark,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Ready,
    Missing,
    /// A file is there but does not match the manifest.
    Broken,
    /// An optional extra that is not installed.
    Off,
}

/// One question the wizard asks, in the order it asks them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// ffmpeg and ffprobe, installed by one shared command.
    Tools { fix: String },
    /// On macOS ffmpeg installs through Homebrew, which is not installed.
    NeedsBrew,
    /// Every missing or broken required model, offered as one download.
    Models {
        fixes: Vec<String>,
        bytes: u64,
        dir: PathBuf,
    },
    /// An optional model. The default is no, unless the file is broken:
    /// that fails the check.
    Extra {
        purpose: String,
        fix: String,
        bytes: u64,
        broken: bool,
    },
    /// Interchangeable optional models, offered as one choice.
    Choice { group: String, options: Vec<Choice> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// e.g. "small (0.5 GB, good)".
    pub label: String,
    pub fix: String,
}

/// The things a user recognises: ffmpeg with ffprobe, the required models
/// as a whole, each optional model, each group of interchangeable models.
enum Part<'a> {
    Tools(Vec<&'a Item>),
    Required(Vec<&'a Item>),
    Extra(&'a Item),
    Group(&'a str, Vec<&'a Item>),
}

fn parts(report: &Report) -> Vec<Part<'_>> {
    let tools: Vec<&Item> = report
        .items
        .iter()
        .filter(|i| i.kind == Kind::Tool)
        .collect();
    let (required, optional): (Vec<&Item>, Vec<&Item>) = report
        .items
        .iter()
        .filter(|i| i.kind == Kind::Model)
        .partition(|i| !i.optional);

    let mut parts = Vec::new();
    if !tools.is_empty() {
        parts.push(Part::Tools(tools));
    }
    if !required.is_empty() {
        parts.push(Part::Required(required));
    }
    let mut groups_seen: Vec<&str> = Vec::new();
    for item in &optional {
        match item.group.as_deref() {
            None => parts.push(Part::Extra(item)),
            Some(group) if !groups_seen.contains(&group) => {
                groups_seen.push(group);
                let members = optional
                    .iter()
                    .copied()
                    .filter(|i| i.group.as_deref() == Some(group))
                    .collect();
                parts.push(Part::Group(group, members));
            }
            Some(_) => {}
        }
    }
    parts
}

pub fn status(report: &Report) -> Vec<Line> {
    parts(report)
        .into_iter()
        .map(|part| match part {
            Part::Tools(items) => line("ffmpeg", &items, false, all_present(&items)),
            Part::Required(items) => {
                line("speech and text models", &items, false, all_present(&items))
            }
            Part::Extra(item) => line(&item.purpose, &[item], true, all_present(&[item])),
            Part::Group(group, items) => line(
                group,
                &items,
                true,
                items.iter().any(|i| i.state == State::Present),
            ),
        })
        .collect()
}

fn all_present(items: &[&Item]) -> bool {
    items.iter().all(|i| i.state == State::Present)
}

fn line(label: &str, items: &[&Item], optional: bool, ready: bool) -> Line {
    let any_missing = items.iter().any(|i| i.state == State::Missing);
    let mark = match (ready, any_missing, optional) {
        (true, _, _) => Mark::Ready,
        (false, true, true) => Mark::Off,
        (false, true, false) => Mark::Missing,
        (false, false, _) => Mark::Broken,
    };
    Line {
        label: label.to_string(),
        mark,
        optional,
    }
}

/// The questions to ask for `report`, in order. Nothing present is asked
/// about, and a group with one member installed is not offered again.
pub fn plan(report: &Report, os: Os, has_brew: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    for part in parts(report) {
        match part {
            Part::Tools(tools) => {
                let mut fixes: Vec<&str> = Vec::new();
                for fix in tools.iter().filter_map(|i| i.fix.as_deref()) {
                    if !fixes.contains(&fix) {
                        fixes.push(fix);
                    }
                }
                if fixes.is_empty() {
                    continue;
                }
                if os == Os::MacOs && !has_brew {
                    steps.push(Step::NeedsBrew);
                    continue;
                }
                steps.extend(fixes.into_iter().map(|fix| Step::Tools {
                    fix: fix.to_string(),
                }));
            }
            Part::Required(models) => {
                let todo: Vec<&Item> = models.into_iter().filter(|i| i.fix.is_some()).collect();
                let Some(first) = todo.first() else { continue };
                steps.push(Step::Models {
                    fixes: todo.iter().filter_map(|i| i.fix.clone()).collect(),
                    bytes: todo.iter().filter_map(|i| i.size).sum(),
                    dir: first
                        .path
                        .as_deref()
                        .and_then(Path::parent)
                        .map(Path::to_path_buf)
                        .unwrap_or_default(),
                });
            }
            Part::Extra(item) => {
                let Some(fix) = &item.fix else { continue };
                steps.push(Step::Extra {
                    purpose: item.purpose.clone(),
                    fix: fix.clone(),
                    bytes: item.size.unwrap_or_default(),
                    broken: item.state == State::Corrupt,
                });
            }
            Part::Group(group, members) => {
                if report.group_satisfied(Some(group)) {
                    continue;
                }
                let options = members
                    .iter()
                    .filter_map(|i| {
                        Some(Choice {
                            label: i.purpose.rsplit(": ").next()?.to_string(),
                            fix: i.fix.clone()?,
                        })
                    })
                    .collect();
                steps.push(Step::Choice {
                    group: group.to_string(),
                    options,
                });
            }
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, state: State) -> Item {
        Item {
            name: name.to_string(),
            kind: Kind::Tool,
            state,
            path: (state == State::Present).then(|| PathBuf::from(format!("/usr/bin/{name}"))),
            fix: (state != State::Present).then(|| "brew install ffmpeg".to_string()),
            optional: false,
            purpose: "video and audio decoding".to_string(),
            group: None,
            size: None,
        }
    }

    fn model(name: &str, state: State, size: u64) -> Item {
        Item {
            name: name.to_string(),
            kind: Kind::Model,
            state,
            path: Some(PathBuf::from(format!("/models/{name}"))),
            fix: (state != State::Present).then(|| format!("curl {name}")),
            optional: false,
            purpose: "test".to_string(),
            group: None,
            size: Some(size),
        }
    }

    fn extra(name: &str, state: State, size: u64) -> Item {
        Item {
            optional: true,
            purpose: "sound-effect names".to_string(),
            ..model(name, state, size)
        }
    }

    fn sized(name: &str, state: State, label: &str) -> Item {
        Item {
            optional: true,
            purpose: format!("speech in other languages: {label}"),
            group: Some("speech in other languages".to_string()),
            ..model(name, state, 1)
        }
    }

    fn report(items: Vec<Item>) -> Report {
        Report {
            ok: !items.iter().any(Item::fails),
            items,
        }
    }

    fn first_run() -> Report {
        report(vec![
            tool("ffmpeg", State::Missing),
            tool("ffprobe", State::Missing),
            model("speech.bin", State::Missing, 77_704_715),
            model("detect.rten", State::Present, 2_510_284),
            model("read.rten", State::Corrupt, 9_716_568),
            extra("sfx.onnx", State::Missing, 16_093_355),
            sized("small.bin", State::Missing, "small (0.5 GB, good)"),
            sized("large.bin", State::Missing, "large (3.1 GB, best)"),
        ])
    }

    #[test]
    fn a_first_run_asks_once_per_thing_in_manifest_order() {
        assert_eq!(
            plan(&first_run(), Os::MacOs, true),
            [
                Step::Tools {
                    fix: "brew install ffmpeg".to_string()
                },
                Step::Models {
                    fixes: vec!["curl speech.bin".to_string(), "curl read.rten".to_string()],
                    bytes: 77_704_715 + 9_716_568,
                    dir: PathBuf::from("/models"),
                },
                Step::Extra {
                    purpose: "sound-effect names".to_string(),
                    fix: "curl sfx.onnx".to_string(),
                    bytes: 16_093_355,
                    broken: false,
                },
                Step::Choice {
                    group: "speech in other languages".to_string(),
                    options: vec![
                        Choice {
                            label: "small (0.5 GB, good)".to_string(),
                            fix: "curl small.bin".to_string()
                        },
                        Choice {
                            label: "large (3.1 GB, best)".to_string(),
                            fix: "curl large.bin".to_string()
                        },
                    ],
                },
            ]
        );
    }

    #[test]
    fn without_homebrew_on_a_mac_ffmpeg_gets_advice_instead_of_a_command() {
        let steps = plan(&first_run(), Os::MacOs, false);
        assert_eq!(steps[0], Step::NeedsBrew);
        assert!(
            !steps.iter().any(|s| matches!(s, Step::Tools { .. })),
            "brew install would fail without brew: {steps:?}"
        );
    }

    #[test]
    fn homebrew_only_matters_on_a_mac() {
        let mut report = first_run();
        for item in report.items.iter_mut().filter(|i| i.kind == Kind::Tool) {
            item.fix = Some("sudo apt-get install -y ffmpeg".to_string());
        }
        assert_eq!(
            plan(&report, Os::Linux, false)[0],
            Step::Tools {
                fix: "sudo apt-get install -y ffmpeg".to_string()
            }
        );
    }

    #[test]
    fn a_broken_optional_model_is_offered_as_a_repair() {
        let report = report(vec![extra("sfx.onnx", State::Corrupt, 5)]);
        assert_eq!(
            plan(&report, Os::MacOs, true),
            [Step::Extra {
                purpose: "sound-effect names".to_string(),
                fix: "curl sfx.onnx".to_string(),
                bytes: 5,
                broken: true,
            }]
        );
    }

    #[test]
    fn a_group_with_one_size_installed_is_not_offered_again() {
        let report = report(vec![
            sized("small.bin", State::Present, "small (0.5 GB, good)"),
            sized("large.bin", State::Missing, "large (3.1 GB, best)"),
        ]);
        assert_eq!(plan(&report, Os::MacOs, true), []);
    }

    #[test]
    fn status_shows_one_line_per_thing_the_user_recognises() {
        let ready = |label: &str, optional| Line {
            label: label.to_string(),
            mark: Mark::Ready,
            optional,
        };
        assert_eq!(
            status(&first_run()),
            [
                Line {
                    label: "ffmpeg".to_string(),
                    mark: Mark::Missing,
                    optional: false
                },
                Line {
                    label: "speech and text models".to_string(),
                    mark: Mark::Missing,
                    optional: false
                },
                Line {
                    label: "sound-effect names".to_string(),
                    mark: Mark::Off,
                    optional: true
                },
                Line {
                    label: "speech in other languages".to_string(),
                    mark: Mark::Off,
                    optional: true
                },
            ]
        );
        let all_there = report(vec![
            tool("ffmpeg", State::Present),
            tool("ffprobe", State::Present),
            model("speech.bin", State::Present, 1),
            extra("sfx.onnx", State::Present, 1),
            sized("small.bin", State::Present, "small"),
            sized("large.bin", State::Missing, "large"),
        ]);
        assert_eq!(
            status(&all_there),
            [
                ready("ffmpeg", false),
                ready("speech and text models", false),
                ready("sound-effect names", true),
                ready("speech in other languages", true),
            ],
            "one installed size is enough for the group"
        );
    }
}
