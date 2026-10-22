//! Real owned children only, without network, providers, or privileged tools.
#![allow(unsafe_code)] // Linux identity/reaping assertions only.
use super::*;
use std::fs::File;
use std::io::{Read, Write};
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

#[test]
fn wait_only_custody_does_not_kill_a_live_supervisor() {
    let scope = test_support::Scope::new();
    let reaper = OwnedReaper::new("wait-only-fixture").unwrap();
    let mut child = Command::new("/bin/sh")
        .args(["-c", "printf ready; read release; printf quiescent; exit 7"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut ready = [0; 5];
    output.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"ready");
    reaper.adopt_wait_only(child);
    assert!(matches!(
        reaper.0.0.lock().unwrap().children[0].1,
        ReapPolicy::WaitOnly
    ));
    drop(reaper);
    input.write_all(b"release\n").unwrap();
    drop(input);
    let mut proof = String::new();
    output.read_to_string(&mut proof).unwrap();
    assert_eq!(proof, "quiescent");
    scope.wait_finished();
}

#[test]
fn controlled_wait_error_retains_live_wait_only_child_until_real_release_exit_and_reap() {
    let scope = test_support::Scope::new();
    let fault = test_support::ObservationFault::arm();
    let reaper = OwnedReaper::new("controlled-wait-error").unwrap();
    let mut sentinel = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let mut child = Command::new("/bin/sh")
        .args(["-c", "printf ready; read release; printf quiescent; exit 7"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    // Install exact child custody before any fixture observation can fail.
    reaper.adopt_wait_only(child);
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(fd >= 0);
    let identity = unsafe { File::from_raw_fd(fd) };
    let mut ready = [0; 5];
    let ready_read = output.read_exact(&mut ready);
    drop(reaper);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !fault.observed() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let injected_while_live = fault.observed() && !scope.finished();
    let mut poll = libc::pollfd {
        fd: identity.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let child_survived = unsafe { libc::poll(&mut poll, 1, 0) } == 0;
    let release = input.write_all(b"release\n");
    drop(input);
    let mut proof = String::new();
    let proof_read = output.read_to_string(&mut proof);
    let child_exited = unsafe { libc::poll(&mut poll, 1, 3000) } > 0;
    let no_premature_receipt = !scope.finished();
    fault.release();
    scope.wait_finished();
    let mut information = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let wait = unsafe {
        libc::waitid(
            libc::P_PIDFD,
            identity.as_raw_fd() as libc::id_t,
            information.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    let actual_reap =
        wait == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
    let sentinel_survived = sentinel.try_wait().unwrap().is_none();
    if sentinel_survived {
        sentinel.kill().unwrap();
    }
    sentinel.wait().unwrap();
    // Every intended assertion follows actual owned child/sentinel cleanup.
    assert!(ready_read.is_ok());
    assert_eq!(&ready, b"ready");
    assert!(injected_while_live && child_survived);
    assert!(release.is_ok() && proof_read.is_ok());
    assert_eq!(proof, "quiescent");
    assert!(child_exited && no_premature_receipt && actual_reap);
    assert!(scope.finished() && sentinel_survived);
}

#[test]
fn controlled_poll_error_retains_terminate_child_before_real_kill_exit_and_reap() {
    let scope = test_support::Scope::new();
    let fault = test_support::ObservationFault::arm();
    let reaper = OwnedReaper::new("controlled-poll-error").unwrap();
    let mut sentinel = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let pid = child.id();
    reaper.adopt(child);
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(fd >= 0);
    let identity = unsafe { File::from_raw_fd(fd) };
    drop(reaper);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !fault.observed() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut poll = libc::pollfd {
        fd: identity.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let retained_live =
        fault.observed() && !scope.finished() && unsafe { libc::poll(&mut poll, 1, 0) } == 0;
    fault.release();
    scope.wait_finished();
    let exited = unsafe { libc::poll(&mut poll, 1, 0) } > 0;
    let mut information = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let wait = unsafe {
        libc::waitid(
            libc::P_PIDFD,
            identity.as_raw_fd() as libc::id_t,
            information.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    let actual_reap =
        wait == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
    let sentinel_survived = sentinel.try_wait().unwrap().is_none();
    if sentinel_survived {
        sentinel.kill().unwrap();
    }
    sentinel.wait().unwrap();
    assert!(retained_live && exited && actual_reap && sentinel_survived);
    assert!(scope.finished());
}
