# ADR-0011: Interface languages and accessibility

- Status: accepted (Phase 1, 2026-11-25)

## Context
GigCraft ships in English and German first; more languages should be one file each, contributed without touching code. The desktop/web UI is egui (Rust), the iOS UI is SwiftUI. Screen-reader users need the same features as everyone else, including on stage.

## Decision
- **Desktop and web:** English source text is the key. Every string goes through `tr!`/`trn!` and is looked up with craft-i18n (from craft-libs, same pin as the other craft crates) in `crates/gigcraft-ui-egui/i18n/<code>.tsv`. A test extracts every `tr!`/`trn!` string from the source and fails on missing rows. The language follows the system, can be picked in the top bar, and set per run with `--lang`.
- **iOS:** the platform's own string catalog (`Localizable.xcstrings`), so iOS per-app language settings, plural rules and tooling work as users expect. No shared catalog with the desktop: the two UIs have different wording and layout.
- **Core:** stays English (command errors, notices). The few fixed texts the UI shows from the core (rig states and notices, stage action names) are listed in `i18n::CORE_TEXTS` and translated in both catalogs.
- **Accessibility:** eframe is built with AccessKit (VoiceOver, Narrator, Orca); symbol-only controls get spoken names. On iOS, chart lines are one VoiceOver element (lyrics as a sentence, chords as its value), headings are marked, perform mode offers Next/Previous as accessibility actions, and chart sizes follow Dynamic Type.

## Consequences
- craft-i18n brings ICU4X (Unicode-3.0 licence, permissive) and pins it to 2.3.x in `Cargo.lock`.
- Translators edit one TSV file and one string catalog; CI catches gaps on the desktop side.
- Error messages from the core remain English until the core gets message ids.
