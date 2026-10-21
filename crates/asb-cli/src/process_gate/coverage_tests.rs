use super::*;
use crate::test_support::IsolatedEnvironment;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn gate_command_preserves_explicit_argument_environment_and_removal_contracts() {
    if let Some(path) = std::env::var_os("ASB_GATE_FIXTURE_WRITER") {
        create_mock_gate(Path::new(&path));
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gate-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let gate = root.join("mock-gate");
    // Creating executable bytes in the shared libtest process can expose its
    // writable descriptor to another test's fork/pre-exec window. CLOEXEC does
    // not close that inherited descriptor until exec, so immediate execution
    // can fail with ETXTBSY even after this thread closes its own writer.
    // A dedicated creator cannot share its writer with sibling tests; waiting
    // for its exit closes every creator descriptor before the real gate call.
    let creator = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_gate::coverage_tests::gate_command_preserves_explicit_argument_environment_and_removal_contracts", "--nocapture"])
        .isolated_env()
        .env("ASB_GATE_FIXTURE_WRITER", &gate)
        .output().unwrap();
    assert!(
        creator.status.success(),
        "{}",
        String::from_utf8_lossy(&creator.stderr)
    );
    assert!(String::from_utf8_lossy(&creator.stdout).contains("1 passed"));
    assert!(gate.is_file());
    let mut launch = crate::launch::ProcessLaunch::new("fixture-client");
    launch.arg("argument with spaces").arg("literal;$value");
    let (mut command, _) = launch.native(Some(&gate));
    command
        .env("ASB_GATE_FIXTURE", "explicit-value")
        .env_remove("HOME");
    let output = command.output().unwrap();
    assert!(output.status.success(), "{:?}", output.status);
    assert_eq!(output.stdout, b"fixture gate accepted\n");
    let mut launch = crate::launch::ProcessLaunch::new("fixture-client");
    launch.arg("invalid\0argument");
    let (mut invalid, _) = launch.native(Some(&gate));
    assert_eq!(
        invalid.spawn().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let (mut missing, _) =
        crate::launch::ProcessLaunch::new("fixture-client").native(Some(&root.join("missing")));
    assert_eq!(missing.spawn().unwrap_err().kind(), io::ErrorKind::NotFound);
    fs::remove_dir_all(root).unwrap();
}

fn create_mock_gate(gate: &Path) {
    fs::write(
        gate,
        r#"#!/bin/sh
[ "$1" = __asb-gate ] || exit 11
[ "$2" = --asb-launch-v1 ] || exit 12
[ "$3" = 0 ] || exit 13
[ "$4" = fixture-client ] || exit 14
[ "$5" = 'argument with spaces' ] || exit 15
[ "$6" = 'literal;$value' ] || exit 16
[ "$ASB_GATE_FIXTURE" = 'explicit-value' ] || exit 17
[ -z "${HOME+x}" ] || exit 18
printf 'fixture gate accepted\n'
"#,
    )
    .unwrap();
    fs::set_permissions(gate, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn resume_waits_for_owned_child_stop_and_refuses_a_child_that_already_exited() {
    let mut child = Command::new("sh")
        .args(["-c", "kill -STOP $$; exit 7"])
        .spawn()
        .unwrap();
    resume(&mut child).unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(7));
    let mut child = Command::new("sh").args(["-c", "exit 3"]).spawn().unwrap();
    child.wait().unwrap();
    assert!(
        resume(&mut child)
            .unwrap_err()
            .to_string()
            .contains("exited before readiness")
    );
    assert_eq!(cancel_gate(&mut child).unwrap().code(), Some(3));
}

#[test]
fn cancellation_resumes_a_stopped_owned_child_so_term_can_complete() {
    let mut child = Command::new("sh")
        .args(["-c", "kill -STOP $$; exec sleep 10"])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap();
        if status
            .lines()
            .any(|line| line.starts_with("State:") && line.split_whitespace().nth(1) == Some("T"))
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let status = cancel_gate(&mut child).unwrap();
    assert!(!status.success());
    assert!(child.try_wait().unwrap().is_some());
}

#[test]
fn launcher_selection_validates_explicit_override_in_a_private_subprocess() {
    const CASE: &str = "ASB_GATE_COVERAGE_CASE";
    if let Ok(case) = std::env::var(CASE) {
        match case.as_str() {
            "default" => assert_eq!(
                executable().unwrap(),
                std::path::PathBuf::from("/proc/self/exe")
            ),
            "invalid" => assert_eq!(
                executable().unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            ),
            "valid" => assert_eq!(executable().unwrap(), std::env::current_exe().unwrap()),
            _ => panic!("invalid fixture case"),
        }
        assert_eq!(run_gate(&[]), std::process::ExitCode::from(2));
        return;
    }
    let name = format!(
        "{}::launcher_selection_validates_explicit_override_in_a_private_subprocess",
        module_path!().split_once("::").unwrap().1
    );
    for (case, path) in [
        ("default", None),
        (
            "invalid",
            Some(std::path::PathBuf::from("relative/missing")),
        ),
        ("valid", Some(std::env::current_exe().unwrap())),
    ] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", &name, "--nocapture"])
            .env(CASE, case)
            .env_remove("ASB_RUNTIME_EXECUTABLE");
        if let Some(path) = path {
            command.env("ASB_RUNTIME_EXECUTABLE", path);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}
