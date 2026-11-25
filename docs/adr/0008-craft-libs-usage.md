# ADR-0008: Using craft-libs

- Status: accepted (Phase 0, 2026-11-25)

## Context
`storytold/craft-libs` (workspace licence MIT OR Apache-2.0) has crates we need. Its README names a release tag `v0.1.0`, but the repository had no tags on 2026-11-25. Cargo fetches the whole repository (~80 MB) for any git dependency.

## Decision
- Pin every craft crate to **one commit** (`rev`) in `[workspace.dependencies]`; update all of them together and run the full test suite.
- Switch to the release tag (or crates.io) once one exists.
- Phase 0 uses: `craft-atomic` (crash-safe saves), `craft-jsonrpc` and `craft-mcp` (MCP server). Next candidates: `craft-audio`, `craft-midi`, `craft-time`, `craft-csv`, `craft-pdf`, `craft-i18n`, `craft-settings`, `craft-ui`.
- Each crate is adopted only after checking that it builds for every target we ship (macOS, Windows, Linux, wasm, iOS).

## Consequences
- Same building blocks and conventions as the Craft apps.
- First build downloads the craft-libs repository; CI caches `~/.cargo/git`.
