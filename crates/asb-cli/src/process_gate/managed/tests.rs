use super::*;
use crate::process_cleanup::test_support;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

fn root() -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gate-managed-fixtures")
        .join(crate::workloads::TaskIdentity::local("gate", None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn command() -> Command {
    let mut command = Command::new("unused-client");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn failed_keeper_creation_never_spawns_a_launcher() {
    let scope = test_support::Scope::new();
    test_support::set_creation_failure(true);
    let result = Admission::new(&mut command(), Path::new("/missing"), "/bin/true", &[]);
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
    let mut command = command();
    let admission = Admission::new(&mut command, &launcher, "/bin/true", &[]).unwrap();
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
    let mut command = command();
    let args = vec!["-c".into(), "test -z \"${ASB_GATE_RECEIPT_FD+x}\" || exit 88; printf captured; printf diagnostic >&2; exit 7".into()];
    let admission = Admission::new(&mut command, &binary, "/bin/sh", &args).unwrap();
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
    std::fs::write(&launcher, "#!/usr/bin/python3\nimport os,signal,socket,struct\ns=socket.socket(fileno=int(os.environ['ASB_GATE_RECEIPT_FD']))\ns.sendall(b'ASBGATE1'+bytes([1,0,0,0])+struct.pack('<I',os.getpid()))\nos.kill(os.getpid(),signal.SIGSTOP)\nos._exit(0)\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = command();
    let admission = Admission::new(&mut command, &launcher, "/bin/true", &[]).unwrap();
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
    let mut command = command();
    let admission = Admission::new(&mut command, &launcher, "/bin/true", &[]).unwrap();
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
