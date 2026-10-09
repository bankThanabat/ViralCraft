# Capability Map: ViralCraft v1

Approved module split for v1. Module ids are stable; specs, plans and tasks refer
to work by these ids. Each module is a crate in one Cargo workspace.

| Module id | Responsibility | Depends on |
|---|---|---|
| `doctor` | Check for ffmpeg, ffprobe, the whisper model and both ocrs model files (with SHA-256). Report what is missing with install or download commands; `--install` asks per item. | — |
| `ingest` | Resolve the input file (argument, or the drop folder, or ask). Normalize to constant frame rate. Probe: rational rate, frame count, resolution, aspect, audio offset. Attach the safe-zone profile. | doctor |
| `shots` | Shot-change detection (ffmpeg `scdet` candidates, then a verified threshold policy) and transition candidates (dissolve, flash). Extract every frame at 720 px on the short side (never upscaled). | ingest |
| `audio` | Speech transcription with DTW word timestamps (whisper-rs adapter), beat/tempo/onset detection in JAMS form, music and speech spans. No on-camera judgement. | ingest |
| `inspect` | Per-frame text-region detection into caption events with ASS-style attributes and animation; overlay events; per-shot similarity transform as Lottie keyframes with confidence; look statistics; safe-zone clearance. | shots |
| `editspec` | Assemble the Markdown edit spec (shots, captions, overlays, transitions, sound, look) from the measurements, with marked judgement slots and the required-field list. | shots, audio, inspect |
| `exports` | Write the OTIO timeline (FilmCraft-compatible layout, `viralcraft` metadata), `captions.ass` and `captions.vtt`. | shots, audio, inspect, judgement |
| `template` | Emit the slot table and observed timings; the agent writes the rules. | shots, audio, inspect, judgement |
| `assets` | The user's clips for an optional rebuild: find the folder (ask if missing), probe, transcribe, name sounds, one thumbnail each. | ingest, audio |
| `prompt` | REBUILD-PROMPT.md: the finished template as editor-agnostic steps, with caption text from the user's script or the assets' transcripts. | editspec, template, assets |
| `skill` | Core SKILL.md for Claude Code, Codex and Cursor: analyze, review the frames and fill judgement.json, finalize, complete the template. | all core modules |
| `skill-filmcraft` | Optional editor SKILL.md: follow REBUILD-PROMPT.md through filmcraft-cli to build and export the video from the user's clips. Other editors later as sibling skills. | prompt |

Build order: doctor → ingest → shots, audio → inspect → editspec → exports, template → assets → prompt → skill → skill-filmcraft

`exports` and `template` depend on the measurement files and on the agent's
`judgement.json`, not on `editspec`; `finalize` runs them after the agent is
done.

Interfaces live at the boundary: each module reads and writes the exact files
listed in SPEC.md, "Run directory", and nothing else.
