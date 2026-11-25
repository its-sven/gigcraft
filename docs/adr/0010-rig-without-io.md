# ADR-0010: The rig as a core layer without I/O

- Status: accepted (Phase 1, 2026-11-25)

## Context
Songs name devices in their MIDI actions ("Play Acoustic", "Helix"). Each platform has its own MIDI API (midir on desktop, CoreMIDI on iOS, later Android MIDI and Web MIDI). Some devices need more than bytes: the Play Acoustic ignores Program Change on USB and is switched by stepping its buttons with read-back.

## Decision
`gigcraft-rig` holds the whole device logic without I/O and without a clock:
- configuration per computer (devices by name → port, profile, simulated; MIDI Learn mappings), stored by the app, not in the library;
- `send_part`, `on_input`, `tick`, `ports_changed`, `panic` return outputs (`send`, `notice`, `action`, `learned`, `config_changed`);
- the Play Acoustic driver and a simulated pedal from `gigcraft-midi`.
Platforms only move bytes and call `tick` at `next_deadline`. The iOS app uses it through `RigCore` (UniFFI, JSON outputs).

## Consequences
- Identical behaviour on every platform, fully testable with simulated time.
- A new MIDI platform needs about 150 lines of glue.
- The perform engine stays device-agnostic: it emits `send_part` effects with the actions; the app hands them to the rig.
