# ADR-0002: Rust core with native UIs

- Status: accepted (Sven, 2026-11-25)

## Context
The ArtCraft apps are pure Rust/egui (desktop + web) and have no iOS/Android build. On stage most musicians use iPad/iPhone, which need CoreMIDI, AVAudioSession, AirTurn/HID pedals, background audio and App Store-conform UX. Safari has no Web MIDI.

## Options
1. **Rust core + egui desktop/web + SwiftUI iOS (UniFFI) + Kotlin Android later** (chosen)
2. Everything in Rust/egui (iPad only via web or experimental egui-on-iOS)
3. Swift first (Apple only)

## Decision
Option 1. All logic (model, formats, commands, perform state, MIDI logic, audio render graph, sync, automation) lives in Rust crates that know no UI. UIs are thin.

## Consequences
- One implementation of the hard parts, tested headless.
- Two UI codebases to maintain (egui, SwiftUI), later a third (Kotlin).
- Build tooling for XCFramework (cargo + UniFFI + xtask) needed from day one.
