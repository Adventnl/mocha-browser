//! Time-based animation primitives for the browser chrome: easing curves and a
//! small tween that maps wall-clock elapsed time to a `0.0..=1.0` progress, plus
//! an indeterminate loading pulse. All pure and headlessly testable; the window
//! driver advances them once per frame from a single [`std::time::Instant`].

/// Easing functions (all map `t in 0..=1` to `0..=1`, monotonic, f(0)=0, f(1)=1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseOut,
    EaseInOut,
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// A one-shot tween from `from` to `to` over `duration_ms`, eased. Driven by a
/// monotonically increasing "now" in milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    start_ms: u128,
    duration_ms: u128,
    from: f32,
    to: f32,
    easing: Easing,
}

impl Tween {
    pub fn new(now_ms: u128, from: f32, to: f32, duration_ms: u128, easing: Easing) -> Tween {
        Tween {
            start_ms: now_ms,
            duration_ms: duration_ms.max(1),
            from,
            to,
            easing,
        }
    }

    /// The eased value at `now_ms`.
    pub fn value(&self, now_ms: u128) -> f32 {
        let elapsed = now_ms.saturating_sub(self.start_ms);
        let t = (elapsed as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * self.easing.apply(t)
    }

    /// Whether the tween has reached its end at `now_ms`.
    pub fn is_done(&self, now_ms: u128) -> bool {
        now_ms.saturating_sub(self.start_ms) >= self.duration_ms
    }
}

/// An indeterminate progress pulse: returns the x-fraction `0..1` of a moving
/// highlight band, used while a page is loading and real byte progress is
/// unknown. Period ~1.1s.
pub fn indeterminate_pulse(now_ms: u128) -> f32 {
    const PERIOD: u128 = 1100;
    (now_ms % PERIOD) as f32 / PERIOD as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_are_anchored_and_monotonic() {
        for e in [Easing::Linear, Easing::EaseOut, Easing::EaseInOut] {
            assert!((e.apply(0.0) - 0.0).abs() < 1e-6);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6);
            let mut prev = -1.0;
            for i in 0..=10 {
                let v = e.apply(i as f32 / 10.0);
                assert!(v >= prev - 1e-6, "monotonic");
                prev = v;
            }
        }
        assert!(Easing::EaseOut.apply(0.5) > 0.5, "ease-out is fast early");
    }

    #[test]
    fn tween_interpolates_and_clamps() {
        let t = Tween::new(1000, 0.0, 100.0, 200, Easing::Linear);
        assert_eq!(t.value(1000), 0.0);
        assert!((t.value(1100) - 50.0).abs() < 0.01);
        assert_eq!(t.value(1200), 100.0);
        assert_eq!(t.value(5000), 100.0, "clamped past the end");
        assert!(!t.is_done(1100));
        assert!(t.is_done(1200));
    }

    #[test]
    fn pulse_wraps_within_unit_interval() {
        for now in [0u128, 250, 550, 1099, 1100, 3000] {
            let v = indeterminate_pulse(now);
            assert!((0.0..1.0).contains(&v));
        }
    }
}
