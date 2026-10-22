//! Mechanistic inherited-writer control, not identification of a historical FD.
#![allow(unsafe_code)] // Child-only fcntl on our live writer; exact pidfd proof.
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Owned {
    child: Option<Child>,
    keeper: crate::process_cleanup::OwnedReaper,
}
impl Owned {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        let keeper = crate::process_cleanup::OwnedReaper::new("fixture-writer-control")
            .map_err(io::Error::other)?;
        let child = command.spawn()?;
        Ok(Self {
            child: Some(child),
            keeper,
        })
    }
    fn stop(mut self) -> io::Result<()> {
        let child = self.child.as_mut().unwrap();
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        let end = Instant::now() + Duration::from_secs(3);
        while child.try_wait()?.is_none() {
            if Instant::now() >= end {
                return Err(io::Error::other("owned control cleanup unconfirmed"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.child = None;
        Ok(())
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            self.keeper.adopt(child);
        }
    }
}

#[test]
fn actual_inherited_writer_negative_control_proves_etxtbsy_after_parent_close() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/executable-writer-controls")
        .join(crate::workloads::TaskIdentity::local("writer", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("fixture-script");
    let mut writer = File::create(&script).unwrap();
    writer
        .write_all(b"#!/bin/sh\nprintf executed > executed\n")
        .unwrap();
    writer
        .set_permissions(fs::Permissions::from_mode(0o700))
        .unwrap();
    let identity = writer.metadata().unwrap();
    let fd = writer.as_raw_fd();
    let mut retainer_command = Command::new("/bin/sleep");
    retainer_command
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Force a real fork and retain ONLY our owned writer across its exec.
    // The parent's CLOEXEC flags stay unchanged. This is a deterministic
    // mechanical control, not a claim about the failed CI child's identity.
    unsafe {
        retainer_command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let retainer = Owned::spawn(&mut retainer_command).unwrap();
    drop(retainer_command);
    let pid = retainer.child.as_ref().unwrap().id();
    let inherited = fs::metadata(format!("/proc/{pid}/fd/{fd}")).unwrap();
    assert_eq!(
        (inherited.dev(), inherited.ino()),
        (identity.dev(), identity.ino())
    );
    // SAFETY: this pidfd pins the directly admitted owned retainer only.
    let pinned = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(pinned >= 0);
    let pinned = unsafe { File::from_raw_fd(pinned) };
    let mut sentinel_command = Command::new("/bin/sleep");
    sentinel_command
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut sentinel = Owned::spawn(&mut sentinel_command).unwrap();
    drop(writer);
    let result = Command::new(&script)
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let error_code = result.as_ref().err().and_then(io::Error::raw_os_error);
    retainer
        .stop()
        .expect("retainer must settle before the intended RED assertion");
    let mut event = libc::pollfd {
        fd: pinned.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut event, 1, 0) }, 1);
    assert_ne!(event.revents & libc::POLLIN, 0);
    let sentinel_alive = sentinel
        .child
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none();
    sentinel.stop().unwrap();
    fs::write(
        root.join("cleanup-confirmed"),
        format!("retainer reaped; sentinel_alive={sentinel_alive}; exec_errno={error_code:?}\n"),
    )
    .unwrap();
    assert!(sentinel_alive);
    assert!(!root.join("executed").exists());
    assert_eq!(error_code, Some(libc::ETXTBSY), "{}", root.display());
}

fn root() -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/executable-writer-fixtures")
        .join(crate::workloads::TaskIdentity::local("writer", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn isolated_writer_closes_all_executable_writers_before_healthy_admission() {
    let root = root();
    let script = root.join("script");
    let bytes = b"#!/bin/sh\nprintf executed > executed\n";
    crate::write_test_script(&root, &script, bytes, Some(0o751)).unwrap();
    assert_eq!(fs::read(&script).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&script).unwrap().permissions().mode() & 0o777,
        0o751
    );
    let identity = fs::metadata(&script).unwrap();
    for entry in fs::read_dir("/proc/self/fd").unwrap() {
        if let Ok(metadata) = fs::metadata(entry.unwrap().path()) {
            assert_ne!(
                (metadata.dev(), metadata.ino()),
                (identity.dev(), identity.ino())
            );
        }
    }
    let status = Command::new(&script).current_dir(&root).status().unwrap();
    assert!(status.success());
    assert_eq!(fs::read(root.join("executed")).unwrap(), b"executed");
    assert!(crate::test_support::fixture_writer::cleanup(&root).unwrap());
}

#[test]
fn writer_preflight_and_keeper_refusal_never_admit_or_change_files() {
    let root = root();
    let script = root.join("script");
    assert!(
        crate::write_test_script(&root, &script, &vec![b'x'; 48 * 1024 + 1], Some(0o700)).is_err()
    );
    assert!(!script.exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    let scope = crate::process_cleanup::test_support::Scope::new();
    crate::process_cleanup::test_support::set_creation_failure(true);
    let result = crate::write_test_script(&root, &script, b"not admitted", Some(0o700));
    crate::process_cleanup::test_support::set_creation_failure(false);
    assert!(result.is_err());
    assert_eq!(scope.count(), 0);
    assert!(!script.exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn standalone_keeper_uses_the_same_writer_and_retains_failed_or_unknown_namespaces() {
    use crate::test_support::fixture_writer::{self, Fault};
    let healthy = root();
    let script = healthy.join("script");
    fixture_writer::write(&healthy, &script, b"#!/bin/sh\nexit 0\n", Some(0o700)).unwrap();
    assert!(Command::new(&script).status().unwrap().success());
    assert!(fixture_writer::cleanup(&healthy).unwrap());
    let failed = root();
    assert!(
        fixture_writer::write(
            &failed,
            &failed.join("missing/script"),
            b"not written",
            Some(0o700)
        )
        .is_err()
    );
    assert!(!fixture_writer::cleanup(&failed).unwrap());
    let held = root();
    let result = fixture_writer::with_fault(Fault::UnknownCleanup, || {
        fixture_writer::write(
            &held,
            &held.join("must-not-admit"),
            b"not written",
            Some(0o700),
        )
    });
    assert!(result.is_err());
    let pid = fs::read_to_string(held.join("writer-child-started")).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while std::path::Path::new(&format!("/proc/{pid}")).exists() {
        assert!(
            Instant::now() < end,
            "standalone keeper lost its exact writer"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!held.join("must-not-admit").exists());
    assert!(!fixture_writer::cleanup(&held).unwrap());
}

#[test]
fn failed_or_unwound_writers_retain_namespaces_and_actual_child_custody() {
    use crate::test_support::fixture_writer::{self, Fault};
    for fault in [
        Fault::ChildFailure,
        Fault::Timeout,
        Fault::Unwind,
        Fault::UnknownCleanup,
        Fault::MarkerReplacement,
        Fault::MarkerMissing,
    ] {
        let root = root();
        let script = root.join("script");
        let mut sentinel_command = Command::new("/bin/sleep");
        sentinel_command.arg("30");
        let mut sentinel = Owned::spawn(&mut sentinel_command).unwrap();
        let scope = crate::process_cleanup::test_support::Scope::new();
        let result = std::panic::catch_unwind(|| {
            fixture_writer::with_fault(fault, || {
                crate::write_test_script(&root, &script, b"#!/bin/sh\nexit 0\n", Some(0o700))
            })
        });
        if fault == Fault::Unwind {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        scope.wait_finished();
        if let Ok(pid) = fs::read_to_string(root.join("writer-child-started")) {
            assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        }
        assert!(
            sentinel
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none()
        );
        sentinel.stop().unwrap();
        assert!(!fixture_writer::cleanup(&root).unwrap());
        assert!(root.exists());
        assert!(
            crate::write_test_script(
                &root,
                &root.join("must-not-admit"),
                b"later writer",
                Some(0o700)
            )
            .is_err()
        );
        assert!(!root.join("must-not-admit").exists());
        if fault == Fault::MarkerReplacement {
            let retained = fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .find(|entry| {
                    entry
                        .file_name()
                        .as_encoded_bytes()
                        .starts_with(b".asb-test-executable-writer-custody-")
                })
                .unwrap();
            assert_eq!(
                fs::read(retained.path().join("settled-marker")).unwrap(),
                b"foreign marker bytes"
            );
        }
        scope.wait_finished();
    }
}
