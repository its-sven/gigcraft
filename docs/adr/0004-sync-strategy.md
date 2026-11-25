# ADR-0004: Sync in three layers

- Status: proposed

1. **Live on stage (Phase 2):** leader/follower protocol with small, transport-neutral messages. MultipeerConnectivity between Apple devices, Bonjour + WebSocket on a LAN for mixed devices and the web viewer. Followers send their own MIDI; MIDI is never relayed over the network.
2. **Folder sync (Phase 1):** the plain-file library in any synced folder. Last writer wins per file, conflicts shown to the user.
3. **Band sync (Phase 3):** self-hosted `gigcraft-server` (Docker) with accounts per band, roles (admin, member, read-only), personal fields, change history and rollback. Data model: per-record versions with field-level merge; evaluate Automerge (MIT, Swift and Rust bindings) before building our own.

CloudKit is optional later (it needs the paid Apple Developer Program, which the App Store release needs anyway).
