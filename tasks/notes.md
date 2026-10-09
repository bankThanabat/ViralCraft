# Build notes

Measurements taken while building, kept so later decisions can cite them.

## Text detection cost (task 8)

ocrs detection only, on half-resolution 540x960 JPEG frames, release build,
Apple Silicon, 2026-10-09:

| run | ms per frame |
|---|---|
| `cargo test -p inspect --release` on cuts-3 | 25.2 |
| `viralcraft inspect` on cuts-3, same machine, other work running | 33.4 |

| `cargo test -p inspect --release` on cuts-3 at 720x1280 (task 11b) | 62.3 |

Budget in SPEC.md was 30 ms. The two runs straddle it, so treat ~30 ms as the
working figure: a 60 s clip at 30 fps spends 45 to 60 s in detection.
Fallback if that is too slow on real clips: detect every 3rd frame and
refine around changes (SPEC.md, open question 2).

## Caption colours (task 8)

Half-resolution frames shrink a 3 px stroke to ~1.5 px, so most stroke pixels
are anti-aliased blends. Cluster medians gave a brown outline on the gold
scene; "farthest from the background" gave a grey outline on the navy scene.
Taking each cluster's 30% farthest from the other ink colour gives black and
white on both. Regression tests use real crops in
`crates/inspect/tests/data/`.

Font size is an estimate from line height: 104 px estimated vs 96 px drawn on
cuts-3 (+8%).

## First real clip (Phase 2 checkpoint, task 11b)

`inbox/Video by theintrovertedattorney.mp4`: 56 s, 1080x1920 VP9, 30 fps,
animated cartoon, voiceover over a music bed, burned-in dialogue captions,
a persistent boxed title, a watermark, a product insert and a punchline cut.

| | first run | after task 11b |
|---|---|---|
| shots | 4, correct | 4, correct |
| transcript | 132 words, near verbatim | 130 words + 2 `sighs` sound events |
| music | none (beat confidence 0.16) | bed 1 s to 52 s at -29.5 dB, second bed under the punchline |
| caption events | 523 (452 line-art flicker) | 35: title, watermark, 4 scene text, 29 dialogue |
| title style | white, grey outline | white on #010002 box, no outline |
| motion | static, but a bogus two-keyframe shot | every shot one keyframe at 100% |
| overlays | 1 (animation) | 2 (both character animation, not stickers) |
| inspect time | 98 s at 540x960 | 114 s at 720x1280 |

Still wrong: OCR on the hand-lettered dialogue font misreads words either way
("qoes", "Tike", "horming", the lone "I" in the title). Phase 3 can use the
transcript's spelling for captions whose words match speech in the same
window. Overlay detection fires on cartoon animation; the agent dismisses it.
Detection measured 24.8 ms per frame on this run at 720x1280 against 62 ms
on the fixture test; the clip's total time is the reliable number.

### Caption re-reading from the full-size source

The typos were not in the source: crops of the original 1080x1920 frames
read cleanly ("here it goes!", "like", "*sigh*") where the 720x1280 frames
gave "qoes", "Tike", "siah". Captions are still found on the small frames,
then each is cropped from the full-size source at three moments and the
most common reading wins. No transcript is used: many videos have no
speech, and creators spell on purpose (the clip writes "h@rming").

On the clip, 15 of 35 captions changed: about ten clear fixes, a few new
small errors (a stray "GAOOW" line, a leading dot, "Wanna"). A lone "I" is
still dropped in places ("butdon't", "Me if had"); that is left to the
agent's review of the frames. Cost: about 4 s more on the 56 s clip.
