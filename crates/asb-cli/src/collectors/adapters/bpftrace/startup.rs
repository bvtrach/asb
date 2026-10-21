//! Bounded failure custody while a custom collector has not become ready.
use crate::process_cleanup::OwnedReaper;
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

pub(super) struct StartupChild {
    child: Option<Child>,
    reaper: OwnedReaper,
}

impl StartupChild {
    pub(super) fn spawn(command: &mut Command, program: &str) -> Result<Self, String> {
        // Establish fallback ownership before admitting an external process.
        let reaper = OwnedReaper::new("bpftrace-startup")?;
        let child = command
            .spawn()
            .map_err(|error| format!("cannot start bpftrace program {program}: {error}"))?;
        Ok(Self {
            child: Some(child),
            reaper,
        })
    }

    pub(super) fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("owned startup child")
    }

    pub(super) fn ready(mut self) -> Child {
        self.child.take().expect("owned startup child")
    }

    pub(super) fn failure(mut self, reason: String) -> String {
        match self.cleanup() {
            Ok(()) => reason,
            Err(cleanup) => format!("{reason}; {cleanup}"),
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if matches!(poll(&mut child), Ok(Some(_))) {
            return Ok(());
        }
        let _ = child.kill();
        let timeout = Duration::from_secs(5);
        #[cfg(all(test, target_os = "linux"))]
        let timeout = tests::timeout_override().unwrap_or(timeout);
        let deadline = Instant::now() + timeout;
        loop {
            let reason = match poll(&mut child) {
                Ok(Some(_)) => return Ok(()),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Ok(None) => "child did not exit within five seconds".to_owned(),
                Err(error) => format!("cannot confirm child exit: {error}"),
            };
            self.reaper.adopt(child);
            return Err(format!(
                "bpftrace startup cleanup was not confirmed: {reason}; an owned reaper retains the child"
            ));
        }
    }
}

fn poll(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
    #[cfg(all(test, target_os = "linux"))]
    if let Some(result) = tests::poll_override() {
        return result;
    }
    child.try_wait()
}

impl Drop for StartupChild {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
