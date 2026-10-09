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

## Amendment, 2026-10-09: optional rebuild step

After the first real clip went end to end, the user extended v1 with an
optional second step, keeping the first exactly as above:

1. **Core (always):** analyze, AI review, finalize. Produces the edit
   spec, template, captions and timeline.
2. **Optional editor step:** the finished template becomes a rebuild
   prompt that an agent follows to build the same format from the user's
   own clips in an editor. FilmCraft first; other editors can be added
   later as further editor steps reading the same prompt.

Decided with the user:
- The user's clips go in `./assets/`; the tool asks for a folder if it is
  missing or empty.
- In FilmCraft the agent builds a project with the clips cut to the slot
  timings, captions as editable text in the matching style, music and
  sound effects placed, and exports an MP4. The user does final touches.
- Caption text comes from the user's script if they give one; otherwise
  from a transcript of the speech in their own clips.
- The user's files are never modified; everything is written to new files.
- Users are not asked to install extra tools for checks (ffmpeg-full was
  dropped); FilmCraft is only needed for the optional step.
