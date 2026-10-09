# ADR-0003: ffmpeg is the only external tool

## Status
Accepted

## Date
2026-10-09

## Context
Every stage needs decoding, and cut detection and frame extraction are
solved problems in ffmpeg. Each extra tool is one more thing for the user
to install and for `doctor` to check.

## Decision
The core needs `ffmpeg` and `ffprobe` and nothing else outside the binary.
Cut scores come from ffmpeg's `scdet`; the Rust code decides what is a cut.
The standard Homebrew ffmpeg is enough. The `ffmpeg-full` build (with
libass) is used only in development, to burn captions in and check their
placement; users are never asked to install it.

## Alternatives considered
- **PySceneDetect / OpenCV**: better detectors, but they bring Python
  (see ADR-0001).
- **A caption preview command for users** (needs libass): removed at the
  user's request; editors show the captions themselves.

## Consequences
- `doctor` checks two tools and offers the install command for the
  platform, asking first.
- Caption placement is verified in a dev-only ignored test.
