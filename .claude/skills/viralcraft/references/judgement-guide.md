# Judgement guide

`judgement.json` holds every decision the CLI cannot measure. Frame numbers
are at the video's own rate, as in EDIT-SPEC.md. `frames/f00042.jpg` is
frame 42. `viralcraft finalize` lists any field still missing.

The shape, with every field:

```json
{
  "version": 1,
  "format_name": "...",
  "hook": { "type": "...", "window": { "start": 0, "duration": 54 }, "why": "..." },
  "shots": [ { "n": 1, "role": "...", "speech": "...", "on_screen": "..." } ],
  "captions": [ { "id": "c1", "kind": "...", "font_substitute": "...", "text_fix": "..." },
                { "id": "c2", "same_as": "c1" } ],
  "overlays": [ { "id": "o1", "kind": "...", "description": "..." } ],
  "look": { "name": "...", "match_by": "..." },
  "story_beats": [ { "start": 0, "duration": 54, "label": "hook: claim" } ],
  "overrides": [ { "path": "shots[2].camera", "value": "static", "reason": "..." } ]
}
```

## format_name

A short name someone would search for: "POV wish granted literally",
"3-step recipe with on-screen amounts". Name the pattern, not the topic.

## hook

Look at every frame from f0 to about 3 seconds, and at the first caption and
first words of the transcript.

- `type`: what stops the scroll. Common values: `text-overlay`,
  `talking-head`, `question`, `pattern-interrupt`, `result-first`,
  `pov`, `product-flash`, `before-after`. Combine with `+` when two work
  together (`text-overlay + product-flash`).
- `window`: the frames that do the hooking. It usually ends at the first cut
  or when the first caption changes.
- `why`: one or two sentences on why a viewer stays. Be concrete: "the
  finished drink is shown before the recipe starts", not "it is engaging".

## shots[n]

`n` is the shot number shown in EDIT-SPEC.md (1, 2, 3...), not a position
in the list. One entry per shot in EDIT-SPEC.md. Look at the shot's middle frame, and its
first and last frames if anything moves.

- `on_screen`: what the viewer sees, in a few words. Describe framing too:
  "hands whisking matcha in a bowl, top-down close-up".
- `role`: `a-roll` (the main subject or speaker), `b-roll` (cutaway that
  illustrates), `insert` (a short close-up of a detail), `title card`,
  `reveal` (the payoff shot). Use others only when none fit.
- `speech`: `on-camera` (the speaker is visible and talking), `voiceover`
  (speech over shots where no one is talking), `none`.

## captions[id]

One entry per caption id. Look at the caption's middle frame. EDIT-SPEC.md
gives its box; the text is inside it.

- `kind`:
  - `caption`: text the editor added to carry speech or narration
  - `title`: a heading that stays up (a hook line, a step name, a recipe name)
  - `watermark`: the creator's handle or app logo, often in a corner
  - `scene-text`: text that is part of the filmed scene (packaging, signs,
    screens, labels). It is removed from the spec.
  If the box holds no readable text at all (a pattern, a shadow), use
  `scene-text`.
- `font_substitute`: a widely available font closest to what you see.
  Match weight (regular, bold, black), width, italic and rounded or square
  ends. Name the weight: "Poppins ExtraBold Italic", "Sarabun Bold",
  "Noto Sans Thai Bold". For Thai use a Thai font (Kanit, Prompt, Sarabun,
  Noto Sans Thai). Not needed for `scene-text`.
- `text_fix`: the correct text, line breaks as `\n`. Required whenever the
  measured text is wrong. The OCR reads the Latin alphabet only, so every
  Thai, Japanese, Chinese, Korean, Arabic or Hindi caption needs one. Copy
  the text exactly as shown, including emoji and deliberate misspellings. Do
  not take it from the transcript.

- `same_as`: when this id is another piece of a caption already judged,
  give only `"same_as": "<that id>"`. Something moving behind a caption
  (sparkles, a hand) splits it into pieces with garbage OCR text, and a
  caption that types or writes on comes out as one piece per stage. Judge
  the first piece fully, with the finished text in `text_fix`, and point
  every other piece at it. Finalize joins them into one caption from the
  first frame to the last. A piece that is a different caption at the same
  place must not be joined.

## overlays[id]

Look at the overlay's middle frame inside its box.

- `kind`: `emoji`, `sticker`, `image`, `arrow`, `shape`, or `not an overlay`
  when it is part of the scene (a hand, a moving object). `not an overlay` is
  removed.
- `description`: what it is, in a few words.

## A-roll / B-roll map

Nothing to write: finalize builds it from the shot roles.

## look

Compare the middle frames of several shots, and the look numbers in
EDIT-SPEC.md (contrast, saturation, warmth).

- `name`: a short grade name: "warm natural", "teal-orange", "bright and
  airy", "flat phone footage".
- `match_by`: how to make new footage look the same, in steps an editor can
  follow: "raise exposure a little, warm white balance, soften contrast,
  keep greens saturated".

## story_beats

Split the whole video into its beats, in order and without gaps:
`{ "start": 0, "duration": 54, "label": "hook: result first" }`. Labels are
`name: what happens`; the name becomes the slot name in TEMPLATE.md (HOOK,
STEP 1, PAYOFF, CTA). Use the transcript, captions and shot list together.
Beats usually start at a cut or a caption change.

## overrides

Use only when a measured camera move is clearly wrong and you can see why,
e.g. a static shot reported as a zoom because a hand filled the frame. The
only supported path is `shots[n].camera`; give the corrected `value` in
words ("static", "slow push-in to 110%") and a `reason`. It replaces the
measured move in the spec, with the reason beside it.
