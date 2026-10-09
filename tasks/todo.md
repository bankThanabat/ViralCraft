# ViralCraft v1 tasks

Plan: `tasks/plan.md`. Spec: `SPEC.md`. One commit per task.

## Phase 1: Foundation

## Task 1: Workspace skeleton and shared types ✅ done

**Description:** `git init`, Cargo workspace, `crates/model` with the versioned
time types every JSON file uses (`RationalTime`, `TimeRange`, `Fps` as a
rational), `apps/viralcraft` with clap and a `doctor` subcommand that only
prints the JSON summary line. `.gitignore` for `target/`, `inbox/`, `out/`.

**Acceptance criteria:**
- [x] `RationalTime::from_frames(54, Fps::new(30000, 1001)).to_seconds()` is `1.8018` within 1e-6, and the serde form is `{"value":54,"rate":{"num":30000,"den":1001}}`
- [x] `TimeRange` end is exclusive: a range starting at f0 with duration 54 does not contain f54
- [x] `viralcraft doctor` prints one JSON line on stdout and exits 0

**Verification:**
- [x] Tests pass: `cargo test --workspace`
- [x] Build succeeds: `cargo build --release`
- [x] Lint clean: `cargo clippy --workspace --all-targets -- -D warnings`

**Dependencies:** None

**Files likely touched:** `Cargo.toml`, `crates/model/{Cargo.toml,src/lib.rs,src/time.rs}`, `apps/viralcraft/{Cargo.toml,src/main.rs}`, `.gitignore`, `rustfmt.toml`

**Estimated scope:** M

## Task 2: doctor checks tools and models ✅ done

**Description:** `crates/doctor` checks `ffmpeg`, `ffprobe`, `ggml-tiny.en.bin`
and both ocrs `.rten` files (path, size, SHA-256). Prints one line per item
with the install or download command for macOS, Linux and Windows. `--install`
asks per item on stderr and runs the command on yes. Never installs otherwise.

**Acceptance criteria:**
- [x] With an injected `which` that finds nothing, the report lists ffmpeg and ffprobe as missing with `brew install ffmpeg` on macOS and exits 1
- [x] A model file with the wrong SHA-256 is reported as `corrupt`, not `present`
- [x] With everything present, exit 0 and the JSON summary has `"ok": true`

**Verification:**
- [x] Tests pass: `cargo test -p doctor`
- [x] Manual check: `viralcraft doctor` on this Mac lists ffmpeg, ffprobe present and the three models with their state

**Dependencies:** 1

**Files likely touched:** `crates/doctor/{Cargo.toml,src/lib.rs,src/report.rs}`, `apps/viralcraft/src/main.rs`

**Estimated scope:** M

## Task 3: ingest resolves input, normalizes to CFR, writes probe.json ✅ done

**Description:** `crates/ingest` resolves the input (explicit path, else the
single file in `--inbox`, else asks on stderr and reads a path from stdin),
runs ffprobe, converts to constant frame rate into `source.cfr.mp4` (stream
copy when already CFR), writes `probe.json` with rational rate, frame count,
size, aspect, audio offset and the named safe-zone profile. Adds the shared
`tools::run` subprocess helper.

**Acceptance criteria:**
- [x] ffprobe JSON parsed from a string literal yields `Fps { num: 30000, den: 1001 }`, 1080x1920, aspect `9:16`, and the audio start offset
- [x] With two files in `inbox/`, resolution asks instead of picking one
- [x] `probe.json` contains `safe_zone: { profile: "tiktok-2026-conservative", top, right, bottom, left }` in normalized units

**Verification:**
- [x] Tests pass: `cargo test -p ingest`
- [x] Manual check: `viralcraft ingest /tmp/vc-verify/media/speech.wav` fails with a clear "no video stream" error

**Dependencies:** 2

**Files likely touched:** `crates/ingest/{Cargo.toml,src/lib.rs,src/probe.rs,src/resolve.rs}`, `crates/model/src/{probe.rs,tools.rs}`, `crates/model/safe-zones.json`, `apps/viralcraft/src/main.rs`

**Estimated scope:** M

## Task 4: fixtures generator and committed fixtures ✅ done

**Description:** `scripts/make-fixtures.py` renders frames with PIL and
assembles them with ffmpeg: `cuts-3.mp4` (6 s, 1080x1920, three solid scenes
with ONE/TWO/THREE at known positions, 120 bpm click), `cuts-3-vfr.mp4` (same
with irregular timestamps), `zoom-1.mp4` (3 s, one scene with a textured
background, scale 100→120 ease-out), `speech.wav` built from per-word `say` clips at known offsets (exact word
times; rebuilt only with `--speech`), and `cuts-3-scene0.png` as the
single-frame OCR reference. A `fixtures.json` records every expected value.

**Acceptance criteria:**
- [x] Each video is under 300 KB and `ffprobe` reports the intended rate, size and duration
- [x] `fixtures.json` lists cut frames, caption text and box centers, beat times, zoom endpoints
- [x] Running the script twice produces byte-identical PNG frames (deterministic)

**Verification:**
- [x] Manual check: `python3 -I scripts/make-fixtures.py` then `ls -la tests/fixtures`
- [x] Build succeeds: `cargo build --release` (unchanged)

**Dependencies:** None

**Files likely touched:** `scripts/make-fixtures.py`, `tests/fixtures/*`, `tests/fixtures/fixtures.json`

**Estimated scope:** S

## Checkpoint: Foundation
- [x] `cargo test --workspace` green in under 2 s
- [x] `viralcraft doctor` correct on this Mac
- [x] `viralcraft ingest tests/fixtures/cuts-3-vfr.mp4` yields a CFR file and a correct `probe.json`
- [ ] Review with human before proceeding (folded into the Phase 2 checkpoint)

## Phase 2: Measurement

## Task 5: shots from scdet, every frame extracted ✅ done

**Description:** `crates/shots` runs ffmpeg `scdet` on `source.cfr.mp4`,
parses the score lines, applies the threshold policy (default 10 on the 0..100
scale, kept in `shots.json` with every candidate's score), builds the segment
list, flags dissolve and flash candidates (score spread over several frames,
or a one-frame luma spike), and extracts every frame at half resolution into
`frames/fNNNNN.jpg` (flat; `shots.json` maps frames to shots).

**Acceptance criteria:**
- [x] `scdet` output parsed from a string literal yields cut frames `[60, 120]` with their scores
- [x] A score run spread over 6 frames is reported as a `dissolve` candidate; an isolated frame with a luma spike and no following change is `flash`
- [x] Ignored test on `cuts-3.mp4`: 3 segments, cuts within ±1 of 60 and 120, 180 frames on disk

**Verification:**
- [x] Tests pass: `cargo test -p shots` and `cargo test -p shots -- --ignored`
- [x] Manual check: `viralcraft shots out/cuts-3` prints segment count 3

**Dependencies:** 3, 4

**Files likely touched:** `crates/shots/{Cargo.toml,src/lib.rs,src/scdet.rs,src/frames.rs}`, `crates/model/src/shots.rs`, `apps/viralcraft/src/main.rs`

**Estimated scope:** M

## Task 6: audio words via whisper-rs adapter ✅ done

**Description:** `crates/audio` extracts 16 kHz mono PCM with ffmpeg, runs
whisper-rs with DTW token timestamps (`DtwModelPreset::TinyEn`) and
`split_on_word`, and an adapter turns segments and tokens into
`audio.json.words[]` (`text`, `start`, `duration` as RationalTime,
`confidence`). Speech spans are the union of words with gaps under 0.5 s.

**Acceptance criteria:**
- [x] The adapter, given hand-built token lists (`" Stop"`, `" scrolling"`, `"."`, with t0/t1/t_dtw), groups punctuation onto the previous word and converts centiseconds to RationalTime at the probe rate
- [x] Tokens with `t_dtw == -1` fall back to segment-proportional timing and are marked `confidence: 0.3`
- [x] Ignored smoke test on `speech.wav`: the 11 words match the fixture text, each start within 3 frames (100 ms at 30 fps) of `speech.json`

**Verification:**
- [x] Tests pass: `cargo test -p audio` and `cargo test -p audio -- --ignored`
- [x] Build succeeds: `cargo build --release`

**Dependencies:** 3, 4

**Files likely touched:** `crates/audio/{Cargo.toml,src/lib.rs,src/whisper.rs,src/words.rs}`, `crates/model/src/audio.rs`

**Estimated scope:** M

## Task 7: beats, tempo and onsets ✅ done

**Description:** Onset-strength envelope by spectral flux (rustfft, 2048
window, 512 hop), onsets by adaptive-threshold peak picking, tempo by
autocorrelation of the envelope in the 60..200 bpm band with half/double
disambiguation, beat phase by comb alignment. Output in JAMS shape: `beats[]`
(time, position, confidence), `tempo {bpm, confidence}`, `onsets[]`. Music
span: first to last beat when tempo confidence ≥ 0.5 (one span in v1).

**Acceptance criteria:**
- [x] A synthetic in-memory click track at 120 bpm yields tempo 120 ±1 and beats within ±20 ms
- [x] The same track with white noise at -10 dB still yields 120 ±2; silence yields `tempo.confidence < 0.2`
- [x] Ignored test on `cuts-3.mp4`: beats at 0.5 s multiples within ±2 frames

**Verification:**
- [x] Tests pass: `cargo test -p audio`
- [x] Manual check: `viralcraft audio out/cuts-3` prints bpm 120

**Dependencies:** 6

**Files likely touched:** `crates/audio/src/{onsets.rs,tempo.rs,beats.rs}`, `crates/model/src/audio.rs`

**Estimated scope:** M

## Task 8: caption events from per-frame text detection ✅ done

**Description:** `crates/inspect` runs ocrs detection on every frame in a shot,
tracks text regions across frames into caption events (appear, change,
disappear), runs recognition once per event on the sharpest frame and again
when the masked region changes, estimates ASS-style attributes (fill color by
the dominant bright pixel in the box, outline by the dark rim, size from box
height over frame height, anchor from box position, alignment from box
centering), tags animations from the first 2..8 frames (pop-in by scale,
slide by position, fade by contrast ramp). Word alignment to the transcript
moved to the output stage (tasks 12/13), since `inspect` does not read
`audio.json`. Measures and logs per-frame detection time.

**Acceptance criteria:**
- [x] Given synthetic frame sequences (text appears at f10, scales 80→100 over 4 frames, disappears at f40), the tracker emits one event `f10..f40` with `animation: pop-in 4f`
- [x] Two different texts in the same box at f10..f20 and f20..f30 become two events
- [x] Ignored test on `cuts-3.mp4`: ONE/TWO/THREE events start and end within ±1 frame of the cuts, box centers within 0.05 of `fixtures.json`, fill `#FFFFFF`, outline `#000000`

**Verification:**
- [x] Tests pass: `cargo test -p inspect` and `-- --ignored`
- [x] Manual check: logged detection time per frame, recorded in `tasks/notes.md`

**Dependencies:** 5

**Files likely touched:** `crates/inspect/{Cargo.toml,src/lib.rs,src/text.rs,src/captions.rs,src/style.rs}`, `crates/model/src/inspect.rs`

**Estimated scope:** M

## Task 9: motion estimate as Lottie keyframes ✅ done

**Description:** Per shot, estimate a global similarity (scale and translation;
rotation reported `unknown` in v1) between consecutive downsampled grayscale
frames with a coarse-to-fine scale sweep and phase correlation, weighted away
from detected text boxes. Accumulate, fit keyframes at shot ends and
inflections, fit bezier handles from the curve shape, report confidence from
correlation peak sharpness.

**Acceptance criteria:**
- [x] Synthetic in-memory frames of a textured image scaled 1.00→1.20 with ease-out over 90 frames yield keyframes `s: 100` at f0 and `s: 120 ±3` at f89 with `i.x` in 0.45..0.7
- [x] A static sequence yields scale 100 ±1 with confidence above 0.9; pure noise yields confidence below 0.3 and `unknown` (threshold calibrated to 0.3, see SPEC.md)
- [x] Ignored test on `zoom-1.mp4`: 100→120 within 3

**Verification:**
- [x] Tests pass: `cargo test -p inspect` and `-- --ignored`
- [x] Manual check: `viralcraft inspect out/zoom-1` prints the transform

**Dependencies:** 5

**Files likely touched:** `crates/inspect/src/{motion.rs,phasecorr.rs,keyframes.rs}`, `crates/model/src/transform.rs`

**Estimated scope:** M

## Task 10: look statistics and safe-zone clearance ✅ done

**Description:** Per shot and overall: dominant colors (16-level histogram bins,
near-identical merged, top 3), luma mean/min/max, saturation mean, warmth as mean R minus B,
computed in Rec.709 from the half-res frames. Per caption and overlay:
clearance from each safe-zone band, and an `inside` flag.

**Acceptance criteria:**
- [x] A synthetic frame of three flat colors returns those three hexes as dominant within ±2 per channel
- [x] A caption box at y 0.90 with the `tiktok-2026-conservative` profile is `inside: false` with bottom clearance negative
- [x] Ignored test on `cuts-3.mp4`: dominant colors match the fixture's scene colors

**Verification:**
- [x] Tests pass: `cargo test -p inspect`

**Dependencies:** 8

**Files likely touched:** `crates/inspect/src/{look.rs,safezone.rs}`, `crates/model/src/inspect.rs`

**Estimated scope:** S

## Task 11: overlay events ✅ done

**Description:** Regions that change against the estimated camera transform
and are not text: frame-difference after warping by the shot transform,
threshold, connected components, track across frames into overlay events with
anchor, size, in/out. Kind is left to judgement. Allowed to be conservative.

**Acceptance criteria:**
- [x] A synthetic sequence with a 0.12×0.07 patch appearing at f12 and leaving at f54 on a static background yields one event with box within 0.02
- [x] The same patch on a zooming background (task 9 transform applied) still yields one event, not a background false positive
- [x] No overlay events on `cuts-3.mp4` apart from the captions (which are excluded as text)

**Verification:**
- [x] Tests pass: `cargo test -p inspect` and `-- --ignored`

**Dependencies:** 9, 10

**Files likely touched:** `crates/inspect/src/overlays.rs`, `crates/model/src/inspect.rs`

**Estimated scope:** M

## Checkpoint: Measurement
- [ ] All Phase 2 ignored tests pass on the fixtures
- [ ] Detection time per frame recorded; under 30 ms or fallback decided
- [ ] Run on one real viral clip supplied by the human; estimators judged
- [ ] Review with human before proceeding

## Phase 3: Outputs

## Task 12: EDIT-SPEC.md writer

**Description:** `crates/editspec` renders the Markdown from probe, shots,
audio and inspect JSON, in the exact section and field layout of `SPEC.md`,
with `unknown` for unmeasured values and `<!-- judgement -->` slots holding
`TODO:`. Snapshot tests with `insta` from fixed JSON inputs.

**Acceptance criteria:**
- [ ] Snapshot of the spec for the `cuts-3` JSON matches and contains every section header from `SPEC.md` in order
- [ ] No field is empty: every value is a number, text, `unknown` or `TODO:`
- [ ] Times render as `1.800s (f54)` and positions as `an8 (0.50, 0.14)`

**Verification:**
- [ ] Tests pass: `cargo test -p editspec`
- [ ] Manual check: open `out/cuts-3/EDIT-SPEC.md`, read it as an editor would

**Dependencies:** 7, 11

**Files likely touched:** `crates/editspec/{Cargo.toml,src/lib.rs,src/render.rs,src/snapshots/*}`

**Estimated scope:** M

## Task 13: captions.vtt and captions.ass writers

**Description:** `crates/exports` writes WebVTT with inline word timestamps and
`line`/`position`/`align` cue settings from the anchor, and ASS with one style
per caption style, `PlayResX/Y` at source size, `\an`, `\pos`, `\fad` or
`\t(\fscx\fscy)` for animations, and `\k` per word when word timing exists.

**Acceptance criteria:**
- [ ] Snapshot tests for both files from the `cuts-3` JSON
- [ ] `captions.vtt` parses with a strict WebVTT parser (a small in-crate validator covering the grammar we emit)
- [ ] Ignored test: `ffmpeg -vf ass=captions.ass` onto `cuts-3.mp4`, OCR the result, each word's box within 0.02 of `inspect.json`

**Verification:**
- [ ] Tests pass: `cargo test -p exports` and `-- --ignored`

**Dependencies:** 8

**Files likely touched:** `crates/exports/{Cargo.toml,src/lib.rs,src/vtt.rs,src/ass.rs}`

**Estimated scope:** M

## Task 14: timeline.otio writer

**Description:** Serde structs for the OTIO 0.17 schemas used (Timeline.1,
Stack.1, Track.1, Clip.2, Gap.1, Transition.1, LinearTimeWarp.1,
FreezeFrame.1, Marker.2, ExternalReference.1, GeneratorReference.1,
RationalTime.1, TimeRange.1), the track layout from `SPEC.md` "OTIO layout",
`metadata.viralcraft` on every object, markers on the track for spans that
cross shots.

**Acceptance criteria:**
- [ ] Snapshot of `timeline.otio` for `cuts-3` JSON
- [ ] A transition at f54 with 8 frames split 4/4 serializes as `in_offset` 4 and `out_offset` 4 at the source rate
- [ ] Ignored test: `python3 -c "import opentimelineio"` round-trip (read, write, read) preserves clip count, source ranges, transition offsets and marker ranges

**Verification:**
- [ ] Tests pass: `cargo test -p exports` and `-- --ignored` (skips with a message if the Python package is absent)
- [ ] Manual check at the checkpoint: import into FilmCraft

**Dependencies:** 7, 11

**Files likely touched:** `crates/exports/src/{otio.rs,otio_schema.rs}`

**Estimated scope:** M

## Task 15: judgement.json and finalize merge

**Description:** `judgement.json` type in `model` with the `required` set.
`finalize` validates it, fills every `TODO:` in `EDIT-SPEC.md`, applies
`overrides`, writes the judgement into OTIO metadata, and fails with a list of
missing required fields otherwise.

**Acceptance criteria:**
- [ ] A judgement missing `format_name` is rejected with that field named
- [ ] After a complete judgement, the rendered spec has zero `TODO:` and the `shots[2].transform` override shows `unknown` with the reason in a comment
- [ ] The OTIO clip for shot 1 has `metadata.viralcraft.role == "a-roll"`

**Verification:**
- [ ] Tests pass: `cargo test -p editspec -p exports -p model`

**Dependencies:** 12, 14

**Files likely touched:** `crates/model/src/judgement.rs`, `crates/editspec/src/merge.rs`, `crates/exports/src/otio.rs`, `apps/viralcraft/src/main.rs`

**Estimated scope:** M

## Task 16: TEMPLATE.md scaffold

**Description:** `crates/template` writes the header, the slot table (one row
per story beat with from/to/kind from shots and judgement), and "Observed
timings" (cut frames, beat grid, caption durations, transform ranges). Rules
section is a `TODO:` for the agent.

**Acceptance criteria:**
- [ ] Snapshot for `cuts-3` with a complete judgement
- [ ] Slot rows equal story beats; each has frame-accurate from/to
- [ ] The Rules section contains exactly one `TODO:` line addressed to the agent

**Verification:**
- [ ] Tests pass: `cargo test -p template`

**Dependencies:** 15

**Files likely touched:** `crates/template/{Cargo.toml,src/lib.rs}`

**Estimated scope:** S

## Task 17: analyze, finalize, clean end to end

**Description:** Wire `viralcraft analyze` (doctor preflight, ingest, shots,
audio, inspect, editspec, exports without judgement), `viralcraft finalize`
(merge, exports with metadata, template), `viralcraft clean`. One JSON summary
line per command.

**Acceptance criteria:**
- [ ] Ignored end-to-end test on `cuts-3.mp4`: every file in the run directory exists, `EDIT-SPEC.md` has no empty field, summary line lists every path
- [ ] `analyze` with a missing model exits 1 with the doctor report and creates no run directory
- [ ] `clean` removes `frames/` and `source.cfr.mp4` and nothing else

**Verification:**
- [ ] Tests pass: `cargo test --workspace -- --ignored`
- [ ] Manual check: full run on the real clip from the Phase 2 checkpoint

**Dependencies:** 16, 13

**Files likely touched:** `apps/viralcraft/src/{main.rs,pipeline.rs}`, `tests/e2e.rs`

**Estimated scope:** M

## Checkpoint: Outputs
- [ ] Full pipeline on `cuts-3.mp4` and on the real clip
- [ ] OTIO round-trips in Python and imports into FilmCraft
- [ ] ASS burn-in check passes
- [ ] Review with human before proceeding

## Phase 4: Skill and docs

## Task 18: the agent skill

**Description:** `skill/viralcraft/SKILL.md` with the run procedure (doctor,
analyze, read `inspect.json` and frames, fill `judgement.json` slot by slot,
finalize, write Rules), plus `references/hook-taxonomy.md`,
`references/look-vocabulary.md` and a worked example `judgement.json`.
Installable by copying into `.claude/skills/` and `.agents/skills/`.

**Acceptance criteria:**
- [ ] SKILL.md frontmatter has `name` and a `description` under 1024 chars that triggers on "analyze this viral video" and "copy this format"
- [ ] Every required judgement field has a step telling the agent what to look at and how to decide
- [ ] A fresh Claude Code session with the skill installed completes a run on `cuts-3.mp4` with no `TODO:` left (manual)

**Verification:**
- [ ] Manual check: the fresh-session run above

**Dependencies:** 17

**Files likely touched:** `skill/viralcraft/{SKILL.md,references/*.md,examples/judgement.json}`

**Estimated scope:** M

## Task 19: ADRs and README

**Description:** ADRs for Rust, agent-side judgement, ffmpeg-only, and
standards-based formats; README with install, doctor, analyze, finalize.

**Acceptance criteria:**
- [ ] Four ADRs in `docs/adr/` following the documentation-and-adrs template
- [ ] README commands match `SPEC.md` "Commands" exactly

**Verification:**
- [ ] Manual check: a reader can go from clone to first run using the README alone

**Dependencies:** 17

**Files likely touched:** `docs/adr/000{1..4}-*.md`, `README.md`

**Estimated scope:** S

## Task 20: blind-rebuild run

**Description:** With the human: pick one real viral video, run the pipeline,
complete the judgement, hand `EDIT-SPEC.md` to an editor (or rebuild it
ourselves in FilmCraft from new footage), compare side by side, record the
result and every field that was missing or wrong in `docs/blind-rebuild-01.md`.

**Acceptance criteria:**
- [ ] `docs/blind-rebuild-01.md` exists with the verdict and the gap list
- [ ] Each gap is filed as an open question in `SPEC.md` or fixed

**Verification:**
- [ ] Manual check: the human agrees the side-by-side reads as the same format, or the gaps are recorded

**Dependencies:** 18, 19

**Files likely touched:** `docs/blind-rebuild-01.md`, `SPEC.md`

**Estimated scope:** S

## Checkpoint: Complete
- [ ] Skill runs end to end in a fresh session
- [ ] Blind rebuild recorded
- [ ] `cargo test --workspace` under 2 s; ignored suite under 60 s
