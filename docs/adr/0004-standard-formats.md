# ADR-0004: Write standard formats, not our own

## Status
Accepted

## Date
2026-10-09

## Context
The spec has to be read by people, by agents and by editors. There is no
open standard for an "edit spec", but each part of one has a standard.

## Decision
- Time is OpenTimelineIO RationalTime: a frame count at a rational rate,
  ranges end-exclusive. Frames are the source of truth.
- `timeline.otio` is the timeline, with placeholders for captions and
  overlays disabled so editors show them without rendering them.
- Captions are WebVTT (with word times) and ASS (with style and position).
- Camera moves are Lottie-style keyframes; beats, tempo and onsets follow
  the JAMS namespaces.
- EDIT-SPEC.md and TEMPLATE.md are Markdown for people and agents.

## Alternatives considered
- **One custom JSON format**: simpler to write, but every editor would need
  an importer.

## Consequences
- FilmCraft imports `timeline.otio` directly; other editors that read OTIO
  should too. OTIO has no picture-size field, so the timeline also carries
  FilmCraft's sequence settings under its own metadata key.
- The Python OTIO library reads our files (checked in an ignored test).
