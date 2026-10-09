//! `judgement.json`: what the agent decides after looking at the frames.
//! Every `<!-- judgement -->` slot in the edit spec has a field here.
//! Fields are optional when reading so that `finalize` can list exactly what
//! is still missing instead of failing to parse.

use serde::{Deserialize, Serialize};

pub const CAPTION_KINDS: [&str; 4] = ["caption", "title", "watermark", "scene-text"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Judgement {
    pub version: u32,
    pub hook: Option<Hook>,
    pub shots: Vec<ShotNote>,
    pub captions: Vec<CaptionNote>,
    pub overlays: Vec<OverlayNote>,
    pub look: Option<LookNote>,
    pub story_beats: Vec<StoryBeat>,
    pub format_name: Option<String>,
    pub overrides: Vec<Override>,
}

/// A stretch of frames: start plus duration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: i64,
    pub duration: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Hook {
    #[serde(rename = "type")]
    pub kind: String,
    pub window: Span,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShotNote {
    pub n: u32,
    pub role: Option<String>,
    pub speech: Option<String>,
    pub on_screen: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptionNote {
    pub id: String,
    /// `caption`, `title`, `watermark` or `scene-text`.
    pub kind: Option<String>,
    pub font_substitute: Option<String>,
    /// The correct text when OCR misread it.
    pub text_fix: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayNote {
    pub id: String,
    /// `emoji`, `sticker`, `image` or `not an overlay`.
    pub kind: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LookNote {
    pub name: Option<String>,
    pub match_by: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StoryBeat {
    pub start: i64,
    pub duration: i64,
    pub label: String,
}

/// The agent's correction of a measured value, with its reason.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Override {
    pub path: String,
    pub value: String,
    pub reason: String,
}

/// The ids a judgement must cover, taken from the measurements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Needs {
    pub shots: Vec<u32>,
    pub captions: Vec<String>,
    pub overlays: Vec<String>,
}

fn filled(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| !s.trim().is_empty())
}

impl Judgement {
    pub fn shot(&self, n: u32) -> Option<&ShotNote> {
        self.shots.iter().find(|s| s.n == n)
    }

    pub fn caption(&self, id: &str) -> Option<&CaptionNote> {
        self.captions.iter().find(|c| c.id == id)
    }

    pub fn overlay(&self, id: &str) -> Option<&OverlayNote> {
        self.overlays.iter().find(|o| o.id == id)
    }

    /// Applies text fixes and removes captions judged to be scene text.
    /// Returns the ids that were removed.
    pub fn apply_to_captions(&self, captions: &mut Vec<crate::CaptionEvent>) -> Vec<String> {
        let mut removed = Vec::new();
        captions.retain(|c| {
            let scene = self.caption(&c.id).and_then(|n| n.kind.as_deref()) == Some("scene-text");
            if scene {
                removed.push(c.id.clone());
            }
            !scene
        });
        for c in captions.iter_mut() {
            if let Some(fix) = self.caption(&c.id).and_then(|n| n.text_fix.clone()) {
                c.lines = fix.split('\n').map(str::to_string).collect();
                c.text = fix;
            }
        }
        removed
    }

    /// Whether an overlay stays: everything except `not an overlay`.
    pub fn keeps_overlay(&self, id: &str) -> bool {
        self.overlay(id).and_then(|o| o.kind.as_deref()) != Some("not an overlay")
    }

    /// Required fields that are still empty, as dotted paths.
    pub fn missing(&self, needs: &Needs) -> Vec<String> {
        let mut out = Vec::new();
        if hook_missing(self.hook.as_ref()) {
            out.push("hook".to_string());
        }
        for &n in &needs.shots {
            let note = self.shot(n);
            for (field, value) in [
                ("role", note.and_then(|s| s.role.clone())),
                ("speech", note.and_then(|s| s.speech.clone())),
                ("on_screen", note.and_then(|s| s.on_screen.clone())),
            ] {
                if !filled(&value) {
                    out.push(format!("shots[{n}].{field}"));
                }
            }
        }
        for id in &needs.captions {
            let note = self.caption(id);
            match note.and_then(|c| c.kind.as_deref()) {
                None | Some("") => out.push(format!("captions[{id}].kind")),
                Some(k) if !CAPTION_KINDS.contains(&k) => out.push(format!(
                    "captions[{id}].kind (caption | title | watermark | scene-text)"
                )),
                Some("scene-text") => {}
                Some(_) => {
                    if !filled(&note.and_then(|c| c.font_substitute.clone())) {
                        out.push(format!("captions[{id}].font_substitute"));
                    }
                }
            }
        }
        for id in &needs.overlays {
            if !filled(&self.overlay(id).and_then(|o| o.kind.clone())) {
                out.push(format!("overlays[{id}].kind"));
            }
        }
        let look = self.look.clone().unwrap_or_default();
        if !filled(&look.name) {
            out.push("look.name".to_string());
        }
        if !filled(&look.match_by) {
            out.push("look.match_by".to_string());
        }
        if self.story_beats.is_empty() {
            out.push("story_beats".to_string());
        }
        if !filled(&self.format_name) {
            out.push("format_name".to_string());
        }
        out
    }
}

fn hook_missing(hook: Option<&Hook>) -> bool {
    match hook {
        None => true,
        Some(h) => h.kind.trim().is_empty() || h.why.trim().is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPLETE: &str = r#"{
      "version": 1,
      "hook": { "type": "text-overlay", "window": { "start": 0, "duration": 54 }, "why": "Bold claim in the first second." },
      "shots": [ { "n": 1, "role": "a-roll", "speech": "none", "on_screen": "navy card" } ],
      "captions": [
        { "id": "c1", "kind": "caption", "font_substitute": "Inter Black" },
        { "id": "c2", "kind": "scene-text", "text_fix": "ONE WISH WILLOW" }
      ],
      "overlays": [ { "id": "o1", "kind": "not an overlay" } ],
      "look": { "name": "flat primaries", "match_by": "solid colour cards" },
      "story_beats": [ { "start": 0, "duration": 60, "label": "hook: count" } ],
      "format_name": "counting cards",
      "overrides": [ { "path": "shots[1].camera", "value": "static", "reason": "flat card, nothing to track" } ]
    }"#;

    fn needs() -> Needs {
        Needs {
            shots: vec![1],
            captions: vec!["c1".into(), "c2".into()],
            overlays: vec!["o1".into()],
        }
    }

    #[test]
    fn a_complete_judgement_has_nothing_missing() {
        let j: Judgement = serde_json::from_str(COMPLETE).unwrap();
        assert_eq!(j.missing(&needs()), Vec::<String>::new());
        assert_eq!(j.hook.as_ref().unwrap().kind, "text-overlay");
    }

    #[test]
    fn a_missing_format_name_is_named() {
        let mut j: Judgement = serde_json::from_str(COMPLETE).unwrap();
        j.format_name = None;
        assert_eq!(j.missing(&needs()), ["format_name"]);
    }

    #[test]
    fn an_empty_judgement_lists_every_slot() {
        let j: Judgement = serde_json::from_str("{}").unwrap();
        let missing = j.missing(&needs());
        for want in [
            "hook",
            "shots[1].role",
            "shots[1].speech",
            "shots[1].on_screen",
            "captions[c1].kind",
            "captions[c2].kind",
            "overlays[o1].kind",
            "look.name",
            "look.match_by",
            "story_beats",
            "format_name",
        ] {
            assert!(
                missing.iter().any(|m| m == want),
                "{want} not in {missing:?}"
            );
        }
    }

    #[test]
    fn a_caption_kind_must_be_one_of_the_four() {
        let mut j: Judgement = serde_json::from_str(COMPLETE).unwrap();
        j.captions[0].kind = Some("subtitle".into());
        assert_eq!(
            j.missing(&needs()),
            ["captions[c1].kind (caption | title | watermark | scene-text)"]
        );
    }

    fn caption(id: &str, text: &str) -> crate::CaptionEvent {
        let fps = crate::Fps::new(30, 1).unwrap();
        crate::CaptionEvent {
            id: id.into(),
            range: crate::TimeRange::from_frames(0, 30, fps),
            text: text.into(),
            lines: vec![text.into()],
            bounds: crate::BoxN {
                cx: 0.5,
                cy: 0.5,
                w: 0.3,
                h: 0.04,
            },
            line_boxes: vec![],
            anchor: "an5".into(),
            fill: "#FFFFFF".into(),
            outline: None,
            box_color: None,
            line_height: 0.04,
            font_px_estimate: 60.0,
            uppercase: false,
            animation: None,
            safe_zone: crate::Clearance {
                top: 0.1,
                right: 0.1,
                bottom: 0.1,
                left: 0.1,
                inside: true,
            },
        }
    }

    #[test]
    fn applying_fixes_text_and_drops_scene_text_and_false_overlays() {
        let j: Judgement = serde_json::from_str(COMPLETE).unwrap();
        let mut captions = vec![
            caption("c1", "Me if had a wish"),
            caption("c2", "ONE WISH WILLOW / GAOOW"),
        ];
        let mut j2 = j.clone();
        j2.captions[0].text_fix = Some("Me if I had a wish".into());

        let kept = j2.apply_to_captions(&mut captions);

        assert_eq!(captions.len(), 1, "scene text leaves the caption track");
        assert_eq!(captions[0].text, "Me if I had a wish");
        assert_eq!(captions[0].lines, ["Me if I had a wish"]);
        assert_eq!(kept.len(), 1);
        assert!(j.keeps_overlay("o2"), "unjudged overlays stay");
        assert!(!j.keeps_overlay("o1"), "'not an overlay' is dropped");
    }

    #[test]
    fn a_shown_caption_needs_a_font_substitute_but_scene_text_does_not() {
        let mut j: Judgement = serde_json::from_str(COMPLETE).unwrap();
        j.captions[0].font_substitute = None;
        assert_eq!(j.missing(&needs()), ["captions[c1].font_substitute"]);
    }
}
