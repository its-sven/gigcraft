# Research: the ArtCraft "Craft App Suite" as a model for GigCraft

*Read 2026-11-25: getartcraft.com (/, /apps, /faq), GitHub org `storytold` (repo list, READMEs of craft-libs, craft-launcher, soundcraft, deckcraft). Summary in my own words.*

## 1. What ArtCraft is

- A suite of ~17 desktop apps, each a clean-room rebuild of a professional tool "from scratch in pure Rust": PhotoCraft, LightCraft, VectorCraft, FilmCraft, EffectCraft, AniCraft, DesignCraft, FigCraft, PdfCraft, WordCraft, DeckCraft, GridCraft, CADCraft, **SoundCraft** (a DAW), TextureCraft, MaterialCraft, Craft Anything; plus ArtCraft Launcher (installer/updater) and ArtCraft Studio (generative AI) [1][2].
- GitHub org **storytold**. Apps: Apache-2.0 on GitHub, READMEs state **MIT OR Apache-2.0**; shared libraries `craft-libs` Apache-2.0 [3][4].
- Status: mostly "in development" / "early alpha".
- The launcher already lists two **community apps** from other makers: SolidCraft (Tangency Labs, parametric CAD) and MailCraft (UCS Labs, mail/calendar/contacts) [5]. The website names no process or criteria for community apps; the only route mentioned is the ArtCraft Discord [1].

## 2. Shared principles (what we copy)

| Principle | How the Craft apps do it | GigCraft |
|---|---|---|
| Pure Rust, native | egui UI, no Electron, no web view | Rust core; egui for desktop/web |
| Platforms | macOS (universal), Windows (x64/x86/Arm), Linux (AppImage, deb, rpm, Flatpak), FreeBSD, **WebAssembly in the browser** | Same for desktop/web, **plus iOS/iPadOS and Android**, which the Craft apps do not cover |
| Local first | files on your machine, no cloud round-trips, no account, no subscription | Same; optional self-hosted sync |
| **Agent-ready** | every click is a *command* with an id and JSON params; the same commands reachable from menus, shortcuts, a CLI (`<app>-cli run --cmd …`), a JSON control channel on a port, and an **MCP server**; programmatic calls never open dialogs; an `inspect` command reports what changed | Same command model in the core, so the band app can be driven by scripts, a stage controller and an AI agent |
| Strict layering | a Cargo workspace of small crates; nothing below the UI knows egui, so the UI can be swapped | Exactly what lets us put SwiftUI and Kotlin UIs on the same core |
| Robustness | never panic on bad input, errors are values, hostile-input-bounded parsers | Essential on stage |
| Quality gate | `cargo xtask ci`: fmt, clippy, tests, asset and layering checks, wasm build; SonarCloud, CodeRabbit | Same gate; add iOS/Android builds |
| i18n | 15 UI languages, `craft-i18n` catalogs | DE + EN first, then via the same catalogs |
| Distribution | GitHub releases with SHA-256 checksums, picked up by ArtCraft Launcher | Same; App Store / TestFlight / Play / F-Droid for mobile |

## 3. Reusable `craft-libs` crates (Apache-2.0, Git tag pins)

Relevant for a band app (check each crate's README and maturity before adopting):

| Crate | Use in GigCraft |
|---|---|
| `craft-audio` | audio file I/O and codecs (WAV, AIFF, FLAC, AAC, Opus, MP3 encoding), streaming, seeking: backing tracks |
| `craft-midi` | Standard MIDI Files (read 0/1/2, write 1): MIDI file playback per song |
| `craft-time` | exact rational ticks, SMPTE, tempo maps, grids: regions, click, MTC |
| `craft-sf2` | SoundFont 2 reader: built-in fallback synth for MIDI files |
| `craft-pdf` | PDF parsing/rasterising/authoring: charts, setlist PDFs |
| `craft-ical`, `craft-dav`, `craft-vcard` | iCalendar, CalDAV/CardDAV: gigs calendar, contacts, Nextcloud |
| `craft-csv` | CSV/TSV with encoding and delimiter sniffing: BandHelper TSV, Setlist Helper CSV import |
| `craft-markdown`, `craft-text` | notes, lyrics formatting |
| `craft-xml` | OpenSong XML, MusicXML import |
| `craft-zip` | archive import/export, backups |
| `craft-settings`, `craft-atomic` | config storage, crash-safe saves |
| `craft-i18n`, `craft-ui`, `craft-dialogs`, `craft-keymap`, `craft-menu`, `craft-print`, `craft-quit` | desktop UI shell like the other Craft apps |
| `craft-ai-core`, `craft-ai` | BYO-key / local-model AI layer (chord detection, stem separation, lyric timing as optional extras) |
| `craft-xtask`, `craft-testkit` | CI gate and test helpers |

SoundCraft's own crates (mix engine, playback on CoreAudio/WASAPI/ALSA/WebAudio, DSP) show how an audio engine is structured, but they are DAW-sized; we take ideas and possibly crates, not the whole engine.

## 4. Gaps for a stage app

- **No iOS/iPadOS/Android** in the suite. A stage app lives on iPad/iPhone, so we need native mobile shells over the Rust core (SwiftUI via UniFFI; Kotlin via UniFFI later). This also gives CoreMIDI, AVAudioEngine, AirTurn, background audio and App Store compliance.
- No real-time network/multi-device features in the libs (leader/follower, sync). We build those.
- Web build: Safari (and so iPad browsers) has no Web MIDI, so the web version is for editing/preparation and viewing, not the main stage client on iPad.

## 5. Naming and "community app" status

- "GigCraft" follows the suite's naming. Until ArtCraft lists it as a community app, the README should say plainly that it is an independent project *inspired by and built on* craft-libs, not an official ArtCraft app, and use its own logo.
- Step for later: introduce the project in the ArtCraft Discord and ask about community-app listing (as SolidCraft and MailCraft did).
- Before going public: check "GigCraft" in EUIPO/USPTO/DPMA and the App Store; check the domain/org names (see [../plan/hosting.md](../plan/hosting.md)).

## Sources

1. https://getartcraft.com/apps, https://getartcraft.com/, https://getartcraft.com/faq
2. https://github.com/storytold/craft-launcher (README)
3. https://github.com/storytold/craft-libs (README, crate list)
4. https://github.com/storytold/soundcraft (README: "For agents and scripts", "How it's built")
5. https://github.com/storytold/deckcraft (README, Crafting Apps table incl. community apps)
