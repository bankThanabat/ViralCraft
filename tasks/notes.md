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
