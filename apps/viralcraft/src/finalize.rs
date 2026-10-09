//! `viralcraft finalize`: after the agent writes `judgement.json`, check it
//! is complete, then write the finished edit spec, every export and the
//! template scaffold (kept if the agent has already completed it).

use std::path::Path;

use anyhow::bail;
use model::Needs;
use serde_json::{json, Value};

pub fn run(run_dir: &Path) -> anyhow::Result<(Value, bool)> {
    let judgement_path = run_dir.join(editspec::JUDGEMENT_FILE);
    if !judgement_path.exists() {
        bail!(
            "{} not found: the agent writes it after reviewing EDIT-SPEC.md",
            judgement_path.display()
        );
    }
    let stale = changed_since(run_dir, &judgement_path);
    if !stale.is_empty() {
        bail!(
            "{} changed after {} was written: shot and caption ids may now point at different \
             things. Review EDIT-SPEC.md and the frames again and rewrite {}",
            stale.join(" and "),
            editspec::JUDGEMENT_FILE,
            editspec::JUDGEMENT_FILE
        );
    }
    let inputs = editspec::load(run_dir)?;
    let judgement = inputs.judgement.clone().unwrap_or_default();
    let needs = Needs {
        shots: inputs.shots.segments.iter().map(|s| s.n).collect(),
        captions: inputs
            .inspect
            .captions
            .iter()
            .map(|c| c.id.clone())
            .collect(),
        overlays: inputs
            .inspect
            .overlays
            .iter()
            .map(|o| o.id.clone())
            .collect(),
    };
    let missing = judgement.missing(&needs);
    if !missing.is_empty() {
        eprintln!("judgement.json is incomplete:");
        for m in &missing {
            eprintln!("  {m}");
        }
        return Ok((
            json!({ "ok": false, "error": "judgement.json is incomplete", "missing": missing }),
            false,
        ));
    }

    let mut files = vec![editspec::run(run_dir)?];
    files.extend(exports::run(run_dir)?);
    files.push(template::run(run_dir, &inputs)?);
    Ok((json!({ "ok": true, "files": files }), true))
}

/// The measured files that number what the judgement refers to (shots,
/// captions, overlays) and are newer than the judgement. Re-running
/// `shots` or `inspect` renumbers them.
fn changed_since(run_dir: &Path, judgement: &Path) -> Vec<&'static str> {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(judged) = modified(judgement) else {
        return Vec::new();
    };
    [shots::SHOTS_FILE, inspect::INSPECT_FILE]
        .into_iter()
        .filter(|f| modified(&run_dir.join(f)).is_some_and(|t| t > judged))
        .collect()
}
