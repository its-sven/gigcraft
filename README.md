# GigCraft

**An open-source app for gigging musicians and bands:** setlists, chord charts and PDFs, backing tracks and click, MIDI preset control for your rig, band sync on stage, and gig organisation. On iPad, iPhone, Mac, Windows, Linux and in the browser. Offline, no account, your data in readable files.

> Status: **Phase 0 (foundation)**. Songs and setlists can be edited from the command line and an MCP server and viewed in a desktop app and an iPad/iPhone app. Not ready for the stage yet. See the [roadmap](docs/plan/roadmap.md).

GigCraft is an independent project. It follows the principles of the [ArtCraft](https://getartcraft.com/) Craft apps (Rust, local-first, every action scriptable from a CLI or an AI agent) and uses their open-source [craft-libs](https://github.com/storytold/craft-libs), but it is not an official ArtCraft app. It is not affiliated with BandHelper, Stage Traxx, OnSong or any other product named in the research notes.

## Try it

Requirements: Rust (installed via [rustup](https://rustup.rs); the version in `rust-toolchain.toml` is picked automatically). For the iOS app: Xcode 16+ and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```sh
# A library folder with demo songs and a setlist
cargo run -p gigcraft-cli -- init ~/GigCraft-Demo --demo

# Command line: every action is a command with JSON parameters
cargo run -p gigcraft-cli -- run -l ~/GigCraft-Demo --cmd 'song.list={"query":"ballad"}'
cargo run -p gigcraft-cli -- commands

# Desktop app (macOS, Windows, Linux)
cargo run -p gigcraft-desktop -- ~/GigCraft-Demo --song "Harbour Lights"

# MCP server for AI agents (stdio)
cargo run -p gigcraft-cli -- mcp -l ~/GigCraft-Demo

# iPad / iPhone app
cargo xtask apple                 # core as XCFramework + Swift bindings
cd apps/apple && xcodegen && open GigCraft.xcodeproj
```

For a device, copy `apps/apple/Config/Local.xcconfig.example` to `Local.xcconfig` and enter your team ID. On first start the app creates a demo library in its Documents folder (visible in the Files app).

A library is a plain folder: `songs/<name>/song.toml` + `chart.cho` (ChordPro), `setlists/<name>.toml`. Deleted items go to `.trash/`, nothing is ever hard-deleted.

## Development

```sh
cargo xtask ci        # format, clippy, tests, layering check
```

| Path | What |
|---|---|
| `crates/gigcraft-model` | data model: songs, parts, MIDI actions, setlists |
| `crates/gigcraft-chordpro` | ChordPro charts (parser: [ChordSketch](https://github.com/koedame/chordsketch), MIT) |
| `crates/gigcraft-store` | the library on disk, crash-safe saves, search |
| `crates/gigcraft-commands` | every action as a command, undo/redo, demo content |
| `crates/gigcraft-automation` | command lines and the MCP server |
| `crates/gigcraft-ffi` | the core for Swift (UniFFI) |
| `crates/gigcraft-ui-egui` | desktop/web interface |
| `apps/cli`, `apps/desktop`, `apps/apple` | the programs |
| `xtask` | build tasks |

See [CONTRIBUTING.md](CONTRIBUTING.md) for the clean-room and licence rules.

## Documents

| | |
|---|---|
| [Feature catalog](docs/plan/feature-catalog.md) | everything the app should do, by phase |
| [Roadmap](docs/plan/roadmap.md) | phases and work plan |
| [Architecture](docs/plan/architecture.md) | Rust core, native UIs, data format, sync |
| [Hosting](docs/plan/hosting.md) | where the project lives |
| [Decisions (ADRs)](docs/adr/) | recorded decisions |
| [Research](docs/research/) | BandHelper & landscape, Stage Traxx, ArtCraft, further tools & wishlist |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
