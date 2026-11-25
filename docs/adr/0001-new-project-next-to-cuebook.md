# ADR-0001: GigCraft is a new project next to Cuebook

- Status: accepted (Sven, 2026-11-25)

## Context
Cuebook (iOS, SwiftUI) does setlists, a pedal preset per song part and Ultimate Guitar links. The goal now is a complete open-source alternative to BandHelper and Stage Traxx on all platforms, with a different architecture (Rust core).

## Decision
Start a new repository `gigcraft`. Cuebook stays a small, finished iOS app and the reference for the Apple shell and the first device profile.

## Consequences
- Clean history and structure for a multi-platform monorepo.
- Code from Cuebook is ported deliberately (MIDI layer, pedal logic, UG import), not moved wholesale.
- Later: a Cuebook → GigCraft importer.
