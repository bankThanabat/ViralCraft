# ADR-0002: The agent judges, the CLI measures

## Status
Accepted. Amended by ADR-0006: an optional cloud voice, with the user's own key.

## Date
2026-10-09

## Context
An edit spec needs facts (cut frames, caption boxes, colours, word times)
and judgements (what a shot is for, why the hook works, which text is
scene text, which font is closest). Facts can be measured and tested.
Judgements need someone to look at the frames.

## Decision
The CLI never calls a hosted model. It writes measurements and a draft
EDIT-SPEC.md with every judgement slot marked `TODO:`. The agent that runs
the CLI looks at the frames and writes `judgement.json`; `viralcraft
finalize` checks that every required field is filled and writes the
finished spec, captions, timeline and template.

## Alternatives considered
- **Call a vision model from the CLI**: needs API keys, costs money per
  run, and ties the tool to one provider.
- **Heuristics for judgement fields**: brittle, and wrong in ways nobody
  sees.

## Consequences
- The CLI stays deterministic and testable; judgement quality depends on
  the agent and on the skill's guide.
- `finalize` refuses an incomplete `judgement.json` and lists what is
  missing, so a run cannot finish half-judged.
- Scripts the OCR cannot read (Thai, Japanese) are read by the agent into
  `text_fix`; the CLI still measures their boxes, timing and style.
