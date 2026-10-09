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
