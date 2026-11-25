# ADR-0009: UniFFI for Swift (and later Kotlin)

- Status: accepted (Phase 0, 2026-11-25)

## Decision
`gigcraft-ffi` exposes a small object API (`Core`: open/create library, list songs, chart, run any command as JSON) with UniFFI proc-macros. `cargo xtask apple` builds static libraries for iOS device, iOS simulator and macOS, generates Swift bindings and packs an XCFramework that the Xcode project links.

## Why UniFFI
Used in production by Mozilla (Firefox for iOS/Android); one definition for Swift and Kotlin; async and error mapping built in. Licence **MPL-2.0**: file-level copyleft that only applies to UniFFI's own files if modified; using it as a dependency and shipping on the App Store is fine.

## Why a JSON command bridge
The UI calls the same commands as CLI and MCP (`Core.run(id, json)`), so new features need no new FFI surface. Typed helpers exist only where the UI needs speed or type safety (song list, chart).
