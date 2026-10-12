#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

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
    let nested = DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current.saturating_add(1));
        current > 0
    });
    if nested {
        Guard { _lock: None }
    } else {
        Guard {
            _lock: Some(
                GATE.get_or_init(|| Mutex::new(()))
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            ),
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
#[macro_export]
macro_rules! f {
    ($args:expr, $stdout:expr, $stderr:expr) => {{
        #[cfg(test)]
        let _fixture_guard = $crate::test_fixture_semaphore::acquire();
        $crate::run_with_default_mode_and_stdin($args, $stdout, $stderr, false, None)
    }};
}
