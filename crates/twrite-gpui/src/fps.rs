//! Frame-rate HUD for interactive performance testing.
//!
//! GPUI (at the pinned revision) ships no FPS widget, so this module
//! provides a minimal one with zero forced repaints:
//!
//! - [`FrameStats`] samples one timestamp per rendered frame and reports
//!   rolling FPS plus average/worst frame times over a short window.
//! - [`fps_badge`] renders those stats as a color-coded chip hosts can drop
//!   into any status bar.
//!
//! The editor canvas records a sample on every prepaint
//! ([`crate::Editor::frame_stats`]), so the number reflects real editor
//! frames: it updates while interacting (typing, selection drags,
//! scrolling) and freezes when idle — which is correct, since no frames are
//! being produced then.

use std::collections::VecDeque;
use std::time::Instant;

use gpui::{IntoElement, ParentElement, Styled, div, rgb};

/// Number of frame timestamps kept; FPS is computed over the whole window
/// (~2s at 60fps).
const WINDOW: usize = 120;

/// Rolling frame-time statistics sampled once per rendered frame.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    frames: VecDeque<Instant>,
}

impl FrameStats {
    /// Creates empty stats (no samples yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// Samples the current time as one rendered frame.
    pub fn record(&mut self) {
        self.record_at(Instant::now());
    }

    /// Samples `now` as one rendered frame.
    ///
    /// Takes the timestamp explicitly so tests can inject exact intervals
    /// and hosts can forward a timestamp they already hold.
    pub fn record_at(&mut self, now: Instant) {
        // Timestamps must be monotonic: a clock step backwards would produce
        // a negative span and a bogus spike, so drop regressions.
        if self.frames.back().is_some_and(|&last| now < last) {
            return;
        }
        if self.frames.len() >= WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back(now);
    }

    /// Number of samples currently in the window.
    pub fn samples(&self) -> usize {
        self.frames.len()
    }

    /// Rolling frames-per-second over the window, or `None` with < 2 samples.
    pub fn fps(&self) -> Option<f32> {
        let n = self.frames.len();
        if n < 2 {
            return None;
        }
        let span = self.span_secs()?;
        if span <= 0.0 {
            return None;
        }
        Some((n - 1) as f32 / span)
    }

    /// Mean frame interval in milliseconds, or `None` with < 2 samples.
    pub fn avg_ms(&self) -> Option<f32> {
        let n = self.frames.len();
        if n < 2 {
            return None;
        }
        let span = self.span_secs()?;
        if span <= 0.0 {
            return None;
        }
        Some(span * 1000.0 / (n - 1) as f32)
    }

    /// Worst single frame interval in the window in milliseconds.
    ///
    /// Useful for selection-jank testing: FPS can look fine on average while
    /// individual frames blow the 16.6ms budget.
    pub fn worst_ms(&self) -> Option<f32> {
        if self.frames.len() < 2 {
            return None;
        }
        self.frames
            .iter()
            .zip(self.frames.iter().skip(1))
            .map(|(a, b)| (*b - *a).as_secs_f32() * 1000.0)
            .reduce(f32::max)
    }

    /// Drops all samples (e.g. when opening a new document to test).
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// Compact one-line summary (`"60 fps · 16.6 ms avg · 18.1 ms worst"`),
    /// or `"fps: …"` before enough samples exist.
    pub fn text(&self) -> String {
        match (self.fps(), self.avg_ms(), self.worst_ms()) {
            (Some(fps), Some(avg), Some(worst)) => {
                format!("{fps:.0} fps · {avg:.1} ms avg · {worst:.1} ms worst")
            }
            _ => "fps: …".to_string(),
        }
    }

    fn span_secs(&self) -> Option<f32> {
        let (first, last) = (self.frames.front()?, self.frames.back()?);
        Some((*last - *first).as_secs_f32())
    }
}

/// Renders [`FrameStats`] as a color-coded status-bar chip.
///
/// Green at ≥ 50fps, amber at ≥ 25fps, red below — so a selection-drag
/// regression is visible at a glance. Returns a plain `div`, so hosts can
/// drop it anywhere (status bars, overlays).
pub fn fps_badge(stats: &FrameStats) -> impl IntoElement {
    let color = match stats.fps() {
        Some(fps) if fps >= 50.0 => rgb(0xa6e3a1),
        Some(fps) if fps >= 25.0 => rgb(0xf9e2af),
        Some(_) => rgb(0xf38ba8),
        None => rgb(0x6c7086),
    };
    div()
        .text_xs()
        .px_2()
        .py_0p5()
        .rounded_md()
        .bg(rgb(0x313244))
        .text_color(color)
        .child(stats.text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn stats_at(ms: &[u64]) -> FrameStats {
        let mut s = FrameStats::new();
        let t0 = Instant::now();
        for &m in ms {
            s.record_at(t0 + Duration::from_millis(m));
        }
        s
    }

    #[test]
    fn empty_and_singleton_yield_no_reading() {
        let empty = FrameStats::new();
        assert_eq!(empty.samples(), 0);
        assert_eq!(empty.fps(), None);
        assert_eq!(empty.avg_ms(), None);
        assert_eq!(empty.worst_ms(), None);
        assert_eq!(empty.text(), "fps: …");

        let mut one = FrameStats::new();
        one.record();
        assert_eq!(one.samples(), 1);
        assert_eq!(one.fps(), None);
    }

    #[test]
    fn steady_60fps_reports_60fps() {
        // Four frames at exact 16ms intervals: 3 intervals / 48ms = 62.5fps.
        let s = stats_at(&[0, 16, 32, 48]);
        assert!((s.fps().unwrap() - 62.5).abs() < 0.01);
        assert!((s.avg_ms().unwrap() - 16.0).abs() < 0.01);
        assert!((s.worst_ms().unwrap() - 16.0).abs() < 0.01);
        assert_eq!(s.text(), "62 fps · 16.0 ms avg · 16.0 ms worst");
    }

    #[test]
    fn worst_frame_tracks_the_spike() {
        // Two smooth frames then one 100ms hitch.
        let s = stats_at(&[0, 16, 32, 132]);
        assert!((s.worst_ms().unwrap() - 100.0).abs() < 0.01);
        // Average dilutes the spike: 132ms / 3 intervals = 44ms.
        assert!((s.avg_ms().unwrap() - 44.0).abs() < 0.01);
    }

    #[test]
    fn window_is_bounded() {
        let t0 = Instant::now();
        let mut s = FrameStats::new();
        for i in 0..(WINDOW + 50) {
            s.record_at(t0 + Duration::from_millis(i as u64));
        }
        assert_eq!(s.samples(), WINDOW);
        assert!(s.fps().unwrap() > 900.0);
    }

    #[test]
    fn clock_regression_is_dropped() {
        let t0 = Instant::now();
        let mut s = FrameStats::new();
        s.record_at(t0);
        s.record_at(t0 + Duration::from_millis(16));
        s.record_at(t0); // backwards: ignored
        assert_eq!(s.samples(), 2);
    }

    #[test]
    fn clear_resets_to_no_reading() {
        let mut s = stats_at(&[0, 16, 32]);
        assert!(s.fps().is_some());
        s.clear();
        assert_eq!(s.samples(), 0);
        assert_eq!(s.fps(), None);
    }
}
