# GigCraft architecture

*Status: proposal, 2026-11-25. Decisions are recorded as ADRs in [../adr/](../adr/).*

## 1. Goals that shape the architecture

1. **One app for the whole gig:** repertoire, charts, backing tracks, MIDI/preset control, band sync and gig organisation (the union of BandHelper and Stage Traxx, see [feature-catalog.md](feature-catalog.md)).
2. **On stage it must not fail:** offline, no account, no crash on bad files, a predictable realtime audio/MIDI path, and nothing happening that the musician did not trigger.
3. **Runs where musicians are:** iPad and iPhone first for the stage; Mac, Windows and Linux for preparation and for keyboard/laptop rigs; a browser for bandmates who only want to read; Android later.
4. **Open and scriptable like the Craft apps:** every action is a command, reachable from UI, keyboard, MIDI, CLI, a control channel and MCP.
5. **Your data stays readable:** plain files (ChordPro + TOML/JSON), importable from and exportable to the other apps.

## 2. Big picture

```
            ┌──────────────── UIs (thin) ─────────────────┐
            │ egui desktop+web │ SwiftUI iOS/iPadOS │ Kotlin Android (later) │
            └────────┬─────────┴─────────┬──────────┴──────────┬─────────────┘
                     │ Rust API          │ UniFFI (Swift)      │ UniFFI (Kotlin)
┌────────────────────▼───────────────────▼─────────────────────▼──────────────┐
│                         gigcraft core (Rust workspace)                       │
│  commands + undo + inspect  ·  model  ·  library/store  ·  perform state     │
│  chordpro · formats (import/export) · midi logic · audio engine (render)     │
│  sync (data) · live (leader/follower protocol) · automation (CLI/ctl/MCP)    │
└───────┬───────────────────────┬─────────────────────────┬───────────────────┘
        │ platform I/O traits   │                         │
  audio device / MIDI ports / network transport / files  (implemented per platform:
  cpal+midir+mdns on desktop · AVAudioEngine+CoreMIDI+MultipeerConnectivity on Apple ·
  WebAudio+Web MIDI+WebSocket in browser · Oboe/AMidi on Android)
                                │
                     optional gigcraft-server (self-hosted sync relay + web viewer, Docker)
```

**Rule (from SoundCraft):** nothing below the UI knows egui, SwiftUI or Kotlin. Platform I/O sits behind traits, so the same core runs everywhere and in tests without devices.

## 3. Repository layout (monorepo)

```
gigcraft/
├── Cargo.toml                 workspace (edition 2024, resolver 3), shared lints/versions
├── crates/
│   ├── gigcraft-model         songs, parts, presets, setlists, sets, events, members; versioned schema
│   ├── gigcraft-chordpro      ChordPro parser + layout model, transpose, capo, Nashville numbers
│   ├── gigcraft-formats       import/export: OnSong, OpenSong, BandHelper TSV, CSV mapping, LRC,
│   │                          setlist text/PDF, Stage Traxx playlist text, MusicXML (later)
│   ├── gigcraft-store         file-based library (one file per song), index, search, smart lists,
│   │                          atomic saves (craft-atomic), migrations
│   ├── gigcraft-commands      command registry (id + JSON params), undo/redo, inspect, demo content
│   ├── gigcraft-perform       show state machine: setlist → song → part/region → page; timers,
│   │                          set breaks, action mapping (pedal/MIDI/keys → commands)
│   ├── gigcraft-midi          message model, device presets & profiles, macros, MIDI Learn,
│   │                          clock/MTC generator, timed MIDI events (no OS I/O here)
│   ├── gigcraft-audio         realtime render graph: multitrack player, buses, click, count-in,
│   │                          regions/loops, fades, pitch/tempo; decode via craft-audio
│   ├── gigcraft-rig           presets → devices, MIDI in → actions, simulated pedal (ADR-0010)
│   ├── gigcraft-sync          data sync (record versions / CRDT, see ADR-0004)
│   ├── gigcraft-live          leader/follower protocol, transport-neutral messages
│   ├── gigcraft-automation    CLI, JSON control channel, MCP server
│   ├── gigcraft-ffi           UniFFI surface for Swift and Kotlin
│   ├── gigcraft-platform-*    desktop I/O (cpal, midir, mdns-sd), web I/O (wasm-bindgen)
│   └── gigcraft-ui-egui       desktop + web UI
├── tools/gigcraft-bindgen     uniffi-bindgen for `cargo xtask apple`
├── apps/
│   ├── cli/                   `gigcraft-cli` (init, run, commands, mcp)
│   ├── desktop/               `gigcraft` (egui)
│   ├── web/                   wasm build (Trunk)
│   ├── apple/                 Xcode project, SwiftUI, Swift package wrapping the XCFramework
│   └── android/               later
├── server/                    gigcraft-server (Axum), Dockerfile
├── device-profiles/           community-maintained device definitions (TOML)
├── xtask/                     `cargo xtask ci|ios|wasm|release` (pattern of craft-xtask)
└── docs/                      research, plan, adr, user docs
```

## 4. Key design points

### 4.1 Data model and files

- **Song** = one folder or one file pair: `song.cho` (ChordPro: lyrics, chords, sections, timestamps) + `song.toml` (metadata: key, tempo, time signature, duration, tags, custom fields, presets per part, audio/PDF attachments, links such as Ultimate Guitar, per-member personal overrides).
- **Setlist** = `setlist.toml`: ordered entries (song refs, set breaks, pauses with timers, notes per entry), sets.
- **Event/gig**, **member**, **device**, **preset** = small TOML/JSON files. Attachments (audio, PDF) stay as ordinary files next to them.
- IDs are stable UUIDs; file names are readable slugs. Unknown fields are preserved on save (forward compatibility).
- This works directly with iCloud Drive, Nextcloud, Syncthing, Git and Dropbox (Files provider), and is the base for import/export.

### 4.2 Commands (Craft pattern)

Every action is `namespace.verb` + JSON params, for example `setlist.add_song`, `perform.next`, `midi.send_preset`, `audio.play`, `chart.transpose`. The same registry backs the menus, shortcuts, MIDI/pedal mapping, the leader protocol, CLI (`gigcraft-cli run --cmd …`), a control port and the MCP server. Commands return a result and an *inspect* diff; programmatic calls never open dialogs.

### 4.3 Realtime audio

- The core renders audio into buffers (`render(frames)`); each platform host pulls from it (AVAudioEngine source node on iOS, cpal on desktop, AudioWorklet on web).
- No allocation, locks or I/O on the audio thread; the UI thread talks to it through lock-free queues. Files are decoded and streamed ahead on a worker thread.
- Sample-accurate scheduling of click, regions, MIDI events and MIDI clock/MTC from one timeline (`craft-time`).
- Output routing: tracks → buses → device channels; click and cue on their own bus for in-ears.

### 4.4 MIDI and devices

- Generic MIDI (PC with bank MSB/LSB, CC, notes, SysEx, song select, clock, MTC) plus **device profiles**: a profile knows a device's preset count, names, quirks and safe ways to switch (for example the TC-Helicon Play Acoustic stepping via front-panel events from Cuebook). Profiles are TOML in `device-profiles/` so the community can add Helix, Kemper, Quad Cortex, Boss, Nord and others.
- Platform ports: CoreMIDI on Apple (Cuebook's MIDI layer is the starting point), `midir` on desktop, Web MIDI in Chromium browsers.

### 4.5 Two kinds of sync

1. **Live (on stage):** a leader broadcasts small messages (song, part, page, scroll, transport, setlist edits); followers act locally and send *their own* MIDI. Transports: MultipeerConnectivity on Apple (no router), Bonjour + WebSocket on LAN for mixed devices and the web viewer.
2. **Data (rehearsal/preparation):** v1 is file-folder sync (any provider). Band sync with permissions comes via the self-hosted `gigcraft-server` (see ADR-0004).

### 4.6 Safety on stage

Panic button (all notes off, stop audio), double-tap guards, seek lock, nothing auto-updates or syncs during a show unless allowed ("show mode"), crash-safe saves, watchdog for audio dropouts, failover tone, battery/thermal warnings, and a dry-run "rehearse the set" mode with a simulated pedal and simulated audio device.

## 5. Technology choices

| Area | Choice | Why |
|---|---|---|
| Core language | Rust (edition 2024) | Craft ecosystem, safety, one core for all platforms |
| Desktop/web UI | egui/eframe + craft-ui | Craft look and feel, wasm build |
| iOS/iPadOS UI | SwiftUI | best stage UX on iPad, CoreMIDI/AVAudioSession/AirTurn, App Store |
| Bindings | UniFFI (Mozilla) | proven Rust↔Swift/Kotlin; Firefox uses it |
| Audio I/O | cpal (desktop), AVAudioEngine (Apple), AudioWorklet (web) | |
| Audio decode | craft-audio, fallback symphonia | pure Rust |
| Pitch/tempo | evaluate Rubber Band (GPL, avoid) vs. signalsmith-stretch (MIT) vs. own | licence must stay permissive |
| MIDI I/O | CoreMIDI, midir, Web MIDI | |
| Discovery | Bonjour/mDNS (`mdns-sd`), MultipeerConnectivity | |
| Server | Axum + SQLite/Postgres, Docker | self-hostable |
| Formats | craft-csv, craft-xml, craft-pdf, craft-ical, craft-zip | reuse |
| CI | GitHub Actions + `cargo xtask ci` (fmt, clippy, tests, layering, wasm, iOS build) | Craft gate |

## 6. Relationship to Cuebook and open-acoustic

- **Cuebook** stays the small, finished iOS app. Its MIDI layer, the Play Acoustic pedal logic, the simulated pedal, Ultimate Guitar link/playlist import and the perform view are the reference for GigCraft's Apple shell and the first device profile.
- **open-acoustic** remains the source of the Play Acoustic SysEx knowledge; GigCraft links to it rather than copying.
- Once GigCraft's iOS app covers Cuebook's features, Cuebook can point users to GigCraft (with an importer for Cuebook's JSON library).
