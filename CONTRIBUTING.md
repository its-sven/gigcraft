# Contributing to GigCraft

Thanks for helping! A few rules keep the project healthy and legally clean.

## Clean room

GigCraft is built from public descriptions of what other apps do (see `docs/research/`), never from their code, UI assets, help texts, icons or file formats that are not publicly documented.

- Do not submit code, text or images copied or derived from BandHelper, Stage Traxx, OnSong or any other proprietary product.
- Do not add song lyrics or chord charts you do not own. Demo content must be your own or in the public domain.
- Store links to chord sites, never scraped content.

## Licences of dependencies

GigCraft is MIT OR Apache-2.0 and ships on app stores. Only permissive dependencies (MIT, Apache-2.0, BSD, Zlib, Unicode-3.0, MPL-2.0 used unmodified). No GPL/LGPL/AGPL.

## Translations

Interface text is written in English and translated per language:

- Desktop and web: `crates/gigcraft-ui-egui/i18n/<code>.tsv` (craft-i18n format: context, English, translation, tab-separated; `@plural` rows hold `one|other`). Every string in the code goes through `tr!`/`trn!`; `cargo test -p gigcraft-ui-egui --test catalog` lists rows that are missing. A new language also needs its code in `OFFERED` in `src/i18n.rs`.
- iOS: `apps/apple/Sources/Localizable.xcstrings` (Xcode string catalog). `xcodebuild -exportLocalizations` shows untranslated strings.

Keep placeholders (`{title}`, `%@`, `%lld`) as they are; their order may change.

## Sign-off

Sign your commits (`git commit -s`) to certify the [Developer Certificate of Origin](https://developercertificate.org/).

## Quality gate

Before a pull request: `cargo xtask ci` (format, clippy without warnings, tests, layering). Core crates must not depend on UI toolkits or binding layers. Code never panics on bad input: errors are values.
