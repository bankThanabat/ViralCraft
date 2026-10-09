//! The whole document, section by section.

use std::fmt::Write;

use model::{Audio, CaptionEvent, Inspect, Probe, RationalTime, Shots, TransitionKind};

use crate::fmt::{cell, norm, num, seconds, time, transform_summary};
use crate::sync::{landing, PictureEvent};

/// Everything the edit spec is written from: the four measurement files.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub probe: Probe,
    pub shots: Shots,
    pub audio: Audio,
    pub inspect: Inspect,
}

const JUDGE: &str = "<!-- judgement -->";

pub fn render(x: &Inputs) -> String {
    let mut d = String::new();
    let name = std::path::Path::new(&x.probe.original)
        .file_name()
        .map_or(x.probe.original.clone(), |n| {
            n.to_string_lossy().into_owned()
        });
    let _ = writeln!(d, "# Edit spec: {name}\n");
    source(&mut d, x);
    hook(&mut d);
    shots(&mut d, x);
    captions(&mut d, x);
    overlays(&mut d, x);
    transitions(&mut d, x);
    sound(&mut d, x);
    look(&mut d, x);
    let _ = writeln!(d, "## Story beats  {JUDGE}\nTODO: numbered beats with frame ranges, e.g. `1. f0–f54 hook: claim`\n");
    let _ = writeln!(d, "## A-roll / B-roll map  {JUDGE}\nTODO: which shots are A-roll (the main subject) and which B-roll (cutaways)\n");
    checklist(&mut d, x);
    d
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

fn hook(d: &mut String) {
    let _ = writeln!(d, "## Hook  {JUDGE}");
    let _ = writeln!(
        d,
        "type: TODO: text-overlay | talking-head | pattern-interrupt | question | ..."
    );
    let _ = writeln!(d, "window: TODO: the opening frames that make a viewer stop, e.g. 0.000s (f0) to 1.800s (f54)");
    let _ = writeln!(d, "why it works: TODO: one or two sentences\n");
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
        let _ = writeln!(
            d,
            "| {} | f{} | f{} | {} | TODO | {} | {} |",
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
        let _ = writeln!(
            d,
            "### Shot {}  ({} to {})",
            seg.n,
            time(at(x, s)),
            time(at(x, e))
        );
        let _ = writeln!(d, "on screen: TODO: what the viewer sees  {JUDGE}");
        let _ = writeln!(
            d,
            "role: TODO: A-roll | B-roll | insert | title card  {JUDGE}"
        );
        let _ = writeln!(d, "speech: TODO: on-camera | voiceover | none  {JUDGE}");
        let camera = match motion {
            Some(m) => format!(
                "{} · confidence {}",
                transform_summary(m.transform.as_ref()),
                norm(m.confidence)
            ),
            None => "unknown".to_string(),
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

/// Colours closer than this (Euclidean RGB) are the same style colour.
const SAME_COLOUR: f64 = 32.0;

fn rgb(hex: &str) -> Option<[f64; 3]> {
    let h = hex.strip_prefix('#')?;
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(f64::from);
    Some([c(0)?, c(2)?, c(4)?])
}

fn same_colour(a: Option<&str>, b: Option<&str>) -> bool {
    match (a.and_then(rgb), b.and_then(rgb)) {
        (Some(x), Some(y)) => {
            x.iter()
                .zip(y)
                .map(|(p, q)| (p - q).powi(2))
                .sum::<f64>()
                .sqrt()
                < SAME_COLOUR
        }
        (None, None) => true,
        _ => false,
    }
}

/// Captions that look alike share a style: same fill, outline and box
/// colours within a tolerance. Size and case are not part of it: OCR line
/// height varies with the letters in a word (34 to 58 px for one font on
/// the first real clip), and creators capitalise single words for
/// emphasis. Position is per caption.
pub(crate) fn caption_styles(caps: &[CaptionEvent]) -> Vec<(String, Vec<usize>)> {
    let mut styles: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, c) in caps.iter().enumerate() {
        let found = styles.iter_mut().find(|(_, members)| {
            let first = &caps[members[0]];
            same_colour(Some(&first.fill), Some(&c.fill))
                && same_colour(first.outline.as_deref(), c.outline.as_deref())
                && same_colour(first.box_color.as_deref(), c.box_color.as_deref())
        });
        match found {
            Some((_, members)) => members.push(i),
            None => styles.push((format!("C{}", styles.len() + 1), vec![i])),
        }
    }
    styles
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

fn captions(d: &mut String, x: &Inputs) {
    let caps = &x.inspect.captions;
    let _ = writeln!(d, "## Captions");
    if caps.is_empty() {
        let _ = writeln!(d, "none detected\n");
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
        let _ = writeln!(
            d,
            "style {id}: font unknown (TODO: substitute) · {size} · fill {} · outline {} · box {} · {case}",
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
        let _ = writeln!(
            d,
            "| {} | {} | f{} | f{} | {} | {} ({}, {}) | {}×{} | {} | {} | TODO |",
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
        let _ = writeln!(
            d,
            "| {} | TODO | f{} | f{} | {} ({}, {}) | {}×{} | {} |",
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
    let _ = writeln!(d, "named: TODO: e.g. teal-orange, lifted blacks  {JUDGE}");
    let _ = writeln!(
        d,
        "match by: TODO: how to grade new footage to this look  {JUDGE}\n"
    );
}

fn checklist(d: &mut String, x: &Inputs) {
    let _ = writeln!(d, "## Judgement checklist");
    let _ = writeln!(d, "- [ ] hook");
    for s in &x.shots.segments {
        let _ = writeln!(d, "- [ ] shot {}: on screen, role, speech", s.n);
    }
    for c in &x.inspect.captions {
        let _ = writeln!(
            d,
            "- [ ] caption {} kind (caption | title | watermark | scene-text) and font substitute",
            c.id
        );
    }
    for o in &x.inspect.overlays {
        let _ = writeln!(d, "- [ ] overlay {} kind", o.id);
    }
    let _ = writeln!(d, "- [ ] look name and match-by notes");
    let _ = writeln!(d, "- [ ] story beats");
    let _ = writeln!(d, "- [ ] A-roll / B-roll map");
    let _ = writeln!(d, "- [ ] format name");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn cuts3() -> Inputs {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/cuts-3"));
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
        Inputs {
            probe: serde_json::from_str(&read("probe.json")).unwrap(),
            shots: serde_json::from_str(&read("shots.json")).unwrap(),
            audio: serde_json::from_str(&read("audio.json")).unwrap(),
            inspect: serde_json::from_str(&read("inspect.json")).unwrap(),
        }
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
