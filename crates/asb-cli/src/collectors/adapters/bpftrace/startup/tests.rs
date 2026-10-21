//! Real owned child identities; no BPF attachment, network, or provider calls.
#![allow(unsafe_code)]
use super::*;
use crate::process_cleanup::test_support;
use std::cell::Cell;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};

thread_local! { static POLL_MODE: Cell<u8> = const { Cell::new(0) }; }
pub(super) fn timeout_override() -> Option<Duration> {
    (POLL_MODE.get() != 0).then_some(Duration::ZERO)
}
pub(super) fn poll_override() -> Option<std::io::Result<Option<ExitStatus>>> {
    match POLL_MODE.get() {
        1 => Some(Ok(None)),
        2 => Some(Err(std::io::Error::other("fixture poll failure"))),
        _ => None,
    }
}
struct PollMode;
impl PollMode {
    fn set(mode: u8) -> Self {
        POLL_MODE.set(mode);
        Self
    }
}
impl Drop for PollMode {
    fn drop(&mut self) {
        POLL_MODE.set(0);
    }
}

fn sleeper() -> Command {
    let mut command = Command::new("/bin/sleep");
    command.arg("30").env_clear();
    command
}

fn identity(child: &mut StartupChild) -> File {
    // SAFETY: pin the directly owned child's current identity before transfer.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.child().id(), 0) } as i32;
    assert!(fd >= 0);
    unsafe { File::from_raw_fd(fd) }
}

fn assert_reaped(identity: &File, pid: u32) {
    let mut poll = libc::pollfd {
        fd: identity.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: observe a pinned identity; waitpid checks that custody was reaped.
    assert_eq!(unsafe { libc::poll(&mut poll, 1, 0) }, 1);
    assert_ne!(poll.revents & libc::POLLIN, 0);
    assert_eq!(
        unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}

#[test]
fn worker_creation_failure_admits_no_child_and_spawn_failure_closes_worker() {
    let scope = test_support::Scope::new();
    let root = std::env::temp_dir().join(format!(
        "asb-bpf-startup-{}",
        crate::workloads::TaskIdentity::local("fixture", None).attempt_id
    ));
    std::fs::create_dir(&root).unwrap();
    let marker = root.join("admitted");
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "printf admitted > \"$1\"", "fixture"])
        .arg(&marker)
        .env_clear();
    test_support::set_creation_failure(true);
    assert!(
        StartupChild::spawn(&mut command, "fixture")
            .err()
            .unwrap()
            .contains("cleanup worker")
    );
    assert!(!marker.exists());
    assert_eq!(scope.count(), 0);
    test_support::set_creation_failure(false);
    assert!(
        StartupChild::spawn(&mut Command::new(root.join("missing")), "missing.bt")
            .err()
            .unwrap()
            .contains("cannot start bpftrace program missing.bt")
    );
    scope.wait_finished();
    assert_eq!(scope.count(), 1);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn readiness_success_transfers_a_still_live_child_without_killing_it() {
    let scope = test_support::Scope::new();
    let startup = StartupChild::spawn(&mut sleeper(), "fixture").unwrap();
    let mut child = startup.ready();
    scope.wait_finished();
    assert!(child.try_wait().unwrap().is_none());
    child.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn readiness_failure_reaps_only_its_child_and_preserves_original_diagnostic() {
    let scope = test_support::Scope::new();
    let mut sentinel = sleeper().spawn().unwrap();
    let mut child = StartupChild::spawn(&mut sleeper(), "fixture").unwrap();
    let pid = child.child().id();
    let pinned = identity(&mut child);
    assert_eq!(
        child.failure("fixture readiness error".into()),
        "fixture readiness error"
    );
    scope.wait_finished();
    assert_reaped(&pinned, pid);
    assert!(sentinel.try_wait().unwrap().is_none());
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}

#[test]
fn uncertain_poll_and_deadline_handoff_preserve_diagnostics_and_reap_owned_identity() {
    for mode in [1, 2] {
        let scope = test_support::Scope::new();
        let _mode = PollMode::set(mode);
        let mut child = StartupChild::spawn(&mut sleeper(), "fixture").unwrap();
        let pid = child.child().id();
        let pinned = identity(&mut child);
        let started = Instant::now();
        let error = child.failure("original startup error".into());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(error.starts_with("original startup error;"));
        assert!(error.contains("owned reaper retains the child"));
        assert!(error.contains(if mode == 1 {
            "did not exit"
        } else {
            "fixture poll failure"
        }));
        scope.wait_finished();
        assert_reaped(&pinned, pid);
    }
}

#[test]
fn unwind_hands_owned_child_to_preestablished_worker_without_waiting_forever() {
    let scope = test_support::Scope::new();
    let _mode = PollMode::set(1);
    let mut child = StartupChild::spawn(&mut sleeper(), "fixture").unwrap();
    let pid = child.child().id();
    let pinned = identity(&mut child);
    let started = Instant::now();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _owned = child;
        panic!("fixture unwinding");
    }));
    assert!(panic.is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    scope.wait_finished();
    assert_reaped(&pinned, pid);
}
