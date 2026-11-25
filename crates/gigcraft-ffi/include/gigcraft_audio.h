// GigCraft real-time audio (ADR-0014). The Rust side is gigcraft-ffi/src/realtime.rs.
#pragma once
#include <stdint.h>

/// Render interleaved frames; call only from the audio thread.
void gigcraft_audio_render(void *_Nullable renderer, float *_Nullable out, uint32_t frames, uint32_t channels);
/// Free a renderer handle after the audio unit has stopped.
void gigcraft_audio_free_renderer(void *_Nullable renderer);
