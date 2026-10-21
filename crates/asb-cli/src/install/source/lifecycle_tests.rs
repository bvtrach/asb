//! Deterministic custody faults; fake curl never opens a network connection.
#![allow(unsafe_code)] // pidfd and waitpid only assert owned-child identity/reaping.
use super::*;
use crate::process_cleanup::test_support;
use std::cell::{Cell, RefCell};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Copy)]
enum Fault {
    Pending,
    PollError,
}
struct Admission {
    pid: u32,
    identity: fs::File,
}
thread_local! {
    static FAULT: Cell<Option<Fault>> = const { Cell::new(None) };
    static ADMISSIONS: RefCell<Option<Vec<Admission>>> = const { RefCell::new(None) };
}

pub(super) fn record_admission(pid: u32) {
    ADMISSIONS.with_borrow_mut(|admissions| {
        if let Some(admissions) = admissions {
            // SAFETY: pin this just-spawned direct child without signalling it.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
            assert!(
                fd >= 0,
                "cannot pin child: {}",
                std::io::Error::last_os_error()
            );
            admissions.push(Admission {
                pid,
                identity: unsafe { fs::File::from_raw_fd(fd) },
            });
        }
    });
}

pub(super) fn cleanup_poll_override() -> Option<std::io::Result<Option<std::process::ExitStatus>>> {
    match FAULT.get() {
        Some(Fault::Pending) => Some(Ok(None)),
        Some(Fault::PollError) => Some(Err(std::io::Error::other("fixture cleanup poll failure"))),
        None => None,
    }
}

pub(super) fn cleanup_timeout_override() -> Option<Duration> {
    FAULT.get().map(|_| Duration::ZERO)
}

struct Observer(test_support::Scope);
impl Observer {
    fn new() -> Self {
        ADMISSIONS.with_borrow_mut(|admissions| {
            assert!(admissions.is_none());
            *admissions = Some(Vec::new());
        });
        Self(test_support::Scope::new())
    }
    fn assert_reaped(&self, count: usize) {
        self.0.wait_finished();
        ADMISSIONS.with_borrow(|admissions| {
            let admissions = admissions.as_ref().unwrap();
            assert_eq!(admissions.len(), count);
            for admission in admissions {
                let mut poll = libc::pollfd {
                    fd: admission.identity.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: observe the pinned child; waitpid queries only its exit custody.
                assert_eq!(
                    unsafe { libc::poll(&mut poll, 1, 0) },
                    1,
                    "child survived cleanup"
                );
                assert_ne!(poll.revents & libc::POLLIN, 0);
                assert_eq!(
                    unsafe {
                        libc::waitpid(admission.pid as i32, std::ptr::null_mut(), libc::WNOHANG)
                    },
                    -1
                );
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD)
                );
            }
        });
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        FAULT.set(None);
        ADMISSIONS.with_borrow_mut(|admissions| *admissions = None);
    }
}
struct Sentinel(std::process::Child);
impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn isolated_installation_custody_faults_preserve_original_errors_and_published_bytes() {
    for case in [
        "download-worker-creation",
        "probe-worker-creation",
        "spawn-failure",
        "download-pending",
        "download-poll-error",
        "probe-pending",
        "probe-poll-error",
        "drop-pending",
        "unwind-poll-error",
        "finish-success-pending",
        "finish-error-poll-error",
        "normal-success",
    ] {
        publication_tests::Fixture::new().run(&format!("lifecycle-{case}"));
    }
}

pub(super) fn run_case(root: &Path, destination: &Path, case: &str) {
    let case = case.strip_prefix("lifecycle-").unwrap();
    let observer = Observer::new();
    let mut sentinel = Sentinel(Command::new("/bin/sleep").arg("30").spawn().unwrap());
    if case.ends_with("pending") {
        FAULT.set(Some(Fault::Pending));
    }
    if case.ends_with("poll-error") {
        FAULT.set(Some(Fault::PollError));
    }
    if case.ends_with("worker-creation") {
        test_support::set_creation_failure(true);
    }
    let started = Instant::now();
    match case {
        "download-worker-creation" | "download-pending" | "download-poll-error" => {
            let error = install_with_timeout(
                "https://fixture.invalid/timeout",
                destination,
                Options {
                    kind: "fixture download",
                    limit: 1024,
                    checksum: None,
                    executable: true,
                },
                |_| Ok(()),
                Duration::ZERO,
            )
            .unwrap_err()
            .to_string();
            if case.ends_with("worker-creation") {
                assert!(
                    error.contains("cannot establish installation cleanup worker"),
                    "{error}"
                );
                assert!(!root.join("download-path").exists());
                assert_eq!(observer.0.count(), 0);
            } else {
                assert!(
                    error.contains(
                        "fixture download download timed out; previous installation was preserved"
                    ),
                    "{error}"
                );
                assert_cleanup_diagnostic(&error, case);
            }
        }
        "probe-worker-creation" | "probe-pending" | "probe-poll-error" => {
            let candidate = root.join("version-candidate");
            fs::write(
                &candidate,
                b"#!/bin/sh\nprintf started > version-started\nexec /bin/sleep 60\n",
            )
            .unwrap();
            fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
            let error = install(
                candidate.to_str().unwrap(),
                destination,
                Options {
                    kind: "fixture version",
                    limit: 1024,
                    checksum: None,
                    executable: true,
                },
                |staged| check_version_with_timeout(staged, "fixture version", Duration::ZERO),
            )
            .unwrap_err()
            .to_string();
            if case.ends_with("worker-creation") {
                assert!(
                    error.contains("cannot establish installation cleanup worker"),
                    "{error}"
                );
                assert!(
                    error.contains("previous installation was preserved"),
                    "{error}"
                );
                assert!(!root.join("version-started").exists());
                assert_eq!(observer.0.count(), 0);
            } else {
                assert!(error.contains("staged fixture version version probe timed out; previous installation was preserved"), "{error}");
                assert_cleanup_diagnostic(&error, case);
            }
        }
        "spawn-failure" => {
            let error = DownloadChild::spawn(
                &mut Command::new(root.join("missing-curl")),
                "fixture download",
                Some("curl"),
            )
            .err()
            .unwrap()
            .to_string();
            assert!(error.contains("needs `curl`"), "{error}");
            assert_eq!(observer.0.count(), 1);
        }
        "drop-pending" | "unwind-poll-error" => {
            let result = std::panic::catch_unwind(|| {
                let child = DownloadChild::spawn(
                    Command::new("/bin/sleep").arg("30"),
                    "fixture sleep",
                    None,
                )
                .unwrap();
                if case.starts_with("unwind") {
                    panic!("fixture installation unwind");
                }
                drop(child);
            });
            assert_eq!(result.is_err(), case.starts_with("unwind"));
        }
        "finish-success-pending" | "finish-error-poll-error" => {
            let mut child =
                DownloadChild::spawn(Command::new("/bin/sleep").arg("30"), "fixture sleep", None)
                    .unwrap();
            let original = if case.starts_with("finish-error") {
                Err(Error::Input("specific original installation error".into()))
            } else {
                Ok("fixture successful value")
            };
            let error = child.finish(original).unwrap_err().to_string();
            assert_cleanup_diagnostic(&error, case);
            if case.starts_with("finish-error") {
                assert!(
                    error.contains("specific original installation error"),
                    "{error}"
                );
            }
            drop(child);
        }
        "normal-success" => {
            let mut child =
                DownloadChild::spawn(&mut Command::new("/bin/true"), "fixture true", None).unwrap();
            child.child().wait().unwrap();
            assert_eq!(
                child.finish(Ok("fixture successful value")).unwrap(),
                "fixture successful value"
            );
            drop(child);
        }
        _ => panic!("unexpected fixture case"),
    }
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{case}: caller cleanup was not bounded"
    );
    observer.assert_reaped(
        if case.ends_with("worker-creation") || case == "spawn-failure" {
            0
        } else {
            1
        },
    );
    assert!(
        sentinel.0.try_wait().unwrap().is_none(),
        "unrelated sentinel was killed"
    );
    publication_tests::assert_old_ready(root, destination);
    publication_tests::no_scratch(destination);
}

fn assert_cleanup_diagnostic(error: &str, case: &str) {
    assert!(
        error.contains("installation process cleanup was not confirmed"),
        "{error}"
    );
    assert!(
        error.contains("an owned reaper retains the child"),
        "{error}"
    );
    assert!(
        error.contains(if case.ends_with("pending") {
            "child did not exit within five seconds"
        } else {
            "fixture cleanup poll failure"
        }),
        "{error}"
    );
}
