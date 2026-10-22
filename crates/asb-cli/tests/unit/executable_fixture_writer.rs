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
fn parent_executable_writer_inherited_by_owned_child_prevents_healthy_exec() {
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
    assert!(
        result.is_ok_and(|status| status.success()),
        "healthy fixture exec refused while inherited writer survived parent close; errno={error_code:?}; owned cleanup confirmed; evidence {}",
        root.display()
    );
}
