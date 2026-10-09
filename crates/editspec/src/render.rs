//! The whole document, section by section. Measured values come from the
//! four measurement files; judgement slots come from `judgement.json` when
//! it exists and are `TODO:` until then.

use std::fmt::Write;

use model::inspect::caption_styles;
use model::{
    Audio, CaptionEvent, Inspect, Judgement, Needs, Probe, RationalTime, Shots, TransitionKind,
};

use crate::fmt::{cell, norm, num, seconds, time, transform_summary};
use crate::sync::{landing, PictureEvent};

/// Everything the edit spec is written from.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub probe: Probe,
    pub shots: Shots,
    pub audio: Audio,
    pub inspect: Inspect,
    /// The agent's `judgement.json`, once written.
    pub judgement: Option<Judgement>,
}

const JUDGE: &str = "<!-- judgement -->";

/// A judgement value, or a TODO hint while it is missing.
fn or_todo(value: Option<&str>, hint: &str) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => v.to_string(),
        None => format!("TODO: {hint}"),
    }
}

pub fn render(x: &Inputs) -> String {
    // Apply the agent's caption fixes and drop scene text and false
    // overlays before anything is written.
    let mut applied = x.clone();
    let mut scene_text = Vec::new();
    if let Some(j) = &x.judgement {
        let before = applied.inspect.captions.clone();
        let removed = j.apply_to_captions(&mut applied.inspect.captions);
        scene_text = before
            .into_iter()
            .filter(|c| removed.contains(&c.id))
            .collect();
        applied.inspect.overlays.retain(|o| j.keeps_overlay(&o.id));
    }
    let x = &applied;

    let mut d = String::new();
    let name = std::path::Path::new(&x.probe.original)
        .file_name()
        .map_or(x.probe.original.clone(), |n| {
            n.to_string_lossy().into_owned()
        });
    let _ = writeln!(d, "# Edit spec: {name}\n");
    let format = or_todo(
        judgement(x).and_then(|j| j.format_name.as_deref()),
        "a short name for this format",
    );
    let _ = writeln!(d, "format: {format}  {JUDGE}\n");
    source(&mut d, x);
    hook(&mut d, x);
    shots(&mut d, x);
    captions(&mut d, x, &scene_text);
    overlays(&mut d, x);
    transitions(&mut d, x);
    sound(&mut d, x);
    look(&mut d, x);
    story_beats(&mut d, x);
    ab_map(&mut d, x);
    checklist(&mut d, x);
    d
}

fn judgement(x: &Inputs) -> Option<&Judgement> {
    x.judgement.as_ref()
}

fn at(x: &Inputs, frame: i64) -> RationalTime {
    RationalTime::from_frames(frame, x.probe.rate)
}

fn source(d: &mut String, x: &Inputs) {
    let p = &x.probe;
    let _ = writeln!(d, "## Source");
    let _ = writeln!(
        d,
        "duration: {} · {} fps · {}x{} ({})",
        time(at(x, p.frames)),
        p.rate,
        p.width,
        p.height,
        p.aspect
    );
    let audio = match &p.audio {
        Some(a) => format!(
            "{} kHz, {} ch, starts {} frames after video",
            num(f64::from(a.sample_rate) / 1000.0),
            a.channels,
            a.offset.value
        ),
        None => "none".to_string(),
    };
    let _ = writeln!(d, "audio: {audio}");
    let z = &p.safe_zone;
    let _ = writeln!(
        d,
        "safe zone: {} (UI bands: top {}, right {}, bottom {}, left {} of the frame)\n",
        z.profile,
        norm(z.top),
        norm(z.right),
        norm(z.bottom),
        norm(z.left)
    );
}

fn hook(d: &mut String, x: &Inputs) {
    let h = judgement(x).and_then(|j| j.hook.as_ref());
    let _ = writeln!(d, "## Hook  {JUDGE}");
    let _ = writeln!(
        d,
        "type: {}",
        or_todo(
            h.map(|h| h.kind.as_str()),
            "text-overlay | talking-head | pattern-interrupt | question | ..."
        )
    );
    let window = h.map(|h| {
        format!(
            "{} to {}",
            time(at(x, h.window.start)),
            time(at(x, h.window.start + h.window.duration))
        )
    });
    let _ = writeln!(
        d,
        "window: {}",
        or_todo(
            window.as_deref(),
            "the opening frames that make a viewer stop, e.g. 0.000s (f0) to 1.800s (f54)"
        )
    );
    let _ = writeln!(
        d,
        "why it works: {}\n",
        or_todo(h.map(|h| h.why.as_str()), "one or two sentences")
    );
}

fn transition_into(x: &Inputs, frame: i64) -> String {
    if frame == 0 {
        return "start".to_string();
    }
    for t in &x.shots.transitions {
        let (s, e) = (t.range.start.value, t.range.end_frame());
        let near = frame >= s - 1 && frame <= e + 1;
        match t.kind {
            TransitionKind::Dissolve if near => {
                return format!("dissolve {}f", t.range.duration.value)
            }
            TransitionKind::Flash if near => return "flash".to_string(),
            _ => {}
        }
    }
    "cut".to_string()
}

fn shots(d: &mut String, x: &Inputs) {
    let _ = writeln!(d, "## Shots");
    let _ = writeln!(d, "| # | in | out | dur | role | camera | transition in |");
    let _ = writeln!(d, "|---|----|-----|-----|------|--------|---------------|");
    for seg in &x.shots.segments {
        let motion = x.inspect.motion.iter().find(|m| m.shot == seg.n);
        let role = judgement(x)
            .and_then(|j| j.shot(seg.n))
            .and_then(|s| s.role.clone())
            .unwrap_or_else(|| "TODO".to_string());
        let _ = writeln!(
            d,
            "| {} | f{} | f{} | {} | {role} | {} | {} |",
            seg.n,
            seg.range.start.value,
            seg.range.end_frame(),
            seconds(seg.range.duration),
            transform_summary(motion.and_then(|m| m.transform.as_ref())),
            transition_into(x, seg.range.start.value)
        );
    }
    let _ = writeln!(d);
    let last = x.shots.segments.len();
    for seg in &x.shots.segments {
        let (s, e) = (seg.range.start.value, seg.range.end_frame());
        let motion = x.inspect.motion.iter().find(|m| m.shot == seg.n);
        let note = judgement(x).and_then(|j| j.shot(seg.n));
        let _ = writeln!(
            d,
            "### Shot {}  ({} to {})",
            seg.n,
            time(at(x, s)),
            time(at(x, e))
        );
        let _ = writeln!(
            d,
            "on screen: {}  {JUDGE}",
            or_todo(
                note.and_then(|n| n.on_screen.as_deref()),
                "what the viewer sees"
            )
        );
        let _ = writeln!(
            d,
            "role: {}  {JUDGE}",
            or_todo(
                note.and_then(|n| n.role.as_deref()),
                "a-roll | b-roll | insert | title card"
            )
        );
        let _ = writeln!(
            d,
            "speech: {}  {JUDGE}",
            or_todo(
                note.and_then(|n| n.speech.as_deref()),
                "on-camera | voiceover | none"
            )
        );
        let path = format!("shots[{}].camera", seg.n);
        let over = judgement(x).and_then(|j| j.overrides.iter().find(|o| o.path == path));
        let camera = match (over, motion) {
            (Some(o), _) => format!("{}  <!-- override: {} -->", o.value, o.reason),
            (None, Some(m)) => format!(
                "{} · confidence {}",
                transform_summary(m.transform.as_ref()),
                norm(m.confidence)
            ),
            (None, None) => "unknown".to_string(),
        };
        let _ = writeln!(d, "camera: {camera}");
        let _ = writeln!(d, "framing: unknown");
        let _ = writeln!(d, "speed: unknown (v1 does not measure retiming)");
        if let Some(l) = x.inspect.shot_looks.iter().find(|l| l.shot == seg.n) {
            let colours: Vec<&str> = l.look.dominant.iter().map(|s| s.hex.as_str()).collect();
            let _ = writeln!(
                d,
                "look: luma mean {} · dominant {}",
                norm(l.look.luma.mean),
                colours.join(" ")
            );
        }
        let out = if seg.n as usize == last {
            "end of video".to_string()
        } else {
            format!("{} at {}", transition_into(x, e), time(at(x, e)))
        };
        let _ = writeln!(d, "transition out: {out}\n");
    }
}

fn animation(c: &CaptionEvent) -> String {
    match &c.animation {
        None => "none".to_string(),
        Some(model::Animation::PopIn { frames, from_scale }) => {
            format!("pop-in {frames}f from {}%", num(*from_scale))
        }
        Some(model::Animation::Slide { frames, dx, dy }) => format!(
            "slide in {frames}f from offset ({}, {})",
            norm(*dx),
            norm(*dy)
        ),
    }
}

fn clearance(c: &model::Clearance) -> String {
    if c.inside {
        return "inside".to_string();
    }
    let sides = [
        ("top", c.top),
        ("right", c.right),
        ("bottom", c.bottom),
        ("left", c.left),
    ];
    let over: Vec<String> = sides
        .iter()
        .filter(|(_, v)| *v < 0.0)
        .map(|(n, v)| format!("{n} {}", norm(*v)))
        .collect();
    format!("outside ({})", over.join(", "))
}

fn captions(d: &mut String, x: &Inputs, scene_text: &[CaptionEvent]) {
    let caps = &x.inspect.captions;
    let note = |id: &str| judgement(x).and_then(|j| j.caption(id));
    let _ = writeln!(d, "## Captions");
    let scene_note = |d: &mut String| {
        if !scene_text.is_empty() {
            let list: Vec<String> = scene_text
                .iter()
                .map(|c| format!("{} {}", c.id, cell(&c.text)))
                .collect();
            let _ = writeln!(d, "scene text, not captions: {}\n", list.join(" · "));
        }
    };
    if caps.is_empty() {
        let _ = writeln!(d, "none detected\n");
        scene_note(d);
        return;
    }
    let styles = caption_styles(caps);
    for (id, members) in &styles {
        let c = &caps[members[0]];
        let mut sizes: Vec<f64> = members.iter().map(|&i| caps[i].font_px_estimate).collect();
        sizes.sort_by(f64::total_cmp);
        let median = sizes[sizes.len() / 2];
        let (lo, hi) = (sizes[0], sizes[sizes.len() - 1]);
        let size = if hi - lo > 0.2 * median {
            format!("~{} px (measured {}–{})", num(median), num(lo), num(hi))
        } else {
            format!("~{} px", num(median))
        };
        let upper = members.iter().filter(|&&i| caps[i].uppercase).count();
        let case = match upper {
            0 => "mixed case".to_string(),
            n if n == members.len() => "uppercase".to_string(),
            n => format!("mixed case, {n} of {} all caps", members.len()),
        };
        let font = members
            .iter()
            .find_map(|&i| note(&caps[i].id).and_then(|n| n.font_substitute.clone()))
            .map_or("font unknown (TODO: substitute)".to_string(), |f| {
                format!("font unknown (substitute: {f})")
            });
        let _ = writeln!(
            d,
            "style {id}: {font} · {size} · fill {} · outline {} · box {} · {case}",
            c.fill,
            c.outline.as_deref().unwrap_or("none"),
            c.box_color.as_deref().unwrap_or("none"),
        );
    }
    let style_of = |i: usize| {
        styles
            .iter()
            .find(|(_, m)| m.contains(&i))
            .map_or("?", |(id, _)| id.as_str())
    };
    let _ = writeln!(
        d,
        "\n| id | style | in | out | text | anchor (x, y) | size | animation | safe zone | kind |"
    );
    let _ = writeln!(
        d,
        "|----|-------|----|-----|------|---------------|------|-----------|-----------|------|"
    );
    for (i, c) in caps.iter().enumerate() {
        let kind = note(&c.id)
            .and_then(|n| n.kind.clone())
            .unwrap_or_else(|| "TODO".to_string());
        let _ = writeln!(
            d,
            "| {} | {} | f{} | f{} | {} | {} ({}, {}) | {}×{} | {} | {} | {kind} |",
            c.id,
            style_of(i),
            c.range.start.value,
            c.range.end_frame(),
            cell(&c.text),
            c.anchor,
            norm(c.bounds.cx),
            norm(c.bounds.cy),
            norm(c.bounds.w),
            norm(c.bounds.h),
            animation(c),
            clearance(&c.safe_zone)
        );
    }
    let _ = writeln!(
        d,
        "\nkind is judgement: caption | title | watermark | scene-text  {JUDGE}\n"
    );
    scene_note(d);
}

fn overlays(d: &mut String, x: &Inputs) {
    let _ = writeln!(d, "## Overlays");
    if x.inspect.overlays.is_empty() {
        let _ = writeln!(d, "none detected\n");
        return;
    }
    let _ = writeln!(
        d,
        "| id | kind | in | out | anchor (x, y) | size | safe zone |"
    );
    let _ = writeln!(
        d,
        "|----|------|----|-----|---------------|------|-----------|"
    );
    for o in &x.inspect.overlays {
        let kind = judgement(x)
            .and_then(|j| j.overlay(&o.id))
            .and_then(|n| n.kind.clone())
            .unwrap_or_else(|| "TODO".to_string());
        let _ = writeln!(
            d,
            "| {} | {kind} | f{} | f{} | {} ({}, {}) | {}×{} | {} |",
            o.id,
            o.range.start.value,
            o.range.end_frame(),
            o.anchor,
            norm(o.bounds.cx),
            norm(o.bounds.cy),
            norm(o.bounds.w),
            norm(o.bounds.h),
            clearance(&o.safe_zone)
        );
    }
    let _ = writeln!(
        d,
        "\nkind is judgement: emoji | sticker | image | not an overlay  {JUDGE}\n"
    );
}

fn transitions(d: &mut String, x: &Inputs) {
    let _ = writeln!(d, "## Transitions");
    if x.shots.transitions.is_empty() {
        let _ = writeln!(d, "none: every shot change is a hard cut\n");
        return;
    }
    let _ = writeln!(d, "| at | type | duration |");
    let _ = writeln!(d, "|----|------|----------|");
    for t in &x.shots.transitions {
        let kind = match t.kind {
            TransitionKind::Dissolve => "dissolve",
            TransitionKind::Flash => "flash",
        };
        let _ = writeln!(
            d,
            "| f{} | {} | {}f |",
            t.range.start.value, kind, t.range.duration.value
        );
    }
    let _ = writeln!(d);
}

fn picture_events(x: &Inputs) -> Vec<PictureEvent> {
    let mut events: Vec<PictureEvent> = x
        .shots
        .segments
        .iter()
        .skip(1)
        .map(|s| PictureEvent {
            kind: format!("cut to shot {}", s.n),
            frame: s.range.start.value,
        })
        .collect();
    events.extend(x.inspect.captions.iter().map(|c| PictureEvent {
        kind: format!("caption {} appears", c.id),
        frame: c.range.start.value,
    }));
    events.extend(x.inspect.overlays.iter().map(|o| PictureEvent {
        kind: format!("overlay {} appears", o.id),
        frame: o.range.start.value,
    }));
    events
}

fn sound(d: &mut String, x: &Inputs) {
    let a = &x.audio;
    let _ = writeln!(d, "## Sound");
    if a.beats.is_empty() {
        let _ = writeln!(
            d,
            "tempo: no steady beat (confidence {})",
            norm(a.tempo.confidence)
        );
    } else {
        let cuts: Vec<i64> = x
            .shots
            .segments
            .iter()
            .skip(1)
            .map(|s| s.range.start.value)
            .collect();
        let on_beat = cuts
            .iter()
            .filter(|c| a.beats.iter().any(|b| (b.time.value - **c).abs() <= 2))
            .count();
        let _ = writeln!(
            d,
            "tempo: {} bpm (confidence {}) · {} beats · cuts on beat: {} of {} within ±2 frames",
            num(a.tempo.bpm),
            norm(a.tempo.confidence),
            a.beats.len(),
            on_beat,
            cuts.len()
        );
    }
    let speech = if a.speech.is_empty() {
        "none".to_string()
    } else {
        let spans: Vec<String> = a
            .speech
            .iter()
            .map(|s| format!("f{}–f{}", s.start.value, s.end_frame()))
            .collect();
        format!("{} words in {}", a.words.len(), spans.join(", "))
    };
    let _ = writeln!(d, "speech: {speech}");
    let _ = writeln!(
        d,
        "gain and fades: unknown (v1 does not measure mix levels per track)\n"
    );

    if a.music.is_empty() {
        let _ = writeln!(d, "music: none detected\n");
    } else {
        let _ = writeln!(d, "| music in | out | bed level | steady beat |");
        let _ = writeln!(d, "|----------|-----|-----------|-------------|");
        for m in &a.music {
            let level = m
                .level_db
                .map_or("unknown".to_string(), |l| format!("{} dBFS", num(l)));
            let _ = writeln!(
                d,
                "| f{} | f{} | {} | {} |",
                m.range.start.value,
                m.range.end_frame(),
                level,
                if m.beat { "yes" } else { "no" }
            );
        }
        let _ = writeln!(d);
    }

    if a.sounds.is_empty() {
        let _ = writeln!(d, "sound effects: none named\n");
    } else {
        let events = picture_events(x);
        let _ = writeln!(d, "| sound | in | out | named by | lands on |");
        let _ = writeln!(d, "|-------|----|-----|----------|----------|");
        for s in &a.sounds {
            let (start, end) = (s.start.value, s.start.value + s.duration.value);
            let on =
                landing(start, end, &events).unwrap_or_else(|| "nothing within 0.5 s".to_string());
            let _ = writeln!(
                d,
                "| {} | f{} | f{} | {} | {} |",
                cell(&s.label),
                start,
                end,
                s.source,
                on
            );
        }
        let _ = writeln!(d);
    }
    let text = if a.transcript.is_empty() {
        "(no speech or named sounds)"
    } else {
        a.transcript.as_str()
    };
    let _ = writeln!(d, "transcript:\n> {text}\n");
}

fn look(d: &mut String, x: &Inputs) {
    let l = &x.inspect.look;
    let _ = writeln!(d, "## Look");
    let colours: Vec<String> = l
        .dominant
        .iter()
        .map(|s| format!("{} ({})", s.hex, norm(s.share)))
        .collect();
    let _ = writeln!(
        d,
        "measured: dominant {} · luma mean {}, min {}, max {} · saturation {} · warmth {:+.2}",
        if colours.is_empty() {
            "unknown".to_string()
        } else {
            colours.join(" ")
        },
        norm(l.luma.mean),
        norm(l.luma.min),
        norm(l.luma.max),
        norm(l.saturation),
        l.warmth
    );
    let note = judgement(x)
        .and_then(|j| j.look.clone())
        .unwrap_or_default();
    let _ = writeln!(
        d,
        "named: {}  {JUDGE}",
        or_todo(note.name.as_deref(), "e.g. teal-orange, lifted blacks")
    );
    let _ = writeln!(
        d,
        "match by: {}  {JUDGE}\n",
        or_todo(
            note.match_by.as_deref(),
            "how to grade new footage to this look"
        )
    );
}

fn story_beats(d: &mut String, x: &Inputs) {
    let _ = writeln!(d, "## Story beats  {JUDGE}");
    let beats = judgement(x)
        .map(|j| j.story_beats.as_slice())
        .unwrap_or_default();
    if beats.is_empty() {
        let _ = writeln!(
            d,
            "TODO: numbered beats with frame ranges, e.g. `1. f0–f54 hook: claim`\n"
        );
        return;
    }
    for (i, b) in beats.iter().enumerate() {
        let _ = writeln!(
            d,
            "{}. f{}–f{} {}",
            i + 1,
            b.start,
            b.start + b.duration,
            b.label
        );
    }
    let _ = writeln!(d);
}

/// Shots grouped by role, once every shot has one.
fn roles(x: &Inputs) -> Option<Vec<(String, Vec<u32>)>> {
    let j = judgement(x)?;
    let mut groups: Vec<(String, Vec<u32>)> = Vec::new();
    for seg in &x.shots.segments {
        let role = j
            .shot(seg.n)?
            .role
            .clone()
            .filter(|r| !r.trim().is_empty())?;
        match groups.iter_mut().find(|(r, _)| *r == role) {
            Some((_, ns)) => ns.push(seg.n),
            None => groups.push((role, vec![seg.n])),
        }
    }
    Some(groups)
}

fn ab_map(d: &mut String, x: &Inputs) {
    let _ = writeln!(d, "## A-roll / B-roll map  {JUDGE}");
    match roles(x) {
        None => {
            let _ = writeln!(
                d,
                "TODO: which shots are A-roll (the main subject) and which B-roll (cutaways)"
            );
        }
        Some(groups) => {
            for (role, ns) in groups {
                let label = match role.as_str() {
                    "a-roll" => "A-roll".to_string(),
                    "b-roll" => "B-roll".to_string(),
                    other => other.to_string(),
                };
                let list: Vec<String> = ns.iter().map(u32::to_string).collect();
                let _ = writeln!(d, "{label}: shots {}", list.join(", "));
            }
        }
    }
    let _ = writeln!(d);
}

fn checklist(d: &mut String, x: &Inputs) {
    let empty = Judgement::default();
    let j = judgement(x).unwrap_or(&empty);
    let needs = Needs {
        shots: x.shots.segments.iter().map(|s| s.n).collect(),
        captions: x.inspect.captions.iter().map(|c| c.id.clone()).collect(),
        overlays: x.inspect.overlays.iter().map(|o| o.id.clone()).collect(),
    };
    let missing = j.missing(&needs);
    let open = |prefix: &str| {
        missing.iter().any(|m| {
            m == prefix
                || m.starts_with(&format!("{prefix}."))
                || m.starts_with(&format!("{prefix} "))
        })
    };
    let tick = |done: bool| if done { "x" } else { " " };
    let _ = writeln!(d, "## Judgement checklist");
    let _ = writeln!(d, "- [{}] hook", tick(!open("hook")));
    for s in &x.shots.segments {
        let _ = writeln!(
            d,
            "- [{}] shot {}: on screen, role, speech",
            tick(!open(&format!("shots[{}]", s.n))),
            s.n
        );
    }
    for c in &x.inspect.captions {
        let _ = writeln!(
            d,
            "- [{}] caption {} kind (caption | title | watermark | scene-text) and font substitute",
            tick(!open(&format!("captions[{}]", c.id))),
            c.id
        );
    }
    for o in &x.inspect.overlays {
        let _ = writeln!(
            d,
            "- [{}] overlay {} kind",
            tick(!open(&format!("overlays[{}]", o.id))),
            o.id
        );
    }
    let _ = writeln!(
        d,
        "- [{}] look name and match-by notes",
        tick(!open("look"))
    );
    let _ = writeln!(d, "- [{}] story beats", tick(!open("story_beats")));
    let _ = writeln!(d, "- [{}] A-roll / B-roll map", tick(roles(x).is_some()));
    let _ = writeln!(d, "- [{}] format name", tick(!open("format_name")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn dir() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/cuts-3"))
    }

    fn cuts3() -> Inputs {
        let read = |name: &str| std::fs::read_to_string(dir().join(name)).unwrap();
        Inputs {
            probe: serde_json::from_str(&read("probe.json")).unwrap(),
            shots: serde_json::from_str(&read("shots.json")).unwrap(),
            audio: serde_json::from_str(&read("audio.json")).unwrap(),
            inspect: serde_json::from_str(&read("inspect.json")).unwrap(),
            judgement: None,
        }
    }

    fn judged() -> Inputs {
        let mut x = cuts3();
        x.judgement = Some(
            serde_json::from_str(&std::fs::read_to_string(dir().join("judgement.json")).unwrap())
                .unwrap(),
        );
        x
    }

    #[test]
    fn a_complete_judgement_leaves_no_todo_and_ticks_every_item() {
        let doc = render(&judged());
        assert!(!doc.contains("TODO"), "{doc}");
        assert!(!doc.contains("- [ ]"));
        assert!(doc.contains("format: counting cards on the beat"));
        assert!(doc.contains("type: text-overlay"));
        assert!(doc.contains("A-roll: shots 1, 2"));
    }

    #[test]
    fn scene_text_leaves_the_caption_table() {
        let doc = render(&judged());
        let captions = &doc[doc.find("## Captions").unwrap()..doc.find("## Overlays").unwrap()];
        assert!(!captions.contains("| c3 |"));
        assert!(captions.contains("scene text, not captions: c3 THREE"));
    }

    #[test]
    fn an_override_shows_with_its_reason() {
        let doc = render(&judged());
        assert!(
            doc.contains(
                "camera: static  <!-- override: flat card, nothing for the tracker to lock on -->"
            ),
            "{doc}"
        );
    }

    #[test]
    fn cuts_3_finalized_snapshot() {
        insta::assert_snapshot!(render(&judged()));
    }

    const SECTIONS: [&str; 11] = [
        "## Source",
        "## Hook",
        "## Shots",
        "## Captions",
        "## Overlays",
        "## Transitions",
        "## Sound",
        "## Look",
        "## Story beats",
        "## A-roll / B-roll map",
        "## Judgement checklist",
    ];

    #[test]
    fn every_section_appears_in_order() {
        let doc = render(&cuts3());
        let mut at = 0;
        for s in SECTIONS {
            let found = doc[at..]
                .find(s)
                .unwrap_or_else(|| panic!("missing or out of order: {s}"));
            at += found + s.len();
        }
    }

    #[test]
    fn no_field_is_left_empty() {
        let doc = render(&cuts3());
        for line in doc.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                if !key.contains('|') && !key.starts_with('#') {
                    assert!(!value.trim().is_empty(), "empty field: {line:?}");
                }
            }
            if line.starts_with('|') {
                for cell in line.trim_matches('|').split('|') {
                    assert!(!cell.trim().is_empty(), "empty cell in {line:?}");
                }
            }
        }
    }

    #[test]
    fn judgement_slots_are_marked_and_listed() {
        let doc = render(&cuts3());
        assert!(doc.contains("## Hook  <!-- judgement -->"));
        assert!(doc.contains("TODO:"));
        let checklist = &doc[doc.find("## Judgement checklist").unwrap()..];
        assert!(checklist.contains("- [ ] hook"));
        assert!(checklist.contains("- [ ] caption c1 kind"));
    }

    #[test]
    fn same_look_captions_share_a_style_despite_small_differences() {
        let x = cuts3();
        let styles = caption_styles(&x.inspect.captions);
        // ONE, TWO, THREE: white with black outline, different positions,
        // outline colours a few shades apart, OCR sizes 100 to 118 px.
        assert_eq!(styles.len(), 1, "{styles:?}");
    }

    #[test]
    fn short_words_and_capitalised_emphasis_stay_in_the_dialogue_style() {
        // Real clip: the same pink dialogue font measured 34 to 58 px
        // because short words have no tall letters, and "LEGALLY" is caps.
        let mut caps = cuts3().inspect.captions;
        for (c, (px, upper)) in caps
            .iter_mut()
            .zip([(58.0, false), (34.0, true), (42.0, false)])
        {
            c.fill = "#E4576B".into();
            c.outline = None;
            c.font_px_estimate = px;
            c.uppercase = upper;
        }
        assert_eq!(caption_styles(&caps).len(), 1);
    }

    #[test]
    fn cuts_3_document_snapshot() {
        insta::assert_snapshot!(render(&cuts3()));
    }
}
