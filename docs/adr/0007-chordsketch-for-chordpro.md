# ADR-0007: ChordSketch as the ChordPro parser

- Status: accepted (Phase 0, 2026-11-25)

## Context
Charts are ChordPro (ADR-0003). Writing a complete ChordPro parser (R6 has well over 100 directives) is a large job. The Perl reference implementation is not usable as a library. ChordSketch (`koedame/chordsketch`) is a Rust implementation of ChordPro R6.101.0.

## Evaluation
- `chordsketch-chordpro` 0.9.1: **MIT**, **no dependencies**, lenient parser with error list, AST, transposition with key-aware spelling, chord parsing, voicings, input-size limit. Builds for wasm (external tools are cfg'd out there).
- Its renderers (PDF/HTML) embed fonts under OFL; the desktop app and playground are **AGPL** and are not used.
- `external_tool.rs` can start `abc2svg`/`lilypond`; GigCraft never calls it.
- Risk: young project (0.x, 16 stars). API may change.

## Decision
Use `chordsketch-chordpro` with an exact version pin (`=0.9.1`) behind our own crate `gigcraft-chordpro`, which converts its AST into a small display model (blocks → lines → chord/lyric segments). No other crate depends on ChordSketch directly, so we can upgrade, fork or replace it in one place.

## Consequences
- Full ChordPro coverage from day one; transposition included.
- Upgrades are deliberate (exact pin + tests in `gigcraft-chordpro`).
- If the project stalls, we fork the MIT crate.
