---
name: viralcraft
description: Turns a viral short-form video (TikTok, Reels, Shorts) into a blind-rebuild-quality edit spec and a reusable format template, then optionally rebuilds that format from the user's own clips. Use when the user says "analyze this viral video", "copy this format", "break down this edit", "what makes this video work", "make a template from this video", or drops a video in ./inbox/.
---

# ViralCraft

The `viralcraft` CLI measures everything that can be measured: cuts, captions
(text, style, position, timing), camera moves, look, words, music, sounds.
You add the judgement it cannot make: what each shot is, what kind each caption
is, the hook, the story, the look's name. Then the CLI writes the finished
EDIT-SPEC.md, captions, timeline and TEMPLATE.md.

Every `viralcraft` command prints exactly one JSON line on stdout
(`{"command": ..., "ok": ...}`), also on failure. Read that line; progress goes
to stderr. Never edit measured files (`*.json` other than `judgement.json`).

## 0. The binary

Run `viralcraft --version`. If it is missing and this is the ViralCraft repo,
build it with `cargo install --path apps/viralcraft`. Otherwise ask the user
where it is installed.

## 1. Tools and models

Run `viralcraft doctor`. When `ok` is false, show the user each missing item
and its install command from `items[].fix`, and run the commands only after
they say yes. Never install without asking.

Optional items (`optional: true`) are features, not failures. Offer them once:
- sound-effect names (YAMNet, 16 MB)
- multilingual speech, for videos not in English: offer the three sizes as
  one choice, small (0.5 GB, good), medium (1.5 GB, better) or large
  (3.1 GB, best), and download only the one the user picks. Without one,
  non-English speech comes out as wrong English words.

## 2. Analyze

Run `viralcraft analyze`. It takes the only video in `./inbox/`. If the JSON
error says there is no video or more than one, ask the user which file and run
`viralcraft analyze <file>`. Results land in `./out/<video name>/`
(`run_dir` in the JSON). Long videos take minutes; let it finish.

## 3. Review: write judgement.json

Read `<run_dir>/EDIT-SPEC.md`. Every `TODO:` is yours, and its
`## Judgement checklist` lists them all. Look at frames before deciding:
`<run_dir>/frames/f00042.jpg` is frame 42 (720 px on the short side, or the
video's own size when it is smaller). Look at
the middle frame of each shot, the middle frame of each caption, and every
frame of the hook window. Write `<run_dir>/judgement.json`.

See `references/judgement-guide.md` for each field: what to look at and how
to decide. `examples/judgement.json` is a finished example from a real clip.

Rules that matter most:
- **Caption text.** OCR reads the Latin alphabet only. For Thai, Japanese,
  Chinese, Korean, Arabic and similar scripts it returns confident garbage
  ("onIGS" for "แจกสูตร"). Read the real words from the frame and put them in
  `text_fix`, line breaks as `\n`. Also fix Latin OCR typos the same way.
  Never take caption text from the transcript: many videos have no speech,
  and creators spell on purpose.
- **Scene text** (signs, packaging, logos in the shot) is `scene-text`; it is
  removed from the spec. The creator's handle is `watermark`.
- **One caption, many ids.** Something moving behind a caption (sparkles, a
  hand, a whisk) can split it into several ids. Judge the first one fully
  and give every other piece only `"same_as": "<first id>"`; finalize joins
  their frames into one caption. A caption that types or writes on is one
  caption too: its partial pieces are `same_as` the finished one.
- Every caption that is not `scene-text` needs a `font_substitute`: a common
  font that looks closest (weight, width, italic, rounded or not).
- Times are frame numbers at the video's rate, as in EDIT-SPEC.md.

## 4. Finalize

Run `viralcraft finalize <run_dir>`. If `ok` is false, `missing` lists the
fields still empty; fill them and run it again. If the error says the
measurements changed after judgement.json was written, the video was
analyzed again and the ids moved: review again from step 3 and rewrite the
judgement. When it passes, it writes the
finished EDIT-SPEC.md, captions.vtt, captions.ass, timeline.otio and a draft
TEMPLATE.md.

## 5. Template

Open `<run_dir>/TEMPLATE.md` and replace each `TODO`:
- **fill with**, per slot: what footage goes there, written so it works for
  any topic ("the finished product, close and centred", not "matcha latte").
- **Rules**: the patterns an editor must keep, drawn from the observed
  timings: cut rhythm, caption placement and length, what the hook does,
  when music enters, recurring sound effects. Say which are strict and which
  are loose.

Running `finalize` again keeps what you wrote in TEMPLATE.md and refreshes
the measured lines; a slot whose frames changed comes back as `TODO`.

Tell the user what the format is in two or three plain sentences and where
the files are.

## 6. Optional: rebuild with the user's clips

Ask whether they want this format rebuilt with their own footage. If yes:
1. If the original has music or named sound effects (EDIT-SPEC.md's Sound
   section), ask whether the user has music or effects to use. They can
   put the files in `./assets/` (a subfolder such as `assets/music/` or
   `assets/sounds/` is fine), or tell you a folder anywhere else. If they
   have none, suggest free sources: Pixabay Music and Sound Effects (no
   credit needed), Freesound (pick CC0 sounds), the YouTube Audio Library.
   Never take sounds out of TikTok's or CapCut's own libraries: their
   licences keep them inside those apps.
2. `viralcraft assets <run_dir> [--sounds <folder>]...` reads the clips in
   `./assets/` and every audio file there and in `--sounds` folders. If
   the JSON error says there are no clips, ask the user for the folder
   and pass `--assets <folder>`. Everything is only read, never changed.
   If a clip or sound has `duplicate_of`, tell the user it is a copy.
   Each sound gets a `role` guess (music, effect, voice): a guess, not a
   decision. When it finds no sounds it asks once, which you cannot answer
   for the user: if `sounds` is empty, ask the user in chat where their
   music and effects are, and run it again with `--sounds <folder>`, or
   go on without sounds if they have none.
3. Ask for a script: the caption lines they want, one per line. If they give
   one, save it as `<run_dir>/rebuild/script.txt` and pass `--script`.
4. No script, and no clip has speech (`words` is 0 for every clip in the
   assets JSON)? Then there is nothing to caption from. Ask the user to
   choose:
   - **AI voiceover.** Ask which voice. `say` is free: the computer's own
     voices, a bit robotic; Thai and English on a Mac. The cloud voices
     sound natural and use the user's own account and key, at a small cost
     per video: `openai` and `elevenlabs` (US), `minimax`, `qwen` and
     `bytedance` (China; `bytedance` is the Doubao voices behind CapCut).
     For Thai, `minimax`, `bytedance` and `elevenlabs` list it; `qwen`
     does not. The user puts the key in `OPENAI_API_KEY`,
     `ELEVENLABS_API_KEY`, `MINIMAX_API_KEY`, `DASHSCOPE_API_KEY` (Qwen,
     Alibaba Model Studio) or `VOLCENGINE_API_KEY` (ByteDance) in the
     shell that runs the CLI, or runs the voiceover command in their own
     terminal, where it asks for the key with hidden typing. For
     ElevenLabs there is an easier way: their own CLI
     (`brew install elevenlabs/tap/elevenlabs`, then the user runs
     `elevenlabs auth login`, which signs in through the browser). When it
     is signed in, `viralcraft voiceover --engine elevenlabs` uses it and
     needs no key. With it you can also list voices:
     `elevenlabs voices get_shared --language th --gender female`. If you run
     it and the key is missing, it fails with a message saying so: give
     the user the exact command to run in their terminal. Never ask them
     to paste a key into the chat. Draft one line per
     caption in the original's language, voice and length, short enough to
     say before the next caption. Show the lines and wait for approval.
     Save them as `<run_dir>/rebuild/script.txt` and run
     `viralcraft voiceover <run_dir> --script <file> --engine <engine>`
     (`--voice`, `--model` to change the defaults). If `too_long` lists
     captions, shorten those lines and run it again.
   - **No captions.** Pass `--no-captions` in the next step.
   Tell the user that platforms ask creators to label AI voices.
5. `viralcraft prompt <run_dir> [--script <file>] [--no-captions]` writes
   `<run_dir>/rebuild/REBUILD-PROMPT.md`. After a voiceover, `--script` can
   be left out; the voiceover's lines are the script.
6. **Choose the sound by the story.** The prompt's Sound section shows
   the original's music and effects slot by slot, then the user's sounds
   with what was measured. Pick music and effects for each slot by what
   that part of the story does (the hook grabs, the steps stay calm, the
   payoff lands), matching the original's energy, tempo and loudness, not
   by file names. Leave an effect out when nothing fits. Tell the user
   which file goes where and why, and wait for their OK.
7. Hand the prompt to an editor skill. With FilmCraft, use the
   `viralcraft-filmcraft` skill. Without an editor skill, give the user the
   prompt's path.

## 7. Clean up

`frames/` and `source.cfr.mp4` are large. Once the user is done, offer
`viralcraft clean <run_dir>`.
