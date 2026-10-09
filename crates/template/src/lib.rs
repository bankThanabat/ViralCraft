//! `template`: the reusable format. The CLI writes the slots and the
//! observed timings; the agent writes "fill with" and the rules, because
//! deciding which patterns are intentional is judgement.

use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use editspec::fmt::{num, seconds, transform_summary};
use editspec::Inputs;
use model::inspect::caption_styles;
use model::RationalTime;

pub const TEMPLATE_FILE: &str = "TEMPLATE.md";

/// `hook: claim` -> `HOOK`; a label without a colon becomes `BEAT n`.
fn slot_name(label: &str, n: usize) -> String {
    match label.split_once(':') {
        Some((name, _)) if !name.trim().is_empty() => name.trim().to_uppercase(),
        _ => format!("BEAT {n}"),
    }
}

pub fn render(x: &Inputs) -> String {
    let j = x.judgement.clone().unwrap_or_default();
    let mut caps = x.inspect.captions.clone();
    j.apply_to_captions(&mut caps);
    let at = |f: i64| RationalTime::from_frames(f, x.probe.rate);
    let segs = &x.shots.segments;
    let cuts: Vec<i64> = segs.iter().skip(1).map(|s| s.range.start.value).collect();
    let a = &x.audio;

    let mut d = String::new();
    let name = std::path::Path::new(&x.probe.original)
        .file_name()
        .map_or(x.probe.original.clone(), |n| {
            n.to_string_lossy().into_owned()
        });
    let format = j
        .format_name
        .clone()
        .unwrap_or_else(|| "untitled format".to_string());
    let beat = if a.beats.is_empty() {
        "no steady beat".to_string()
    } else {
        format!("{} bpm", num(a.tempo.bpm))
    };
    let _ = writeln!(d, "# Template: {format}\n");
    let _ = writeln!(
        d,
        "derived from {name} · {} · {} · {} shots · {} cuts · {beat} · {} captions\n",
        seconds(at(x.probe.frames)),
        x.probe.aspect,
        segs.len(),
        cuts.len(),
        caps.len()
    );

    let styles = caption_styles(&caps);
    let _ = writeln!(d, "## Slots");
    let _ = writeln!(
        d,
        "| slot | from | to | length | shots | picture | text | fill with |"
    );
    let _ = writeln!(
        d,
        "|------|------|----|--------|-------|---------|------|-----------|"
    );
    for (i, b) in j.story_beats.iter().enumerate() {
        let (start, end) = (b.start, b.start + b.duration);
        let in_beat: Vec<u32> = segs
            .iter()
            .filter(|s| s.range.start.value < end && s.range.end_frame() > start)
            .map(|s| s.n)
            .collect();
        let mut picture: Vec<String> = in_beat
            .iter()
            .filter_map(|&n| j.shot(n).and_then(|s| s.role.clone()))
            .collect();
        picture.dedup();
        let mut text: Vec<String> = Vec::new();
        for (id, members) in &styles {
            let count = members
                .iter()
                .filter(|&&m| (start..end).contains(&caps[m].range.start.value))
                .count();
            if count > 0 {
                text.push(format!("{id} ×{count}"));
            }
        }
        let shots: Vec<String> = in_beat.iter().map(u32::to_string).collect();
        let _ = writeln!(
            d,
            "| {} | f{start} | f{end} | {} | {} | {} | {} | TODO |",
            slot_name(&b.label, i + 1),
            seconds(at(end - start)),
            if shots.is_empty() {
                "none".to_string()
            } else {
                shots.join(", ")
            },
            if picture.is_empty() {
                "unknown".to_string()
            } else {
                picture.join(" + ")
            },
            if text.is_empty() {
                "none".to_string()
            } else {
                text.join(", ")
            },
        );
    }
    if j.story_beats.is_empty() {
        let _ = writeln!(
            d,
            "| BEAT 1 | f0 | f{} | {} | all | unknown | none | TODO |",
            x.probe.frames,
            seconds(at(x.probe.frames))
        );
    }

    let _ = writeln!(d, "\n## Observed timings");
    let lengths: Vec<f64> = segs.iter().map(|s| s.range.duration.to_seconds()).collect();
    let mean = lengths.iter().sum::<f64>() / lengths.len().max(1) as f64;
    let (lo, hi) = lengths
        .iter()
        .fold((f64::MAX, 0.0f64), |(l, h), &v| (l.min(v), h.max(v)));
    let cut_list: Vec<String> = cuts.iter().map(|c| format!("f{c}")).collect();
    let _ = writeln!(
        d,
        "- cuts: {} (shots {:.3}s on average, {:.3}s to {:.3}s)",
        if cut_list.is_empty() {
            "none".to_string()
        } else {
            cut_list.join(", ")
        },
        mean,
        if lo == f64::MAX { 0.0 } else { lo },
        hi
    );
    if a.beats.is_empty() {
        let _ = writeln!(
            d,
            "- beat: none steady (confidence {:.2})",
            a.tempo.confidence
        );
    } else {
        let on_beat = cuts
            .iter()
            .filter(|c| a.beats.iter().any(|b| (b.time.value - **c).abs() <= 2))
            .count();
        let _ = writeln!(
            d,
            "- beat: {} bpm (confidence {:.2}); cuts on beat: {on_beat} of {} within ±2 frames",
            num(a.tempo.bpm),
            a.tempo.confidence,
            cuts.len()
        );
    }
    if caps.is_empty() {
        let _ = writeln!(d, "- captions: none");
    } else {
        let durations: Vec<f64> = caps.iter().map(|c| c.range.duration.to_seconds()).collect();
        let avg = durations.iter().sum::<f64>() / durations.len() as f64;
        let mut anchors: Vec<String> = Vec::new();
        for c in &caps {
            if !anchors.contains(&c.anchor) {
                anchors.push(c.anchor.clone());
            }
        }
        let _ = writeln!(
            d,
            "- captions: {} in {} style(s); on screen {:.3}s on average; anchors {}",
            caps.len(),
            styles.len(),
            avg,
            anchors.join(" ")
        );
    }
    for seg in segs {
        let motion = x.inspect.motion.iter().find(|m| m.shot == seg.n);
        let path = format!("shots[{}].camera", seg.n);
        let camera = match j.overrides.iter().find(|o| o.path == path) {
            Some(o) => o.value.clone(),
            None => transform_summary(motion.and_then(|m| m.transform.as_ref())),
        };
        let _ = writeln!(d, "- camera, shot {}: {camera}", seg.n);
    }
    if a.music.is_empty() {
        let _ = writeln!(d, "- music: none");
    } else {
        let beds: Vec<String> = a
            .music
            .iter()
            .map(|m| {
                format!(
                    "f{}–f{}{}",
                    m.range.start.value,
                    m.range.end_frame(),
                    if m.beat { " with beat" } else { "" }
                )
            })
            .collect();
        let _ = writeln!(d, "- music: {}", beds.join(", "));
    }
    if a.sounds.is_empty() {
        let _ = writeln!(d, "- sounds: none named");
    } else {
        let list: Vec<String> = a
            .sounds
            .iter()
            .map(|s| format!("{} at f{}", s.label, s.start.value))
            .collect();
        let _ = writeln!(d, "- sounds: {}", list.join(", "));
    }
    let look = j.look.clone().unwrap_or_default();
    let _ = writeln!(
        d,
        "- look: {} ({})",
        look.name.unwrap_or_else(|| "unnamed".to_string()),
        look.match_by
            .unwrap_or_else(|| "no match notes".to_string())
    );

    let _ = writeln!(d, "\n## Rules  <!-- judgement -->");
    let _ = writeln!(
        d,
        "TODO: the rules an editor follows to rebuild this format with new footage, drawn from the timings above (e.g. \"cut on every beat\", \"one caption per spoken sentence at 37% height\"), with any exceptions."
    );
    d
}

/// Writes `TEMPLATE.md` unless the agent has already completed it (no
/// `TODO:` left), so re-running finalize never overwrites finished work.
pub fn run(run_dir: &Path, inputs: &Inputs) -> io::Result<PathBuf> {
    let path = run_dir.join(TEMPLATE_FILE);
    let finished = fs::read_to_string(&path).is_ok_and(|t| !t.contains("TODO:"));
    if !finished {
        fs::write(&path, render(inputs))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn judged() -> editspec::Inputs {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../editspec/tests/data/cuts-3"
        ));
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
        editspec::Inputs {
            probe: serde_json::from_str(&read("probe.json")).unwrap(),
            shots: serde_json::from_str(&read("shots.json")).unwrap(),
            audio: serde_json::from_str(&read("audio.json")).unwrap(),
            inspect: serde_json::from_str(&read("inspect.json")).unwrap(),
            judgement: Some(serde_json::from_str(&read("judgement.json")).unwrap()),
        }
    }

    #[test]
    fn one_slot_per_story_beat_with_frames() {
        let doc = render(&judged());
        let rows: Vec<&str> = doc
            .lines()
            .filter(|l| l.starts_with("| ") && !l.starts_with("| slot"))
            .collect();
        assert_eq!(rows.len(), 3, "{doc}");
        assert!(rows[0].starts_with("| HOOK | f0 | f60 |"), "{}", rows[0]);
        assert!(rows[1].starts_with("| BUILD | f60 | f120 |"), "{}", rows[1]);
        assert!(
            rows[2].starts_with("| PAYOFF | f120 | f180 |"),
            "{}",
            rows[2]
        );
    }

    #[test]
    fn the_rules_section_is_one_todo_for_the_agent() {
        let doc = render(&judged());
        let rules = &doc[doc.find("## Rules").unwrap()..];
        assert_eq!(rules.matches("TODO:").count(), 1);
    }

    #[test]
    fn observed_timings_name_the_cuts_and_the_beat() {
        let doc = render(&judged());
        assert!(doc.contains("cuts: f60, f120"), "{doc}");
        assert!(doc.contains("120 bpm"));
    }

    #[test]
    fn a_camera_override_replaces_the_measured_move() {
        // cuts-3's judgement overrides shot 1's camera to "static".
        let doc = render(&judged());
        assert!(doc.contains("- camera, shot 1: static\n"), "{doc}");
    }

    #[test]
    fn cuts_3_template_snapshot() {
        insta::assert_snapshot!(render(&judged()));
    }
}
