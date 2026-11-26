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

**Status (2026-11-25):** implemented on desktop and iOS; not yet tested on a real gig or with a real pedal.

| Item | State |
|---|---|
| 1 Library and song editor | done (desktop, iOS) |
| 2 Chart viewer (ChordPro, transpose, capo) and PDF | done; desktop opens PDFs in the system viewer (in-app PDF rendering later) |
| 3 Setlists: sets, breaks, overrides, same song twice, share as text | done; PDF print later |
| 4 Perform mode, panic, keep screen on | done |
| 5 Page turners (keys) and MIDI in with Learn | done; hold gesture on desktop, press on iOS |
| 6 MIDI out, Play Acoustic profile and driver, simulated pedal | done (`gigcraft-midi`, `gigcraft-rig`, midir, CoreMIDI); untested with hardware |
| 7 Import/export | done (`gigcraft-formats`); lossless zip export later (the library folder itself is lossless) |
| 8 Folder sync with conflict detection | done; iOS folder choice with bookmark |
| 9 Accessibility, DE/EN | done: German and English on desktop (craft-i18n catalog, checked by a test) and iOS (string catalog); screen readers via AccessKit (desktop) and VoiceOver labels, headings, chart lines read as sentences, Dynamic Type (iOS); a native-speaker review of the German and a VoiceOver session on a device are still open |
| 10 Beta (TestFlight/SideStore, real gigs) | CI builds an unsigned IPA for SideStore/AltStore on every push (artifact) and on v* tags (release); TestFlight and real gigs open |

Open points found on the way: open-acoustic has no licence file (GigCraft uses only its documented protocol facts); whether the Play Acoustic numbers presets from 0 or 1 in Program Change must be checked on the hardware (the driver learns the offset at run time; the Cuebook importer assumes 1); text from the core (error messages, most rig notices) is still English.

Original plan:

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

**Status (2026-11-25):** items 1–3 work on the desktop and as followers on iOS; items 4–7 are open. Not yet tried with a real band, a real audio interface or on a venue network.

| Item | State |
|---|---|
| 1 Live protocol, LAN transport, web viewer with QR code | done: `gigcraft-live` (no I/O), `gigcraft-live-net` (WebSocket + viewer on one port, Bonjour announcement and discovery), ADR-0013; MultipeerConnectivity open |
| 2 Followers with own charts/presets, ready, cues | done on desktop and iOS; iPad/iPhone can lead too (same Rust server through `LeaderCore`, Bonjour through the system) |
| 3 Audio engine | engine and hosts done: desktop (cpal, ADR-0012) and iOS (AVAudioEngine through a real-time C function, ADR-0014), wired into perform mode and both song editors; iOS uses every output of an audio interface (up to 8, rebuilt when one is plugged in); cue slots, speed/pitch open |
| 4 MIDI clock/MTC, timed MIDI, macros, MIDI files | MIDI clock done (per device: always at the song tempo, or while tracks play with Start/Stop; desktop thread measured at < 0.3 ms deviation, iOS scheduled with CoreMIDI timestamps); parts (and their presets) follow the backing track by section name; MTC, free timed MIDI events, macros, MIDI file playback open |
| 5 Timed lyrics, PDF timestamps, external display | section start times: the chart follows the backing track and marks the section (desktop, iOS); second screen: desktop window, iOS external display (AirPlay/HDMI), chart or lyrics only; line-level timing and PDF page timestamps open |
| 6 Variants, history, queue, backups, settings export | backups done (daily ZIP outside the library, keep 14, back up now; desktop and iOS); settings export/import done (desktop); variants, history, queue open |
| 7 Web build (wasm) | open (the UI crate keeps network and audio behind hooks so it can build for the web) |
| 8 From rehearsals (0.2.3) | done: one import for text, links, web shortcuts and saved pages; Ultimate Guitar details (title, artist, key, capo, tuning, never the chart) and the link kept and shown; auto-scroll (stage view everywhere, song view on iOS; key S, footswitch); iPhone layouts for the song and stage views; readable, closable error messages |
| 9 Rehearse together (0.2.4) | done: scroll sync in live sessions (by section, any screen size; switchable per member), band mark "from here" for everyone, link editing, desktop song-view auto-scroll; per-device notes at chart places open |

Also done along the way: help window (F1) and tooltips on every control (desktop), help sheet (iOS); the design pass (amber LED accent, display readouts, status LEDs, app icon).

1. Live protocol (`gigcraft-live`): messages, roles, MultipeerConnectivity transport (Apple), Bonjour + WebSocket transport (desktop/LAN), web viewer with QR code.
2. Followers with own charts/presets; ready status; cue messages.
3. Audio engine (`gigcraft-audio`): multitrack player, buses → outputs, click/count-in, fades/crossfades, regions and cue slots, pitch/speed; iOS host (AVAudioEngine) and desktop host (cpal).
   - Done (branch `phase-2/audio`, ADR-0012): `gigcraft-audio` (clips via craft-audio, tracks with gain/pan/mute/solo/bus, click and count-in, play/pause/stop/seek with declick, fades, crossfades, loop regions, marker jumps now or on the next bar), song settings `tracks`/`click`/`markers` in song.toml, desktop host `gigcraft-audio-cpal`.
   - Wired into the desktop: Play/Stop in perform mode, tracks and click in the song editor, audio device in the rig window.
   - Play/stop as a stage action (key P, learnable footswitch); PANIC stops the audio.
   - iOS host: `AudioCore` (UniFFI) loads and decodes, an `AVAudioSourceNode` renders through `gigcraft_audio_render` (ADR-0014); stereo for now.
   - Open: multichannel outputs on iOS (channel map), cue slots.
   - TODO: playback speed and pitch. craft-audio-dsp has building blocks but nothing drop-in: `stretch::Wsola` is allocation-free but leaves the grain chain per track to the caller (and it interacts with loops, jumps and the click grid), and the phase-vocoder pitch effect allocates per STFT frame, so it cannot run in the callback. Options: WSOLA in the renderer for speed, offline-rendered pitch variants cached with the clip.
4. MIDI clock/MTC, timed MIDI events, macros, MIDI file playback.
5. Timed lyrics and PDF timestamps; external display.
6. Song variants, history, queue; scheduled backups; settings export.
7. Web build (wasm) for editing and viewing.

### Wanted for later (from rehearsals)

- **Scroll sync in a live session:** every device shows the same position of the chart as the leader (opt-in per follower).
- **Notes at a place in the chart, per device:** a musician's own remarks pinned to a line, kept only on that device (or in a personal layer), not in the shared song.
- **Rehearsal marker for everyone:** mark a place ("from here", "bar 17") during rehearsal and every device in the session shows it.

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
