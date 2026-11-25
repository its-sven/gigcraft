# GigCraft feature catalog

*Everything a complete alternative to BandHelper and Stage Traxx needs, plus what musicians ask for and what nobody offers yet. Built from [../research/](../research/). Status 2026-11-25.*

**Phase:** P1 = solo on stage (MVP) · P2 = band on stage · P3 = band organisation · P4 = pro & extras · L = later/ideas. See [roadmap.md](roadmap.md).
**Source:** BH = BandHelper · ST = Stage Traxx · OS = other apps (OnSong, forScore, MobileSheets, AbleSet…) · W = wishlist (users ask for it) · G = gap, nobody does it well · CB = Cuebook already has it.

## 1. Repertoire (songs)

| Feature | Phase | Source |
|---|---|---|
| Song fields: title, artist, key, tempo, time signature, duration, starting pitch, capo, year, tags, colour, notes | P1 | BH ST CB |
| Unlimited custom fields | P1 | BH |
| Parts/sections per song (intro, verse, solo…) each with its own presets | P1 | CB |
| Versions/variants of a song (per band, per gig: key, BPM, arrangement); "smart copies" inheriting from the original | P2 | BH W |
| Personal fields and personal transpose per member | P3 | BH W |
| Attachments: PDF, images, audio, video, MIDI file, links (Ultimate Guitar, YouTube, Spotify) | P1 (PDF, links) · P2 (audio/MIDI) · P4 (video) | BH ST CB |
| Full-text search with operators (AND/OR/NOT, `title:`), sort by any field, filters | P1 | ST BH |
| Smart lists (saved rules + limit) | P1 | BH ST |
| Ratings, play count, last played, play history | P2 | BH ST |
| Duplicate detection on import | P1 | ST |

## 2. Charts and lyrics

| Feature | Phase | Source |
|---|---|---|
| ChordPro as native chart format (directives, sections, comments) | P1 | BH OS |
| Viewer: font size, one/two columns, dark/high-contrast, chords above or inline, section highlighting | P1 | BH |
| Transpose (with key detection), capo, chord-only view, Nashville numbers, solfège/German H | P1 | BH ST W |
| Chord diagrams for guitar, ukulele, piano | P2 | OS |
| PDF charts: fit width/height, two-up, crop, tap zones, saved zoom, colour inversion | P1 | BH ST |
| Annotations on PDF and text (pen, highlighter, text), private or shared | P2 (private) · P3 (shared) | BH ST W |
| Autoscroll by duration or speed, continuous with drag-to-resync | P1 | BH ST |
| Time-coded lyrics (LRC-style), line/word highlighting, tap-along timing editor | P2 | ST |
| PDF autoscroll by timestamps (jump to page/position) | P2 | ST |
| Several charts per song and per instrument/role (bass tab for bassist) | P3 | W G |
| Editor with syntax highlighting, live preview, chord insertion helper | P1 | ST |
| Lead sheets/song books as PDF (one page per song, whole setlist) | P2 | BH |
| iReal Pro chart import/view | L | OS |
| Ultimate Guitar: open links in the UG app, import playlists as setlists (links only, no scraping of tabs) | P1 | CB |
| Voice-following scroll (follows the singer) | L | W G |

## 3. Setlists and shows

| Feature | Phase | Source |
|---|---|---|
| Setlists with drag and drop, notes per entry, colours | P1 | BH ST CB |
| Several sets per gig, set breaks and pauses with countdown, running time, projected finish | P1 | BH ST W |
| Same song twice, optional songs, "maybe" songs | P1 | W |
| Key/tempo/notes per setlist entry (override) | P1 | W CB |
| Shuffle, copy from earlier setlists, templates | P2 | BH |
| Played songs greyed out, "done" tracking, history → setlist | P2 | ST W |
| Print and share: PDF, text, large-print stage copy, QR code to the live viewer | P1 (PDF/text) · P2 (QR) | BH W |
| Queue (play next/last) during a show | P2 | ST |
| Setlist from pasted text, from UG playlist, from another app's export | P1 | BH CB |

## 4. Perform mode (stage UI)

| Feature | Phase | Source |
|---|---|---|
| Big, distraction-free view: chart + current/next song + part + clock + set timer | P1 | CB BH |
| Next/previous song, part, page with on-screen buttons, gestures, keys, pedals | P1 | CB |
| Screen stays on, do-not-disturb hint, battery/thermal warning | P1 | CB G |
| Assignable on-screen controls (buttons, toggles, knobs) | P2 | ST |
| Visual metronome / beat flash, section labels on the progress bar | P2 | ST W |
| External display (HDMI/AirPlay) with its own content (lyrics for audience/singer, chords hidden) | P2 | ST |
| Simple mode for less technical bandmates | P2 | W |
| Teleprompter mode (mirrored, large type) | P2 | OS |

## 5. Control inputs

| Feature | Phase | Source |
|---|---|---|
| Bluetooth page turners / HID keyboards; click, double-click, hold mapped to actions; Learn screen | P1 | BH ST |
| MIDI in → actions (PC, CC, notes, song select), channel filter, MIDI Learn, press/release, CC pickup | P1 | BH ST |
| Per-song MIDI trigger loads the song | P2 | ST |
| AirTurn AirDirect / background reception (where the OS allows) | P2 | BH ST |
| OSC in, WebSocket/HTTP control, Stream Deck | P2 | OS |
| Apple Watch / phone as remote | P2 (phone) · L (Watch) | BH ST |

## 6. MIDI out and devices

| Feature | Phase | Source |
|---|---|---|
| Presets: bank MSB/LSB + PC, multiple CC, notes, SysEx, song select; delay and order between messages | P1 | BH ST |
| Presets per song and per part; pinned/global presets; paired (on/off) and child presets | P1 (song/part) · P2 (rest) | BH CB |
| Send on song select (opt-in), *before* the song starts, at a timestamp, on play/stop | P1 (select) · P2 (timed) | BH ST W |
| MIDI macros (named reusable sequences) | P2 | ST |
| **Device profiles** (community TOML): preset names/count, 0/1-based numbering, quirks, safe switching; first profile TC-Helicon Play Acoustic/Electric | P1 | CB G |
| Read back current preset from devices that support it; confirm messages arrived | P1 (Play Acoustic) · P2 | CB W G |
| MIDI clock out from song tempo, MIDI Timecode (25/30 fps, offset) | P2 | BH ST |
| Standard MIDI File playback synced with audio, built-in SoundFont fallback synth | P2 | BH ST |
| Ports: USB, Bluetooth LE MIDI, network (RTP) MIDI, virtual ports | P1 (USB/BLE) · P2 | BH ST |
| Ableton Link | P2 | OS |

## 7. Audio: backing tracks, click, mixer

| Feature | Phase | Source |
|---|---|---|
| Multitrack player: up to 32 tracks per song, stems from file names (pattern) | P2 | ST |
| Per track: volume, pan, mute, solo, mute groups, EQ, "don't transpose" flag, loudness normalisation | P2 (basic) · P4 (EQ) | ST |
| Buses → hardware outputs, stereo/mono, routing remembered per interface; click/cue on in-ear bus | P2 | ST W |
| Mix presets per show format (full band, duo, rehearsal) | P3 | W |
| Click with accents, subdivisions, count-in, pre-roll, tap tempo, spoken cues | P2 | BH ST OS |
| Regions/markers: loop, skip, repeat, hold; cue slots (jump now / arm at end); marker import from Logic/Reaper/Cubase | P2 | ST |
| Fades, crossfades, gapless autoplay, trim, start delay | P2 | ST |
| Pitch shift and speed change | P2 | BH ST |
| Starting-pitch button, chromatic tuner | P2 | BH OS |
| Reference recordings for practice, loop A–B, in-app recording of rehearsals | P2 | BH |
| AUv3/CLAP effect inserts on buses | L | ST |
| Stage safety: failover tone, drift correction, double-tap guards, seek lock | P2 | ST |
| Hot-standby second device following via timecode with A/B failover | P4 | G |

## 8. Practice

| Feature | Phase | Source |
|---|---|---|
| Practice mode: loop parts, slow down, mute own instrument stem | P2 | OS W |
| Practice log and goals per member | P4 | BH |
| Rehearsal setlists with "to learn" status, per-member readiness | P3 | W |
| Local stem separation to build practice mixes | L | W G |

## 9. Live band sync (on stage)

| Feature | Phase | Source |
|---|---|---|
| Leader/follower: song, part, page, scroll, transport, setlist edits follow the leader | P2 | BH ST OS |
| No router needed between Apple devices; LAN for mixed devices; works without internet | P2 | BH ST |
| Followers show their *own* chart/part and fire their *own* MIDI | P2 | ST G |
| Viewer (read-only) and controller roles, password for control | P2 | ST |
| Ready status per member + "all ready" signal; cue/chat messages ("2 more choruses") | P2 | W G |
| Web viewer for anyone on the LAN (QR code), no install | P2 | OS G |
| Latency display, reconnect, leader hand-over | P2 | ST |

## 10. Data sync, accounts, backup

| Feature | Phase | Source |
|---|---|---|
| Library as plain files in any synced folder (iCloud Drive, Nextcloud, Syncthing, Git) | P1 | CB G |
| Self-hosted band server (Docker): accounts, band workspaces, roles (admin/member/read-only/add-only) | P3 | BH |
| Offline edits queued, field-level merge, conflict view, change history with rollback | P3 | BH ST |
| Per-device download rules for large files | P3 | BH |
| Backups: full / database only, to external drives, scheduled; restore with or without device settings | P1 (manual) · P2 (scheduled) | ST W |
| Optional CloudKit sync (Apple) | L | – |

## 11. Band organisation

| Feature | Phase | Source |
|---|---|---|
| Calendar: gigs, rehearsals, recurring events, categories, setlist per event | P3 | BH |
| Availability requests, substitutes per role, lineup per event | P3 | BH W G |
| Reminders (push/mail), iCal feed, CalDAV sync (Nextcloud) | P3 | BH |
| Gig sheet: venue, times, load-in, contacts, parking, set times, dress code, notes | P3 | BH G |
| Members, roles, instruments; parts and PDF packets per role | P3 | W G |
| Files shared with the band | P3 | BH |
| Checklists (packing list, tasks) with assignment and due dates | P4 | BH |
| Public widget/page: upcoming gigs, repertoire | P4 | BH |

## 12. Business

| Feature | Phase | Source |
|---|---|---|
| Contacts (venues, bookers, sound engineers) with log and follow-ups | P4 | BH |
| Income/expenses per gig, fees, split per member, settle-up report | P4 | BH |
| Invoices (PDF), savings funds | P4 | BH |
| Contracts/riders as documents | L | OS |

## 13. Production

| Feature | Phase | Source |
|---|---|---|
| Stage plot builder → input list → monitor plan, generated from the lineup | P4 | BH G |
| Equipment list per gig | P4 | BH |
| Lighting cues via MIDI/OSC to a desk or QLC+; DMX (Art-Net/sACN) | P4 (MIDI/OSC) · L (DMX) | BH ST |
| Video playback / projection on an external display | L | BH |
| LTC/SMPTE out | L | OS |

## 14. Import, export, interoperability

| Feature | Phase | Source |
|---|---|---|
| Import ChordPro, OnSong, OpenSong XML, plain text, PDF, Word/RTF (as attachment) | P1 (ChordPro/OnSong/text/PDF) · P2 | BH ST |
| Import BandHelper's documented TSV layout; generic CSV with column mapping; Setlist Helper (ChordPro export) | P1 | BH |
| Import Cuebook library; Ultimate Guitar links/playlists | P1 | CB |
| Import Stage Traxx playlist text (title/artist matching) | P2 | ST |
| Export everything lossless (folder/zip), ChordPro, TSV, setlist PDF/text, iCal | P1 | W G |
| Settings export/import | P2 | W |
| MusicXML import (view), DAWproject/markers import | L | OS |

## 15. Automation and extensibility (Craft pattern)

| Feature | Phase | Source |
|---|---|---|
| Every action is a command (id + JSON), with undo and inspect | P1 | ArtCraft |
| CLI (`gigcraft-cli`), JSON control channel, MCP server for AI agents | P1 (CLI/MCP basic) · P2 | ArtCraft |
| Scripting/plugins (e.g. WASM or Lua), themes | L | W G |
| Community device profiles and themes repository | P2 | G |

## 16. AI (optional, local, BYO key or local models)

| Feature | Phase | Source |
|---|---|---|
| OCR/PDF import with metadata fill (title, key, tempo) | L | W G |
| Chord detection from audio → editable ChordPro | L | W G |
| Lyric timing against the backing track (Whisper) | L | G |
| Stem separation for practice mixes (Demucs) | L | W G |
| "Build me a setlist of 90 min, mostly upbeat, end with X" (via MCP/AI layer) | L | G |

## 17. Platforms, accessibility, languages

| Feature | Phase | Source |
|---|---|---|
| iPad, iPhone | P1 | all |
| macOS, Windows, Linux (egui) | P1 | ArtCraft |
| Web (wasm): editing, viewer; no MIDI in Safari | P2 | ArtCraft |
| Android | P4 | BH W |
| Apple Watch, visionOS | L | BH ST |
| VoiceOver/screen readers, large type, high contrast, full pedal-only operation | P1 (basics) · P2 | W G |
| German and English; more via craft-i18n catalogs | P1 | ArtCraft |

## 18. Reliability and safety

| Feature | Phase | Source |
|---|---|---|
| Fully offline, no account needed for solo use | P1 | ST W |
| Never crashes on bad files; crash-safe saves; fast with 2,000+ songs | P1 | ArtCraft W |
| Show mode: no updates, no sync surprises, panic button (all notes off, stop audio) | P1 | G |
| Simulated pedal and simulated audio device for rehearsing without hardware | P1 | CB |
| Health check before the gig: files present, devices connected, battery | P2 | G |

## 19. Privacy and legal

No tracking, no ads, no account for solo use; data in readable files; no song database shipped; links instead of scraped tabs; clean-room development (see ADR-0005).
