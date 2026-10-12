use std::ffi::OsString;
use std::io::Write;

#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};
#[cfg(test)]
use std::time::{Duration, Instant};

#[cfg(test)]
thread_local! {
    static DEPTH: Cell<u8> = const { Cell::new(0) };
}

#[cfg(test)]
static GATE: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub struct Guard {
    _lock: Option<MutexGuard<'static, ()>>,
}

#[cfg(test)]
pub fn acquire() -> Guard {
    try_acquire(Duration::from_secs(5)).expect("process-fixture semaphore acquisition timed out")
}

#[cfg(test)]
fn try_acquire(timeout: Duration) -> Option<Guard> {
    let nested = DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current.saturating_add(1));
        current > 0
    });
    if nested {
        return Some(Guard { _lock: None });
    }
    let gate = GATE.get_or_init(|| Mutex::new(()));
    let deadline = Instant::now() + timeout;
    loop {
        match gate.try_lock() {
            Ok(lock) => return Some(Guard { _lock: Some(lock) }),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                return Some(Guard {
                    _lock: Some(poisoned.into_inner()),
                });
            }
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
                return None;
            }
        }
    }
}

#[cfg(test)]
impl Drop for Guard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Runs one CLI request while serializing process-backed test fixtures.
pub fn run(args: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    #[cfg(test)]
    let _fixture_guard = acquire();
    crate::run_with_default_mode_and_stdin(args, stdout, stderr, false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn positive_nested_and_drop_paths_are_reentrant() {
        let outer = acquire();
        let inner = acquire();
        drop(inner);
        drop(outer);
        assert!(try_acquire(Duration::ZERO).is_some());
    }

    #[test]
    fn hostile_contention_times_out_and_recovers_after_release() {
        let held = acquire();
        let contender = thread::spawn(|| try_acquire(Duration::from_millis(10)).is_none());
        assert!(contender.join().unwrap());
        drop(held);
        assert!(try_acquire(Duration::from_millis(10)).is_some());
    }
}
