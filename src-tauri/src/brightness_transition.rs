use std::{
    thread,
    time::{Duration, Instant},
};

const CANCELLATION_INTERVAL: Duration = Duration::from_millis(10);

/// A transition-local, monotonic clock. Tests can advance it without sleeping.
pub trait Clock {
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

pub struct RealClock(Instant);

impl RealClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Clock for RealClock {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TransitionReport {
    pub completed: bool,
    pub writes: usize,
    /// Samples discarded as obsolete or equal to the last successful write.
    /// Future samples left untouched by cancellation are not counted here.
    pub skipped: usize,
    pub elapsed: Duration,
}

fn report(clock: &impl Clock, completed: bool, writes: usize, skipped: usize) -> TransitionReport {
    TransitionReport {
        completed,
        writes,
        skipped,
        elapsed: clock.elapsed(),
    }
}

/// Calculate the same fraction of a Duration without a floating-point clock or
/// multiplying the whole nanosecond count by an unbounded sample number.
fn deadline(duration: Duration, frame: usize, count: usize) -> Duration {
    if frame == count {
        return duration;
    }
    let nanos = duration.as_nanos();
    let denominator = count as u128;
    let numerator = frame as u128;
    let scaled = nanos / denominator * numerator + (nanos % denominator) * numerator / denominator;
    Duration::new(
        (scaled / 1_000_000_000) as u64,
        (scaled % 1_000_000_000) as u32,
    )
}

/// Run a serial transition using the existing precomputed brightness curve.
///
/// A blocking write cannot be interrupted. When it returns, cancellation is
/// checked first and the next sample is chosen from the *current* elapsed time,
/// never from the backlog of missed deadlines. The final target is not issued
/// before `duration`, and completion requires a matching readback that was not
/// superseded while the read itself was blocked.
#[allow(clippy::too_many_arguments)] // Explicit I/O injection keeps the hardware path and tests identical.
pub fn run_transition(
    clock: &impl Clock,
    values: &[u32],
    start: u32,
    target: u32,
    duration: Duration,
    mut is_cancelled: impl FnMut() -> bool,
    mut write: impl FnMut(u32) -> Result<(), String>,
    read: impl FnOnce() -> Result<u32, String>,
) -> Result<TransitionReport, String> {
    if values.last().is_some_and(|value| *value != target) || (values.is_empty() && start != target)
    {
        return Err("亮度渐变帧终点无效，未执行硬件操作".into());
    }
    let mut next = 0;
    let mut last_written = start;
    let mut writes = 0;
    let mut skipped = 0;
    while next < values.len() {
        if is_cancelled() {
            return Ok(report(clock, false, writes, skipped));
        }
        let elapsed = clock.elapsed();
        let due = deadline(duration, next + 1, values.len());
        if elapsed < due {
            clock.sleep((due - elapsed).min(CANCELLATION_INTERVAL));
            continue;
        }

        let mut selected = next;
        while selected + 1 < values.len()
            && deadline(duration, selected + 2, values.len()) <= elapsed
        {
            selected += 1;
        }
        skipped += selected - next;
        let value = values[selected];
        // The production curve keeps the endpoint unique, but retaining this
        // guard also makes a duplicate/clipped endpoint respect total duration.
        if value == target && elapsed < duration {
            skipped += 1;
            next = selected + 1;
            continue;
        }
        if value != last_written {
            if is_cancelled() {
                return Ok(report(clock, false, writes, skipped));
            }
            write(value)?;
            writes += 1;
            last_written = value;
            if is_cancelled() {
                return Ok(report(clock, false, writes, skipped));
            }
        } else {
            skipped += 1;
        }
        next = selected + 1;
    }

    if is_cancelled() {
        return Ok(report(clock, false, writes, skipped));
    }
    let current = read()?;
    if is_cancelled() {
        return Ok(report(clock, false, writes, skipped));
    }
    if current != target {
        return Err(format!("亮度读回不一致：目标 {target}，实际 {current}"));
    }
    Ok(report(clock, true, writes, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeClock(Cell<Duration>);

    impl FakeClock {
        fn advance(&self, duration: Duration) {
            self.0.set(self.0.get() + duration);
        }
    }

    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.0.get()
        }

        fn sleep(&self, duration: Duration) {
            self.advance(duration);
        }
    }

    fn descending_samples(count: usize) -> Vec<u32> {
        (1..=count)
            .map(|frame| (100 * (count - frame) / count) as u32)
            .collect()
    }

    fn timed_transition(write_ms: u64, read_ms: u64) -> (TransitionReport, Vec<(Duration, u32)>) {
        let clock = FakeClock::default();
        let current = Cell::new(100);
        let events = RefCell::new(Vec::new());
        let values = descending_samples(30);
        let result = run_transition(
            &clock,
            &values,
            100,
            0,
            Duration::from_millis(300),
            || false,
            |value| {
                events.borrow_mut().push((clock.elapsed(), value));
                clock.advance(Duration::from_millis(write_ms));
                current.set(value);
                Ok(())
            },
            || {
                clock.advance(Duration::from_millis(read_ms));
                Ok(current.get())
            },
        )
        .unwrap();
        (result, events.into_inner())
    }

    #[test]
    fn fast_device_obeys_every_deadline_and_exact_final_duration() {
        let (result, events) = timed_transition(0, 40);
        assert!(result.completed);
        assert_eq!(result.writes, 30);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.elapsed, Duration::from_millis(340));
        for (index, (at, _)) in events.iter().enumerate() {
            assert_eq!(*at, Duration::from_millis(10 * (index as u64 + 1)));
        }
        assert_eq!(events.last(), Some(&(Duration::from_millis(300), 0)));
    }

    #[test]
    fn fifty_ms_writes_skip_expired_samples_instead_of_sending_all_thirty() {
        let (result, events) = timed_transition(50, 40);
        assert!(result.completed);
        assert_eq!(result.writes, 7);
        assert_eq!(result.skipped, 23);
        assert_eq!(result.elapsed, Duration::from_millis(400));
        let times: Vec<_> = events.iter().map(|event| event.0.as_millis()).collect();
        assert_eq!(times, [10, 60, 110, 160, 210, 260, 310]);
        assert_eq!(events.last().unwrap().1, 0);
        assert!(events.windows(2).all(|pair| pair[0].1 > pair[1].1));
    }

    #[test]
    fn two_hundred_ms_writes_do_not_accumulate_a_backlog() {
        let (result, events) = timed_transition(200, 40);
        assert!(result.completed);
        assert_eq!(result.writes, 3);
        assert_eq!(result.skipped, 27);
        assert_eq!(result.elapsed, Duration::from_millis(650));
        assert_eq!(events.last(), Some(&(Duration::from_millis(410), 0)));
    }

    #[test]
    fn one_write_longer_than_duration_is_followed_only_by_the_endpoint() {
        let (result, events) = timed_transition(1000, 5);
        assert_eq!(result.writes, 2);
        assert_eq!(result.skipped, 28);
        assert_eq!(events.last(), Some(&(Duration::from_millis(1010), 0)));
        assert_eq!(result.elapsed, Duration::from_millis(2015));
    }

    #[test]
    fn equal_integer_samples_and_the_initial_value_are_not_repeated() {
        let clock = FakeClock::default();
        let events = RefCell::new(Vec::new());
        let result = run_transition(
            &clock,
            &[100, 80, 80, 40, 40, 0],
            100,
            0,
            Duration::from_millis(600),
            || false,
            |value| {
                events.borrow_mut().push((clock.elapsed(), value));
                Ok(())
            },
            || Ok(0),
        )
        .unwrap();
        assert_eq!(result.writes, 3);
        assert_eq!(result.skipped, 3);
        assert_eq!(
            events.into_inner(),
            vec![
                (Duration::from_millis(200), 80),
                (Duration::from_millis(400), 40),
                (Duration::from_millis(600), 0),
            ]
        );
    }

    #[test]
    fn zero_duration_sends_only_the_final_target_immediately() {
        let clock = FakeClock::default();
        let events = RefCell::new(Vec::new());
        let result = run_transition(
            &clock,
            &[90, 80, 60, 0],
            100,
            0,
            Duration::ZERO,
            || false,
            |value| {
                events.borrow_mut().push((clock.elapsed(), value));
                Ok(())
            },
            || Ok(0),
        )
        .unwrap();
        assert_eq!(result.writes, 1);
        assert_eq!(result.skipped, 3);
        assert_eq!(events.into_inner(), vec![(Duration::ZERO, 0)]);
    }

    #[test]
    fn duplicate_endpoints_are_deferred_until_the_final_deadline() {
        let clock = FakeClock::default();
        let events = RefCell::new(Vec::new());
        let result = run_transition(
            &clock,
            &[50, 0, 0],
            100,
            0,
            Duration::from_millis(300),
            || false,
            |value| {
                events.borrow_mut().push((clock.elapsed(), value));
                Ok(())
            },
            || Ok(0),
        )
        .unwrap();
        assert_eq!(result.writes, 2);
        assert_eq!(result.skipped, 1);
        assert_eq!(
            events.into_inner(),
            vec![
                (Duration::from_millis(100), 50),
                (Duration::from_millis(300), 0),
            ]
        );
    }

    #[test]
    fn equal_start_and_target_still_verify_without_a_write_or_wait() {
        let clock = FakeClock::default();
        let reads = Cell::new(0);
        let result = run_transition(
            &clock,
            &[],
            25,
            25,
            Duration::from_millis(300),
            || false,
            |_| panic!("unchanged brightness must not be written"),
            || {
                reads.set(reads.get() + 1);
                Ok(25)
            },
        )
        .unwrap();
        assert_eq!(result.writes, 0);
        assert_eq!(reads.get(), 1);
        assert!(result.completed);
        assert_eq!(clock.elapsed(), Duration::ZERO);
    }

    #[test]
    fn low_brightness_span_preserves_duration_and_restoration_endpoint() {
        let clock = FakeClock::default();
        let events = RefCell::new(Vec::new());
        run_transition(
            &clock,
            &[99, 100],
            98,
            100,
            Duration::from_millis(301),
            || false,
            |value| {
                events.borrow_mut().push((clock.elapsed(), value));
                Ok(())
            },
            || Ok(100),
        )
        .unwrap();
        assert_eq!(
            events.into_inner(),
            vec![
                (Duration::from_micros(150500), 99),
                (Duration::from_millis(301), 100),
            ]
        );
    }

    #[test]
    fn cancellation_before_start_never_reads_or_writes() {
        let clock = FakeClock::default();
        let result = run_transition(
            &clock,
            &[0],
            100,
            0,
            Duration::ZERO,
            || true,
            |_| panic!("cancelled write"),
            || panic!("cancelled read"),
        )
        .unwrap();
        assert!(!result.completed);
        assert_eq!(result.writes, 0);
    }

    #[test]
    fn cancellation_during_wait_is_checked_in_bounded_sleep_slices() {
        let clock = FakeClock::default();
        let result = run_transition(
            &clock,
            &[0],
            100,
            0,
            Duration::from_millis(300),
            || clock.elapsed() >= Duration::from_millis(5),
            |_| panic!("cancelled write"),
            || panic!("cancelled read"),
        )
        .unwrap();
        assert!(!result.completed);
        assert_eq!(result.writes, 0);
        assert_eq!(clock.elapsed(), Duration::from_millis(10));
    }

    #[test]
    fn cancellation_during_blocked_write_stops_before_another_io() {
        let clock = FakeClock::default();
        let cancelled = Cell::new(false);
        let result = run_transition(
            &clock,
            &[50, 0],
            100,
            0,
            Duration::from_millis(100),
            || cancelled.get(),
            |_| {
                clock.advance(Duration::from_millis(200));
                cancelled.set(true);
                Ok(())
            },
            || panic!("must not verify a superseded write"),
        )
        .unwrap();
        assert!(!result.completed);
        assert_eq!(result.writes, 1);
    }

    #[test]
    fn cancellation_during_final_write_prevents_verification() {
        let clock = FakeClock::default();
        let cancelled = Cell::new(false);
        let result = run_transition(
            &clock,
            &[0],
            100,
            0,
            Duration::ZERO,
            || cancelled.get(),
            |_| {
                cancelled.set(true);
                Ok(())
            },
            || panic!("must not read after cancellation"),
        )
        .unwrap();
        assert!(!result.completed);
        assert_eq!(result.writes, 1);
    }

    #[test]
    fn cancellation_while_readback_is_blocked_cannot_report_completion() {
        let clock = FakeClock::default();
        let cancelled = Cell::new(false);
        let result = run_transition(
            &clock,
            &[0],
            100,
            0,
            Duration::ZERO,
            || cancelled.get(),
            |_| Ok(()),
            || {
                clock.advance(Duration::from_millis(40));
                cancelled.set(true);
                Ok(0)
            },
        )
        .unwrap();
        assert!(!result.completed);
        assert_eq!(result.writes, 1);
    }

    #[test]
    fn write_failure_propagates_without_reading_or_sending_another_sample() {
        let clock = FakeClock::default();
        let result = run_transition(
            &clock,
            &[50, 0],
            100,
            0,
            Duration::from_millis(100),
            || false,
            |_| Err("write failed".into()),
            || panic!("failed write must not be reported as restored"),
        );
        assert_eq!(result, Err("write failed".into()));
    }

    #[test]
    fn read_failure_or_mismatch_cannot_report_completion() {
        let clock = FakeClock::default();
        let failed = run_transition(
            &clock,
            &[0],
            100,
            0,
            Duration::ZERO,
            || false,
            |_| Ok(()),
            || Err("read failed".into()),
        );
        assert_eq!(failed, Err("read failed".into()));
        let mismatch = run_transition(
            &clock,
            &[0],
            100,
            0,
            Duration::ZERO,
            || false,
            |_| Ok(()),
            || Ok(5),
        );
        assert!(mismatch.unwrap_err().contains("目标 0，实际 5"));
    }

    #[test]
    fn malformed_curve_is_rejected_before_io() {
        let clock = FakeClock::default();
        for values in [vec![], vec![50, 1]] {
            let result = run_transition(
                &clock,
                &values,
                100,
                0,
                Duration::ZERO,
                || false,
                |_| panic!("invalid transition must not write"),
                || panic!("invalid transition must not read"),
            );
            assert!(result.is_err());
        }
    }

    #[test]
    fn fractional_deadlines_do_not_advance_the_final_target_early() {
        let clock = FakeClock::default();
        let events = RefCell::new(Vec::new());
        run_transition(
            &clock,
            &[67, 33, 0],
            100,
            0,
            Duration::from_millis(301),
            || false,
            |value| {
                events.borrow_mut().push((clock.elapsed(), value));
                Ok(())
            },
            || Ok(0),
        )
        .unwrap();
        assert_eq!(
            events.borrow().last(),
            Some(&(Duration::from_millis(301), 0))
        );
        assert!(events.borrow()[0].0 < Duration::from_millis(101));
    }
}
