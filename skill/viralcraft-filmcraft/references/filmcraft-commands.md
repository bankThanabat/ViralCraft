# REBUILD-PROMPT.md steps as FilmCraft commands

Verified against FilmCraft 0.4.0 (`filmcraft-cli`). Over MCP the same
command ids and params apply. Over the CLI, write the commands one JSON
object per line, `{"id": ..., "params": {...}}`, and run them with
`filmcraft-cli --project <p.fcproj> --save --compact run <file.jsonl>`.
Each line prints `{"ok": ..., "result": ...}`. Read the ids it returns.

## Time

FilmCraft counts **ticks**: 254016000000 per second. For a frame at rate
`num/den`: `ticks = frame × 254016000000 × den / num`. This is always a whole
number for the common rates. At 30 fps one frame is 8467200000 ticks; at
29.97 (30000/1001) it is 8475667200.

`timeline.place` takes `frame` for where the clip starts on the timeline,
but `sourceIn` and `duration` only take ticks. A string such as `"60f"` is
ignored without an error, and the clip runs to its end. Always pass ticks.

## 1. Sequence

Import the clips first. This creates the project:

```
filmcraft-cli --save-as rebuild/rebuild.fcproj --compact import <clip> <clip> ...
```

It prints `{"errors": [], "items": [1, 2, ...]}`, one item id per file in
order. Then:

```json
{"id":"file.newSequence","params":{"name":"rebuild","width":1080,"height":1920,"fps":30,"video":3,"audio":3}}
```

Use the prompt's size and rate. For 29.97 pass `"fps":29.97`. Video track 1
holds the picture and track 3 the captions. Audio track 1 holds the clips'
own sound and tracks 2 and 3 hold music and effects.

## 2. Picture

For each shot in the prompt's shot table, place the chosen clip:

```json
{"id":"timeline.place","params":{"item":2,"track":"V1","frame":60,"sourceIn":592704000000,"duration":508032000000}}
```

- `item`: the import id of the clip
- `frame`: the shot's `from`
- `sourceIn`: where in the clip to start, in ticks
- `duration`: `to - from` frames, in ticks

It returns `{"clips": [video, audio]}`; a clip without sound returns one id.
Then scale every placed video clip whose size differs from the sequence:

```json
{"id":"clip.fillFrame","params":{"clips":[9,13]}}
```

This returns the scale it used. It fills the frame and crops the overflow,
which is what the prompt asks for. Do not use `clip.fitToFrame`: it leaves
black bars.

## 3. Captions

Make each caption a text graphic on video track 3 (`"track": 2` counts from
0) so it stays editable and keeps its own position and style:

```json
{"id":"graphics.newText","params":{"text":"FIRST","position":[540,342],"size":118,"font":"Arial Black","seconds":2,"track":2,"time":0,"newClip":true}}
```

- `time`: the caption's `from`, in ticks
- `seconds`: its length, `(to - from) / fps`
- `size`: the prompt's px size
- `font`: the style's font. Add `fontStyle` (e.g. `"Bold"`) when the font
  has styles.

It returns `{"clip": id, "layer": 0}`. A track holds one clip at a time:
when two captions overlap in time (two lines shown as separate captions),
the second fails with "V3 is not free here". Put it on the next track
(`"track": 3`); create the sequence with enough video tracks.

Style it:

```json
{"id":"graphics.set","params":{"clip":15,"layer":0,"props":{"align":"center","fill_color":"#FEFFFF","stroke":true,"stroke_width":8,"stroke_color":"#000104"}}}
```

- outline `none`: `"stroke": false`
- box colour: `"background": true` and the box colour
- multi-line text: put `\n` in `text`

**Centre it where the prompt says.** `position` is the start of the first
baseline, not the centre, so place, measure and move. Style it first:
alignment, font and size change the bounds, so measure after them.

1. `{"id":"graphics.list","params":{"clip":15}}` returns the layer's
   `localBounds` `[left, top, right, bottom]` around `position`.
2. Set `position` to the prompt's pixel centre minus the bounds' centre:
   `x = px - (left + right) / 2`, `y = py - (top + bottom) / 2`.

   ```json
   {"id":"graphics.set","params":{"clip":15,"layer":0,"props":{"position":[540,388.67]}}}
   ```

## 4. Transitions

Hard cuts need nothing. For a dissolve or flash, select the two clips at the
cut and use `sequence.applyVideoTransition`; check its params with
`filmcraft-cli describe sequence.applyVideoTransition`.

## 5. Sound

Place music and effect clips sound-only on audio tracks 2 and 3:

```json
{"id":"timeline.place","params":{"item":4,"track":"A2","frame":15,"duration":1397088000000}}
```

Change levels with `clip.audioGain`. Check its params with `describe`.

A voiceover is a list of WAV files, one per caption, in
`rebuild/voiceover/`. Import them with the clips. Place each one on A3 at
its frame:

```json
{"id":"timeline.place","params":{"item":21,"track":"A3","frame":219}}
```

Leave out `duration` so the whole line plays. Lower the clips' own sound
under it:

```json
{"id":"clip.audioGain","params":{"clips":[12,14],"mode":"set","db":-18}}
```

## 6. Export

Save, then export the whole sequence:

```
filmcraft-cli --project rebuild/rebuild.fcproj --compact export rebuild/rebuild.mp4 --format h264
```

It prints the frame count. Check with
`ffprobe -v error -show_entries stream=width,height,r_frame_rate,nb_frames -of compact rebuild/rebuild.mp4`:
same size, rate and frame count as the prompt's sequence.

## Checking your work

- `filmcraft-cli --project <p> --compact inspect sequence`: every clip's
  `startFrame`, `endFrame` and `sourceIn`. Cuts must land on the prompt's
  frames.
- `filmcraft-cli --project <p> render --seconds 1.5 --out check.png`: look at
  a frame. Render one in the middle of every caption and compare it with the
  original's frame at the same time.
