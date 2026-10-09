# ViralCraft v1: statement of intent

Confirmed 2026-10-09 after an interview-me session.

## Outcome

A CLI and agent skill that turns a dropped viral video into an edit spec detailed
enough that an editor who never saw the original could rebuild the same format from
the spec alone. The tool then generalizes that spec into a reusable template with
slots for anyone's story and assets.

Think of the output as a PRD or technical spec, but written for an editor.

## User

Short-form creators and their editors who already run an agent harness
(Claude Code, Codex, Cursor). The project owner is user zero.

## Why now

Agents can watch frames, detect cuts and drive editors over MCP, so the recipe no
longer has to be rebuilt by hand. The ArtCraft Craft apps (FilmCraft, EffectCraft,
PhotoCraft) expose CLI, JSON control channel and MCP interfaces, which gives a later
rebuild step an open target.

## Success

The blind-rebuild test. Hand the Markdown spec and new footage to an editor who has
not seen the original; the side-by-side reads as the same format.

Every segment carries numbers, not adjectives: timecodes in and out, caption text
with font, size, color and position, zoom amounts and durations, color values.

## Analysis stages

1. **Frame-by-frame timeline.** Shot-change detection plus frame inspection
   produces a segment list. Each segment records what is on screen, A-roll vs
   B-roll, hook, story beat, captions and their position, text animation,
   framing and crop, camera motion, speed ramps, zooms, transitions, effects,
   stickers and emoji, color grade, sound (music, beat sync, SFX, voiceover vs
   on-screen speech), and safe-zone placement.
2. **Generalization.** The timeline becomes a reusable template: roles, slots
   and timing rules that a user can fill with their own story and assets.

## Output

Per analysis, three files:

- A Markdown edit spec: timeline, hook, A-roll/B-roll, story beats, captions,
  effects, transitions, grade, sound, plus the generalized template. This is the
  file that passes the blind-rebuild test.
- An OpenTimelineIO (OTIO) timeline, with `viralcraft` metadata on each clip and
  marker (role, hook type, beat index, grade notes). FilmCraft, DaVinci and
  Premiere can open it.
- WebVTT captions.

## Input

A video file from a known drop folder. If the folder is empty or holds more than
one candidate, the skill asks for the path instead of guessing.

Video first. Images come later.

## Setup

On first run the skill checks for required tools (ffmpeg, a shot detector, the
OTIO library, and so on), lists anything missing with the install command, and
asks before installing. It never installs silently.

## Constraint

v1 is analysis only. No rendering, no accounts, no web UI, no link downloading.

## Out of scope

- Driving the Craft apps to produce a finished video
- TikTok, Reels or Shorts link ingestion
- Image analysis
- Any hosted service
