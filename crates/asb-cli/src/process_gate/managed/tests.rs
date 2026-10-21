use super::*;
use crate::process_cleanup::test_support;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;

fn root() -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gate-managed-fixtures")
        .join(crate::workloads::TaskIdentity::local("gate", None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn command(gate: &Path, program: &str, args: &[String]) -> Command {
    let mut launch = crate::launch::ProcessLaunch::new(program);
    for arg in args {
        launch.arg(arg);
    }
    let (mut command, _) = launch.native(Some(gate));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

struct ReleaseOnDrop(std::path::PathBuf);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, "release fixture");
    }
}

#[test]
fn failed_keeper_creation_never_spawns_a_launcher() {
    let scope = test_support::Scope::new();
    test_support::set_creation_failure(true);
    let result = Admission::new(&mut command(Path::new("/missing"), "/bin/true", &[]));
    test_support::set_creation_failure(false);
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("fixture resource exhaustion")
    );
    assert_eq!(scope.count(), 0);
}

#[test]
fn startup_poll_error_transfers_real_child_custody_without_resuming_it() {
    let scope = test_support::Scope::new();
    let root = root();
    let launcher = root.join("stopped-startup");
    let marker = root.join("must-not-run");
    std::fs::write(
        &launcher,
        format!(
            "#!/bin/sh\ntrap '' TERM\nkill -STOP $$\nprintf unsafe > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command(&launcher, "/bin/true", &[]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    let pid = gate.id();
    let error = gate
        .stop_starting_before(Instant::now() + Duration::from_secs(1), |_| {
            Err(io::Error::other("fixture poll failure"))
        })
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("fixture poll failure") && error.contains("wait-only custody"),
        "{error}"
    );
    assert!(gate.child.is_none());
    drop(gate);
    scope.wait_finished();
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(!marker.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_stopped_launcher_is_never_resumed_on_admission_failure() {
    let scope = test_support::Scope::new();
    let root = root();
    let launcher = root.join("legacy");
    let marker = root.join("must-not-run");
    std::fs::write(
        &launcher,
        format!(
            "#!/bin/sh\ntrap '' TERM\nkill -STOP $$\nprintf unsafe > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command(&launcher, "/bin/true", &[]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    let error = gate.resume().unwrap_err().to_string();
    assert!(error.contains("ABI is incompatible"), "{error}");
    assert!(error.contains("rebuild ASB"));
    gate.stop().unwrap();
    drop(gate);
    scope.wait_finished();
    assert!(!marker.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn actual_gate_confirms_nonzero_status_and_hides_its_proof_descriptor() {
    let scope = test_support::Scope::new();
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("asb");
    assert!(
        binary.is_file(),
        "build current ASB binary before this test"
    );
    let args = vec!["-c".into(), "test -z \"${ASB_GATE_RECEIPT_FD+x}\" || exit 88; printf captured; printf diagnostic >&2; exit 7".into()];
    let mut command = command(&binary, "/bin/sh", &args);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    let mut stdout = gate.take_stdout().unwrap();
    let mut stderr = gate.take_stderr().unwrap();
    gate.resume().unwrap();
    assert_eq!(gate.wait().unwrap().code(), Some(7));
    assert_eq!(gate.try_wait().unwrap().unwrap().code(), Some(7));
    let mut out = String::new();
    let mut err = String::new();
    stdout.read_to_string(&mut out).unwrap();
    stderr.read_to_string(&mut err).unwrap();
    assert_eq!(out, "captured");
    assert_eq!(err, "diagnostic");
    drop(gate);
    scope.wait_finished();
}

#[test]
fn protocol_fixture_exit_without_quiescence_is_never_a_clean_completion() {
    let scope = test_support::Scope::new();
    let root = root();
    let launcher = root.join("protocol-only.py");
    std::fs::write(&launcher, "#!/usr/bin/python3\nimport os,signal,socket,struct\ns=socket.socket(fileno=int(os.environ['ASB_GATE_RECEIPT_FD']))\ns.sendall(b'ASBGATE2'+bytes([1,0,0,0])+struct.pack('<I',os.getpid()))\nos.kill(os.getpid(),signal.SIGSTOP)\nos._exit(0)\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command(&launcher, "/bin/true", &[]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    gate.resume().unwrap();
    let error = gate.wait().unwrap_err().to_string();
    assert!(error.contains("without a quiescence receipt"), "{error}");
    assert!(gate.observed.unwrap().success());
    assert!(gate.stop().unwrap_err().to_string().contains("unconfirmed"));
    assert!(
        gate.child.is_none(),
        "finished child must leave public custody without being signalled again"
    );
    drop(gate);
    scope.wait_finished();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_hello_is_rejected_without_admitting_the_fixture_client() {
    let scope = test_support::Scope::new();
    let root = root();
    let launcher = root.join("malformed.py");
    let marker = root.join("must-not-run");
    std::fs::write(&launcher, format!("#!/usr/bin/python3\nimport os,signal,socket\ns=socket.socket(fileno=int(os.environ['ASB_GATE_RECEIPT_FD']))\ns.sendall(b'not-compatible!!')\nos.kill(os.getpid(),signal.SIGSTOP)\nopen({:?},'w').write('unsafe')\n",marker.to_str().unwrap())).unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command(&launcher, "/bin/true", &[]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    assert!(
        gate.resume()
            .unwrap_err()
            .to_string()
            .contains("ABI is incompatible")
    );
    gate.stop().unwrap();
    drop(gate);
    scope.wait_finished();
    assert!(!marker.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn previous_real_abi_hello_is_rejected_before_new_target_wire_can_execute() {
    let scope = test_support::Scope::new();
    let root = root();
    let launcher = root.join("previous-abi.py");
    let marker = root.join("must-not-run");
    // ASBGATE1 was the prior valid ABI, not random malformed input. It cannot
    // interpret the new explicit target-argv0 framing and must not get CONT.
    std::fs::write(&launcher, format!("#!/usr/bin/python3\nimport os,signal,socket,struct\ns=socket.socket(fileno=int(os.environ['ASB_GATE_RECEIPT_FD']))\ns.sendall(b'ASBGATE1'+bytes([1,0,0,0])+struct.pack('<I',os.getpid()))\nos.kill(os.getpid(),signal.SIGSTOP)\nopen({:?},'w').write('old runtime admitted incompatible arguments')\n",marker.to_str().unwrap())).unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command(&launcher, "/bin/true", &[]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    assert!(
        gate.resume()
            .unwrap_err()
            .to_string()
            .contains("ABI is incompatible")
    );
    gate.stop().unwrap();
    drop(gate);
    scope.wait_finished();
    assert!(!marker.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellation_deadline_retains_a_live_protocol_guardian_until_its_own_finish() {
    let scope = test_support::Scope::new();
    let root = root();
    let launcher = root.join("delayed-guardian.py");
    let release = root.join("release");
    let complete = root.join("quiescent");
    // Protocol/keeper fixture only: actual descendant qualification is covered
    // by the native gate integration matrix, not this controllable stand-in.
    std::fs::write(&launcher, format!("#!/usr/bin/python3\nimport os,signal,socket,struct,time\nfrom pathlib import Path\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\ns=socket.socket(fileno=int(os.environ['ASB_GATE_RECEIPT_FD']))\ns.sendall(b'ASBGATE2'+bytes([1,0,0,0])+struct.pack('<I',os.getpid()))\nos.kill(os.getpid(),signal.SIGSTOP)\nwhile not Path({:?}).exists(): time.sleep(.005)\nPath({:?}).write_text('finished under retained custody')\nos._exit(0)\n", release.to_str().unwrap(), complete.to_str().unwrap())).unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command(&launcher, "/bin/true", &[]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    let _release_on_failure = ReleaseOnDrop(release.clone());
    gate.resume().unwrap();
    let pid = gate.id();
    let error = gate
        .stop_before(Instant::now() + Duration::from_millis(100))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("unconfirmed") && error.contains("wait-only custody"),
        "{error}"
    );
    assert!(gate.child.is_none());
    drop(gate);
    assert!(
        std::fs::read_to_string(format!("/proc/{pid}/status"))
            .unwrap()
            .lines()
            .all(|line| !line.starts_with("State:") || line.split_whitespace().nth(1) != Some("Z"))
    );
    std::fs::write(&release, "release fixture").unwrap();
    scope.wait_finished();
    assert_eq!(
        std::fs::read_to_string(&complete).unwrap(),
        "finished under retained custody"
    );
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_cancellation_quiesces_real_double_forked_term_resistant_writers() {
    use std::os::fd::{AsRawFd, FromRawFd};
    let scope = test_support::Scope::new();
    let root = root();
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("asb");
    let helper_pid = root.join("helper");
    let release = root.join("release");
    let late_write = root.join("must-not-write");
    let script = format!(
        "import os,signal,time\nfrom pathlib import Path\nassert 'ASB_GATE_RECEIPT_FD' not in os.environ\nlinks=[]\nfor p in Path('/proc/self/fd').iterdir():\n try: links.append(os.readlink(p))\n except FileNotFoundError: pass\nassert not any(p.startswith('socket:') for p in links), links\nfirst=os.fork()\nif first == 0:\n os.setsid()\n if os.fork() != 0: os._exit(0)\n signal.signal(signal.SIGTERM,signal.SIG_IGN)\n p=Path({:?}); t=p.with_suffix('.tmp'); t.write_text(str(os.getpid())); t.replace(p)\n while not Path({:?}).exists(): time.sleep(.005)\n Path({:?}).write_text('unsafe')\n time.sleep(30)\n os._exit(0)\nos.waitpid(first,0)\ntime.sleep(30)\n",
        helper_pid.to_str().unwrap(),
        release.to_str().unwrap(),
        late_write.to_str().unwrap()
    );
    let mut command = command(&binary, "/usr/bin/python3", &["-c".into(), script]);
    let admission = Admission::new(&mut command).unwrap();
    let mut gate = admission.spawn(&mut command).unwrap();
    gate.resume().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !helper_pid.exists() {
        assert!(
            Instant::now() < deadline,
            "double-forked fixture never started"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid: u32 = std::fs::read_to_string(&helper_pid)
        .unwrap()
        .parse()
        .unwrap();
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(fd >= 0);
    let identity = unsafe { std::fs::File::from_raw_fd(fd) };
    assert!(!gate.stop().unwrap().success());
    assert!(gate.receiver.quiescent.is_some());
    let mut event = libc::pollfd {
        fd: identity.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut event, 1, 0) }, 1);
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "confirmed gate left an unreaped helper"
    );
    std::fs::write(&release, "after confirmed quiescence").unwrap();
    assert!(!late_write.exists());
    drop(gate);
    scope.wait_finished();
    std::fs::remove_dir_all(root).unwrap();
}
