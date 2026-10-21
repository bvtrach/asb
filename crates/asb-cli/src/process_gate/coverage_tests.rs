use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

#[test]
fn install_preserves_explicit_argument_environment_and_removal_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gate-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let gate = root.join("mock-gate");
    fs::write(&gate,"#!/bin/sh\n[ \"$1\" = __asb-gate ] || exit 11\n[ \"$2\" = fixture-client ] || exit 12\n[ \"$3\" = 'argument with spaces' ] || exit 13\n[ \"$4\" = 'literal;$value' ] || exit 14\n[ \"$ASB_GATE_FIXTURE\" = 'explicit-value' ] || exit 15\n[ -z \"${HOME+x}\" ] || exit 16\n[ -n \"$ASB_GATE_OWNER_PID\" ] || exit 17\nprintf 'fixture gate accepted\\n'\n").unwrap();
    fs::set_permissions(&gate, fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = Command::new("unused-original-program");
    command
        .env("ASB_GATE_FIXTURE", "explicit-value")
        .env_remove("HOME");
    install(
        &mut command,
        &gate,
        "fixture-client",
        &["argument with spaces".into(), "literal;$value".into()],
    );
    let output = command.output().unwrap();
    assert!(output.status.success(), "{:?}", output.status);
    assert_eq!(output.stdout, b"fixture gate accepted\n");
    let mut invalid = Command::new("unused-original-program");
    install(
        &mut invalid,
        &gate,
        "fixture-client",
        &["invalid\0argument".into()],
    );
    assert_eq!(
        invalid.spawn().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let mut missing = Command::new("unused-original-program");
    install(&mut missing, &root.join("missing"), "fixture-client", &[]);
    assert_eq!(missing.spawn().unwrap_err().kind(), io::ErrorKind::NotFound);
    fs::remove_dir_all(root).unwrap();
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
