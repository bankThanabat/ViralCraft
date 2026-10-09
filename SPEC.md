# Spec: ViralCraft v1

Intent: `docs/intent/viralcraft-v1.md`. Module split: `CAPABILITY-MAP.md`.

## Objective

A Rust CLI (`viralcraft`) plus an agent skill that turn a viral short-form video
into an edit spec an editor can rebuild from without seeing the original, then
generalize that spec into a reusable template.

Two kinds of work are involved, and they live in different places:

- **Measurement** is deterministic and lives in the CLI: cuts, timecodes,
  on-screen text and where it sits, colors, zoom amounts, beats, transcript.
- **Judgement** lives in the agent running the skill: which segment is the
  hook, what is A-roll vs B-roll, where the story beats fall, what the format
  is once you strip the content out. The CLI leaves marked slots for these; the
  skill walks the agent through filling them while looking at the keyframes.

The split means no API key and no per-run cost, and the CLI output is useful on
its own. It also means the CLI alone never produces a finished spec.

User stories:

- As a creator, I drop `hook.mp4` in `./inbox/`, tell my agent "analyze it",
  and get `out/hook/EDIT-SPEC.md` I can hand to my editor.
- As an editor, I open `out/hook/timeline.otio` in FilmCraft or DaVinci and
  see the cut structure already laid out.
- As a creator, I open `out/hook/TEMPLATE.md` and see slots ("talking-head
  hook, 0 to 1.8s, text top third") I can fill with my own story.

## Tech Stack

- Rust stable 1.80+, edition 2021, Cargo workspace.
- External tools: `ffmpeg` and `ffprobe` (the only required binaries).
- Crates (pin versions in `Cargo.toml` at first use):
  - `clap` (derive) for the CLI
  - `serde`, `serde_json` for every intermediate file and the OTIO writer
  - `anyhow` at the binary edge, `thiserror` inside library crates
  - `image` for decoding extracted JPEG/PNG frames
  - `ocrs = "0.13"` with `rten = "0.26"` for on-screen text. Verified on
    Apple Silicon 2026-10-09: two burned-in captions on a 1080x1920 frame read
    back exactly, centers within 0.005 of where they were drawn, 0.05 s per
    frame. Feed frames at full or half size; at quarter size (270x480) it
    dropped a line. `rten` must match the version `ocrs` pins or the `Model`
    types conflict.
  - `whisper-rs = "0.16"` for transcription, with `ggml-tiny.en.bin` (78 MB)
    as the default model. Verified on Apple Silicon 2026-10-09: 3.2 s of
    synthesized speech transcribed word-for-word with token timestamps in
    0.15 s on CPU (BLAS backend; the `metal` feature was not enabled).
    `doctor` downloads the model file on request. Word timing uses whisper's
    DTW token timestamps (`DtwModelPreset::TinyEn`, `t_dtw` per token) with
    `split_on_word`; the token-to-word grouping lives in a tested adapter in
    `crates/audio` that turns whisper-rs segments into `audio.json` words.
  - `ocrs` needs two model files, `text-detection.rten` (2.5 MB) and
    `text-recognition.rten` (9.7 MB), from
    `https://ocrs-models.s3-accelerate.amazonaws.com/`. `doctor` downloads
    them on request to `~/.cache/viralcraft/` and checks their SHA-256. The
    default models read the Latin alphabet only, so non-Latin captions come
    back `unknown` with the box still reported.
  - `tracing`, `tracing-subscriber` for logs
- Shot detection, frame extraction, audio extraction and the raw audio envelope
  come from `ffmpeg` filters (`scdet`, `select`, `showinfo`, `astats`), driven
  as subprocesses. No `ffmpeg-next` bindings in v1.

## Commands

```
Build:        cargo build --release
Test:         cargo test --workspace
Lint:         cargo clippy --workspace --all-targets -- -D warnings
Format:       cargo fmt --all -- --check
Slow tests:   cargo test --workspace -- --ignored      # needs ffmpeg, uses fixtures
Run:          ./target/release/viralcraft <subcommand>
```

CLI surface:

```
viralcraft doctor [--install]           # check tools and models; --install asks per item
viralcraft analyze [FILE] [--inbox DIR] [--out DIR] [--safe-zone PROFILE]
                                        # full pipeline; FILE defaults to the
                                        # single file in --inbox, else asks
viralcraft finalize <RUN_DIR>           # after the agent fills judgement.json:
                                        # merge into EDIT-SPEC.md, write OTIO
                                        # metadata, captions.ass/.vtt, TEMPLATE.md
viralcraft clean <RUN_DIR>              # delete frames/ and source.cfr.mp4
viralcraft shots <RUN_DIR>              # run one stage on an existing run dir;
viralcraft audio <RUN_DIR>              # same for ingest, inspect, editspec, exports
```

Every subcommand prints a one-line JSON summary on stdout (paths written,
counts) and human logs on stderr, so an agent can parse the result and a
person can read the progress. Failures follow the same contract:
`{"command": ..., "ok": false, "error": "..."}` on stdout and exit code 1.

## Project Structure

```
Cargo.toml                 → workspace
apps/viralcraft/           → the binary: clap, wiring, prompts to the user
crates/doctor/             → tool checks, install commands
crates/ingest/             → input resolution, ffprobe
crates/shots/              → scdet parsing, segment list, keyframe extraction
crates/audio/              → whisper, beats and onsets, voiceover flag
crates/inspect/            → OCR, color stats, motion estimate, framing
crates/editspec/           → Markdown writer with judgement slots
crates/exports/            → OTIO and WebVTT writers
crates/template/           → TEMPLATE.md scaffold
crates/model/              → shared serde types for every JSON file below
skill/viralcraft/SKILL.md  → the agent skill (installed with npx skills add)
skill/viralcraft/references/ → hook taxonomy, grade vocabulary, examples
tests/fixtures/            → two tiny generated videos (see Testing)
docs/intent/               → confirmed intent
docs/adr/                  → decisions (Rust, agent-side judgement, ffmpeg-only)
inbox/                     → default drop folder (gitignored)
out/                       → run directories (gitignored)
```

### Run directory

`viralcraft analyze inbox/hook.mp4` writes `out/hook/`:

```
source.cfr.mp4    ingest   the input re-muxed or re-encoded to constant frame rate
probe.json        ingest   rate (rational), frames, width, height, aspect, audio offset
shots.json        shots    segments[] and transition candidates, in frames
frames/           shots    every frame, 720 px on the short side, as JPEG (f00000.jpg ...)
audio.json        audio    words[] with DTW times, beats[], tempo, onsets[], music/speech spans
inspect.json      inspect  caption events, overlay events, transform keyframes, look stats
EDIT-SPEC.md      editspec measured sections filled; judgement slots marked
timeline.otio     exports  OTIO 0.17 schema; viralcraft metadata on every object
captions.ass      exports  styled captions (SubStation Alpha); burnable with ffmpeg
captions.vtt      exports  plain captions with word timestamps
judgement.json    agent    written by the agent following SKILL.md
TEMPLATE.md       template emitted by finalize, then completed by the agent
```

Stage inputs are exact, and the capability map uses the same graph:

| stage | reads | writes |
|---|---|---|
| ingest | input file | source.cfr.mp4, probe.json |
| shots | source.cfr.mp4, probe.json | shots.json, frames/ |
| audio | source.cfr.mp4, probe.json | audio.json |
| inspect | probe.json, shots.json, frames/ | inspect.json |
| editspec | probe, shots, audio, inspect | EDIT-SPEC.md |
| exports | probe, shots, audio, inspect, judgement (optional) | timeline.otio, captions.ass, captions.vtt |
| template | probe, shots, audio, inspect, judgement | TEMPLATE.md |

Every frame is extracted, not a 2 fps sample, at 720 px on the short side
(never upscaled). Half size was tried first; on the first real clip
(1080x1920) it left dialogue text about 19 px tall and OCR misread words
("throuah", "thinas"). A 60 s clip at 30 fps is 1,800
JPEGs at 720x1280, roughly 150 to 250 MB, which caption-change detection
and motion estimation need. `frames/` is deleted by `viralcraft clean` and
is never committed.

## Time, position and color conventions

These apply to every JSON file, the Markdown spec, and OTIO metadata. Each one
is borrowed from an open standard so an editor's tools already understand it.

- **Clock.** `ingest` converts the input to constant frame rate once
  (`ffmpeg -vf fps=<rate> -fps_mode cfr -af aresample=async=1`), because phone
  and TikTok files are often variable frame rate and "frame N" means nothing
  on those. After that, every time is an OTIO `RationalTime` `{value, rate}`
  with `rate` the rational frame rate from `probe.json` (`30000/1001`, not
  `29.97`). Markdown shows `1.800s (f54)`; JSON stores `{"value": 54, "rate":
  30}`. Ranges are start plus duration, end exclusive, like OTIO `TimeRange`.
  `probe.json` records the audio start offset relative to video frame 0.
- **Position and size.** Normalized `0..1` with origin top-left, like WebVTT
  `position`/`line` percentages and Lottie positions scaled by the canvas.
  A caption or overlay is placed by its anchor, named with the ASS numpad
  (`an7` top-left ... `an5` center ... `an3` bottom-right), plus the anchor's
  `(x, y)`. Boxes are `w×h`. Pixel values at the source resolution may follow
  in parentheses.
- **Transforms.** Lottie's transform vocabulary: `a` anchor, `p` position,
  `s` scale in percent, `r` rotation in degrees, `o` opacity 0..100. Animated
  values are keyframe lists `{t, s, i, o}` where `t` is the frame, `s` the
  value and `i`/`o` the cubic-bezier easing handles (`o:{x:0,y:0}
  i:{x:0.58,y:1}` is ease-out). `h:1` marks a hold.
- **Color.** `#RRGGBB` sRGB. Luma and saturation stats as `0..1` floats,
  computed in Rec.709.
- **Measured vs judged.** A value the CLI could not measure is written
  `unknown`, never omitted and never guessed. A field needing the agent is
  marked `<!-- judgement -->` and holds `TODO:` until `judgement.json` fills
  it. Every judgement field is listed in the `required` set below, so a spec
  with a remaining `TODO:` is not finished.

## Edit spec format

`EDIT-SPEC.md` is the deliverable that must pass the blind-rebuild test. Fixed
sections, fixed field names, so an agent and an editor both know where to look.
It is organized as three independent tracks (shots, captions, overlays) plus
sound and look, because captions and effects do not line up with cuts.

```markdown
# Edit spec: hook.mp4

## Source
14.967s · 30/1 fps · 1080x1920 (9:16) · 449 frames · audio stereo 48 kHz, offset 0
safe zone profile: tiktok-2026-conservative (top 0.078, right 0.153, bottom 0.167, left 0.056)

## Hook                                      <!-- judgement -->
type: <text-overlay | talking-head | pattern-interrupt | ...>
window: 0.000s (f0) to 1.800s (f54)
why it works: <one or two sentences>

## Shots
| # | in | out | dur | kind | role | on screen | transform | transition in |
|---|----|-----|-----|------|------|-----------|-----------|---------------|
| 1 | f0 | f54 | 1.800 | talking-head | A-roll / hook | ... | scale 100→112 ease-out, anchor (0.50,0.40) | cut |
| 2 | f54 | f110 | 1.867 | screen-rec | B-roll | ... | none | dissolve 8f (4/4) |

### Shot 1  (0.000s to 1.800s, f0 to f54)
on screen:       <measured description + judgement>
role:            A-roll, hook                           <!-- judgement -->
speech:          on-camera                              <!-- judgement -->
framing:         medium close-up, subject center (0.42, 0.38), head height 0.21
transform:       anchor (0.50, 0.40) · scale 100 → 112 over f0..f54 · ease-out
                 (o 0,0 i 0.58,1) · position fixed · confidence 0.91
speed:           1.0x (no retime)
transition in:   cut
transition out:  cut at f54, 0 frames from beat 3 (f47 ± 2)
look:            see Look; this shot luma mean 0.42, dominant #1E2A38 #C9A96E #F2EFE9

## Captions
style C1: font <unknown, substitute: Inter Black> · size 0.050 of frame height (96 px)
          · fill #FFFFFF · outline #000000 width 3 px · shadow 0 · box none
          · bold · tracking 0 · line height 1.2 · align center · anchor an8
          · uppercase · max 2 lines
| id | style | in | out | text | anchor (x, y) | box | animation | word timing |
|----|-------|----|-----|------|---------------|-----|-----------|-------------|
| c1 | C1 | f0 | f54 | STOP SCROLLING | an8 (0.50, 0.14) | 0.79×0.044 | pop-in 4f scale 80→100 ease-out | 1 word/beat, highlight #FFE600 |
| c2 | C1 | f54 | f110 | this one trick | an8 (0.50, 0.14) | 0.62×0.044 | none | per-word, from transcript |
clearance: all captions inside safe zone (min clearance 0.06 from bottom band)

## Overlays
| id | kind | in | out | anchor (x, y) | size | transform | source |
|----|------|----|-----|---------------|------|-----------|--------|
| o1 | emoji | f12 | f54 | an5 (0.82, 0.30) | 0.12×0.07 | rotation -8°, pop-in 3f | 🔥 |
| o2 | image | f54 | f110 | an5 (0.50, 0.62) | 0.80×0.45 | fixed | product shot, drop shadow 12 px |

## Transitions
| at | type | duration | split (out/in) | parameters |
|----|------|----------|----------------|------------|
| f54 | dissolve | 8f | 4/4 | none |
| f110 | custom | 2f | 0/2 | white flash, opacity 100→0 |

## Sound
tempo 115 bpm (confidence 0.82) · beat grid f0 + n×15.65f · downbeats every 4
cuts on beat: 7 of 9 within ±2 frames
| track | in | out | gain | fade in/out | notes |
|-------|----|-----|------|-------------|-------|
| speech | f0 | f449 | 0 dB | none | on-camera f0..f54, voiceover f54..f449 <!-- judgement --> |
| music | f0 | f449 | -14 dB | 0 / 15f | present, ducked under speech -6 dB |
| sfx | f54 | f58 | -8 dB | none | whoosh on cut |
| sfx | f126 | f129 | -8 dB | none | pop |

## Look
measured: dominant #1E2A38 #C9A96E #F2EFE9 · luma mean 0.42 · min 0.06 · max 0.97
          · saturation mean 0.38 · warmth (R-B) +0.08 · per-shot table in inspect.json
named:    teal-orange, lifted blacks            <!-- judgement -->
match by: lift shadows to 0.06, warm the mids, push saturation until mean ≈ 0.38

## Story beats                                <!-- judgement -->
1. f0–f54     hook: claim
2. f54–f192   problem ...

## A-roll / B-roll map                        <!-- judgement -->
A-roll: shots 1, 3, 6 (talking head)
B-roll: shots 2, 4, 5 (screen recording, product shot, stock)
```

What each section borrows:

- **Captions** use the ASS/SubStation Alpha style vocabulary (`Fontname`,
  `Fontsize`, `PrimaryColour`, `OutlineColour`, `BackColour`, `Bold`,
  `ScaleX/Y`, `Spacing`, `BorderStyle` 1 outline or 3 opaque box, `Outline`,
  `Shadow`, `Alignment` an1..an9, margins) and its animation tags for the
  animation column (`\fad`, `\move`, `\t(\fscx\fscy)`, `\k` karaoke
  centiseconds per word). `captions.ass` is the same data in that format, so
  `ffmpeg -vf ass=captions.ass` burns it onto any video for a visual check
  (needs an ffmpeg built with libass; Homebrew's `ffmpeg` is not, `ffmpeg-full` is).
  Word timing in `captions.vtt` uses inline cue timestamps
  (`<00:00:01.200>word`), which is how WebVTT does word highlighting.
  Font family is reported `unknown` with an agent-chosen substitute; OCR cannot
  identify fonts.
- **Transitions** map to OTIO `Transition.1`: `in_offset` and `out_offset`
  are the frames taken from each neighbor, `transition_type` is
  `SMPTE_Dissolve` or `Custom_Transition`, and parameters go in
  `metadata.viralcraft`. A hard cut is the absence of a transition.
- **Speed** maps to OTIO `LinearTimeWarp.1` (`time_scalar`, 2.0 = twice as
  fast) and `FreezeFrame.1`. Ramps are a sequence of LinearTimeWarps on split
  clips; v1 measures constant speed per shot and writes `unknown` for ramps.
- **Overlays** are clips on video tracks above the shots track. OTIO stacks
  compose bottom to top, so track order is layer order. Each overlay clip
  carries a Lottie-shaped transform in `metadata.viralcraft.transform`.
- **Sound** uses JAMS namespaces for the measured parts: `beat` events
  (time, metrical position, confidence), `tempo` (bpm, confidence 0..1) and
  `onset`. "On beat" means within ±2 frames, the editing equivalent of the
  ±70 ms window beat-tracking research uses. Tracks are OTIO audio tracks
  with gain and fades in metadata.
- **Sound sync.** Viral edits tie sounds to picture: a whoosh on a
  transition, a pop when a caption or zoom lands. Each onset and labelled
  sound within ±2 frames of a cut, transition, caption entrance, overlay
  entrance or zoom start is listed against that event in a `sync` column,
  so the editor knows which sound goes with which edit. Requested by the
  user after the first real clip.
- **Safe zones** have no published TikTok spec. v1 ships one named profile,
  `tiktok-2026-conservative` (top 150 px, right 165 px, bottom 320 px, left
  60 px on 1080x1920, from measured overlay files), stores it in
  `probe.json`, and reports each caption's clearance. Profiles are a JSON
  file so they can be updated without a release.

## Caption and overlay detection

Two fps sampling misses captions that flash for ten frames, so `inspect` works
per frame:

1. Compute a text-region mask on every frame (ocrs detection only, no
   recognition; measured at 25 to 33 ms per 540x960 frame and about 62 ms
   per 720x1280 frame).
2. A caption event begins when a new text region appears and ends when it
   disappears; a region whose box moves or scales over its first 2..8 frames
   is tagged with that animation (pop-in, slide, fade from opacity ramp).
3. Run recognition once per event on its sharpest frame, and again whenever
   the mask inside the box changes (word highlight, typewriter).
4. Match recognized text against transcript words by time to assign per-word
   timing; unmatched captions keep OCR-only timing.

Overlays (emoji, stickers, images) are regions that are not text and not part
of the shot's camera motion. v1 predicts each frame from the shot's first
frame warped by the measured camera transform, takes regions that differ
(outside captions), and keeps those tracked for at least 6 frames at a steady
size and place; moving subjects change shape and drop out. It reports
position, size and in/out; kind is a judgement call. Known limits: an overlay
already on the shot's first frame looks like scene and is missed, and shots
with `unknown` motion are skipped.

## Motion estimation

Per shot, each frame's 135x240 grayscale thumbnail is compared with the
shot's first frame: for candidate scales of the first frame (1% steps,
re-centred until the best is not at the edge, then refined between steps),
phase correlation gives the translation and a peak height. Captions on
screen during the shot are masked out. The best scale per frame forms a
curve that is fitted to Lottie keyframes: one keyframe when scale moves
less than 1%, otherwise first and last frame with `cubic-bezier(x1, 0, x2,
1)` handles fitted to the curve. Position is reported the same way.
Rotation is not measured in v1.

Confidence is the median correlation peak over the shot. Below 0.3 the
transform is written `unknown` and the agent inspects the frames.
Calibrated 2026-10-09: the real `zoom-1.mp4` scores 0.69 and recovers
100→119.9 with handles (0, 0, 0.55, 1) against the true (0, 0, 0.58, 1);
unrelated frames score under 0.3; a flat single-colour shot scores 0 and
is `unknown`. The measurement is the apparent camera transform; whether
the editor used a zoom keyframe or a punch-in crop is the editor's choice.

## judgement.json

Written by the agent after inspecting frames, following SKILL.md. Every
`<!-- judgement -->` slot in the spec has a field here, and the agent can
override any measured value with a reason.

```json
{
  "version": 1,
  "hook": { "type": "text-overlay", "window": { "start": 0, "duration": 54 }, "why": "..." },
  "shots": [
    { "n": 1, "role": "a-roll", "beat": "hook", "speech": "on-camera",
      "on_screen": "creator to camera, kitchen" }
  ],
  "captions": [
    { "id": "c1", "kind": "caption", "font_substitute": "Inter Black", "text_fix": null },
    { "id": "c4", "kind": "scene-text", "text_fix": "ONE WISH WILLOW" }
  ],
  "overlays": [ { "id": "o1", "kind": "emoji", "description": "fire emoji" } ],
  "sound": { "music_role": "bed", "speech_kind": [ { "start": 0, "duration": 54, "kind": "on-camera" } ] },
  "look": { "name": "teal-orange, lifted blacks", "match_by": "..." },
  "story_beats": [ { "start": 0, "duration": 54, "label": "hook: claim" } ],
  "format_name": "problem-agitate-solve talking head with B-roll proof",
  "overrides": [
    { "path": "shots[2].transform", "value": "unknown", "reason": "subject motion, not a zoom" }
  ]
}
```

Required before a spec is finished: `hook`, every shot's `role` and `speech`,
every caption's `kind` (`caption`, `title`, `watermark` or `scene-text`) and
its `font_substitute` when the font is `unknown`, every overlay's `kind`,
`look.name`, `story_beats`, `format_name`. OCR cannot tell burned-in captions
from text that is part of the scene (the first real clip's product packaging
read as captions), so the agent decides; `scene-text` captions stay in
`inspect.json` but leave the edit spec's caption track.

`finalize` merges it into EDIT-SPEC.md, writes it to `metadata.viralcraft` on
the matching OTIO clip, marker or transition, and emits TEMPLATE.md.

## OTIO layout

`timeline.otio` follows what FilmCraft writes (`crates/interchange/src/otio.rs`
in that repo), so a FilmCraft import is the reference check:

- `Timeline.1` → `Stack.1` → tracks bottom to top: `V1 shots`, `V2 captions`,
  `V3 overlays`, then `A1 speech`, `A2 music`, `A3 sfx`.
- Shots are `Clip.2` with `ExternalReference.1` to `source.cfr.mp4`,
  `source_range` in RationalTime at the source rate, and
  `media_references: {"DEFAULT_MEDIA": ...}` with
  `active_media_reference_key`.
- Captions and overlays are `Clip.2` with `GeneratorReference.1`
  (`generator_kind: "viralcraft_caption"` / `"viralcraft_overlay"`) and their
  full record in `metadata.viralcraft`.
- Transitions are `Transition.1` between shot clips.
- Markers (`Marker.2`) carry hook window, story beats and beat grid; a marker's
  `marked_range` is in its owning item's time frame, so markers that span
  shots go on the track, not on a clip.
- Everything ViralCraft knows and OTIO cannot express sits under
  `metadata.viralcraft` on the nearest object; nothing is lost on round trip.

## TEMPLATE.md

Generalization of the spec. Content stripped, structure kept, with slots:

```markdown
# Template: problem-agitate-solve talking head with B-roll proof
derived from hook.mp4 · 15s · 9:16 · 9 cuts · 115 bpm

## Slots
| slot | from | to | kind | fill with |
|------|------|----|------|-----------|
| HOOK | 0.0s | 1.8s | talking-head + text overlay top third | your claim, 3–5 words |
| PROBLEM | 1.8s | 6.4s | A-roll | ... |

## Rules
- Cut on every third beat (0.52s grid)
- Caption: sans bold ~5% of frame height, white, 3px black stroke, pop-in 0.12s
- Zoom in 12% over each A-roll segment, ease-out
```

The CLI writes the slot table and an "Observed timings" list (cut frames,
beat grid, caption durations, transform ranges) from measurements and
judgement.json. The agent writes the Rules section, the "fill with" column and
the format name, because deciding that "cut every third beat" is intentional
rather than coincidence is a judgement.

## Code Style

```rust
/// A contiguous run of frames between two detected cuts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    /// 1-based index, stable across every file in the run directory.
    pub n: u32,
    pub in_frame: u64,
    /// Exclusive.
    pub out_frame: u64,
}

impl Segment {
    pub fn duration_secs(&self, fps: f64) -> f64 {
        (self.out_frame - self.in_frame) as f64 / fps
    }
}

pub fn segments_from_cuts(cut_frames: &[u64], total_frames: u64) -> Vec<Segment> {
    let bounds = std::iter::once(0)
        .chain(cut_frames.iter().copied())
        .chain(std::iter::once(total_frames));
    bounds
        .clone()
        .zip(bounds.skip(1))
        .enumerate()
        .map(|(i, (a, b))| Segment { n: i as u32 + 1, in_frame: a, out_frame: b })
        .collect()
}
```

Conventions:

- Frame numbers are `u64`, seconds are `f64`, never mixed in one struct field.
- Library crates return `Result<T, CrateError>` with `thiserror`; only the binary
  uses `anyhow`.
- Subprocess calls go through one `tools::run(cmd, args)` helper that captures
  stderr and includes it in the error. No bare `Command::new` elsewhere.
- Every JSON file has a `version: 1` field and a serde type in `crates/model`.
- Functions that parse tool output take `&str`, not a path, so tests feed them
  strings.
- No `unwrap` outside tests. `expect` only with a message saying why it cannot
  fail.
- `rustfmt` defaults; `clippy` clean with `-D warnings`.

## Testing Strategy

Guided by the project testing philosophy: fast, isolated, repeatable.

- **Unit tests** (`cargo test`, must finish in under 2 seconds total): every
  parser and every pure function. ffmpeg `scdet` output, `showinfo` lines,
  whisper JSON, OCR boxes, beat grids, the Markdown writer, the OTIO and VTT
  writers. Inputs are string literals and small in-memory frames (`image`
  buffers built in the test). No subprocesses, no filesystem beyond `tempfile`.
- **Snapshot tests** for `EDIT-SPEC.md`, `timeline.otio`, `captions.vtt` and
  `TEMPLATE.md` from fixed `*.json` inputs, using `insta`. A format change is a
  reviewed snapshot change.
- **Integration tests** (`#[ignore]`, run with `-- --ignored` and in CI): the
  whole pipeline on two fixtures generated by ffmpeg once and committed:
  - `tests/fixtures/cuts-3.mp4`: 6s, 1080x1920, three solid-color scenes with
    a hard cut every 2s, burned-in text "ONE", "TWO", "THREE", and a 120 bpm
    click track. Expected: 3 segments at frames 0/60/120, OCR finds the three
    words, beats at 0.5s spacing.
  - `tests/fixtures/zoom-1.mp4`: 3s, one scene, a 1.0→1.2 zoom. Expected: 1
    segment, motion estimate within 0.03 of 1.2.
  Fixtures are under 300 KB each. The generator script lives in `scripts/` so
  they can be rebuilt. Homebrew's `ffmpeg` is built without `drawtext`, so
  the generator renders text frames with Python PIL and uses ffmpeg only to
  assemble them into video.
- **No fast tests call whisper or ocrs.** The boundary is the adapter from
  whisper-rs segments and tokens into `audio.json`, and from ocrs line results
  into `inspect.json`; those adapters take plain Rust structs built in the
  test. One ignored smoke test runs each model on `tests/fixtures/speech.wav`
  (the `say`-generated sentence from the verification run) and `frame.png`,
  with timing tolerance ±100 ms per word and box tolerance 0.02.
- Coverage is not a target. Every parser has a malformed-input test.

## Boundaries

**Always**

- Run `cargo fmt`, `cargo clippy -D warnings` and `cargo test` before a commit.
- Keep every intermediate file as versioned JSON with a type in `crates/model`.
- Write `unknown` for a value the CLI cannot measure. Never invent a number.
- Ask before installing anything; `doctor` prints the command and waits.
- Keep the Markdown section names and field names in this spec stable; the
  skill and the snapshots depend on them.

**Ask first**

- Adding a crate dependency, especially anything that compiles native code.
- Changing the run-directory layout or any JSON schema (bump `version`).
- Changing a fixture (it invalidates expected values in integration tests).
- Adding a second external tool beyond ffmpeg.

**Never**

- Download videos from a URL. v1 is file input only.
- Render or modify the user's assets. Analysis only.
- Call a hosted model from the CLI. Judgement stays in the agent.
- Commit `inbox/`, `out/`, model weights or any user video.
- Delete or skip a failing test to get to green.

## Success Criteria

- `viralcraft doctor` checks ffmpeg, ffprobe, the whisper model and both ocrs
  models. It prints one line per item with the install or download command,
  exits 0 when all are present and 1 otherwise, and never installs.
  `viralcraft doctor --install` asks per item and runs the command on yes.
- `viralcraft analyze` with one file in `inbox/` runs without prompting; with
  zero or two files it asks for a path. If `doctor` would fail, `analyze`
  prints the same report and exits 1 before touching the input.
- On `cuts-3.mp4`: 3 segments with cut frames within ±1 frame of 60 and 120;
  OCR returns ONE, TWO, THREE each with a box whose center is within 0.05 of
  the burned-in position; caption events start and end within ±1 frame;
  beats detected at 0.5 s ± 2 frames with tempo 120 ± 1 bpm.
- On `cuts-3.mp4`, a VFR variant (`cuts-3-vfr.mp4`, same content with
  irregular timestamps) produces identical `shots.json` after `ingest`.
- On `zoom-1.mp4`: one segment; motion reported as `zoom 1.00→1.2x` with x
  within 0.03.
- `timeline.otio` round-trips through the Python `opentimelineio` reference
  library (read, write, read; durations, source ranges, transition offsets and
  marker ranges equal) and imports into FilmCraft with the same clip count,
  trims and transitions. FilmCraft is the one verified editor for v1; DaVinci
  and Premiere are untested.
- `captions.ass` burned onto `cuts-3.mp4` with `ffmpeg -vf ass=` places each
  word within 0.02 of its OCR box; checked by OCR-ing the burned output.
- `captions.vtt` validates with a WebVTT validator and lines up with the
  transcript word timestamps.
- `EDIT-SPEC.md` contains no empty fields: every field is a measured value,
  `unknown`, or a `<!-- judgement -->` slot. After `finalize`, no `TODO:`
  remains.
- Blind-rebuild test, run manually before calling v1 done: an editor rebuilds
  one real viral video from `EDIT-SPEC.md` plus new footage, and a side-by-side
  reads as the same format. Record the result in `docs/blind-rebuild-01.md`.
- `cargo test --workspace` under 2 seconds; the ignored integration suite
  under 60 seconds on an M-series Mac.

## Decisions

- **Judgement is agent-side.** Confirmed 2026-10-09. The CLI never calls a
  hosted model; a `--vision` path can come later using the same
  `judgement.json` contract.
- **`ocrs` and `whisper-rs` verified on Apple Silicon** 2026-10-09 (see Tech
  Stack). No shell-out fallback needed for v1.
- **Drop folder is `./inbox/` and output is `./out/`, relative to the working
  directory.** Confirmed 2026-10-09. Both can be overridden with `--inbox` and
  `--out`.
- **Caption, transition, transform, sound and safe-zone formats borrow from
  open standards** (ASS styles and tags, WebVTT inline timestamps, OTIO
  Transition/LinearTimeWarp/FreezeFrame, Lottie transforms and keyframes,
  JAMS beat/tempo/onset, a named safe-zone profile). Decided 2026-10-09 after
  the Codex spec review. `captions.ass` is added as a third export so captions
  can be burned in and checked visually.
- **Voiceover vs on-camera is a judgement field**, not an `audio` measurement.
  `audio` reports speech spans; the agent labels them.
- **VFR is detected from packet timestamps**, not ffprobe's rate fields,
  which can report equal real and average rates for a jittered file. A file
  is VFR when any frame interval is more than a quarter frame off the nominal
  rate; it is then re-encoded at the nearest standard rate. CFR files are
  stream-copied. Decided 2026-10-09 in task 3.
- **Word timing rule.** Measured on `speech.wav` against ground truth:
  whisper's DTW time marks a word's end (about 0.1 s late on average), and
  token `t0` is a good start that sometimes drifts early into the previous
  word. A word therefore ends at its latest DTW time and starts at `t0`, never
  before the previous word's end. Worst start error on the fixture is 3
  frames (100 ms) at 30 fps, so word-highlight timing is good to about 3
  frames with the tiny model. Decided 2026-10-09 in task 6.
- **Sound-effect names are optional and go into the transcript.** Google's
  YAMNet (Apache-2.0, 16 MB, AudioSet classes, run through rten) names
  non-speech sounds; `doctor --install` asks whether the user wants it.
  Sounds are written in brackets where they happen, closed-caption style
  (`here it goes. [whoosh] [thunk]`), in `audio.json`'s `transcript`.
  YAMNet works in 0.48 s steps; the user asked for this placement rather
  than frame-exact timing. Speech, singing, music and room-tone classes are
  ignored, scores under 0.3 dropped. Chosen over EfficientAT (more accurate,
  PyTorch only) because it runs today; one function wraps it so it can be
  swapped. On the first real clip it found `[whoosh] [thunk]` on the
  punchline cut and misnamed a jingle `[car alarm]`. Decided 2026-10-09.
- **Speed ramps are `unknown` in v1.** Constant per-shot speed is measured;
  ramps are left to the agent and the editor.

## Open Questions

1. **Beat detection quality** on compressed TikTok audio with speech over
   music is unknown. The fixture uses a clean click; a real-video check is
   part of the blind-rebuild run.
2. **Per-frame text detection cost.** Running ocrs detection on every frame
   is unmeasured; the verification run did detection plus recognition on a
   full-res frame in 50 ms, so detection alone at half res should be well
   under that, giving under 60 s for a 60 s clip. Measure in the first
   `inspect` slice. If it is too slow, detect every 3rd frame and refine
   around changes.
3. **Overlay detection quality.** Frame differencing against the camera
   transform is untested on real clips. If it is noisy, v1 reports overlays
   only where the agent marks them.
