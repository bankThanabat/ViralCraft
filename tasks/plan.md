# Implementation Plan: ViralCraft v1

Source: `SPEC.md`, `CAPABILITY-MAP.md`, `docs/intent/viralcraft-v1.md`.
Tasks: `tasks/todo.md`.

## Overview

A Rust workspace that turns a short-form video into an edit spec. Twenty tasks
in four phases. Each task lands one tested, committed slice. The three risky
measurements (shot detection, beat tracking, motion estimation) come early so
they fail early.

The project is not a git repository yet. Task 1 runs `git init`; every task
after that ends in one commit.

## Architecture decisions

Already recorded in `SPEC.md`, "Decisions". The ones that shape the task order:

- Measurement in the CLI, judgement in the agent. Nothing in the CLI calls a
  model API.
- ffmpeg and ffprobe are the only external tools. Everything else is a crate.
- Every stage reads and writes exact files in the run directory; the `model`
  crate owns every JSON type, so it is built first.
- Timing, captions, transitions, transforms and beats use the OTIO, ASS,
  Lottie and JAMS vocabularies.

Crates the plan adds (listed once here so approving the plan approves them):
`clap`, `serde`, `serde_json`, `anyhow`, `thiserror`, `tracing`,
`tracing-subscriber`, `image`, `ocrs 0.13` + `rten 0.26`, `whisper-rs 0.16`,
`hound`, `rustfft` (onset envelope), `sha2` (model check; downloads run the
`curl` command the user approves), `which`, `tempfile`, `insta` (snapshots). `whisper-rs` and `rten`
compile native code; both were built on this machine during verification.

## Dependency graph

```
model ──┬── doctor ── ingest ──┬── shots ──┬── inspect(captions)
        │                      │           ├── inspect(motion)
        │                      │           ├── inspect(look, safe zone)
        │                      │           └── inspect(overlays)
        │                      └── audio(words) ── audio(beats)
        │
        └── fixtures ─────────────────────────── (used by every integration test)

shots + audio + inspect ── editspec ── finalize ──┬── exports(vtt, ass)
                                                  ├── exports(otio)
                                                  └── template
analyze/finalize/clean wiring ── skill ── docs ── blind rebuild
```

## Task list

### Phase 1: Foundation (tasks 1 to 4)

Workspace, shared types, `doctor`, `ingest`, and the committed fixtures that
every later integration test needs.

### Checkpoint: Foundation

- `cargo test --workspace` green in under 2 s
- `viralcraft doctor` reports every tool and model on this machine
- `viralcraft ingest` turns `cuts-3-vfr.mp4` into a CFR file with a correct
  `probe.json`
- Review with human

### Phase 2: Measurement (tasks 5 to 11)

Shots, words, beats, captions, motion, look, overlays. Each writes its JSON
file and has a unit test on parsed strings plus an ignored integration test on
a fixture.

### Checkpoint: Measurement

- `cuts-3.mp4`: 3 shots at f60/f120 ±1, captions ONE/TWO/THREE with boxes
  within 0.05, beats at 0.5 s ±2 frames, tempo 120 ±1
- `zoom-1.mp4`: one shot, scale 100→120 within 3, confidence above 0.6
- `speech.wav`: transcript word-for-word, word times within ±100 ms
- Review with human; this is where the risky estimators get judged

### Phase 3: Outputs (tasks 12 to 17)

Markdown spec, captions in VTT and ASS, OTIO, `judgement.json` merge,
template, and the `analyze` / `finalize` / `clean` wiring.

### Checkpoint: Outputs

- `viralcraft analyze tests/fixtures/cuts-3.mp4` produces every file in the run
  directory with no empty fields
- `timeline.otio` round-trips through the Python reference library
- `captions.ass` burns onto the fixture and OCRs back within 0.02
- Review with human

### Phase 4: Skills, rebuild and docs (tasks 18 to 23)

Replanned 2026-10-09 with the user. The core skill (analyze, review,
finalize) plus an optional editor step: `assets` reads the user's clips,
`prompt` writes REBUILD-PROMPT.md, and a FilmCraft skill follows it through
filmcraft-cli to build and export the video. Then ADRs and README, and the
blind-rebuild run with the user's own clips.

### Checkpoint: Complete

- Skill installs into `.claude/skills/` and `.agents/skills/` and walks an agent
  through a run end to end
- `docs/blind-rebuild-01.md` records the result
- No `TODO:` left in a finalized `EDIT-SPEC.md`

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| `scdet` misses jump cuts or fires on flashes | Task 5 exposes the threshold and keeps every candidate with its score in `shots.json`; the agent can promote or demote boundaries in `judgement.json` overrides. Tuned on real clips at the Phase 2 checkpoint. |
| Beat tracking on speech-over-music is wrong | Task 7 reports tempo with confidence; below 0.5 the beat grid is `unknown` and cuts are reported against onsets only. |
| Motion estimation without OpenCV | Task 9 uses a coarse-to-fine global similarity search (scale sweep plus phase correlation) on downsampled grayscale, background-weighted. If confidence is low on real clips, the field is `unknown` and the agent reads the frames. |
| Per-frame text detection too slow | Task 8 measures it; fallback is every 3rd frame with refinement, already in the spec. |
| Overlay detection noisy | Task 11 is last in Phase 2 and allowed to ship reporting only agent-marked overlays. |
| OTIO written by hand is subtly wrong | Task 14's ignored test round-trips through `opentimelineio` in Python and imports into FilmCraft at the checkpoint. |
| `whisper-rs` and `rten` build time | Both already compiled here; CI caches `target/`. |

## Open questions

Carried from `SPEC.md`: beat quality on real audio, per-frame detection cost,
overlay detection quality. All three get answered at the Phase 2 checkpoint,
not before.

## Parallelization

Within Phase 2, tasks 6 and 7 (audio) are independent of tasks 8 to 11
(inspect) and could run in a second session. Within Phase 3, tasks 13 and 14
(the two export writers) are independent of each other. Everything else is
sequential.
