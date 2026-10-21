//! Real owned children only, without network, providers, or privileged tools.
#![allow(unsafe_code)] // Linux identity/reaping assertions only.
use super::*;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use std::process::Command;

#[test]
fn creation_failure_and_empty_scope_leave_no_waiting_worker() {
    let scope = test_support::Scope::new();
    test_support::set_creation_failure(true);
    assert!(
        OwnedReaper::new("fixture")
            .err()
            .unwrap()
            .contains("fixture resource exhaustion")
    );
    assert_eq!(scope.count(), 0);
    test_support::set_creation_failure(false);
    drop(OwnedReaper::new("fixture").unwrap());
    scope.wait_finished();
    assert_eq!(scope.count(), 1);
}

#[test]
fn poisoned_handoff_retains_and_reaps_a_real_direct_child_without_drop_panic() {
    let scope = test_support::Scope::new();
    let reaper = OwnedReaper::new("fixture").unwrap();
    let poisoned = std::panic::catch_unwind(|| {
        let _state = reaper.0.0.lock().unwrap();
        panic!("fixture handoff poison");
    });
    assert!(poisoned.is_err());
    let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let pid = child.id();
    // SAFETY: pin the directly owned child's identity before custody transfer.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(fd >= 0);
    let identity = unsafe { File::from_raw_fd(fd) };
    reaper.adopt(child);
    drop(reaper);
    scope.wait_finished();
    let mut poll = libc::pollfd {
        fd: identity.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: poll observes the pinned child; waitpid only checks exit custody.
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
