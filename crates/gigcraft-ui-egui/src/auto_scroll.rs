//! Auto-scroll: the chart moves by itself, as fast as the song is long.
//! Used by the stage view and the song view; the speed setting is shared.

/// Speed steps (1.0 = the chart goes by in the song's length).
const FACTORS: [f32; 9] = [0.5, 0.6, 0.75, 0.9, 1.0, 1.15, 1.3, 1.6, 2.0];

/// One step slower or faster.
pub(crate) fn step(factor: f32, up: bool) -> f32 {
    let i = FACTORS
        .iter()
        .position(|x| (x - factor).abs() < 0.01)
        .unwrap_or(4);
    let j = if up {
        (i + 1).min(FACTORS.len() - 1)
    } else {
        i.saturating_sub(1)
    };
    FACTORS[j]
}

/// Points per second: the chart goes by in the song's length (a little is
/// left on screen at the end); without a length, about a line every two
/// seconds.
pub(crate) fn speed(
    content: f32,
    view: f32,
    length_secs: Option<u32>,
    size: f32,
    factor: f32,
) -> f32 {
    let base = match length_secs.filter(|s| *s > 20) {
        Some(secs) => ((content - view * 0.6).max(0.0) / secs as f32).max(4.0),
        None => size * 0.6,
    };
    base * if factor > 0.0 { factor } else { 1.0 }
}

/// Auto-scroll state of one scroll area.
#[derive(Debug, Clone, Default)]
pub(crate) struct State {
    /// On: the time of the last frame it moved.
    pub(crate) running: Option<f64>,
    pub(crate) offset: f32,
    pub(crate) view: f32,
    pub(crate) content: f32,
}

impl State {
    pub(crate) fn toggle(&mut self, now: f64) {
        self.running = if self.running.is_some() {
            None
        } else {
            Some(now)
        };
    }

    /// Where the area should be this frame (from wherever it is now, so
    /// scrolling by hand just moves the starting point).
    pub(crate) fn next_offset(
        &self,
        now: f64,
        length_secs: Option<u32>,
        size: f32,
        factor: f32,
    ) -> Option<f32> {
        let last = self.running?;
        let dt = (now - last).clamp(0.0, 0.25) as f32;
        Some(self.offset + speed(self.content, self.view, length_secs, size, factor) * dt)
    }

    /// After the frame: remember the sizes; stop at the end of the chart.
    pub(crate) fn after(&mut self, now: f64, offset: f32, view: f32, content: f32) {
        self.offset = offset;
        if view > 0.0 {
            self.view = view;
        }
        if content > 0.0 {
            self.content = content;
        }
        if self.running.is_some() {
            let end = (self.content - self.view).max(0.0);
            self.running = (offset < end - 0.5).then_some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_stops_at_the_end() {
        assert!((step(1.0, true) - 1.15).abs() < 1e-6);
        assert!((step(0.5, false) - 0.5).abs() < 1e-6);
        // A 2000 pt chart in a 500 pt window over 200 s: (2000 - 300) / 200.
        assert!((speed(2000.0, 500.0, Some(200), 20.0, 1.0) - 8.5).abs() < 1e-4);
        let mut s = State::default();
        s.after(0.0, 0.0, 500.0, 2000.0);
        s.toggle(0.0);
        let next = s.next_offset(0.1, Some(200), 20.0, 1.0).unwrap();
        assert!(next > 0.0);
        s.after(0.1, next, 500.0, 2000.0);
        assert!(s.running.is_some());
        s.after(0.2, 1500.0, 500.0, 2000.0);
        assert!(s.running.is_none());
    }
}
