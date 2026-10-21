//! Linux identity-pinned direct-child fixtures; no live provider or host state.
#![allow(unsafe_code)]
use super::*;
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};

thread_local! {
    static READER_FAIL: Cell<bool> = const { Cell::new(false) };
    static AFTER_SPAWN: Cell<u8> = const { Cell::new(0) };
    static POLL_FAIL: Cell<u8> = const { Cell::new(0) };
    static IDENTITIES: RefCell<Vec<(u32, File)>> = const { RefCell::new(Vec::new()) };
    static READERS: RefCell<Vec<Arc<AtomicBool>>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn reader_receipt() -> Arc<AtomicBool> {
    let receipt = Arc::new(AtomicBool::new(false));
    READERS.with_borrow_mut(|readers| readers.push(Arc::clone(&receipt)));
    receipt
}
fn readers_finished() {
    let deadline = Instant::now() + Duration::from_secs(3);
    READERS.with_borrow_mut(|readers| {
        for receipt in readers.drain(..) {
            while !receipt.load(Ordering::Acquire) {
                assert!(
                    Instant::now() < deadline,
                    "stopped output worker did not exit"
                );
                std::thread::sleep(POLL);
            }
        }
    });
}

pub(super) fn reader_creation() -> Result<(), String> {
    if READER_FAIL.get() {
        Err("fixture reader creation failure".into())
    } else {
        Ok(())
    }
}
pub(super) fn after_spawn(pid: u32) -> Result<(), String> {
    if AFTER_SPAWN.get() != 0 {
        // SAFETY: pin the directly owned child before any cleanup or handoff.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        assert!(fd >= 0);
        if AFTER_SPAWN.get() == 3 {
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: wait only for the pinned fixture to exit, without reaping.
            assert_eq!(unsafe { libc::poll(&mut poll, 1, 2000) }, 1);
        }
        IDENTITIES.with_borrow_mut(|ids| ids.push((pid, unsafe { File::from_raw_fd(fd) })));
    }
    match AFTER_SPAWN.get() {
        1 => Err("fixture primary startup failure".into()),
        2 => panic!("fixture startup unwind"),
        _ => Ok(()),
    }
}
pub(super) fn cleanup_timeout() -> Option<Duration> {
    (POLL_FAIL.get() != 0).then_some(Duration::ZERO)
}
pub(super) fn poll_override() -> Option<std::io::Result<Option<std::process::ExitStatus>>> {
    match POLL_FAIL.get() {
        1 => Some(Ok(None)),
        2 => Some(Err(std::io::Error::other("fixture poll failure"))),
        _ => None,
    }
}

fn command(script: &str) -> Command {
    let python = crate::tools::host_executable(std::path::Path::new("python3")).unwrap();
    let mut command = Command::new(python);
    command
        .env_clear()
        .args(["-u", "-c", script])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[test]
fn output_limits_and_post_readiness_drain_are_bounded() {
    for (script, expected) in [
        (
            "import sys,time;sys.stdout.write('x'*8192);sys.stdout.flush();time.sleep(30)",
            "byte or line limit",
        ),
        (
            "import time;print('x'*8192,flush=True);time.sleep(30)",
            "byte or line limit",
        ),
        (
            "import time;print(('x'*2048+'\\n')*40,flush=True);time.sleep(30)",
            "byte or line limit",
        ),
        (
            "import time;print('noise\\n'*64,flush=True);time.sleep(30)",
            "message limit",
        ),
        ("import time;time.sleep(30)", "within 5 seconds"),
    ] {
        let mut local = Local::new().unwrap();
        let start = Instant::now();
        let error = local.spawn(&mut command(script), true).unwrap_err();
        let pid = local.children[0].id();
        assert!(error.contains(expected), "{error}");
        drop(local);
        assert!(start.elapsed() < Duration::from_secs(7));
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    let mut local = Local::new().unwrap();
    assert_eq!(local.spawn(&mut command("import sys,time;print('Codex Bridge API key: fixture-secret',flush=True);sys.stdout.write('x'*1000000);sys.stdout.flush();time.sleep(30)"), true).unwrap(), "fixture-secret");
    std::thread::sleep(Duration::from_millis(100));
    assert!(local.children[0].try_wait().unwrap().is_none());
    assert!(!local.readers[0].worker.as_ref().unwrap().is_finished());
    drop(local);
    readers_finished();
}

#[test]
fn fallible_reader_and_reaper_precede_every_child() {
    let scope = crate::process_cleanup::test_support::Scope::new();
    crate::process_cleanup::test_support::set_creation_failure(true);
    assert!(Local::new().is_err());
    crate::process_cleanup::test_support::set_creation_failure(false);
    let mut local = Local::new().unwrap();
    READER_FAIL.set(true);
    assert!(
        local
            .spawn(&mut command("raise Exception('must not start')"), true)
            .is_err()
    );
    READER_FAIL.set(false);
    assert!(local.children.is_empty());
    local.spawn(&mut command("import time;print('Codex Bridge API key: fixture-secret',flush=True);time.sleep(30)"), true).unwrap();
    READER_FAIL.set(true);
    assert!(
        local
            .spawn(&mut command("raise Exception('must not start')"), false)
            .is_err()
    );
    READER_FAIL.set(false);
    assert_eq!(local.children.len(), 1);
    drop(local);
    scope.wait_finished();
    readers_finished();
}

#[test]
fn cleanup_stops_reader_even_when_parent_retains_an_open_output_writer() {
    let mut local = Local::new().unwrap();
    let (mut input, writer) = UnixStream::pair().unwrap();
    input.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let (sender, _receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || drain_output(&mut input, &stopped, &sender, true));
    local.readers.push(Reader {
        stop,
        worker: Some(worker),
    });
    // A retained writer prevents EOF, but cooperative cancellation still exits.
    assert!(local.cleanup().is_ok());
    assert!(local.readers.is_empty());
    drop(writer);
}

#[test]
fn ready_marker_cannot_admit_an_already_exited_child() {
    for (script, key) in [
        (
            "print('Codex Bridge API key: fixture-secret',flush=True)",
            true,
        ),
        ("print('ASB cli2key proxy ready',flush=True)", false),
    ] {
        let mut local = Local::new().unwrap();
        AFTER_SPAWN.set(3);
        let error = local.spawn(&mut command(script), key).unwrap_err();
        AFTER_SPAWN.set(0);
        assert!(error.contains("exited at readiness"), "{error}");
        assert!(!error.contains("fixture-secret"));
        drop(local);
        readers_finished();
        IDENTITIES.with_borrow_mut(Vec::clear);
    }
}

#[test]
fn errors_unwinds_and_uncertain_exit_retain_direct_child_custody() {
    let mut sentinel = command("import time;time.sleep(30)")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    for children in [1, 2] {
        for failure in [1, 2, 3, 4] {
            let scope = crate::process_cleanup::test_support::Scope::new();
            let mut local = Local::new().unwrap();
            let script = "import time;print('Codex Bridge API key: fixture-secret',flush=True);time.sleep(30)";
            let mut pids = Vec::new();
            if children == 2 {
                local.spawn(&mut command(script), true).unwrap();
                pids.push(local.children[0].id());
            }
            AFTER_SPAWN.set(if failure <= 2 { failure } else { 1 });
            POLL_FAIL.set(if failure >= 3 { failure - 2 } else { 0 });
            let start = Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let error = local.spawn(&mut command(script), true).unwrap_err();
                pids.extend(local.children.iter().map(Child::id));
                let error = local.failure(error);
                assert!(error.contains("fixture primary startup failure"));
                if failure >= 3 {
                    assert!(error.contains("owned reaper retains"));
                }
            }));
            assert_eq!(result.is_err(), failure == 2);
            AFTER_SPAWN.set(0);
            POLL_FAIL.set(0);
            scope.wait_finished();
            readers_finished();
            assert!(start.elapsed() < Duration::from_secs(2));
            IDENTITIES.with_borrow_mut(|ids| {
                for (pid, pinned) in ids.drain(..) {
                    let mut poll = libc::pollfd {
                        fd: pinned.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: valid pinned descriptor and owned child identity.
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
            });
            for pid in pids {
                assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
            }
            assert!(sentinel.try_wait().unwrap().is_none());
        }
    }
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}

#[test]
fn explicit_session_finish_is_once_reports_uncertainty_and_retains_custody() {
    use crate::providers::ProviderSession;
    for mode in [0, 1, 2] {
        let scope = crate::process_cleanup::test_support::Scope::new();
        let mut owner = Local::new().unwrap();
        owner.spawn(&mut command("import time;print('Codex Bridge API key: fixture-secret',flush=True);time.sleep(30)"), true).unwrap();
        let mut bridge = super::super::Bridge {
            api_key: "fixture-secret".into(),
            base_url: "http://127.0.0.1:1/v1".into(),
            _session: super::super::Session::Local { _owner: owner },
        };
        POLL_FAIL.set(mode);
        let result = bridge.finish();
        POLL_FAIL.set(0);
        assert_eq!(result.is_err(), mode != 0);
        if let Err(error) = result {
            assert!(error.to_string().contains("owned reaper retains"));
        }
        assert!(
            bridge.finish().is_ok(),
            "already-settled finish must not run cleanup twice"
        );
        drop(bridge);
        scope.wait_finished();
        readers_finished();
    }
}
