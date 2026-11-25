# ADR-0005: MIT OR Apache-2.0, clean room

- Status: proposed

## Decision
- Licence **MIT OR Apache-2.0** (dual, like the Craft apps and most Rust crates). craft-libs (Apache-2.0) are compatible.
- No GPL/LGPL dependencies (App Store conflicts; e.g. Rubber Band, OpenSongApp code).
- Clean room: features are built from public descriptions in `docs/research/`; no code, UI assets, texts or icons from BandHelper, Stage Traxx, OnSong etc.; no scraping of chord sites (links only); no song database shipped.
- Contributions with DCO sign-off; `CONTRIBUTING.md` states the clean-room rule.
- Name, logo and README make clear GigCraft is independent and not an official ArtCraft app unless ArtCraft lists it as a community app.
