//! OpenTimelineIO timeline, field-for-field as OTIO 0.18 writes it. Shots
//! are clips of the source; captions, overlays, music and sounds are
//! generator clips on their own tracks, stacked in lanes where they
//! overlap. Everything ViralCraft knows rides along in
//! `metadata.viralcraft`.

use model::{Audio, Fps, Inspect, Judgement, Probe, Shots, TransitionKind};
use serde_json::{json, Value};

pub fn rt(value: i64, rate: Fps) -> Value {
    json!({"OTIO_SCHEMA": "RationalTime.1", "rate": rate.as_f64(), "value": value as f64})
}

fn tr(start: i64, duration: i64, rate: Fps) -> Value {
    json!({"OTIO_SCHEMA": "TimeRange.1", "start_time": rt(start, rate), "duration": rt(duration, rate)})
}

/// Lane per item so items in one lane never overlap. Items must be sorted
/// by start; each takes the first lane that is free by its start.
pub fn lanes(items: &[(i64, i64)]) -> Vec<usize> {
    let mut ends: Vec<i64> = Vec::new();
    items
        .iter()
        .map(
            |&(start, end)| match ends.iter().position(|&e| e <= start) {
                Some(lane) => {
                    ends[lane] = end;
                    lane
                }
                None => {
                    ends.push(end);
                    ends.len() - 1
                }
            },
        )
        .collect()
}

fn clip(name: &str, source_range: Value, media: Value, meta: Value) -> Value {
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": name,
        "source_range": source_range,
        "media_references": {"DEFAULT_MEDIA": media},
        "active_media_reference_key": "DEFAULT_MEDIA",
        "effects": [],
        "markers": [],
        "enabled": true,
        "color": null,
        "metadata": {"viralcraft": meta},
    })
}

fn gap(duration: i64, rate: Fps) -> Value {
    json!({
        "OTIO_SCHEMA": "Gap.1",
        "name": "",
        "source_range": tr(0, duration, rate),
        "effects": [],
        "markers": [],
        "enabled": true,
        "color": null,
        "metadata": {},
    })
}

fn source_media(frames: i64, rate: Fps) -> Value {
    json!({
        "OTIO_SCHEMA": "ExternalReference.1",
        "name": "source",
        "target_url": "source.cfr.mp4",
        "available_range": tr(0, frames, rate),
        "available_image_bounds": null,
        "metadata": {},
    })
}

fn generator(kind: &str, duration: i64, rate: Fps) -> Value {
    json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "name": "",
        "generator_kind": kind,
        "parameters": {},
        "available_range": tr(0, duration, rate),
        "available_image_bounds": null,
        "metadata": {},
    })
}

fn track(name: String, kind: &str, children: Vec<Value>, markers: Vec<Value>) -> Value {
    json!({
        "OTIO_SCHEMA": "Track.1",
        "name": name,
        "kind": kind,
        "children": children,
        "source_range": null,
        "effects": [],
        "markers": markers,
        "enabled": true,
        "color": null,
        "metadata": {},
    })
}

/// A timed item for a generator track.
struct Item {
    start: i64,
    end: i64,
    name: String,
    kind: &'static str,
    meta: Value,
}

/// Generator clips at their times with gaps between, padded to `total`.
fn lay_out(items: &[&Item], total: i64, rate: Fps) -> Vec<Value> {
    let mut children = Vec::new();
    let mut at = 0;
    for it in items {
        let (start, end) = (it.start.max(at), it.end.min(total));
        if end <= start {
            continue;
        }
        if start > at {
            children.push(gap(start - at, rate));
        }
        // Placeholders are not media: disabled, they show as labelled clips
        // but never render (FilmCraft turns unknown generators into black).
        let mut placeholder = clip(
            &it.name,
            tr(0, end - start, rate),
            generator(it.kind, end - start, rate),
            it.meta.clone(),
        );
        placeholder["enabled"] = json!(false);
        children.push(placeholder);
        at = end;
    }
    if total > at {
        children.push(gap(total - at, rate));
    }
    children
}

/// Tracks for items stacked in non-overlapping lanes. Always at least one.
fn lane_tracks(mut items: Vec<Item>, total: i64, rate: Fps) -> Vec<Vec<Value>> {
    items.sort_by_key(|i| i.start);
    let lane_of = lanes(&items.iter().map(|i| (i.start, i.end)).collect::<Vec<_>>());
    let count = lane_of.iter().max().map_or(1, |m| m + 1);
    (0..count)
        .map(|lane| {
            let members: Vec<&Item> = items
                .iter()
                .zip(&lane_of)
                .filter(|(_, &l)| l == lane)
                .map(|(i, _)| i)
                .collect();
            lay_out(&members, total, rate)
        })
        .collect()
}

/// FilmCraft's sequence settings, in the shape its own OTIO export writes.
/// OTIO has no standard field for picture size; FilmCraft reads this and
/// otherwise assumes 1920x1080. Other editors ignore it.
fn filmcraft_settings(probe: &Probe) -> Value {
    let rate = if probe.rate.den == 1 {
        probe.rate.num.to_string()
    } else {
        format!("{:.2}", probe.rate.as_f64())
    };
    json!({
        "width": probe.width,
        "height": probe.height,
        "frame_rate": {"num": probe.rate.num, "den": probe.rate.den},
        "par": [1, 1],
        "sample_rate": probe.audio.as_ref().map_or(48_000, |a| a.sample_rate),
        "drop_frame": false,
        "preset": format!("Custom {}x{} {rate}", probe.width, probe.height),
        "audio_master": "Stereo",
        "preview_codec": "ProRes 422",
        "max_bit_depth": false,
        "max_render_quality": false,
        "working_space": "Rec. 709",
        "color": {"working": "Rec709", "wide_gamut": false, "auto_tone_map": true},
    })
}

pub fn write(probe: &Probe, shots: &Shots, audio: &Audio, inspect: &Inspect) -> Value {
    write_judged(probe, shots, audio, inspect, None)
}

/// Like [`write`], with the agent's judgement attached to each shot and
/// caption clip under `metadata.viralcraft.judgement`.
pub fn write_judged(
    probe: &Probe,
    shots: &Shots,
    audio: &Audio,
    inspect: &Inspect,
    judgement: Option<&Judgement>,
) -> Value {
    let rate = probe.rate;
    let total = probe.frames;

    // V1: shots as clips of the source, dissolves between them.
    let mut v1 = Vec::new();
    for (i, seg) in shots.segments.iter().enumerate() {
        if i > 0 {
            let cut = seg.range.start.value;
            let dissolve = shots.transitions.iter().find(|t| {
                t.kind == TransitionKind::Dissolve
                    && t.range.start.value <= cut
                    && t.range.end_frame() >= cut
            });
            if let Some(t) = dissolve {
                v1.push(json!({
                    "OTIO_SCHEMA": "Transition.1",
                    "name": "dissolve",
                    "transition_type": "SMPTE_Dissolve",
                    "in_offset": rt(cut - t.range.start.value, rate),
                    "out_offset": rt(t.range.end_frame() - cut, rate),
                    "metadata": {"viralcraft": {"range": t.range}},
                }));
            }
        }
        let motion = inspect.motion.iter().find(|m| m.shot == seg.n);
        let look = inspect
            .shot_looks
            .iter()
            .find(|l| l.shot == seg.n)
            .map(|l| &l.look);
        v1.push(clip(
            &format!("shot {}", seg.n),
            tr(seg.range.start.value, seg.range.duration.value, rate),
            source_media(total, rate),
            json!({"shot": seg.n, "motion": motion, "look": look, "judgement": judgement.and_then(|j| j.shot(seg.n))}),
        ));
    }
    let mut v1_markers: Vec<Value> = audio
        .beats
        .iter()
        .map(|b| {
            json!({
                "OTIO_SCHEMA": "Marker.2",
                "name": "beat",
                "color": "GREEN",
                "comment": "",
                "marked_range": tr(b.time.value, 0, rate),
                "metadata": {"viralcraft": {"confidence": b.confidence}},
            })
        })
        .collect();
    v1_markers.extend(
        shots
            .transitions
            .iter()
            .filter(|t| t.kind == TransitionKind::Flash)
            .map(|t| {
                json!({
                    "OTIO_SCHEMA": "Marker.2",
                    "name": "flash",
                    "color": "WHITE",
                    "comment": "",
                    "marked_range": tr(t.range.start.value, t.range.duration.value, rate),
                    "metadata": {},
                })
            }),
    );

    let captions: Vec<Item> = inspect
        .captions
        .iter()
        .map(|c| Item {
            start: c.range.start.value,
            end: c.range.end_frame(),
            name: c.text.replace('\n', " / "),
            kind: "viralcraft_caption",
            meta: {
                let mut v = serde_json::to_value(c).expect("captions serialize");
                v["judgement"] = serde_json::to_value(judgement.and_then(|j| j.caption(&c.id)))
                    .expect("notes serialize");
                v
            },
        })
        .collect();
    let overlays: Vec<Item> = inspect
        .overlays
        .iter()
        .map(|o| Item {
            start: o.range.start.value,
            end: o.range.end_frame(),
            name: o.id.clone(),
            kind: "viralcraft_overlay",
            meta: serde_json::to_value(o).expect("overlays serialize"),
        })
        .collect();
    let music: Vec<Item> = audio
        .music
        .iter()
        .map(|m| Item {
            start: m.range.start.value,
            end: m.range.end_frame(),
            name: "music".to_string(),
            kind: "viralcraft_music",
            meta: serde_json::to_value(m).expect("music serializes"),
        })
        .collect();
    let sounds: Vec<Item> = audio
        .sounds
        .iter()
        .map(|s| Item {
            start: s.start.value,
            end: s.start.value + s.duration.value.max(1),
            name: s.label.clone(),
            kind: "viralcraft_sound",
            meta: serde_json::to_value(s).expect("sounds serialize"),
        })
        .collect();

    let mut speech = Vec::new();
    let mut at = 0;
    for span in &audio.speech {
        let (start, end) = (span.start.value.max(at), span.end_frame().min(total));
        if end <= start {
            continue;
        }
        if start > at {
            speech.push(gap(start - at, rate));
        }
        speech.push(clip("speech", tr(start, end - start, rate), source_media(total, rate), json!({"words": audio.words.iter().filter(|w| w.start.value >= start && w.start.value < end).count()})));
        at = end;
    }
    if total > at {
        speech.push(gap(total - at, rate));
    }

    let mut tracks = vec![track("V1 shots".into(), "Video", v1, v1_markers)];
    let mut v = 2;
    for (k, lane) in lane_tracks(captions, total, rate).into_iter().enumerate() {
        tracks.push(track(
            format!("V{v} captions {}", k + 1),
            "Video",
            lane,
            vec![],
        ));
        v += 1;
    }
    let overlay_lanes = lane_tracks(overlays, total, rate);
    let single = overlay_lanes.len() == 1;
    for (k, lane) in overlay_lanes.into_iter().enumerate() {
        let name = if single {
            format!("V{v} overlays")
        } else {
            format!("V{v} overlays {}", k + 1)
        };
        tracks.push(track(name, "Video", lane, vec![]));
        v += 1;
    }
    tracks.push(track("A1 speech".into(), "Audio", speech, vec![]));
    let music_lane = lane_tracks(music, total, rate)
        .into_iter()
        .next()
        .unwrap_or_default();
    tracks.push(track("A2 music".into(), "Audio", music_lane, vec![]));
    for (k, lane) in lane_tracks(sounds, total, rate).into_iter().enumerate() {
        tracks.push(track(
            format!("A{} sounds {}", k + 3, k + 1),
            "Audio",
            lane,
            vec![],
        ));
    }

    let name = std::path::Path::new(&probe.original)
        .file_name()
        .map_or(probe.original.clone(), |n| n.to_string_lossy().into_owned());
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": name,
        "global_start_time": null,
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": tracks,
            "source_range": null,
            "effects": [],
            "markers": [],
            "enabled": true,
            "color": null,
            "metadata": {},
        },
        "metadata": {
            "filmcraft": {"settings": filmcraft_settings(probe)},
            "viralcraft": {
            "version": 1,
            "rate": probe.rate,
            "frames": total,
            "size": [probe.width, probe.height],
            "tempo": audio.tempo,
            "transcript": audio.transcript,
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata::cuts3;
    use model::{Fps, Shots, TimeRange, Transition, TransitionKind};

    fn shots3() -> Shots {
        let dir = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../editspec/tests/data/cuts-3"
        ));
        serde_json::from_str(&std::fs::read_to_string(dir.join("shots.json")).unwrap()).unwrap()
    }

    fn timeline() -> serde_json::Value {
        let (probe, audio, inspect) = cuts3();
        write(&probe, &shots3(), &audio, &inspect)
    }

    fn tracks(t: &serde_json::Value) -> Vec<serde_json::Value> {
        t["tracks"]["children"].as_array().unwrap().clone()
    }

    fn length(track: &serde_json::Value) -> f64 {
        track["children"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["OTIO_SCHEMA"] != "Transition.1")
            .map(|c| c["source_range"]["duration"]["value"].as_f64().unwrap())
            .sum()
    }

    #[test]
    fn rational_time_matches_otio_json() {
        let fps = Fps::new(30, 1).unwrap();
        assert_eq!(
            rt(54, fps),
            serde_json::json!({"OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 54.0})
        );
    }

    #[test]
    fn overlapping_items_go_to_separate_lanes() {
        // A title over the whole clip, two dialogue lines, an overlap.
        assert_eq!(
            lanes(&[(0, 100), (10, 20), (20, 30), (25, 40)]),
            [0, 1, 1, 2]
        );
    }

    #[test]
    fn tracks_are_shots_captions_overlays_then_audio() {
        let names: Vec<String> = tracks(&timeline())
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "V1 shots",
                "V2 captions 1",
                "V3 overlays",
                "A1 speech",
                "A2 music",
                "A3 sounds 1"
            ]
        );
        let kinds: Vec<&str> = ["Video", "Video", "Video", "Audio", "Audio", "Audio"].to_vec();
        let got: Vec<String> = tracks(&timeline())
            .iter()
            .map(|t| t["kind"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(got, kinds);
    }

    #[test]
    fn every_track_runs_the_full_length() {
        for t in tracks(&timeline()) {
            assert_eq!(length(&t), 180.0, "{}", t["name"]);
        }
    }

    #[test]
    fn shots_are_clips_of_the_source_and_captions_carry_their_record() {
        let t = timeline();
        let v1 = &tracks(&t)[0];
        let shot2 = &v1["children"][1];
        assert_eq!(shot2["OTIO_SCHEMA"], "Clip.2");
        assert_eq!(
            shot2["media_references"]["DEFAULT_MEDIA"]["target_url"],
            "source.cfr.mp4"
        );
        assert_eq!(shot2["source_range"]["start_time"]["value"], 60.0);

        let captions = &tracks(&t)[1]["children"];
        let one = captions
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "ONE")
            .unwrap();
        assert_eq!(
            one["media_references"]["DEFAULT_MEDIA"]["generator_kind"],
            "viralcraft_caption"
        );
        assert_eq!(one["metadata"]["viralcraft"]["anchor"], "an8");
    }

    #[test]
    fn a_dissolve_splits_its_frames_around_the_cut() {
        let (probe, audio, inspect) = cuts3();
        let mut shots = shots3();
        let fps = probe.rate;
        shots.transitions.push(Transition {
            kind: TransitionKind::Dissolve,
            range: TimeRange::from_frames(56, 8, fps),
        });
        let t = write(&probe, &shots, &audio, &inspect);
        let v1 = tracks(&t)[0]["children"].as_array().unwrap().clone();
        let tr = v1
            .iter()
            .find(|c| c["OTIO_SCHEMA"] == "Transition.1")
            .unwrap();
        assert_eq!(tr["transition_type"], "SMPTE_Dissolve");
        assert_eq!(tr["in_offset"]["value"], 4.0);
        assert_eq!(tr["out_offset"]["value"], 4.0);
    }

    #[test]
    fn judgement_rides_along_on_shot_and_caption_clips() {
        let (probe, audio, inspect) = cuts3();
        let dir = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../editspec/tests/data/cuts-3"
        ));
        let j: model::Judgement =
            serde_json::from_str(&std::fs::read_to_string(dir.join("judgement.json")).unwrap())
                .unwrap();
        let t = write_judged(&probe, &shots3(), &audio, &inspect, Some(&j));
        let shot1 = &tracks(&t)[0]["children"][0];
        assert_eq!(
            shot1["metadata"]["viralcraft"]["judgement"]["role"],
            "a-roll"
        );
        let one = tracks(&t)[1]["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "ONE")
            .cloned()
            .unwrap();
        assert_eq!(
            one["metadata"]["viralcraft"]["judgement"]["kind"],
            "caption"
        );
    }

    #[test]
    fn filmcraft_gets_the_real_frame_size_and_rate() {
        // OTIO has no standard field for picture size; FilmCraft reads its own
        // sequence settings from metadata.filmcraft and otherwise assumes
        // 1920x1080, which turned the first real clip landscape.
        let t = timeline();
        let settings = &t["metadata"]["filmcraft"]["settings"];
        assert_eq!(
            (settings["width"].as_u64(), settings["height"].as_u64()),
            (Some(1080), Some(1920))
        );
        assert_eq!(
            settings["frame_rate"],
            serde_json::json!({"num": 30, "den": 1})
        );
        assert_eq!(settings["preset"], "Custom 1080x1920 30");
    }

    #[test]
    fn placeholder_clips_are_disabled_so_they_never_render() {
        // FilmCraft imports unknown generators as Black Video; enabled clips
        // on tracks above the shots blacked out the picture.
        let t = timeline();
        let captions = &tracks(&t)[1]["children"];
        let one = captions
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "ONE")
            .unwrap();
        assert_eq!(one["enabled"], false);
        assert_eq!(
            tracks(&t)[0]["children"][0]["enabled"],
            true,
            "shots stay enabled"
        );
    }

    #[test]
    fn beats_are_markers_on_the_shots_track() {
        let t = timeline();
        let markers = tracks(&t)[0]["markers"].as_array().unwrap().clone();
        assert_eq!(markers.len(), 12);
        assert_eq!(markers[0]["OTIO_SCHEMA"], "Marker.2");
        assert_eq!(markers[0]["name"], "beat");
    }

    #[test]
    fn cuts_3_otio_snapshot() {
        insta::assert_snapshot!(serde_json::to_string_pretty(&timeline()).unwrap());
    }
}
