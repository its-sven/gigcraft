# ADR-0014: Audio on iOS – a C function for the real-time callback

- Status: accepted (Phase 2, 2026-11-26)

## Context
The audio engine (`gigcraft-audio`, ADR-0012) renders in the platform's audio callback and must not allocate, lock or block there. On iOS the callback is an `AVAudioSourceNode` render block in Swift. UniFFI calls are not suitable inside it: every call converts arguments and results through buffers it allocates, and errors go through a call status the generated code checks. The workspace forbids `unsafe` (`unsafe_code = "deny"`).

## Decision
- Everything that may allocate stays on UniFFI (`AudioCore`): load and decode a song's files, preload the next song, play, stop, status.
- The audio thread gets the engine's `Renderer` once as an opaque handle (`AudioCore::take_renderer`) and calls two plain C functions exported by `gigcraft-ffi`: `gigcraft_audio_render(handle, out, frames, channels)` and `gigcraft_audio_free_renderer(handle)`. Their header is added to the XCFramework next to the UniFFI header (`cargo xtask apple`).
- `unsafe` is allowed in that one module (`gigcraft-ffi/src/realtime.rs`), with the conditions written next to each block: the handle comes from `take_renderer` and is freed once; `out` holds `frames × channels` floats; only the audio thread calls `render`.
- Swift keeps one preallocated interleaved buffer and copies into the node's channel buffers.

## Consequences
- The renderer runs allocation-free on iOS as on the desktop (cpal).
- iOS starts with a stereo output; routing buses 3/4… to a multichannel interface needs a channel map on the output node (later).
- Two hand-written C declarations must match the Rust signatures; a test in `gigcraft-ffi` calls them like Swift does.
