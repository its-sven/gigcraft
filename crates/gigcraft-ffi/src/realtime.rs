//! The audio thread's entry points (ADR-0014): plain C functions, because the
//! render callback must not allocate or lock, which UniFFI calls would.
#![allow(
    unsafe_code,
    reason = "C ABI for the real-time audio callback, see ADR-0014"
)]

use gigcraft_audio::Renderer;
use std::ffi::c_void;

/// Render `frames` interleaved frames of `channels` channels into `out`.
///
/// # Safety
/// `renderer` comes from `AudioCore::take_renderer` and has not been freed;
/// only one thread (the audio thread) calls this for a given renderer; `out`
/// points to at least `frames * channels` writable `f32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gigcraft_audio_render(
    renderer: *mut c_void,
    out: *mut f32,
    frames: u32,
    channels: u32,
) {
    if renderer.is_null() || out.is_null() || channels == 0 || frames == 0 {
        return;
    }
    let len = frames as usize * channels as usize;
    // SAFETY: the caller guarantees `renderer` is a live `Box<Renderer>` from
    // `take_renderer`, used by this thread only, and `out` holds `len` floats.
    let (renderer, out) = unsafe {
        (
            &mut *renderer.cast::<Renderer>(),
            std::slice::from_raw_parts_mut(out, len),
        )
    };
    renderer.render(out, channels as usize);
}

/// Free a renderer handle (after the audio unit has stopped).
///
/// # Safety
/// `renderer` comes from `AudioCore::take_renderer`, is freed only once, and
/// no thread renders with it any more.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gigcraft_audio_free_renderer(renderer: *mut c_void) {
    if renderer.is_null() {
        return;
    }
    // SAFETY: see above; the pointer came from `Box::into_raw`.
    drop(unsafe { Box::from_raw(renderer.cast::<Renderer>()) });
}

/// The renderer as a handle for the functions above.
pub(crate) fn into_handle(renderer: Renderer) -> u64 {
    Box::into_raw(Box::new(renderer)) as usize as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioCore, Core};

    /// The way the iOS app drives it: load on a worker, render through the C ABI.
    #[test]
    fn renders_the_click_through_the_c_functions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib").display().to_string();
        let core = Core::open_or_create_demo(path, "Demo".into()).unwrap();
        let song = core
            .songs(String::new())
            .into_iter()
            .find(|s| s.title == "Harbour Lights")
            .unwrap();
        let audio = AudioCore::new(48_000, 512);
        assert!(audio.has_audio(core.clone(), song.id.clone()));
        audio.load_song(core.clone(), song.id.clone()).unwrap();
        let handle = audio.take_renderer();
        assert_ne!(handle, 0);
        assert_eq!(audio.take_renderer(), 0);
        audio.play(true).unwrap();
        let mut buf = vec![0.0f32; 512 * 2];
        let mut loud = 0;
        for _ in 0..100 {
            // SAFETY: the handle is live and `buf` holds 512 stereo frames.
            unsafe {
                gigcraft_audio_render(handle as usize as *mut c_void, buf.as_mut_ptr(), 512, 2)
            };
            loud += buf.iter().filter(|s| s.abs() > 0.01).count();
        }
        assert!(loud > 0, "the click should be audible");
        assert!(audio.status().playing);
        audio.stop().unwrap();
        // Null and empty arguments are ignored.
        // SAFETY: null pointers are checked first.
        unsafe { gigcraft_audio_render(std::ptr::null_mut(), buf.as_mut_ptr(), 512, 2) };
        // SAFETY: freed exactly once, nothing renders any more.
        unsafe { gigcraft_audio_free_renderer(handle as usize as *mut c_void) };
    }

    /// The header Swift compiles against declares exactly these functions.
    #[test]
    fn the_c_header_matches() {
        let h = include_str!("../include/gigcraft_audio.h");
        assert!(h.contains("void gigcraft_audio_render(void *_Nullable renderer, float *_Nullable out, uint32_t frames, uint32_t channels);"));
        assert!(h.contains("void gigcraft_audio_free_renderer(void *_Nullable renderer);"));
    }
}
