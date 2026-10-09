# ADR-0005: Rebuilding is an optional step, one skill per editor

## Status
Accepted

## Date
2026-10-09

## Context
The goal is to copy a viral format with the user's own footage. The
analysis is useful on its own; the rebuild needs the user's clips, a
script or transcript, and an editor. Editors differ in how they are
driven.

## Decision
The core skill ends at a finished spec and template. The rebuild is
optional: `viralcraft assets` reads the user's clips (only reads them),
and `viralcraft prompt` writes REBUILD-PROMPT.md, editor-agnostic steps
with nothing editor-specific in them. Each editor gets its own skill that
follows the prompt. The first is FilmCraft, driven over its MCP server
when connected, else its CLI.

Caption text comes from the user's script; without one, from the speech
in their clips; without either, the agent drafts lines and waits for the
user's approval.

## Alternatives considered
- **Generate an editor project file directly from the CLI**: ties the CLI
  to one editor's file format, and skips the judgement of which clip fits
  which slot.

## Consequences
- More editors can be added without changing the CLI.
- The FilmCraft command mapping is verified by an ignored test that builds
  a small rebuild and checks the export's cuts and caption positions.
