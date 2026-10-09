# ViralCraft

Drop in a viral short video. Get back an edit spec precise enough to rebuild
it blind: every cut, caption (text, font, size, colour, position, timing),
camera move, sound effect, music bed and story beat, as numbers. Also get a
reusable template of the format, and optionally a rebuild of that format
with your own clips.

The `viralcraft` CLI measures. An AI agent (Claude Code, Codex, Cursor)
running the `viralcraft` skill looks at the frames and adds the judgement:
what each shot is for, why the hook works, which text is a caption and which
is scene text. See `docs/adr/` for why it is split this way.

## Install

Needs Rust 1.80+ and ffmpeg.

```
cargo install --path apps/viralcraft
viralcraft doctor
```

`doctor` checks ffmpeg, ffprobe and the model files (speech, text
detection and recognition). `viralcraft doctor --install` offers to install
or download each missing item and asks first. It also offers two optional
features: sound-effect names (16 MB), and speech in other languages
(choose small 0.5 GB, medium 1.5 GB or large 3.1 GB).

Install the skills by copying them into your agent's skills folder, e.g.
for Claude Code in this project:

```
mkdir -p .claude/skills
cp -r skill/viralcraft skill/viralcraft-filmcraft .claude/skills/
```

## Use it through the agent

Put the video in `./inbox/` and ask your agent "analyze this viral video".
The skill runs the steps below, reviews the frames and finishes the spec.

## The steps

```
viralcraft doctor [--install]           # check tools and models; --install asks per item
viralcraft analyze [FILE] [--inbox DIR] [--out DIR] [--safe-zone PROFILE] [--threshold N]
                                        # full pipeline; FILE defaults to the
                                        # single file in --inbox, else asks
viralcraft finalize <RUN_DIR>           # after the agent fills judgement.json:
                                        # merge into EDIT-SPEC.md, write OTIO
                                        # metadata, captions.ass/.vtt, TEMPLATE.md
viralcraft assets <RUN_DIR> [--assets DIR]   # optional rebuild: read the user's clips
viralcraft prompt <RUN_DIR> [--script FILE] [--no-captions]
                                        # optional rebuild: REBUILD-PROMPT.md
viralcraft voiceover <RUN_DIR> --script FILE [--engine say|openai|elevenlabs|minimax|qwen]
                                [--voice V] [--model M]
                                        # optional rebuild: AI voice per caption
viralcraft clean <RUN_DIR>              # delete frames/ and source.cfr.mp4
viralcraft ingest [FILE] [--inbox DIR] [--out DIR] [--safe-zone PROFILE]
viralcraft shots <RUN_DIR> [--threshold N]   # run one stage on an existing run dir;
viralcraft audio <RUN_DIR>              # same for inspect, editspec, exports
```

Every command prints one JSON line on stdout, also on failure
(`{"command": ..., "ok": false, "error": ...}`), and its progress on stderr.

1. **Analyze.** `viralcraft analyze` writes `out/<video name>/` with the
   measurements, every frame, and a draft `EDIT-SPEC.md` whose judgement
   slots say `TODO:`.
2. **Review.** The agent looks at the frames and writes `judgement.json`
   (the skill's `references/judgement-guide.md` covers each field).
3. **Finalize.** `viralcraft finalize out/<video name>` checks the judgement,
   lists anything missing, and writes the finished `EDIT-SPEC.md`,
   `captions.vtt`, `captions.ass`, `timeline.otio` and a draft
   `TEMPLATE.md`. The agent then fills the template's "fill with" column and
   rules.

## Optional: rebuild with your clips

Put your clips in `./assets/`. They are only read, never changed.

1. `viralcraft assets out/<video name>` probes each clip, transcribes any
   speech, names sounds, and saves a thumbnail strip, all in
   `out/<video name>/rebuild/`.
2. `viralcraft prompt out/<video name> [--script lines.txt]` writes
   `rebuild/REBUILD-PROMPT.md`: steps any editor agent can follow. Caption
   text comes from your script (one line per caption), else from the speech
   in your clips, else the agent drafts lines for you to approve.
   If your clips have no talking and you have no script, you choose: an
   AI voiceover (the agent writes the lines, you approve them, and
   `viralcraft voiceover` reads them aloud with the Mac's own voices for
   free, or with OpenAI, ElevenLabs, MiniMax or Qwen using your own key
   in `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `MINIMAX_API_KEY` or
   `DASHSCOPE_API_KEY`), or no captions (`--no-captions`).
3. With FilmCraft, the `viralcraft-filmcraft` skill builds it: clips cut to
   the slots, captions as styled editable text, music and sounds placed,
   exported to `rebuild/rebuild.mp4`. Connect FilmCraft's MCP server for
   the best results (`claude mcp add filmcraft -- filmcraft-cli mcp`); the
   skill falls back to `filmcraft-cli`.

## Output

| file | what |
|------|------|
| `EDIT-SPEC.md` | the full spec: source, hook, shots, captions, overlays, transitions, sound, look, story beats |
| `TEMPLATE.md` | the format without the content: slots, timings, rules |
| `timeline.otio` | OpenTimelineIO timeline, opens in editors that read OTIO |
| `captions.vtt`, `captions.ass` | captions with word times (VTT) and style and position (ASS) |
| `*.json` | the raw measurements, in frames at the video's rate |

## Develop

```
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test --workspace -- --ignored      # needs ffmpeg and the models
```

Some ignored tests need more: `FILMCRAFT_CLI=/path/to/filmcraft-cli` for the
FilmCraft tests, `OTIO_PYTHON=/path/to/python` (with opentimelineio) for the
OTIO round trip. `SPEC.md` is the full specification.
