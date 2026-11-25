# ADR-0013: Live sessions on the local network

- Status: accepted (Phase 2, 2026-11-25)

## Context
On stage one musician leads the setlist; the others need the same song, their own chart (key, capo, instrument part) and their own pedal presets. Venues have unreliable or no internet; band members use phones, tablets and laptops, not all with GigCraft installed.

## Decision
- **Protocol in the core without I/O** (`gigcraft-live`, like the rig in ADR-0010): JSON messages tagged by `type`, versioned (`PROTOCOL`), unknown fields ignored. The leader sends a `Snapshot` (setlist position, current and next item; the current song with its ChordPro source and the parsed chart in the gig key); followers join with name and instrument, say ready, and send cues. Ready resets when the song changes.
- **One LAN port** (`gigcraft-live-net`, desktop): `/` serves a self-contained web viewer, `/ws` the WebSocket. Band members without the app scan a QR code and follow in the browser. Other GigCraft apps join the same address; they show their own chart if their library has the song and send their own presets to the devices in their own rig.
- **iOS** follows through `URLSessionWebSocketTask`; the state machine is the same Rust code (`FollowerCore` via UniFFI).
- **No relay, no account, no TLS.** Nothing leaves the local network. Browsers cannot verify certificates for private addresses, so TLS would only add warnings. Followers can only say ready and send cues; only the leader moves the show.

## Consequences
- Works on a router or a phone hotspot without internet.
- Anyone on the same network can follow and send cues; on shared venue Wi-Fi the band should use its own hotspot (documented in the help).
- Discovery is by address/QR code; Bonjour and Apple's MultipeerConnectivity can be added as further transports without changing the protocol.
