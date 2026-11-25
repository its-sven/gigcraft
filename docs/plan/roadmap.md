# GigCraft roadmap and work plan

*Status 2026-11-25. Phases build on each other; each phase ends with something usable on a real gig. Features per phase: [feature-catalog.md](feature-catalog.md). Architecture: [architecture.md](architecture.md).*

## Overview

| Phase | Goal | Done when |
|---|---|---|
| **0 Foundation** | workspace, CI, core data model, Rust↔Swift pipeline proven | `cargo xtask ci` green; a SwiftUI app shows songs loaded by the Rust core; egui shell opens a library |
| **1 Solo on stage (MVP)** | replaces Cuebook and BandHelper "Basic" for one musician | Sven plays a full gig with GigCraft on iPad: setlist, charts, pedal presets, page turner |
| **2 Band on stage** | leader/follower, backing tracks, click, timed lyrics | a band plays a gig with 3+ devices following, tracks and click on separate outputs |
| **3 Band organisation** | self-hosted server, band sync, calendar, members | a band prepares a gig fully in GigCraft (availability → setlist → parts) |
| **4 Pro & extras** | business, production, Android, redundancy | feature parity with BandHelper Pro and Stage Traxx 4 |
| **L Later** | AI modules, DMX, video, plugins, Watch | as demand shows |

## Phase 0: Foundation

**Status (2026-11-25):** done except the items marked open below.

| Item | State |
|---|---|
| Workspace, lints, `cargo xtask ci` (fmt, clippy, tests, layering), GitHub Actions | done |
| ChordSketch evaluated and adopted (ADR-0007), craft-libs pinned (ADR-0008), UniFFI (ADR-0009) | done |
| `gigcraft-model`, `-chordpro`, `-store`, `-commands` (23 commands, undo/redo, inspect) | done |
| `gigcraft-cli` (`init`, `run`, `commands`, `mcp`) with craft-jsonrpc/craft-mcp | done |
| egui desktop shell: library, search, chart with transpose, setlists | done |
| `cargo xtask apple` → XCFramework + Swift bindings; SwiftUI app on iPad simulator | done |
| Demo library (own songs) | done |
| Automerge vs. own record versioning | open, needed before Phase 3 |
| Web (wasm) build of the egui UI | open, moved to Phase 2 |
| Test on a real iPad (signing with a personal team) | open |
| CODE_OF_CONDUCT, SECURITY | open |

Original plan:

1. **Repo and tooling**
   - Cargo workspace (edition 2024, resolver 3), shared lints, `rust-toolchain.toml`.
   - `xtask` with `ci` (fmt, clippy `-D warnings`, tests, layering check "no UI crate below the UI", wasm build), `ios` (XCFramework + Swift package), `release`.
   - GitHub Actions: Linux/macOS/Windows test matrix, wasm, iOS build on macOS runner.
   - README, CONTRIBUTING (clean room, DCO), CODE_OF_CONDUCT, SECURITY.
2. **Evaluate building blocks** (one day each, written up as ADRs)
   - ChordSketch crates (MIT) as the ChordPro engine vs. own parser.
   - craft-libs: craft-audio, craft-midi, craft-time, craft-csv, craft-pdf, craft-atomic, craft-settings, craft-i18n, craft-ui (pin tag v0.1.0+).
   - UniFFI vs. swift-bridge for the Swift layer.
   - Automerge vs. own record versioning (for Phase 3, decide early because it shapes IDs and the model).
3. **Core crates, first cut**
   - `gigcraft-model`: Song, Part, Preset, Setlist, SetlistEntry, Set, Device, DeviceProfile; serde, schema version.
   - `gigcraft-store`: folder layout (ADR-0003), load/save, atomic writes, index, search, file watching.
   - `gigcraft-commands`: registry, undo/redo, inspect; first commands `song.*`, `setlist.*`.
   - `gigcraft-automation`: `gigcraft-cli run --cmd`, `commands --json`, MCP server skeleton.
4. **Shells**
   - `gigcraft-ffi` + `apps/apple`: SwiftUI app listing songs from the core (proves the pipeline end to end, incl. on a device with a free Apple ID / SideStore as with Cuebook).
   - `apps/desktop`: egui window with library and setlist list.
5. **Test fixtures**: a demo library (public-domain/self-written songs only), golden files for formats.

## Phase 1: Solo on stage (MVP)

Order matters: each step is usable on its own.

1. Library and song editor (fields, parts, tags, search, smart lists) – core + both UIs.
2. Chart viewer: ChordPro render (transpose, capo, font, columns, dark), PDF viewer with fit/zoom/crop.
3. Setlists: sets, breaks, timers, notes/overrides per entry, same song twice, print/share PDF and text.
4. Perform mode: big view, next/prev song/part/page, keep screen on, panic button, show mode.
5. Control inputs: HID page turners + MIDI in mapping with Learn (shared action layer).
6. MIDI out: presets per song/part, generic MIDI + device profile Play Acoustic (port from Cuebook incl. simulated pedal), read-back.
7. Import/export: ChordPro, OnSong, plain text, PDF, BandHelper TSV, CSV mapping, Cuebook JSON, UG links/playlists; lossless export (zip).
8. Folder sync (iCloud Drive/Nextcloud via Files) with conflict view; manual backups.
9. Accessibility basics, DE/EN.
10. **Beta**: TestFlight (needs the Apple Developer Program, 99 USD/yr) or SideStore IPA; desktop builds on GitHub releases. Sven tests on real gigs; fix list.

## Phase 2: Band on stage

1. Live protocol (`gigcraft-live`): messages, roles, MultipeerConnectivity transport (Apple), Bonjour + WebSocket transport (desktop/LAN), web viewer with QR code.
2. Followers with own charts/presets; ready status; cue messages.
3. Audio engine (`gigcraft-audio`): multitrack player, buses → outputs, click/count-in, fades/crossfades, regions and cue slots, pitch/speed; iOS host (AVAudioEngine) and desktop host (cpal).
4. MIDI clock/MTC, timed MIDI events, macros, MIDI file playback.
5. Timed lyrics and PDF timestamps; external display.
6. Song variants, history, queue; scheduled backups; settings export.
7. Web build (wasm) for editing and viewing.

## Phase 3: Band organisation

1. `gigcraft-server` (Axum, SQLite/Postgres, Docker): auth, band workspaces, roles, sync API, file storage, history/rollback.
2. Sync client in the core; offline queue; conflict UI; per-device download rules.
3. Calendar and events (iCal feed, CalDAV), availability, substitutes, lineup, gig sheet.
4. Members, roles, parts per role, shared annotations, band files.
5. Hosting guide (Docker Compose, reverse proxy, backups) – Sven's own instance as the first.

## Phase 4: Pro & extras

Stage plot → input list → monitor plan; equipment and checklists; contacts; finances and invoices; mix presets; hot-standby device with failover; lighting via MIDI/OSC; Android app (Kotlin + UniFFI); practice log; public gig widget.

## Later

AI modules (OCR import, chord detection, lyric timing, stem separation) via craft-ai; DMX; video; plugins/scripting; Apple Watch; voice-following scroll; iReal Pro/MusicXML.

## Working mode

- One feature per branch, small PRs, `cargo xtask ci` green before review.
- Each phase step becomes an issue with acceptance criteria taken from the feature catalog.
- Independent crates (e.g. formats vs. audio engine) can be developed in parallel; never two people on the same crate at once.
- Real-device tests on iPad for every perform-mode change; simulated pedal and audio device in CI.

## Open decisions (for Sven)

1. Go public right away or after Phase 0/1? (Recommendation: private until Phase 0 is done, then public.)
2. Apple Developer Program (99 USD/yr) for TestFlight/App Store and CloudKit – when?
3. Approach ArtCraft (Discord) about community-app status – after Phase 1 with something to show.
4. Domain (gigcraft.app/.org) and trademark check before going public.
