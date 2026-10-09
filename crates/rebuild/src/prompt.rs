//! `REBUILD-PROMPT.md`: the finished template turned into steps any editor
//! agent can follow with the user's clips. Nothing here names an editor.

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use editspec::fmt::{animation, cell, norm, num, seconds, transform_summary};
use editspec::Inputs;
use model::inspect::caption_styles;
use model::{Assets, CaptionEvent, RationalTime, TransitionKind};

use crate::assets::{ASSETS_FILE, REBUILD_DIR};

pub const PROMPT_FILE: &str = "REBUILD-PROMPT.md";

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("{0} is missing; run `viralcraft finalize` first")]
    NoTemplate(PathBuf),
    #[error("{0} still has TODO: slots; finish the review before rebuilding")]
    TemplateTodo(PathBuf),
    #[error("{0} is missing; run `viralcraft assets` first")]
    NoAssets(PathBuf),
    #[error("{0} was made from a different script; run `viralcraft voiceover` again")]
    VoiceoverMismatch(PathBuf),
    #[error("{path}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error(transparent)]
    Run(#[from] editspec::EditSpecError),
}

/// One caption line per non-empty line of the script.
pub fn script_lines(script: &str) -> Vec<String> {
    script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

pub struct PromptInputs<'a> {
    pub run: &'a Inputs,
    pub template: &'a str,
    pub assets: &'a Assets,
    pub script: Option<&'a [String]>,
    /// An AI voice reading the script, made by `viralcraft voiceover`.
    pub voiceover: Option<&'a model::Voiceover>,
    /// False when the user chose a rebuild without captions.
    pub captions: bool,
}

/// Where caption text comes from, in order of preference.
enum TextSource<'a> {
    Script(&'a [String]),
    Transcripts,
    Draft,
    /// The user chose no captions.
    Off,
}

/// The captions a rebuild remakes, in order: the judged captions with
/// pieces joined, without scene text and without the original creator's
/// watermark, which is theirs.
pub fn rebuilt_captions(run: &Inputs) -> Vec<CaptionEvent> {
    let j = run.judgement.clone().unwrap_or_default();
    let mut caps = run.inspect.captions.clone();
    j.apply_to_captions(&mut caps);
    caps.retain(|c| j.caption(&c.id).and_then(|n| n.kind.as_deref()) != Some("watermark"));
    caps
}

pub fn render(x: &PromptInputs) -> String {
    let r = x.run;
    let rate = r.probe.rate;
    let at = |f: i64| RationalTime::from_frames(f, rate);
    let j = r.judgement.clone().unwrap_or_default();
    let kind = |c: &CaptionEvent| {
        j.caption(&c.id)
            .and_then(|n| n.kind.clone())
            .unwrap_or_else(|| "caption".to_string())
    };
    let caps = rebuilt_captions(r);
    let clips: Vec<&model::Asset> = x
        .assets
        .clips
        .iter()
        .filter(|c| c.duplicate_of.is_none())
        .collect();
    let source = match x.script {
        _ if !x.captions => TextSource::Off,
        Some(lines) => TextSource::Script(lines),
        None if clips.iter().any(|c| !c.words.is_empty()) => TextSource::Transcripts,
        None => TextSource::Draft,
    };
    let format = j
        .format_name
        .clone()
        .unwrap_or_else(|| "untitled format".to_string());

    let mut d = String::new();
    let _ = writeln!(d, "# Rebuild: {format}\n");
    let _ = writeln!(
        d,
        "Remake this format with the user's own clips, in any editor. Work through the steps in order. \
         Times are frames at {} fps: f{} is one second. A range from fA to fB ends just before fB.\n",
        num(rate.as_f64()),
        num(rate.as_f64())
    );

    let _ = writeln!(d, "## 1. Sequence");
    let _ = writeln!(
        d,
        "- {} x {} ({}), {} fps, {} frames ({})",
        r.probe.width,
        r.probe.height,
        r.probe.aspect,
        num(rate.as_f64()),
        r.probe.frames,
        seconds(at(r.probe.frames))
    );
    let _ = writeln!(d, "- import every clip listed under \"Your clips\"");
    let _ = writeln!(
        d,
        "- a clip of another size or shape is scaled to fill the frame and the overflow cropped; no black bars\n"
    );

    let _ = writeln!(d, "## 2. Picture");
    let _ = writeln!(
        d,
        "Fill each slot with footage that matches \"fill with\". Cut where the original cut so the rhythm stays. \
         Never stretch or slow a clip to fill time; trim instead. Pick clips by looking at their thumbnails.\n"
    );
    let _ = writeln!(
        d,
        "| slot | from | to | length | fill with | clips long enough |"
    );
    let _ = writeln!(
        d,
        "|------|------|----|--------|-----------|-------------------|"
    );
    let slots = template::slots(x.template);
    for s in &slots {
        let need = at(s.to - s.from).to_seconds();
        let fits: Vec<&str> = clips
            .iter()
            .filter(|c| c.seconds + 1e-6 >= need)
            .map(|c| c.file.as_str())
            .collect();
        let _ = writeln!(
            d,
            "| {} | f{} | f{} | {} | {} | {} |",
            s.name,
            s.from,
            s.to,
            seconds(at(s.to - s.from)),
            cell(&s.fill_with),
            if fits.is_empty() {
                "none: join two clips".to_string()
            } else {
                fits.join(", ")
            }
        );
    }
    let _ = writeln!(d, "\nShots inside the slots:\n");
    let _ = writeln!(d, "| shot | from | to | length | slot | camera |");
    let _ = writeln!(d, "|------|------|----|--------|------|--------|");
    for seg in &r.shots.segments {
        let (from, to) = (seg.range.start.value, seg.range.end_frame());
        let slot = slots
            .iter()
            .find(|s| from >= s.from && from < s.to)
            .map_or("none", |s| s.name.as_str());
        let motion = r.inspect.motion.iter().find(|m| m.shot == seg.n);
        let _ = writeln!(
            d,
            "| {} | f{from} | f{to} | {} | {slot} | {} |",
            seg.n,
            seconds(at(to - from)),
            transform_summary(motion.and_then(|m| m.transform.as_ref()))
        );
    }
    let _ = writeln!(d);

    captions(&mut d, x, &caps, &source, &kind);

    let _ = writeln!(d, "## 4. Transitions");
    if r.shots.transitions.is_empty() {
        let _ = writeln!(d, "Every shot change is a hard cut.\n");
    } else {
        for t in &r.shots.transitions {
            let k = match t.kind {
                TransitionKind::Dissolve => "dissolve",
                TransitionKind::Flash => "white flash",
            };
            let _ = writeln!(
                d,
                "- {k} at f{} lasting {} frames",
                t.range.start.value, t.range.duration.value
            );
        }
        let _ = writeln!(d, "\nEvery other shot change is a hard cut.\n");
    }

    sound(&mut d, x, &clips);

    let _ = writeln!(d, "## 6. Look");
    let look = j.look.clone().unwrap_or_default();
    let _ = writeln!(
        d,
        "{}: {}\n",
        look.name.unwrap_or_else(|| "unnamed".to_string()),
        look.match_by
            .unwrap_or_else(|| "keep the clips as shot".to_string())
    );

    let _ = writeln!(d, "## 7. Rules");
    let _ = writeln!(d, "{}\n", template::rules(x.template));

    let _ = writeln!(d, "## 8. Done when");
    let cuts: Vec<String> = r
        .shots
        .segments
        .iter()
        .skip(1)
        .map(|s| format!("f{}", s.range.start.value))
        .collect();
    let _ = writeln!(
        d,
        "- the sequence is {} frames at {} fps and {} x {}",
        r.probe.frames,
        num(rate.as_f64()),
        r.probe.width,
        r.probe.height
    );
    let _ = writeln!(
        d,
        "- the picture cuts at {} (each within 1 frame)",
        if cuts.is_empty() {
            "no point: one shot".to_string()
        } else {
            cuts.join(", ")
        }
    );
    if x.captions {
        let _ = writeln!(
            d,
            "- every caption above shows at its frames, place and style, as editable text"
        );
    }
    if x.voiceover.is_some() {
        let _ = writeln!(
            d,
            "- every voiceover line plays from its frame, and the video is labelled as having an AI voice where the platform asks"
        );
    }
    let _ = writeln!(d, "- no frame is black or empty, and no clip is stretched");
    let _ = writeln!(
        d,
        "- the export plays start to end; watch it next to the original\n"
    );

    your_clips(&mut d, x);
    d
}

fn captions(
    d: &mut String,
    x: &PromptInputs,
    caps: &[CaptionEvent],
    source: &TextSource,
    kind: &dyn Fn(&CaptionEvent) -> String,
) {
    let r = x.run;
    let j = r.judgement.clone().unwrap_or_default();
    let _ = writeln!(d, "## 3. Captions");
    if caps.is_empty() {
        let _ = writeln!(d, "None.\n");
        return;
    }
    if let TextSource::Off = source {
        let _ = writeln!(d, "None: the user chose a rebuild without captions.\n");
        return;
    }
    let _ = writeln!(
        d,
        "Make every caption editable text, not burned into the picture. Positions are the centre of the text \
         as a fraction of the frame (0,0 top left) and in pixels.\n"
    );
    let styles = caption_styles(caps);
    for (id, members) in &styles {
        let c = &caps[members[0]];
        let font = members
            .iter()
            .find_map(|&i| {
                j.caption(&caps[i].id)
                    .and_then(|n| n.font_substitute.clone())
            })
            .unwrap_or_else(|| "a bold sans serif".to_string());
        let mut sizes: Vec<f64> = members.iter().map(|&i| caps[i].font_px_estimate).collect();
        sizes.sort_by(f64::total_cmp);
        let _ = writeln!(
            d,
            "- style {id}: {font}, about {} px, fill {}, outline {}, box {}{}",
            num(sizes[sizes.len() / 2]),
            c.fill,
            c.outline.as_deref().unwrap_or("none"),
            c.box_color.as_deref().unwrap_or("none"),
            if members.iter().all(|&i| caps[i].uppercase) {
                ", all caps"
            } else {
                ""
            }
        );
    }
    let _ = writeln!(d);
    match source {
        TextSource::Script(_) => {
            let _ = writeln!(
                d,
                "Text: the user's script, one line per caption in order.\n"
            );
        }
        TextSource::Transcripts => {
            let _ = writeln!(
                d,
                "Text: no script was given. Write each caption from what is said in the clip under it \
                 (transcripts under \"Your clips\", word times in rebuild/assets.json), about as long as the original line. \
                 Show the lines to the user before building.\n"
            );
        }
        TextSource::Draft => {
            let _ = writeln!(
                d,
                "Text: no script and no speech in the clips. Stop and ask the user which they want:\n\
                 - an AI voiceover: draft one line per caption below, in the voice and length of the \
                 original, get the user's approval, save it as a script and run \
                 `viralcraft voiceover <run_dir> --script <file> --engine <say|openai|elevenlabs|minimax|qwen>`, \
                 then make this prompt again with `--script <file>`\n\
                 - no captions: make this prompt again with `--no-captions`\n"
            );
        }
        TextSource::Off => {}
    }
    let _ = writeln!(
        d,
        "| id | from | to | kind | style | text | original | centre | pixels | animation |"
    );
    let _ = writeln!(
        d,
        "|----|------|----|------|-------|------|----------|--------|--------|-----------|"
    );
    let mut next_line = 0usize;
    for (i, c) in caps.iter().enumerate() {
        let style = styles
            .iter()
            .find(|(_, m)| m.contains(&i))
            .map_or("?", |(id, _)| id.as_str());
        let text = match source {
            TextSource::Script(lines) => match lines.get(next_line) {
                Some(l) => {
                    next_line += 1;
                    cell(l)
                }
                None => "draft one: the script ran out".to_string(),
            },
            TextSource::Transcripts => "from the clip's speech".to_string(),
            TextSource::Draft | TextSource::Off => "draft".to_string(),
        };
        let _ = writeln!(
            d,
            "| {} | f{} | f{} | {} | {style} | {text} | {} | ({}, {}) | ({}, {}) | {} |",
            c.id,
            c.range.start.value,
            c.range.end_frame(),
            kind(c),
            cell(&c.text),
            norm(c.bounds.cx),
            norm(c.bounds.cy),
            (c.bounds.cx * f64::from(r.probe.width)).round(),
            (c.bounds.cy * f64::from(r.probe.height)).round(),
            animation(c.animation.as_ref())
        );
    }
    if let TextSource::Script(lines) = source {
        if next_line < lines.len() {
            let rest: Vec<&str> = lines[next_line..].iter().map(String::as_str).collect();
            let _ = writeln!(
                d,
                "\nScript lines with no caption slot: {}. Ask the user whether to add captions or drop them.",
                rest.join(" / ")
            );
        }
    }
    let _ = writeln!(d);
}

fn sound(d: &mut String, x: &PromptInputs, clips: &[&model::Asset]) {
    let a = &x.run.audio;
    let _ = writeln!(d, "## 5. Sound");
    let speech = clips.iter().any(|c| !c.words.is_empty());
    let _ = writeln!(
        d,
        "- clip audio: {}",
        match (speech, x.voiceover.is_some()) {
            (_, true) => "keep the clips' own sound low (about -18 dB) under the voiceover",
            (true, false) => "keep the speech of the clips you place",
            (false, false) => "the clips have no speech",
        }
    );
    if let Some(v) = x.voiceover {
        let _ = writeln!(
            d,
            "- voiceover ({} voice {}): place each line on its own audio track at its frame",
            v.engine, v.voice
        );
        for l in &v.lines {
            let _ = writeln!(
                d,
                "  - f{}: {} ({:.2}s{})",
                l.start,
                l.file,
                l.seconds,
                if l.fits {
                    String::new()
                } else {
                    format!(", runs {} frames past the next caption: shorten the line or let it overlap", (l.seconds * x.run.probe.rate.as_f64()).round() as i64 - l.room)
                }
            );
        }
    }
    if a.music.is_empty() {
        let _ = writeln!(d, "- music: none in the original");
    } else {
        for m in music_runs(&a.music) {
            let _ = writeln!(
                d,
                "- music from f{} to f{}{}{}",
                m.from,
                m.to,
                m.level_db
                    .map_or(String::new(), |l| format!(" at about {} dB", num(l))),
                if m.beat { ", with a steady beat" } else { "" }
            );
        }
        let with_music: Vec<&str> = clips
            .iter()
            .filter(|c| !c.music.is_empty())
            .map(|c| c.file.as_str())
            .collect();
        let _ = writeln!(
            d,
            "  - clips with music in them: {}",
            if with_music.is_empty() {
                "none; ask the user for a track".to_string()
            } else {
                with_music.join(", ")
            }
        );
    }
    if a.sounds.is_empty() {
        let _ = writeln!(d, "- sound effects: none named in the original");
    } else {
        for s in &a.sounds {
            let found: Vec<String> = clips
                .iter()
                .flat_map(|c| {
                    c.sounds
                        .iter()
                        .filter(|cs| cs.label == s.label)
                        .map(move |cs| format!("{} at f{}", c.file, cs.start.value))
                })
                .collect();
            let _ = writeln!(
                d,
                "- [{}] at f{}: {}",
                s.label,
                s.start.value,
                if found.is_empty() {
                    "no clip has this sound; use a library effect or skip it".to_string()
                } else {
                    format!("found in {}", found.join(", "))
                }
            );
        }
    }
    let _ = writeln!(d);
}

/// Music beds closer than this are one bed with quiet moments, for an
/// editor placing a track.
const MUSIC_GAP_FRAMES: i64 = 60;

struct MusicRun {
    from: i64,
    to: i64,
    level_db: Option<f64>,
    beat: bool,
}

/// Beds joined across short gaps, with the loudest level and any beat.
fn music_runs(music: &[model::MusicSpan]) -> Vec<MusicRun> {
    let mut runs: Vec<MusicRun> = Vec::new();
    for m in music {
        let (from, to) = (m.range.start.value, m.range.end_frame());
        match runs.last_mut() {
            Some(r) if from - r.to <= MUSIC_GAP_FRAMES => {
                r.to = r.to.max(to);
                r.level_db = match (r.level_db, m.level_db) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                r.beat |= m.beat;
            }
            _ => runs.push(MusicRun {
                from,
                to,
                level_db: m.level_db,
                beat: m.beat,
            }),
        }
    }
    runs
}

fn your_clips(d: &mut String, x: &PromptInputs) {
    let _ = writeln!(d, "## Your clips");
    let _ = writeln!(d, "From {}.\n", x.assets.folder);
    for c in &x.assets.clips {
        if let Some(original) = &c.duplicate_of {
            let _ = writeln!(d, "### {}\nsame file as {original}; skip it\n", c.file);
            continue;
        }
        let _ = writeln!(d, "### {}", c.file);
        let _ = writeln!(
            d,
            "{:.3}s · {} x {} · {} fps · thumbnail {}",
            c.seconds,
            c.width,
            c.height,
            num(c.rate.as_f64()),
            c.thumbnail
        );
        let speech = if c.words.is_empty() {
            "none".to_string()
        } else {
            format!("{} ({})", c.transcript, c.language)
        };
        let mut sounds: Vec<&str> = Vec::new();
        for s in &c.sounds {
            if !sounds.contains(&s.label.as_str()) {
                sounds.push(&s.label);
            }
        }
        let _ = writeln!(d, "speech: {speech}");
        if !sounds.is_empty() {
            let _ = writeln!(d, "sounds: {}", sounds.join(", "));
        }
        let _ = writeln!(d);
    }
}

/// Reads a finished run and its assets and writes REBUILD-PROMPT.md. A
/// voiceover made by `viralcraft voiceover` is used when one exists; its
/// lines are the script unless `script` is given, which must match them.
pub fn run(run_dir: &Path, script: Option<&Path>, captions: bool) -> Result<PathBuf, PromptError> {
    let template_path = run_dir.join(template::TEMPLATE_FILE);
    let template = fs::read_to_string(&template_path)
        .map_err(|_| PromptError::NoTemplate(template_path.clone()))?;
    if template.contains("TODO:") {
        return Err(PromptError::TemplateTodo(template_path));
    }
    let dir = run_dir.join(REBUILD_DIR);
    let assets_path = dir.join(ASSETS_FILE);
    let assets_json =
        fs::read_to_string(&assets_path).map_err(|_| PromptError::NoAssets(assets_path.clone()))?;
    let assets: Assets =
        serde_json::from_str(&assets_json).map_err(|source| PromptError::Json {
            path: assets_path,
            source,
        })?;
    let mut lines = match script {
        Some(p) => Some(script_lines(&fs::read_to_string(p).map_err(|source| {
            PromptError::Io {
                path: p.to_path_buf(),
                source,
            }
        })?)),
        None => None,
    };
    let voice_path = dir.join(crate::voiceover::VOICEOVER_FILE);
    let voiceover: Option<model::Voiceover> = match fs::read_to_string(&voice_path) {
        Ok(text) => Some(
            serde_json::from_str(&text).map_err(|source| PromptError::Json {
                path: voice_path.clone(),
                source,
            })?,
        ),
        Err(_) => None,
    };
    if let Some(v) = &voiceover {
        let spoken: Vec<String> = v.lines.iter().map(|l| l.text.clone()).collect();
        match &lines {
            Some(l) if *l != spoken => return Err(PromptError::VoiceoverMismatch(voice_path)),
            Some(_) => {}
            None => lines = Some(spoken),
        }
    }
    let inputs = editspec::load(run_dir)?;
    let doc = render(&PromptInputs {
        run: &inputs,
        template: &template,
        assets: &assets,
        script: lines.as_deref(),
        voiceover: voiceover.as_ref(),
        captions,
    });
    let out = dir.join(PROMPT_FILE);
    fs::write(&out, doc).map_err(|source| PromptError::Io {
        path: out.clone(),
        source,
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(name: &str) -> String {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data"));
        fs::read_to_string(dir.join(name)).unwrap()
    }

    fn run_inputs() -> Inputs {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../editspec/tests/data/cuts-3"
        ));
        let read = |name: &str| fs::read_to_string(dir.join(name)).unwrap();
        Inputs {
            probe: serde_json::from_str(&read("probe.json")).unwrap(),
            shots: serde_json::from_str(&read("shots.json")).unwrap(),
            audio: serde_json::from_str(&read("audio.json")).unwrap(),
            inspect: serde_json::from_str(&read("inspect.json")).unwrap(),
            judgement: Some(serde_json::from_str(&read("judgement.json")).unwrap()),
        }
    }

    fn prompt(assets: &Assets, script: Option<&[String]>) -> String {
        render(&PromptInputs {
            run: &run_inputs(),
            template: &data("TEMPLATE.md"),
            assets,
            script,
            voiceover: None,
            captions: true,
        })
    }

    fn assets() -> Assets {
        serde_json::from_str(&data("assets.json")).unwrap()
    }

    fn caption_rows(doc: &str) -> Vec<String> {
        let section = &doc[doc.find("## 3. Captions").unwrap()..doc.find("## 4.").unwrap()];
        section
            .lines()
            .filter(|l| l.starts_with("| c"))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn slots_and_rules_are_read_from_the_finished_template() {
        let t = data("TEMPLATE.md");
        let slots = template::slots(&t);
        assert_eq!(slots.len(), 3);
        assert_eq!(
            slots[1],
            template::Slot {
                name: "BUILD".into(),
                from: 60,
                to: 120,
                fill_with: "the second item, same framing".into()
            }
        );
        assert!(template::rules(&t).starts_with("- Cut on the beat"));
        assert!(template::rules(&t).ends_with("no caption."));
    }

    #[test]
    fn script_lines_fill_the_captions_in_order() {
        let script = script_lines("FIRST\n\n  SECOND  \nTHIRD\n");
        let doc = prompt(&assets(), Some(&script));
        let rows = caption_rows(&doc);
        assert_eq!(rows.len(), 2, "c3 is scene text and is not rebuilt");
        assert!(rows[0].contains("| FIRST |"), "{}", rows[0]);
        assert!(rows[1].contains("| SECOND |"), "{}", rows[1]);
        assert!(doc.contains("Script lines with no caption slot: THIRD"));
    }

    #[test]
    fn without_a_script_text_comes_from_the_clips_speech() {
        let doc = prompt(&assets(), None);
        assert!(doc.contains("Write each caption from what is said"));
        assert!(caption_rows(&doc)[0].contains("from the clip's speech"));
        assert!(doc.contains("speech: first we pour [tick] (en)"));
    }

    #[test]
    fn duplicates_are_skipped_and_short_clips_are_not_offered() {
        let doc = prompt(&assets(), None);
        assert!(doc.contains("same file as shelf.mp4; skip it"));
        let hook = doc.lines().find(|l| l.starts_with("| HOOK |")).unwrap();
        assert!(hook.ends_with("| pour.mp4 |"), "{hook}");
    }

    #[test]
    fn sounds_are_matched_to_clips_that_contain_them() {
        let doc = prompt(&assets(), None);
        assert!(
            doc.contains("- [tick] at f14: found in pour.mp4 at f40"),
            "{doc}"
        );
        assert!(doc.contains("clips with music in them: shelf.mp4"));
    }

    #[test]
    fn a_bed_with_short_gaps_is_one_music_line() {
        let r = model::Fps::new(30, 1).unwrap();
        let bed = |from: i64, to: i64, db: f64| model::MusicSpan {
            range: model::TimeRange::from_frames(from, to - from, r),
            level_db: Some(db),
            beat: false,
        };
        // The Thai clip's quiet bed came out as 19 pieces.
        let runs = music_runs(&[
            bed(180, 330, -38.4),
            bed(345, 420, -38.7),
            bed(3000, 3090, -39.6),
        ]);
        let got: Vec<(i64, i64, Option<f64>)> =
            runs.iter().map(|m| (m.from, m.to, m.level_db)).collect();
        assert_eq!(got, [(180, 420, Some(-38.4)), (3000, 3090, Some(-39.6))]);
    }

    #[test]
    fn a_clip_without_words_has_no_speech_and_lists_its_sounds_once() {
        let mut silent = assets();
        let pour = &mut silent.clips[0];
        pour.words.clear();
        pour.transcript = "[tick] [tick]".into();
        let tick = pour.sounds[0].clone();
        pour.sounds.push(tick);
        let doc = prompt(&silent, None);
        assert!(doc.contains("### pour.mp4\n4.500s · 720 x 1280 · 30 fps · thumbnail rebuild/thumbs/pour.jpg\nspeech: none\nsounds: tick\n"), "{doc}");
    }

    fn voiceover() -> model::Voiceover {
        let line =
            |caption: &str, text: &str, start: i64, seconds: f64, room: i64| model::VoiceLine {
                caption: caption.into(),
                text: text.into(),
                file: format!("rebuild/voiceover/{caption}.wav"),
                start,
                seconds,
                room,
                fits: seconds * 30.0 <= room as f64,
            };
        model::Voiceover {
            version: 1,
            engine: "say".into(),
            voice: "Samantha".into(),
            model: String::new(),
            lines: vec![
                line("c1", "First we pour", 0, 1.2, 60),
                line("c2", "then we whisk it hard for a long time", 60, 2.5, 60),
            ],
        }
    }

    #[test]
    fn a_voiceover_is_placed_line_by_line_and_its_text_captions_it() {
        let v = voiceover();
        let script: Vec<String> = v.lines.iter().map(|l| l.text.clone()).collect();
        let doc = render(&PromptInputs {
            run: &run_inputs(),
            template: &data("TEMPLATE.md"),
            assets: &assets(),
            script: Some(&script),
            voiceover: Some(&v),
            captions: true,
        });
        assert!(
            doc.contains("- voiceover (say voice Samantha): place each line"),
            "{doc}"
        );
        assert!(
            doc.contains("  - f0: rebuild/voiceover/c1.wav (1.20s)\n"),
            "{doc}"
        );
        assert!(
            doc.contains(
                "  - f60: rebuild/voiceover/c2.wav (2.50s, runs 15 frames past the next caption"
            ),
            "{doc}"
        );
        assert!(caption_rows(&doc)[0].contains("| First we pour |"));
        assert!(doc.contains("about -18 dB) under the voiceover"));
        assert!(doc.contains("labelled as having an AI voice"));
    }

    #[test]
    fn without_captions_the_caption_step_is_empty() {
        let doc = render(&PromptInputs {
            run: &run_inputs(),
            template: &data("TEMPLATE.md"),
            assets: &assets(),
            script: None,
            voiceover: None,
            captions: false,
        });
        assert!(
            doc.contains("None: the user chose a rebuild without captions."),
            "{doc}"
        );
        assert!(caption_rows(&doc).is_empty());
        assert!(!doc.contains("every caption above"));
    }

    #[test]
    fn with_no_script_and_no_speech_the_user_chooses_voiceover_or_none() {
        let mut silent = assets();
        for c in &mut silent.clips {
            c.words.clear();
        }
        let doc = prompt(&silent, None);
        assert!(doc.contains("an AI voiceover"), "{doc}");
        assert!(doc.contains(
            "viralcraft voiceover <run_dir> --script <file> --engine <say|openai|elevenlabs|minimax|qwen>"
        ));
        assert!(doc.contains("no captions: make this prompt again with `--no-captions`"));
    }

    #[test]
    fn a_voiceover_from_another_script_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let cuts3 = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../editspec/tests/data/cuts-3"
        ));
        for f in [
            "probe.json",
            "shots.json",
            "audio.json",
            "inspect.json",
            "judgement.json",
        ] {
            fs::copy(cuts3.join(f), dir.path().join(f)).unwrap();
        }
        fs::write(dir.path().join("TEMPLATE.md"), data("TEMPLATE.md")).unwrap();
        let rb = dir.path().join("rebuild");
        fs::create_dir(&rb).unwrap();
        fs::write(rb.join("assets.json"), data("assets.json")).unwrap();
        fs::write(
            rb.join("voiceover.json"),
            serde_json::to_string(&voiceover()).unwrap(),
        )
        .unwrap();
        let script = dir.path().join("s.txt");
        fs::write(&script, "Something else\nentirely\n").unwrap();
        let err = run(dir.path(), Some(&script), true).unwrap_err();
        assert!(matches!(err, PromptError::VoiceoverMismatch(_)), "{err}");
        let ok = run(dir.path(), None, true).unwrap();
        let doc = fs::read_to_string(ok).unwrap();
        assert!(caption_rows(&doc)[1].contains("| then we whisk it hard for a long time |"));
    }

    #[test]
    fn nothing_names_an_editor() {
        let doc = prompt(&assets(), None).to_lowercase();
        for editor in ["filmcraft", "premiere", "capcut", "resolve", "final cut"] {
            assert!(!doc.contains(editor), "{editor}");
        }
    }

    #[test]
    fn refuses_while_the_template_has_todo() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("TEMPLATE.md"), "fill with | TODO: x").unwrap();
        let err = run(dir.path(), None, true).unwrap_err();
        assert!(matches!(err, PromptError::TemplateTodo(_)), "{err}");
    }

    #[test]
    fn refuses_without_assets() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("TEMPLATE.md"), data("TEMPLATE.md")).unwrap();
        let err = run(dir.path(), None, true).unwrap_err();
        assert!(matches!(err, PromptError::NoAssets(_)), "{err}");
        assert!(err.to_string().contains("viralcraft assets"));
    }

    #[test]
    fn cuts_3_prompt_with_script_snapshot() {
        let script = script_lines("FIRST\nSECOND\n");
        insta::assert_snapshot!(prompt(&assets(), Some(&script)));
    }

    #[test]
    fn cuts_3_prompt_without_script_snapshot() {
        insta::assert_snapshot!(prompt(&assets(), None));
    }
}
