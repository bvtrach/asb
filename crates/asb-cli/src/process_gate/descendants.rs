//! Bounded teardown in an exclusive, short-lived agent supervisor process.
//!
//! Never construct this in the shared runner: it owns *all* children of the
//! calling process. Kernel child ownership plus pidfds prevents stale proc PIDs
//! from signalling unrelated processes, including under PID reuse.
#![allow(unsafe_code)]

use std::collections::BTreeSet;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

pub(crate) struct DescendantScope {
    owner: u32,
    attempted: bool,
}

impl DescendantScope {
    pub(crate) fn new() -> io::Result<Self> {
        super::become_child_subreaper()?;
        // Probe the required kernel API before starting a client. Our own
        // process is not a child, so the ownership check cannot send a signal.
        signal_and_reap_owned_child(std::process::id(), None)?;
        Ok(Self {
            owner: std::process::id(),
            attempted: false,
        })
    }

    /// Reap exited adopted services without signalling any live process.
    /// Only an exclusive supervisor may use this after reaping its principal.
    pub(crate) fn reap_exited(&self) -> io::Result<()> {
        if self.owner != std::process::id() {
            return Err(io::Error::other(
                "workload service supervisor changed process",
            ));
        }
        let children = children(self.owner)?;
        if children.len() > 65536 {
            return Err(io::Error::other(
                "workload service child list exceeded its bound",
            ));
        }
        for pid in children {
            signal_and_reap_owned_child(pid, None)?;
        }
        Ok(())
    }

    /// Called after the principal client is reaped, before accepting its
    /// submission. A short TERM grace is followed by KILL; repeated sweeps
    /// retain and reap double-forked helpers as their parents exit.
    pub(crate) fn quiesce(&mut self) -> io::Result<()> {
        self.attempted = true;
        if self.owner != std::process::id() {
            return Err(io::Error::other(
                "agent descendant scope changed supervisor process",
            ));
        }
        let started = Instant::now();
        let mut terminated = BTreeSet::new();
        loop {
            let children = children(self.owner)?;
            if children.is_empty() {
                return Ok(());
            }
            if started.elapsed() >= Duration::from_secs(5) {
                return Err(io::Error::other(
                    "agent descendants did not stop within the cleanup deadline",
                ));
            }
            for pid in children {
                if terminated.len() > 65536 {
                    return Err(io::Error::other(
                        "agent descendant churn exceeded its cleanup bound",
                    ));
                }
                let signal = if started.elapsed() >= Duration::from_millis(500) {
                    Some(libc::SIGKILL)
                } else if terminated.insert(pid) {
                    Some(libc::SIGTERM)
                } else {
                    None
                };
                signal_and_reap_owned_child(pid, signal)?;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for DescendantScope {
    fn drop(&mut self) {
        if !self.attempted {
            let _ = self.quiesce();
        }
    }
}

fn children(owner: u32) -> io::Result<Vec<u32>> {
    let mut bytes = Vec::new();
    std::fs::File::open(format!("/proc/{owner}/task/{owner}/children"))?
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 512 * 1024 {
        return Err(io::Error::other(
            "agent child list exceeded its cleanup bound",
        ));
    }
    parse_children(&bytes)
}

fn parse_children(bytes: &[u8]) -> io::Result<Vec<u32>> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| io::Error::other("invalid agent child list"))?;
    text.split_whitespace()
        .map(|value| {
            value
                .parse::<u32>()
                .ok()
                .filter(|pid| *pid > 1 && *pid <= i32::MAX as u32)
                .ok_or_else(|| io::Error::other("invalid agent child identity"))
        })
        .collect()
}

fn signal_and_reap_owned_child(pid: u32, signal: Option<i32>) -> io::Result<()> {
    // SAFETY: the syscall takes integer arguments, and a successful result is
    // a newly owned descriptor. It pins identity, not just the numeric PID.
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if descriptor < 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        };
    }
    let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor as i32) };
    let mut information = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    // WNOWAIT proves that the pinned process is our child without releasing
    // its exit information. The descriptor remains safe if it exits or its
    // numeric PID is reused before the following signal operation.
    let owned = unsafe {
        libc::waitid(
            libc::P_PIDFD,
            descriptor.as_raw_fd() as libc::id_t,
            information.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if owned < 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ECHILD) {
            Ok(())
        } else {
            Err(error)
        };
    }
    if let Some(signal) = signal {
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                descriptor.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(io::Error::last_os_error());
        }
    }
    let result = unsafe {
        libc::waitid(
            libc::P_PIDFD,
            descriptor.as_raw_fd() as libc::id_t,
            information.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG,
        )
    };
    if result < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ECHILD) {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_list_rejects_non_process_and_out_of_range_targets() {
        assert_eq!(parse_children(b"12 34\n").unwrap(), vec![12, 34]);
        assert!(parse_children(b"").unwrap().is_empty());
        for input in ["0", "1", "-1", "2147483648", "a", "12\x00"] {
            assert!(parse_children(input.as_bytes()).is_err());
        }
    }

    #[test]
    fn kernel_ownership_check_refuses_to_signal_the_test_process_itself() {
        signal_and_reap_owned_child(std::process::id(), Some(libc::SIGKILL)).unwrap();
    }
}
