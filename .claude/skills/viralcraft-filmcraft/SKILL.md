---
name: viralcraft-filmcraft
description: Builds a ViralCraft REBUILD-PROMPT.md in the FilmCraft video editor. Places the user's clips on the slots, adds the captions as styled editable text, places music and sound effects, then exports rebuild.mp4. Use after the viralcraft skill has written rebuild/REBUILD-PROMPT.md, or when the user asks to "rebuild this format in FilmCraft" or "edit my clips like this video in FilmCraft".
---

# ViralCraft → FilmCraft

Follow `<run_dir>/rebuild/REBUILD-PROMPT.md` step by step in FilmCraft. The
prompt says what to build; `references/filmcraft-commands.md` says which
FilmCraft command does each step, with the exact params. Read both before
starting.

## 0. Which FilmCraft

Prefer FilmCraft's **MCP server**: it lets you render frames and check your
own work as you go. If tools named `command_run`, `command_batch`,
`media_import`, `sequence_inspect` and `render_frame` are available, use them.
- Import with `media_import` and absolute paths.
- Run commands with `command_batch` (`{steps: [{id, params}]}`). Its
  command ids and params are the ones in the reference.
- Save with the `file.saveAs` command (`{"path": ...}`). Export with
  `file.exportMedia` (`{"path": ..., "format": "h264", "wait": true}`).
- If the user has the FilmCraft app open with a bridge, they can watch you
  build.

Without MCP tools, use the **CLI**, `filmcraft-cli`: `import`, then
`run <file.jsonl>` for batches of commands, `inspect`, `render`, `export`.

If neither is there, stop and tell the user how to set it up:
- get FilmCraft (it includes `filmcraft-cli`):
  https://getartcraft.com/apps/filmcraft
- for MCP in Claude Code:
  `claude mcp add filmcraft -- filmcraft-cli mcp`
  (add `--bridge 127.0.0.1:<port>` to drive the open app)

## 1. Before building

- `REBUILD-PROMPT.md` must exist. If not, run the `viralcraft` skill's
  rebuild step first (`viralcraft assets`, then `viralcraft prompt`).
- If the prompt says caption text must be drafted, or written from the
  clips' speech, write the lines.
- Choose a clip for every slot and shot. Look at each clip's thumbnail
  (`rebuild/thumbs/*.jpg`) and match the slot's "fill with". Pick the part
  of the clip to use (`sourceIn`): the moment that matches, not always the
  start.
- Show the user one plan: the caption lines, the clip for each slot, and
  the music and effects picked for each slot with why. Wait for one OK,
  and change what they ask before building.

## 2. Build

Work in `<run_dir>/rebuild/`. Never write to the user's clips.

1. Import every clip the prompt lists (skip ones marked "skip it").
2. Create the sequence at the prompt's size and rate.
3. Place each shot's clip on V1 at the shot's frames, trimmed in ticks.
   Fill the frame on every clip whose size differs.
4. Add each caption as a text graphic on V3 with its font, size, fill,
   outline and box. Then centre it on the prompt's pixel position, using
   the place, measure and move steps in the reference.
5. Add transitions only where the prompt lists them.
6. Place music and sound effects on A2 and A3 at the prompt's frames. A
   voiceover's line files go on their own audio track, each at its frame,
   with the clips' own sound lowered under it (`clip.audioGain`).
7. Save to `rebuild/rebuild.fcproj`.

Batch commands: one `command_batch`, or one `run` file, per step. After each
batch, read the results. A failed command reports `ok: false` with an
error. Fix it before going on.

## 3. Check

- Inspect the sequence: the cut frames must match the prompt within 1 frame,
  and nothing may be empty.
- Render the middle frame of every caption. Each caption must be readable,
  in place and in style. Compare a few frames with the original's
  `<run_dir>/frames/fNNNNN.jpg` at the same frame.
- Fix what is off, then check again.

## 4. Export

Export the whole sequence to `<run_dir>/rebuild/rebuild.mp4` (h264). Check
it with ffprobe: same size, rate and frame count as the prompt. Then tell the
user where `rebuild.mp4` and `rebuild.fcproj` are. Ask them to watch it next
to the original and say what feels different.
