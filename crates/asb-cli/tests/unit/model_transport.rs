//! Only owned Python fixtures: no curl, network, credentials or host collectors.
use super::*;
use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
std::thread_local! {
    static ADMITTED: std::cell::RefCell<Option<Vec<(u32,std::fs::File)>>> = const { std::cell::RefCell::new(None) };
    static CLEANUP_FAULT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static PANIC_ADMISSION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(target_os = "linux")]
pub(super) fn cleanup_poll_override() -> Option<std::io::Result<Option<ExitStatus>>> {
    match CLEANUP_FAULT.get() {
        1 => Some(Ok(None)),
        2 => Some(Err(std::io::Error::other("fixture cleanup poll error"))),
        _ => None,
    }
}
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub(super) fn admitted(pid: u32) {
    use std::os::fd::FromRawFd;
    ADMITTED.with_borrow_mut(|admissions| {
        if let Some(admissions) = admissions {
            // SAFETY: pidfd_open pins exactly the child just spawned by this
            // thread, before any wait can reap it; the returned fd is owned.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
            assert!(fd >= 0, "pidfd admission failed");
            admissions.push((pid, unsafe { std::fs::File::from_raw_fd(fd as i32) }));
        }
    });
    assert!(
        !PANIC_ADMISSION.get(),
        "fixture panic after child admission"
    );
}

#[cfg(target_os = "linux")]
struct Admissions;
#[cfg(target_os = "linux")]
impl Admissions {
    fn new() -> Self {
        ADMITTED.with_borrow_mut(|admissions| {
            assert!(admissions.is_none());
            *admissions = Some(Vec::new());
        });
        Self
    }
    #[allow(unsafe_code)]
    fn assert_reaped(&self, count: usize) {
        use std::os::fd::AsRawFd;
        ADMITTED.with_borrow(|admissions| {
            let admissions = admissions.as_ref().unwrap();
            assert_eq!(admissions.len(), count);
            for (pid, pidfd) in admissions {
                let mut poll = libc::pollfd {
                    fd: pidfd.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: pinned pidfd remains owned; no child or process
                // outside this fixture is signalled by these observations.
                assert_eq!(
                    unsafe { libc::poll(&mut poll, 1, 0) },
                    1,
                    "original child {pid} is still alive"
                );
                assert_eq!(
                    unsafe { libc::waitpid(*pid as i32, std::ptr::null_mut(), libc::WNOHANG) },
                    -1
                );
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD),
                    "owned child must already be reaped"
                );
            }
        });
    }
}
#[cfg(target_os = "linux")]
impl Drop for Admissions {
    fn drop(&mut self) {
        ADMITTED.with_borrow_mut(|admissions| *admissions = None);
        CLEANUP_FAULT.set(0);
        PANIC_ADMISSION.set(false);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn creation_failure_never_spawns_and_uncertain_cleanup_retains_owned_custody() {
    use crate::process_cleanup::test_support;
    let fixture = Fixture::new();
    let admissions = Admissions::new();
    let workers = test_support::Scope::new();
    test_support::set_creation_failure(true);
    let mut command =
        fixture.command("from pathlib import Path; Path('spawned').write_text('bad')");
    let error = unix::capture(&mut command, b"", "fixture", budget())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("cannot establish model-discovery cleanup worker"),
        "{error}"
    );
    assert!(!fixture.0.join("spawned").exists());
    admissions.assert_reaped(0);
    test_support::set_creation_failure(false);
    for fault in [1, 2] {
        CLEANUP_FAULT.set(fault);
        let mut command = fixture.command("import time; time.sleep(10)");
        let start = Instant::now();
        let error = unix::capture(
            &mut command,
            b"",
            "fixture",
            unix::Budget {
                io: Duration::from_millis(50),
                total: Duration::from_millis(150),
            },
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains("timed out"),
            "original operation error retained: {error}"
        );
        assert!(error.contains("owned reaper retains the child"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(3));
        CLEANUP_FAULT.set(0);
    }
    workers.wait_finished();
    admissions.assert_reaped(2);
    assert_eq!(workers.count(), 2);
}

#[cfg(target_os = "linux")]
#[test]
fn unwinding_after_admission_reaps_only_the_owned_child() {
    let fixture = Fixture::new();
    let admissions = Admissions::new();
    let workers = crate::process_cleanup::test_support::Scope::new();
    PANIC_ADMISSION.set(true);
    let result = std::panic::catch_unwind(|| {
        let mut command = fixture.command("import time; time.sleep(10)");
        let _ = unix::capture(&mut command, b"", "fixture", budget());
    });
    assert!(result.is_err());
    PANIC_ADMISSION.set(false);
    workers.wait_finished();
    admissions.assert_reaped(1);
}

struct Fixture(std::path::PathBuf);
struct Sentinel(std::process::Child);
impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Fixture {
    fn new() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/model-transport-fixtures")
            .join(crate::workloads::TaskIdentity::local("transport", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn command(&self, source: &str) -> Command {
        let mut command = Command::new("/usr/bin/python3");
        command
            .args(["-c", source])
            .current_dir(&self.0)
            .env_clear()
            .env("HOME", &self.0)
            .env("PATH", &self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn budget() -> unix::Budget {
    unix::Budget {
        io: Duration::from_millis(300),
        total: Duration::from_millis(800),
    }
}

#[test]
fn simultaneous_pipe_pressure_and_metadata_tail_are_bounded_without_deadlock() {
    let fixture = Fixture::new();
    let mut command = fixture.command(
        r#"
import os,sys
os.write(1,b'x'*131072)
os.write(2,b'private-untrusted-data'*8192)
data=sys.stdin.buffer.read()
os.write(1,str(len(data)).encode())
os.write(2,b'\nASB_HTTP_STATUS=200\nASB_PROXY_STATUS=0\nASB_CONTENT_TYPE=application/json\n')
"#,
    );
    let config = vec![b'a'; 256 * 1024];
    let captured = unix::capture(
        &mut command,
        &config,
        "fixture",
        unix::Budget {
            io: Duration::from_secs(3),
            total: Duration::from_secs(4),
        },
    )
    .unwrap();
    assert!(captured.status.success());
    assert_eq!(captured.stdout.len(), 131072 + 6);
    assert!(captured.stdout.ends_with(b"262144"));
    assert_eq!(captured.stderr.len(), 8192);
    assert_eq!(
        super::super::CurlMetadata::parse(&captured.stderr).http_status,
        Some(200)
    );
}

#[test]
fn exact_body_limit_succeeds_and_one_additional_byte_is_rejected() {
    assert_eq!(MAX_RESPONSE, 16 * 1024 * 1024);
    for extra in [0, 1] {
        let fixture = Fixture::new();
        #[cfg(target_os = "linux")]
        let admissions = Admissions::new();
        let mut command = fixture.command(
            r#"
import os,sys
sys.stdin.buffer.read()
length=int(os.environ['BODY_BYTES'])
sys.stdout.buffer.write(b'['+b' '*(length-2)+b']')
sys.stdout.buffer.flush()
"#,
        );
        command.env("BODY_BYTES", (MAX_RESPONSE + extra).to_string());
        let result = unix::capture(
            &mut command,
            b"",
            "fixture",
            unix::Budget {
                io: Duration::from_secs(3),
                total: Duration::from_secs(4),
            },
        );
        if extra == 0 {
            let captured = result.unwrap();
            assert!(captured.status.success());
            assert_eq!(captured.stdout.len(), MAX_RESPONSE);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&captured.stdout).unwrap(),
                serde_json::json!([])
            );
        } else {
            let error = result.err().unwrap().to_string();
            assert!(error.contains("16 MiB response limit"), "{error}");
        }
        #[cfg(target_os = "linux")]
        admissions.assert_reaped(1);
    }
}

#[test]
fn blocked_stdin_and_never_ending_output_return_before_the_absolute_deadline() {
    #[cfg(target_os = "linux")]
    let admissions = Admissions::new();
    let mut sentinel = Sentinel(
        Command::new("/usr/bin/python3")
            .args(["-c", "import time; time.sleep(30)"])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for (source, config, reason) in [
        (
            "import time; time.sleep(10)",
            vec![b'x'; 1024 * 1024],
            "sending request",
        ),
        (
            "import sys,time; sys.stdin.buffer.read(); time.sleep(10)",
            vec![],
            "response streams",
        ),
        (
            "import os,sys; sys.stdin.buffer.read();\nwhile True: os.write(1,b'x'*65536)",
            vec![],
            "16 MiB",
        ),
        (
            "import os; os.close(0)",
            vec![b'x'; 1024 * 1024],
            "cannot send",
        ),
    ] {
        let fixture = Fixture::new();
        let mut command = fixture.command(source);
        let start = Instant::now();
        let bounds = if reason == "16 MiB" {
            unix::Budget {
                io: Duration::from_secs(3),
                total: Duration::from_secs(4),
            }
        } else {
            budget()
        };
        let error = unix::capture(&mut command, &config, "fixture", bounds)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(reason), "{error}");
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "caller must not join an unbounded IO worker"
        );
    }
    #[cfg(target_os = "linux")]
    admissions.assert_reaped(4);
    assert!(
        sentinel.0.try_wait().unwrap().is_none(),
        "unrelated sentinel survives"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn descendant_inherited_streams_cannot_extend_the_transport_lifetime() {
    for held in ["stdout", "stderr"] {
        let fixture = Fixture::new();
        let admissions = Admissions::new();
        let mut command = fixture.command(
            r#"
import os,sys,time
sys.stdin.buffer.read()
pid=os.fork()
if pid==0:
    from pathlib import Path
    os.close(2 if os.environ['HELD']=='stdout' else 1)
    Path('descendant').write_text(str(os.getpid()))
    deadline=time.monotonic()+5
    while not Path('release').exists() and time.monotonic()<deadline: time.sleep(0.005)
    os._exit(0)
os._exit(0)
"#,
        );
        command.env("HELD", held);
        let start = Instant::now();
        let error = unix::capture(&mut command, b"", "fixture", budget())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("response streams"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(3));
        admissions.assert_reaped(1);
        let pid: u32 = fs::read_to_string(fixture.0.join("descendant"))
            .unwrap()
            .parse()
            .unwrap();
        // Pin only the positively identified fixture descendant, and release it
        // cooperatively. ASB must not claim or kill arbitrary descendants.
        #[allow(unsafe_code)]
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        assert!(fd >= 0);
        use std::os::fd::{AsRawFd, FromRawFd};
        #[allow(unsafe_code)]
        let pidfd = unsafe { std::fs::File::from_raw_fd(fd as i32) };
        fs::write(fixture.0.join("release"), "release owned fixture").unwrap();
        let mut poll = libc::pollfd {
            fd: pidfd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        #[allow(unsafe_code)]
        let exited = unsafe { libc::poll(&mut poll, 1, 3000) };
        assert_eq!(exited, 1, "released fixture descendant exits independently");
    }
}

#[test]
fn missing_configured_pipe_is_an_error_and_production_budget_reserves_cleanup() {
    assert_eq!(unix::Budget::PRODUCTION.io, Duration::from_secs(30));
    assert_eq!(unix::Budget::PRODUCTION.total, Duration::from_secs(35));
    let fixture = Fixture::new();
    #[cfg(target_os = "linux")]
    let admissions = Admissions::new();
    let mut command = fixture.command("import time; time.sleep(10)");
    command.stderr(Stdio::null());
    let error = unix::capture(&mut command, b"", "fixture", budget())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("cannot configure model discovery pipes: stderr missing"),
        "{error}"
    );
    #[cfg(target_os = "linux")]
    admissions.assert_reaped(1);
}

#[test]
fn closed_streams_still_wait_for_child_and_nonzero_status_is_retained() {
    let fixture = Fixture::new();
    let mut command =
        fixture.command("import os,time; os.close(0); os.close(1); os.close(2); time.sleep(10)");
    let error = unix::capture(&mut command, b"", "fixture", budget())
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("waiting for curl to exit"), "{error}");
    let mut command = fixture
        .command("import sys; sys.stdin.buffer.read(); sys.stdout.write('{}'); sys.exit(22)");
    let captured = unix::capture(&mut command, b"", "fixture", budget()).unwrap();
    assert_eq!(captured.status.code(), Some(22));
    assert_eq!(captured.stdout, b"{}");
}
