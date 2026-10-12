# ViralCraft

Copy the edit of a viral short video onto your own clips.

Give ViralCraft a TikTok, Reel or Short that works. It measures every cut,
caption, camera move, sound effect and music cue, then rebuilds the same
format from your clips, with your words and your music. You talk to an AI
agent (Claude Code or Codex) in plain words; the agent runs ViralCraft for
you.

## Copy a video with your clips

You need [Node.js](https://nodejs.org) 18 or newer and
[git](https://git-scm.com/downloads). Then, in a terminal:

```
git clone https://github.com/bankThanabat/ViralCraft
cd ViralCraft
npm install
npm run setup
```

`npm install` fetches the `viralcraft` program. `npm run setup` is a wizard
that installs the rest: ffmpeg, which reads video, and 90 MB of speech and
text models. It asks before it installs anything, and you can run it again
at any time.

1. Put the viral video in the `inbox/` folder.
2. Put your clips in the `assets/` folder. Add any music or sound effects
   you want to use there too.
3. Open [Claude Code](https://claude.com/claude-code) or Codex in the
   `ViralCraft` folder and say:

   > copy this video with my clips

The agent analyzes the video, looks at its frames, and picks a clip of yours
for every shot and a sound for every cue. It shows you the plan (captions,
clips, music) and waits for your OK. Then it builds the video in
[FilmCraft](https://getartcraft.com/apps/filmcraft), a free video editor,
and exports `rebuild.mp4`. Without FilmCraft you still get the full edit
plan, which any editor or editing agent can follow.

Your files in `inbox/` and `assets/` are only read, never changed.
Everything ViralCraft writes goes to `out/<video name>/`.

Need music? Pixabay Music and Sound Effects, Freesound (pick CC0 sounds) and
the YouTube Audio Library are free. Sounds from TikTok's or CapCut's own
libraries are licensed for use inside those apps only.

## What you get

Even without a rebuild, every analysis leaves these files in
`out/<video name>/`:

| File | What it is |
|------|------------|
| `EDIT-SPEC.md` | The whole edit as numbers: hook, shots, captions (text, font, size, colour, position, timing), overlays, transitions, sound, look, story beats. |
| `TEMPLATE.md` | The format without the content: slots, timings, and the rules an editor must keep. |
| `timeline.otio` | An OpenTimelineIO timeline for editors that open OTIO. |
| `captions.vtt`, `captions.ass` | The captions with word times (VTT), and with style and position (ASS). |
| `rebuild/REBUILD-PROMPT.md` | Step-by-step rebuild instructions with your clips. |
| `rebuild/rebuild.mp4` | The finished video, when FilmCraft built it. |

The analysis is split in two. The `viralcraft` program measures what can be
measured. The agent looks at the frames and adds the judgement: what each
shot is for, why the hook works, which text is a caption and which is a
sign in the scene. `docs/adr/` explains why.

## Commands

The agent runs these for you. To run one yourself, use `npx viralcraft` in
the `ViralCraft` folder.

```
viralcraft setup                        # wizard: install ffmpeg and the models, asking first
viralcraft doctor                       # check tools and models, change nothing
viralcraft analyze [FILE]               # measure the video, write a draft EDIT-SPEC.md
viralcraft finalize <RUN_DIR>           # merge the agent's judgement.json, write the spec,
                                        # captions, timeline and TEMPLATE.md
viralcraft assets <RUN_DIR> [--sounds DIR]...
                                        # read your clips and sounds for the rebuild
viralcraft prompt <RUN_DIR> [--script FILE] [--no-captions]
                                        # write rebuild/REBUILD-PROMPT.md
viralcraft voiceover <RUN_DIR> --script FILE [--engine ENGINE]
                                        # an AI voice reads each caption line
viralcraft clean <RUN_DIR>              # delete the large frames/ and source.cfr.mp4
```

`analyze` takes the only video in `inbox/` when you leave out `FILE`.
`viralcraft <command> --help` lists every option.

Every command prints exactly one JSON line on stdout, also on failure
(`{"command": ..., "ok": false, "error": ...}`), and its progress on stderr.
`setup` leaves the JSON line out when its output goes to a terminal.

**Voiceover.** When your clips have no talking and you have no script, the
agent can write lines for you to approve and read them aloud. The `say`
engine is free and uses your Mac's own voices. `openai`, `elevenlabs`,
`minimax`, `qwen` and `bytedance` sound natural and use your own key in
`OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `MINIMAX_API_KEY`,
`DASHSCOPE_API_KEY` or `VOLCENGINE_API_KEY`. When a key is missing, the
command asks for it in your terminal with hidden typing and never saves it.
If you sign in to ElevenLabs' own CLI (`elevenlabs auth login`),
`--engine elevenlabs` needs no key.

**Other languages.** The default speech model is English only. For Thai,
Japanese, Spanish and other languages, choose a multilingual model in
`npm run setup`: small (0.5 GB), medium (1.5 GB) or large (3.1 GB).

## Develop

ViralCraft is a Rust workspace: the CLI in `apps/viralcraft`, one crate per
stage in `crates/`. You need Rust 1.80+ and ffmpeg.

```
npm run build                            # cargo build --release, into bin/native/
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test --workspace -- --ignored      # needs ffmpeg and the models
```

Some ignored tests need more: `FILMCRAFT_CLI=/path/to/filmcraft-cli` for the
FilmCraft tests, and `OTIO_PYTHON=/path/to/python` (with opentimelineio) for
the OTIO round trip.

`npm install` downloads a prebuilt binary from this repo's GitHub release
for the version in `package.json`. When there is none for your platform, it
builds from source if Rust is installed. Pushing a tag `vX.Y.Z` runs
`.github/workflows/release.yml`, which builds macOS, Linux and Windows
binaries, uploads them, and checks `npm install` against them.

The agent skills live in `.claude/skills/`; `npm install` links them into
`.agents/skills/` for Codex. `SPEC.md` is the full specification,
`CAPABILITY-MAP.md` the module map, and `docs/adr/` the decisions.

## License

Apache-2.0. See `LICENSE`.
