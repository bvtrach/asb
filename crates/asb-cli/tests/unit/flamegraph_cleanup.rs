//! PID-backed direct-child ownership regressions; no host perf or provider.
#![cfg(target_os = "linux")]
#![allow(unsafe_code)] // Linux identity/reaping assertions only; no production unsafe code.
use super::*;
use std::cell::RefCell;
use std::fs;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::PathBuf;

struct Admission {
    name: &'static str,
    pid: u32,
    identity: File,
}
thread_local! { static ADMISSIONS: RefCell<Option<Vec<Admission>>> = const { RefCell::new(None) }; }

pub(super) fn record_admission(name: &'static str, pid: u32) {
    ADMISSIONS.with_borrow_mut(|entries| {
        if let Some(entries) = entries {
            // SAFETY: pidfd_open observes this just-spawned direct child. The
            // returned descriptor is owned once by File; no signals use PIDs.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
            assert!(
                fd >= 0,
                "cannot pin {name}: {}",
                std::io::Error::last_os_error()
            );
            entries.push(Admission {
                name,
                pid,
                identity: unsafe { File::from_raw_fd(fd) },
            });
        }
    });
}
struct Observer;
impl Observer {
    fn new() -> Self {
        ADMISSIONS.with_borrow_mut(|entries| {
            assert!(entries.is_none());
            *entries = Some(Vec::new());
        });
        Self
    }
    fn assert_reaped(&self, count: usize) {
        ADMISSIONS.with_borrow(|entries| {
            let entries = entries.as_ref().unwrap();
            assert_eq!(entries.len(), count);
            for admission in entries {
                let mut poll = libc::pollfd {
                    fd: admission.identity.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: poll points to one valid descriptor; waitpid only
                // queries an originally-owned child, never signals anything.
                assert_eq!(
                    unsafe { libc::poll(&mut poll, 1, 0) },
                    1,
                    "{} survived cleanup",
                    admission.name
                );
                assert_ne!(poll.revents & libc::POLLIN, 0);
                assert_eq!(
                    unsafe {
                        libc::waitpid(admission.pid as i32, std::ptr::null_mut(), libc::WNOHANG)
                    },
                    -1,
                    "{} was not reaped",
                    admission.name
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
        ADMISSIONS.with_borrow_mut(|entries| *entries = None);
    }
}
struct Sentinel(Child);
impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn isolated_pipeline_reaps_all_admitted_children_on_every_exit() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    const CASE: &str = "ASB_FLAMEGRAPH_CLEANUP_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/flamegraph-cleanup-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let name = format!(
            "{}::isolated_pipeline_reaps_all_admitted_children_on_every_exit",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "perf-spawn-fails",
            "folded-blocked",
            "collapse-spawn-fails",
            "perf-fails-collapse-blocked",
            "collapse-fails",
            "collapse-timeout",
            "folded-unlinked",
            "svg-blocked",
            "render-spawn-fails",
            "render-fails",
            "render-timeout",
            "status-blocked",
            "success",
            "preflight-success",
            "preflight-nonzero",
            "preflight-spawn-fails",
            "preflight-timeout",
        ] {
            let directory = root.join(case);
            fs::create_dir_all(directory.join("empty-path")).unwrap();
            let fixture = directory.join("fixture.py");
            fs::write(
                &fixture,
                r#"#!/usr/bin/python3
import os,sys,time
from pathlib import Path
case=os.environ['ASB_FLAMEGRAPH_CLEANUP_CASE']
role=Path(sys.argv[0]).name
if '--help' in sys.argv:
    if case=='preflight-timeout': time.sleep(60)
    sys.exit(3 if case=='preflight-nonzero' else 0)
if role=='perf':
    if case in ('folded-blocked','collapse-spawn-fails'): time.sleep(60)
    sys.stdout.write('fixture perf samples\n');sys.stdout.flush()
    sys.exit(7 if case=='perf-fails-collapse-blocked' else 0)
if role=='stackcollapse-perf.pl':
    if case in ('perf-fails-collapse-blocked','collapse-timeout'): time.sleep(60)
    sys.stdin.buffer.read()
    if case=='collapse-fails': sys.exit(8)
    if case=='folded-unlinked': Path('measurements/flamegraph.folded').unlink()
    print('fixture;frame 1')
elif role=='flamegraph':
    sys.stdin.buffer.read()
    if case=='render-timeout': time.sleep(60)
    if case=='render-fails': sys.exit(9)
    print('<svg xmlns="http://www.w3.org/2000/svg"><text>fixture</text></svg>')
else: raise AssertionError('unrecognized fixture role')
"#,
            )
            .unwrap();
            fs::set_permissions(&fixture, fs::Permissions::from_mode(0o700)).unwrap();
            for (tool, executable) in [
                ("perf", "perf"),
                ("flamegraph", "stackcollapse-perf.pl"),
                ("flamegraph", "flamegraph"),
            ] {
                let path = directory
                    .join("tools")
                    .join(tool)
                    .join("bin")
                    .join(executable);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                if case != "perf-spawn-fails" || tool != "perf" {
                    symlink(&fixture, path).unwrap();
                }
            }
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .current_dir(&directory)
                .env_clear()
                .env(CASE, case)
                .env("HOME", &directory)
                .env("PATH", directory.join("empty-path"));
            if case == "collapse-spawn-fails" || case == "preflight-spawn-fails" {
                command.env(
                    "ASB_FLAMEGRAPH_STACKCOLLAPSE",
                    directory.join("missing-collapse"),
                );
            }
            if case == "render-spawn-fails" {
                command.env("ASB_FLAMEGRAPH_RENDER", directory.join("missing-render"));
            }
            if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
                command.env("LLVM_PROFILE_FILE", profile);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
        fs::remove_dir_all(root).unwrap();
        return;
    };
    let root = PathBuf::from(std::env::var_os("HOME").unwrap());
    if case.starts_with("preflight-") {
        let mut sudo = std::collections::BTreeSet::new();
        let selected = vec!["flamegraph".to_owned()];
        let tools = root.join("tools");
        let mut context = crate::collectors::PreflightContext {
            measurement_dir: &root,
            selected: &selected,
            tool_dir: Some(&tools),
            sudo_collectors: &mut sudo,
        };
        let observer = Observer::new();
        let result = ADAPTER.preflight(&mut context);
        match case.as_str() {
            "preflight-timeout" => {
                assert!(result.unwrap_err().contains("did not finish its preflight"));
                observer.assert_reaped(1);
            }
            "preflight-spawn-fails" => {
                assert!(
                    result
                        .unwrap_err()
                        .contains("cannot execute flamegraph tool")
                );
                observer.assert_reaped(0);
            }
            _ => {
                result.unwrap();
                observer.assert_reaped(2);
            }
        }
        assert!(sudo.is_empty());
        return;
    }
    let mut session = measurements::Session::start_in_execution_namespace(
        &root.join("measurements"),
        &["agent-memory".into()],
    )
    .unwrap();
    session.tool_dir = Some(root.join("tools"));
    let data = root.join("perf.data");
    fs::write(&data, b"synthetic perf fixture").unwrap();
    session.set_artifact(crate::collectors::SharedArtifact::StackSamples, data);
    for (case_name, file) in [
        ("folded-blocked", "flamegraph.folded"),
        ("svg-blocked", "flamegraph.svg"),
        ("status-blocked", "flamegraph.json"),
    ] {
        if case == case_name {
            fs::create_dir(session.dir.join(file)).unwrap();
        }
    }
    let mut sentinel = Sentinel(Command::new("/bin/sleep").arg("30").spawn().unwrap());
    let observer = Observer::new();
    let result = session.write_flamegraph_with_timeout(Duration::from_secs(2));
    let (count, error) = match case.as_str() {
        "perf-spawn-fails" => (0, Some("cannot start perf script")),
        "folded-blocked" => (1, Some("cannot create flamegraph folded output")),
        "collapse-spawn-fails" => (1, Some("cannot start flamegraph stack collapse")),
        "folded-unlinked" => (2, Some("cannot open collapsed flamegraph output")),
        "svg-blocked" => (2, Some("cannot create flamegraph.svg")),
        "render-spawn-fails" => (2, Some("cannot start flamegraph renderer")),
        "status-blocked" => (3, Some("cannot create")),
        "render-fails" | "render-timeout" | "success" => (3, None),
        _ => (2, None),
    };
    if let Some(expected) = error {
        let error = result.unwrap_err();
        assert!(error.contains(expected), "{error}");
    } else {
        result.unwrap();
        let status: serde_json::Value =
            serde_json::from_slice(&fs::read(session.dir.join("flamegraph.json")).unwrap())
                .unwrap();
        assert_eq!(
            status["status"],
            if case == "success" {
                "completed"
            } else {
                "unavailable"
            }
        );
    }
    observer.assert_reaped(count);
    assert!(
        sentinel.0.try_wait().unwrap().is_none(),
        "unrelated sentinel was stopped"
    );
}

#[test]
fn guard_unwind_and_explicit_reaper_handoff_retain_exact_child_ownership() {
    let observer = Observer::new();
    let result = std::panic::catch_unwind(|| {
        let mut children = PipelineChildren::default();
        children.add(
            "unwinding stage",
            Command::new("/bin/sleep").arg("30").spawn().unwrap(),
        );
        panic!("fixture unwind");
    });
    assert!(result.is_err());
    observer.assert_reaped(1);
    drop(observer);
    let observer = Observer::new();
    let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    record_admission("deferred stage", child.id());
    defer_reaping(child).join().unwrap();
    observer.assert_reaped(1);
}
