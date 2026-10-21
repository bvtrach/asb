//! Bounded FIFO scheduling of distinct configurations, weighted by replicas.
use crate::{Error, benchmark};
use serde::{Deserialize, Serialize};
use std::sync::{Condvar, Mutex, mpsc};

pub(crate) const MAX_JOBS: usize = 32;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scheduling {
    pub jobs: usize,
    pub max_active_attempts: usize,
}

impl Scheduling {
    pub(super) fn new(
        jobs: usize,
        budget: Option<usize>,
        weights: &[usize],
    ) -> Result<Self, Error> {
        let maximum = weights.iter().copied().max().unwrap_or(1);
        let limits = Self {
            jobs,
            max_active_attempts: budget.unwrap_or(maximum.max(jobs)),
        };
        limits.validate(weights)?;
        Ok(limits)
    }

    pub(super) fn validate(&self, weights: &[usize]) -> Result<(), Error> {
        if !(1..=MAX_JOBS).contains(&self.jobs) {
            return Err(Error::Input(format!(
                "sweep jobs must be between 1 and {MAX_JOBS}"
            )));
        }
        if !(1..=benchmark::MAX_CONCURRENCY).contains(&self.max_active_attempts)
            || weights
                .iter()
                .any(|weight| *weight == 0 || *weight > self.max_active_attempts)
        {
            return Err(Error::Input(format!(
                "sweep active-attempt budget must be between 1 and {} and at least each configuration's replica concurrency; increase --max-active-attempts or reduce concurrency",
                benchmark::MAX_CONCURRENCY
            )));
        }
        Ok(())
    }
}

pub(super) enum Event {
    Started(usize),
    Finished(usize, Result<(), Error>),
}

#[derive(Default)]
struct State {
    next: usize,
    active: usize,
    stopped: bool,
}

struct Queue<'a> {
    state: Mutex<State>,
    changed: Condvar,
    weights: &'a [usize],
    budget: usize,
}

impl Queue<'_> {
    fn take(&self) -> Option<(usize, Permit<'_, '_>)> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if state.stopped || state.next == self.weights.len() {
                return None;
            }
            let index = state.next;
            let cost = self.weights[index];
            if state.active + cost <= self.budget {
                state.next += 1;
                state.active += cost;
                return Some((index, Permit { queue: self, cost }));
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn stop(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .stopped = true;
        self.changed.notify_all();
    }
}

struct Permit<'q, 'w> {
    queue: &'q Queue<'w>,
    cost: usize,
}

impl Drop for Permit<'_, '_> {
    fn drop(&mut self) {
        self.queue
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .active -= self.cost;
        self.queue.changed.notify_all();
    }
}

/// Output callbacks stay on the caller thread. On output/spawn failure stop
/// admission and drain already running work before returning; never detach it.
pub(super) fn run(
    limits: Scheduling,
    weights: &[usize],
    execute: impl Fn(usize) -> Result<(), Error> + Sync,
    mut observe: impl FnMut(Event) -> Result<(), Error>,
) -> Result<(), Error> {
    limits.validate(weights)?;
    let queue = Queue {
        state: Mutex::new(State::default()),
        changed: Condvar::new(),
        weights,
        budget: limits.max_active_attempts,
    };
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel(limits.jobs * 2);
        let mut handles = Vec::new();
        let mut failure = None;
        for _ in 0..limits.jobs.min(weights.len()) {
            let sender = sender.clone();
            let queue = &queue;
            let execute = &execute;
            match std::thread::Builder::new().spawn_scoped(scope, move || {
                while let Some((index, _permit)) = queue.take() {
                    if sender.send(Event::Started(index)).is_err() {
                        break;
                    }
                    let result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(index)))
                            .unwrap_or_else(|_| {
                                Err(Error::Run(
                                    "sweep worker panicked; inspect its private result artifacts"
                                        .into(),
                                ))
                            });
                    if sender.send(Event::Finished(index, result)).is_err() {
                        break;
                    }
                }
            }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    queue.stop();
                    failure = Some(Error::Run(format!(
                        "cannot start bounded sweep worker: {error}"
                    )));
                    break;
                }
            }
        }
        drop(sender);
        for event in receiver {
            // Even a broken terminal must not hide outcomes of admitted work.
            // Stop new admission, but deliver every finished result while
            // joining the owned workers. Never replace the first output error.
            if (failure.is_none() || matches!(event, Event::Finished(..)))
                && let Err(error) = observe(event)
            {
                queue.stop();
                if failure.is_none() {
                    failure = Some(error);
                }
            }
        }
        for handle in handles {
            if handle.join().is_err() && failure.is_none() {
                failure = Some(Error::Run(
                    "sweep scheduler worker failed outside execution".into(),
                ));
            }
        }
        failure.map_or(Ok(()), Err)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn parallel_tasks_overlap_without_exceeding_weighted_replica_budget() {
        let weights = vec![2; 64];
        let limits = Scheduling::new(4, Some(4), &weights).unwrap();
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let begun = (Mutex::new(0), Condvar::new());
        let mut started = Vec::new();
        let mut finished = Vec::new();
        run(
            limits,
            &weights,
            |index| {
                let cost = active.fetch_add(weights[index], Ordering::SeqCst) + weights[index];
                peak.fetch_max(cost, Ordering::SeqCst);
                assert!(cost <= 4, "replicas must count against the shared budget");
                if index < 2 {
                    let mut count = begun.0.lock().unwrap();
                    *count += 1;
                    begun.1.notify_all();
                    let (_guard, timeout) = begun
                        .1
                        .wait_timeout_while(count, Duration::from_secs(5), |count| *count < 2)
                        .unwrap();
                    assert!(
                        !timeout.timed_out(),
                        "both distinct configurations must actually execute concurrently"
                    );
                }
                std::thread::sleep(Duration::from_millis(1));
                active.fetch_sub(weights[index], Ordering::SeqCst);
                Ok(())
            },
            |event| {
                match event {
                    Event::Started(index) => started.push(index),
                    Event::Finished(index, result) => {
                        result?;
                        finished.push(index);
                    }
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(peak.load(Ordering::SeqCst), 4);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        started.sort_unstable();
        finished.sort_unstable();
        assert_eq!(started, (0..weights.len()).collect::<Vec<_>>());
        assert_eq!(finished, started);
    }

    #[test]
    fn failed_and_panicked_tasks_release_slots_and_do_not_hide_other_results() {
        let weights = vec![1; 20];
        let mut completed = Vec::new();
        let mut failures = Vec::new();
        run(
            Scheduling::new(3, Some(3), &weights).unwrap(),
            &weights,
            |index| {
                if index == 1 {
                    panic!("synthetic owned-worker panic");
                }
                if index == 4 {
                    return Err(Error::Run("synthetic task failure".into()));
                }
                Ok(())
            },
            |event| {
                if let Event::Finished(index, result) = event {
                    completed.push(index);
                    if result.is_err() {
                        failures.push(index);
                    }
                }
                Ok(())
            },
        )
        .unwrap();
        completed.sort_unstable();
        failures.sort_unstable();
        assert_eq!(completed, (0..20).collect::<Vec<_>>());
        assert_eq!(failures, [1, 4]);
    }

    #[test]
    fn broken_output_stops_admission_and_joins_running_workers() {
        let weights = vec![1; 100];
        let entered = AtomicUsize::new(0);
        let exited = AtomicUsize::new(0);
        let mut started = 0;
        let mut finished = 0;
        let error = run(
            Scheduling::new(2, Some(2), &weights).unwrap(),
            &weights,
            |_| {
                entered.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                exited.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            |event| {
                match event {
                    Event::Started(_) => started += 1,
                    Event::Finished(_, _) => finished += 1,
                }
                Err(Error::Io("synthetic broken terminal".into()))
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("synthetic broken terminal"));
        assert_eq!(started, 1, "no presentation starts after output failure");
        assert_eq!(finished, exited.load(Ordering::SeqCst));
        assert_eq!(
            entered.load(Ordering::SeqCst),
            exited.load(Ordering::SeqCst)
        );
        assert!(entered.load(Ordering::SeqCst) < weights.len());
    }

    #[test]
    fn failed_output_still_delivers_every_admitted_failure_and_retains_first_error() {
        let admitted = (Mutex::new(0), Condvar::new());
        let mut failures = Vec::new();
        let error = run(
            Scheduling::new(2, Some(2), &[1, 1]).unwrap(),
            &[1, 1],
            |index| {
                let mut count = admitted.0.lock().unwrap();
                *count += 1;
                admitted.1.notify_all();
                let (_count, timeout) = admitted
                    .1
                    .wait_timeout_while(count, Duration::from_secs(5), |count| *count < 2)
                    .unwrap();
                assert!(!timeout.timed_out(), "both owned tasks must be admitted");
                Err(Error::Run(format!("owned failure {index}")))
            },
            |event| {
                if let Event::Finished(index, result) = event {
                    failures.push((index, result.unwrap_err().to_string()));
                    return Err(Error::Io(format!("output failure {index}")));
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(failures.len(), 2);
        assert!(
            error
                .to_string()
                .contains(&format!("output failure {}", failures[0].0))
        );
        failures.sort();
        assert_eq!(
            failures,
            vec![(0, "owned failure 0".into()), (1, "owned failure 1".into())]
        );
    }

    #[test]
    fn admission_is_validated_before_spawning_and_defaults_preserve_replica_choices() {
        let defaults = Scheduling::new(1, None, &[8, 1]).unwrap();
        assert_eq!(defaults.jobs, 1);
        assert_eq!(defaults.max_active_attempts, 8);
        assert_eq!(
            Scheduling::new(4, None, &[1]).unwrap().max_active_attempts,
            4
        );
        for (jobs, budget, weights) in [
            (0, None, vec![1]),
            (33, None, vec![1]),
            (1, Some(0), vec![1]),
            (1, Some(65), vec![1]),
            (2, Some(1), vec![2]),
            (1, None, vec![0]),
        ] {
            let limits = Scheduling {
                jobs,
                max_active_attempts: budget.unwrap_or(1),
            };
            assert!(
                run(
                    limits,
                    &weights,
                    |_| panic!("must not start invalid work"),
                    |_| Ok(())
                )
                .is_err()
            );
        }
    }
}
