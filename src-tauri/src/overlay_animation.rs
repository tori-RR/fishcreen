use std::time::{Duration, Instant};

const FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, Copy)]
struct Transition {
    from: f64,
    to: f64,
    started: Instant,
    duration: Duration,
}

/// A time-driven clock shared by both fade directions. Cursor sampling only
/// changes its target; it never decides when an animation frame is rendered.
#[derive(Default)]
pub struct OverlayAnimator {
    value: f64,
    target: f64,
    transition: Option<Transition>,
    next_frame: Option<Instant>,
}

impl OverlayAnimator {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `started` may precede the current tick: idle fade-in is anchored to the
    /// actual idle threshold, not to whichever cursor sample first notices it.
    pub fn set_target(&mut self, target: f64, duration: Duration, started: Instant) {
        let target = target.clamp(0.0, 1.0);
        if self.target == target {
            return;
        }
        let from = self.value_at(started);
        self.target = target;
        self.value = from;
        if duration.is_zero() || from == target {
            self.value = target;
            self.transition = None;
            self.next_frame = Some(started);
        } else {
            self.transition = Some(Transition {
                from,
                to: target,
                started,
                duration,
            });
            self.next_frame = Some(started + FRAME_INTERVAL.min(duration));
        }
    }

    fn value_at(&self, now: Instant) -> f64 {
        match self.transition {
            Some(transition) => {
                let progress = now
                    .saturating_duration_since(transition.started)
                    .as_secs_f64()
                    / transition.duration.as_secs_f64();
                transition.from + (transition.to - transition.from) * progress.clamp(0.0, 1.0)
            }
            None => self.value,
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.next_frame
    }

    /// Skips stale animation deadlines after a delayed controller iteration.
    pub fn tick(&mut self, now: Instant) -> Option<f64> {
        if now < self.next_frame? {
            return None;
        }
        self.value = self.value_at(now);
        if let Some(transition) = self.transition {
            let end = transition.started + transition.duration;
            if now >= end {
                self.value = transition.to;
                self.transition = None;
                self.next_frame = None;
            } else {
                let elapsed_frames = now.saturating_duration_since(transition.started).as_nanos()
                    / FRAME_INTERVAL.as_nanos();
                // Duration is at most five seconds, so this conversion cannot overflow.
                let next = transition.started + FRAME_INTERVAL * (elapsed_frames as u32 + 1);
                self.next_frame = Some(next.min(end));
            }
        } else {
            self.next_frame = None;
        }
        Some(self.value)
    }
}

/// Match the Win32 adapter: any positive opacity has a minimum visible alpha
/// of one; zero means hidden rather than an allocated transparent black window.
pub fn opacity_alpha(opacity: f64) -> u8 {
    if opacity <= 0.0 {
        0
    } else {
        (opacity.clamp(0.0, 1.0) * 255.0).round().max(1.0) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_second_fade_advances_independently_of_cursor_sampling() {
        let start = Instant::now();
        let mut animator = OverlayAnimator::default();
        animator.set_target(1.0, Duration::from_secs(5), start);
        let mut values = Vec::new();
        let mut now = start;
        while let Some(next) = animator.next_deadline() {
            assert!(next > now);
            now = next;
            if let Some(value) = animator.tick(now) {
                values.push(opacity_alpha(value));
            }
        }
        values.dedup();
        assert!(
            values.len() > 200,
            "only {} distinct alpha values",
            values.len()
        );
        assert_eq!(values.last(), Some(&255));
        assert_eq!(now.duration_since(start), Duration::from_secs(5));
        assert!(animator.tick(now + Duration::from_secs(1)).is_none());
    }

    #[test]
    fn repeated_cursor_targets_do_not_restart_the_animation() {
        let start = Instant::now();
        let mut animator = OverlayAnimator::default();
        animator.set_target(1.0, Duration::from_secs(5), start);
        for step in 1..=66 {
            let now = start + Duration::from_millis(step * 75);
            animator.set_target(1.0, Duration::from_secs(5), now);
            animator.tick(now);
        }
        assert_eq!(animator.tick(start + Duration::from_secs(5)), Some(1.0));
        assert!(animator.next_deadline().is_none());
    }

    #[test]
    fn a_late_idle_sample_keeps_the_original_thirty_to_thirty_five_second_window() {
        let activity = Instant::now();
        let threshold = activity + Duration::from_secs(30);
        let mut animator = OverlayAnimator::default();
        animator.set_target(1.0, Duration::from_secs(5), threshold);
        let sampled = animator
            .tick(threshold + Duration::from_millis(75))
            .unwrap();
        assert!((sampled - 0.015).abs() < 1e-9);
        assert_eq!(animator.tick(activity + Duration::from_secs(35)), Some(1.0));
        assert!(animator.next_deadline().is_none());
    }

    #[test]
    fn reversing_both_directions_preserves_the_current_value() {
        let start = Instant::now();
        let mut animator = OverlayAnimator::default();
        animator.set_target(1.0, Duration::from_secs(5), start);
        let half = start + Duration::from_millis(2500);
        assert_eq!(animator.tick(half), Some(0.5));
        animator.set_target(0.0, Duration::from_millis(500), half);
        assert_eq!(animator.value_at(half), 0.5);
        let reverse = half + Duration::from_millis(250);
        assert_eq!(animator.tick(reverse), Some(0.25));
        animator.set_target(1.0, Duration::from_secs(5), reverse);
        assert_eq!(animator.value_at(reverse), 0.25);
        assert!(animator.tick(reverse + Duration::from_millis(16)).unwrap() > 0.25);
    }

    #[test]
    fn completed_hidden_and_immediate_states_have_no_high_frequency_deadline() {
        let start = Instant::now();
        let mut animator = OverlayAnimator::default();
        assert!(animator.next_deadline().is_none());
        animator.set_target(1.0, Duration::ZERO, start);
        assert_eq!(animator.tick(start), Some(1.0));
        assert!(animator.next_deadline().is_none());
        animator.set_target(0.0, Duration::from_millis(500), start);
        assert_eq!(animator.tick(start + Duration::from_millis(500)), Some(0.0));
        assert!(animator.next_deadline().is_none());
        animator.set_target(1.0, Duration::from_secs(5), start);
        animator.reset();
        assert!(animator.next_deadline().is_none());
        assert!(animator.tick(start + Duration::from_secs(1)).is_none());
    }

    #[test]
    fn delayed_ticks_skip_old_deadlines_instead_of_bursting() {
        let start = Instant::now();
        let mut animator = OverlayAnimator::default();
        animator.set_target(1.0, Duration::from_secs(5), start);
        let now = start + Duration::from_millis(777);
        assert!((animator.tick(now).unwrap() - 0.1554).abs() < 1e-9);
        assert_eq!(
            animator.next_deadline(),
            Some(start + Duration::from_millis(784))
        );
        assert!(animator.tick(now).is_none());
    }

    #[test]
    fn alpha_quantization_matches_the_native_adapter() {
        assert_eq!(opacity_alpha(0.0), 0);
        assert_eq!(opacity_alpha(0.0001), 1);
        assert_eq!(opacity_alpha(0.5), 128);
        assert_eq!(opacity_alpha(1.0), 255);
    }
}
