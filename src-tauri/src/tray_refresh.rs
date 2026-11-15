use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Coalesces queued refreshes without holding an application lock while the
/// dispatcher or the refresh callback runs.
#[derive(Default)]
pub struct RefreshGate {
    pending: AtomicBool,
}

impl RefreshGate {
    pub fn request<E>(
        self: &Arc<Self>,
        dispatch: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), E>,
        refresh: impl FnOnce() + Send + 'static,
    ) -> Result<(), E> {
        if self.pending.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let gate = self.clone();
        let result = dispatch(Box::new(move || {
            // A state change during this refresh may queue the next refresh.
            gate.pending.store(false, Ordering::Release);
            refresh();
        }));
        if result.is_err() {
            self.pending.store(false, Ordering::Release);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Mutex};

    type Task = Box<dyn FnOnce() + Send>;

    #[test]
    fn concurrent_refreshes_coalesce_and_read_latest_state() {
        let gate = Arc::new(RefreshGate::default());
        let queue = Arc::new(Mutex::new(Vec::<Task>::new()));
        let state = Arc::new(AtomicUsize::new(0));
        let observed = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..32 {
                let (gate, queue, state, observed) =
                    (gate.clone(), queue.clone(), state.clone(), observed.clone());
                scope.spawn(move || {
                    gate.request(
                        |task| {
                            queue.lock().unwrap().push(task);
                            Ok::<_, ()>(())
                        },
                        move || observed.store(state.load(Ordering::Acquire), Ordering::Release),
                    )
                    .unwrap();
                });
            }
        });
        state.store(7, Ordering::Release);
        let tasks = std::mem::take(&mut *queue.lock().unwrap());
        assert_eq!(tasks.len(), 1);
        for task in tasks {
            task();
        }
        assert_eq!(observed.load(Ordering::Acquire), 7);
        assert!(!gate.pending.load(Ordering::Acquire));
    }

    #[test]
    fn inline_dispatch_can_request_another_refresh_without_a_lock_cycle() {
        let gate = Arc::new(RefreshGate::default());
        let nested = gate.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let result = count.clone();
        gate.request(
            |task| {
                task();
                Ok::<_, ()>(())
            },
            move || {
                nested
                    .request(
                        |task| {
                            task();
                            Ok::<_, ()>(())
                        },
                        move || {
                            result.fetch_add(1, Ordering::AcqRel);
                        },
                    )
                    .unwrap();
            },
        )
        .unwrap();
        assert_eq!(count.load(Ordering::Acquire), 1);
    }

    #[test]
    fn dispatch_failure_does_not_suppress_future_refreshes() {
        let gate = Arc::new(RefreshGate::default());
        assert!(gate.request(|_| Err::<(), _>("closed"), || {}).is_err());
        let count = Arc::new(AtomicUsize::new(0));
        let result = count.clone();
        gate.request(
            |task| {
                task();
                Ok::<_, ()>(())
            },
            move || {
                result.fetch_add(1, Ordering::AcqRel);
            },
        )
        .unwrap();
        assert_eq!(count.load(Ordering::Acquire), 1);
    }
}
