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
        #[cfg(test)]
        if DENIED_CHILD.get() == pid {
            return Err(io::Error::from_raw_os_error(libc::EPERM));
        }
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
thread_local! {
    // Scoped to an exclusive reexec fixture and exact owned pid, after the real
    // kernel ownership check. No environment-controlled production fault.
    static DENIED_CHILD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
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

    #[test]
    fn one_owned_signal_denial_does_not_starve_later_owned_siblings() {
        use std::process::Command;
        if let Some(root) = std::env::var_os("ASB_MATERIAL_CUSTODY_ROOT") {
            let mut scope = DescendantScope::new().unwrap();
            // libtest runs the fixture on a worker, whereas the real gate is
            // single-threaded. Exit the spawning thread so its children become
            // the process leader's actual children before using production's
            // /proc/<leader>/task/<leader>/children observation.
            let (mut denied, mut sibling) = std::thread::spawn(|| {
                (
                    Command::new("/bin/sleep").arg("30").spawn().unwrap(),
                    Command::new("/bin/sleep").arg("30").spawn().unwrap(),
                )
            })
            .join()
            .unwrap();
            let population = children(std::process::id()).unwrap();
            assert_eq!(population, vec![denied.id(), sibling.id()]);
            DENIED_CHILD.set(denied.id());
            let error = scope.quiesce().unwrap_err();
            DENIED_CHILD.set(0);
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
            let sibling_alive = loop {
                if sibling.try_wait().unwrap().is_some() {
                    break false;
                }
                if std::time::Instant::now() >= deadline {
                    break true;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            // Real cleanup before publishing observations; the outer guardian
            // still owns any orphan if a fixture assertion/unwind fails.
            denied.kill().unwrap();
            if sibling_alive {
                sibling.kill().unwrap();
            }
            denied.wait().unwrap();
            sibling.wait().unwrap();
            std::fs::write(
                std::path::PathBuf::from(root).join("sibling.json"),
                serde_json::to_vec(&serde_json::json!({
                    "error_errno":error.raw_os_error(),"sibling_alive":sibling_alive,
                    "exact_children_reaped":true
                }))
                .unwrap(),
            )
            .unwrap();
            return;
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/material50-fixtures")
            .join(crate::workloads::TaskIdentity::local("sibling-denial", None).attempt_id);
        std::fs::create_dir_all(&root).unwrap();
        let status = Command::new("python3")
            .args(["-I", "-S", "-B"])
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/material-transport-custody.py"))
            .arg("guardian")
            .arg(&root)
            .arg(std::env::current_exe().unwrap())
            .arg("process_gate::descendants::tests::one_owned_signal_denial_does_not_starve_later_owned_siblings")
            .arg("sibling")
            .status()
            .unwrap();
        let read = |name| -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
        };
        let guardian = read("guardian.json");
        let observed = read("sibling.json");
        println!(
            "fixture={} guardian={guardian} sibling={observed}",
            root.display()
        );
        assert!(status.success());
        assert_eq!(guardian["child_status"], 0);
        assert_eq!(guardian["sentinel_survived"], true);
        assert_eq!(observed["error_errno"], libc::EPERM);
        assert_eq!(observed["exact_children_reaped"], true);
        assert_eq!(
            observed["sibling_alive"], false,
            "first EPERM starved a separately owned sibling; all fixture cleanup already complete"
        );
    }
}
