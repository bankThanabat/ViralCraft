# ADR-0006: A cloud voice only with the user's own key

## Status
Accepted. Makes one exception to ADR-0002.

## Date
2026-10-09

## Context
When the user's clips have no speech and they have no script, a rebuild
has nothing to caption. The user asked to offer an AI voiceover instead,
and to let users choose the voice and bring their own key. The system's
own voices (macOS `say`) are free and offline but sound robotic; cloud
voices (OpenAI, ElevenLabs) sound natural but cost money and need an
account.

## Decision
`viralcraft voiceover` speaks the user's approved script, one file per
caption, with the engine the user picks: `say` by default, or a cloud
voice with the user's own key: `openai`, `elevenlabs` (US), `minimax`,
`qwen`, `bytedance` (China), from `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`,
`MINIMAX_API_KEY`, `DASHSCOPE_API_KEY` or `VOLCENGINE_API_KEY`. The user asked for Chinese labs
to be offered alongside the US ones. The key reaches curl on stdin and is never written
to a file, logged, or put on a command line. Only the script text is sent.
The agent never asks for a key in chat. Models and voices are flags, so a
retired model is a flag change.

## Alternatives considered
- **Only `say`**: no account needed, but too robotic for posting.
- **One built-in cloud provider**: simpler, but ties users to one vendor
  and one country's services.
- **Script only, the user records**: still possible (skip the voiceover
  and record over the rebuild), but not what the user asked for.

## Consequences
- The CLI stays offline unless the user chooses a cloud voice.
- Platforms and OpenAI's policy ask for AI voices to be disclosed; the
  rebuild prompt's "done when" says so.
- The cloud engines are covered by unit tests of the request shape and by
  ignored tests that run only with a real key.
