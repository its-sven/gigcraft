# ADR-0012: The audio engine as a core layer without I/O

- Status: accepted (Phase 2, 2026-11-25)

## Context
Phase 2 plays backing tracks, a click and a count-in on stage, with the click on the in-ears and the tracks on front of house. Each platform has its own audio API (cpal on desktop, AVAudioEngine on iOS, later Android and Web Audio). Audio callbacks run on a real-time thread: no allocation, no locks, no waiting, or the sound drops out. The workspace denies `unsafe`, which rules out the usual hand-written lock-free ring buffers.

## Decision
`gigcraft-audio` holds the whole player without I/O, threads or a clock, like the rig (ADR-0010):
- **Clips** are decoded into memory as `f32` with craft-audio (WAV, AIFF, FLAC, MP3, AAC/M4A, Ogg and more; symphonia, MPL-2.0, covers some formats inside it) and resampled to the output rate once, on the loading thread, with craft-audio-dsp's offline resampler. Files are limited to 2^28 samples.
- **A song** is tracks (gain, pan, mute/solo, an output bus = a stereo pair), a tempo grid, the click settings and markers. Bus *n* is outputs 2n+1/2n+2; a bus the device lacks is dropped and reported, never folded onto front of house.
- **The click** is generated (accent on beat 1, its own bus), on a sample-exact grid from the song's first frame; the count-in runs on a grid ending at the play position.
- **Transport**: play/pause/stop/seek fade over 5 ms; fades and crossfades use two decks; loops wrap and next-bar jumps land sample-exactly, immediate jumps dip for 5 ms. Gain changes glide over 10 ms.
- **Threads**: `engine()` returns a `Controller` (UI thread) and a `Renderer` (audio callback). Commands travel through a bounded `std::sync::mpsc::sync_channel`, used only with `try_send`/`try_recv`: its slots are allocated up front and it takes no lock unless a thread blocks on it, which none does. Songs travel boxed and go back through a second channel to be freed on the control side. Position, state and meters come back through atomics. A test with a counting allocator checks that `render` neither allocates nor frees.
- **Data**: `Song` gains optional `tracks`, `click` and `markers` in song.toml, absent when unused, so older files round-trip unchanged.
- **Hosts** are thin: `gigcraft-audio-cpal` opens a device, picks a configuration (fewest channels that are enough, `f32` preferred) and calls `render` from the callback. Core crates may not depend on cpal or host crates (layering check).

## Consequences
- The same mix, click and transport on every platform, tested offline sample by sample.
- Whole songs live in memory: about 23 MB per stereo track-minute at 48 kHz, fine for stems on a laptop, to be watched on older iPads.
- Commands act at the start of the next audio buffer (a few milliseconds), not on an exact frame; musical timing (loops, next-bar jumps, the click) is sample-exact inside the renderer.
- Speed and pitch are not done: craft-audio-dsp has no drop-in real-time stretcher (see the roadmap).
- A new platform needs only a small host (the cpal one is about 200 lines): open a device, call `render`.
