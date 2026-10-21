//! Capture/preflight protocol fixtures only; fake binaries never attach probes.
use super::*;
use std::path::PathBuf;

#[cfg(unix)]
#[test]
fn isolated_bpftrace_preflight_and_attachment_failures() {
    use std::collections::BTreeSet;
    use std::os::unix::fs::PermissionsExt;
    const CASE: &str = "ASB_BPFTRACE_CAPTURE_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/bpftrace-capture-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let name = format!(
            "{}::isolated_bpftrace_preflight_and_attachment_failures",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "disabled",
            "no-program",
            "builtin",
            "builtin-sudo",
            "builtin-no-sudo",
            "custom",
            "custom-invalid",
            "custom-missing",
            "missing-executable",
            "unexecutable",
            "failed-ready",
            "custom-missing-start",
            "builtin-unavailable",
            "builtin-hard-missing",
        ] {
            let directory = root.join(case);
            fs::create_dir_all(directory.join("commands")).unwrap();
            let program = directory.join("program.bt");
            if case != "custom-missing" {
                fs::write(&program,if case=="custom-invalid"{"tracepoint:syscalls:sys_enter_read { @host = count(); }\n"}else{"tracepoint:syscalls:sys_enter_read /@asb_tree[pid]/ { @asb_reads = count(); }\n"}).unwrap();
            }
            if !matches!(
                case,
                "missing-executable"
                    | "custom-missing-start"
                    | "builtin-unavailable"
                    | "builtin-hard-missing"
            ) {
                let path = directory.join("commands/bpftrace");
                let body = match case {
                    "builtin-sudo" | "builtin-no-sudo" => "#!/bin/sh\nexit 2\n",
                    "failed-ready" => {
                        "#!/bin/sh\nif [ \"$1\" = -e ]; then exit 0; fi\nprintf 'fixture attachment failed\\n' >&2\nexit 2\n"
                    }
                    _ => {
                        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$ASB_BPFTRACE_CAPTURE_ROOT/arguments\"\nprintf 'ASB_BPFTRACE_READY\\n'\n"
                    }
                };
                fs::write(&path, body).unwrap();
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(if case == "unexecutable" { 0o600 } else { 0o700 }),
                )
                .unwrap();
            }
            let sudo = directory.join("commands/sudo");
            fs::write(
                &sudo,
                if case == "builtin-sudo" {
                    "#!/bin/sh\nexit 0\n"
                } else {
                    "#!/bin/sh\nexit 1\n"
                },
            )
            .unwrap();
            fs::set_permissions(sudo, fs::Permissions::from_mode(0o700)).unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .current_dir(&directory)
                .env(CASE, case)
                .env("ASB_BPFTRACE_CAPTURE_ROOT", &directory)
                .env("PATH", directory.join("commands"))
                .env_remove("ASB_BPFTRACE_PROFILES")
                .env_remove("ASB_BPFTRACE_PROGRAMS")
                .env(
                    "ASB_MEASURE_BPFTRACE",
                    if case.starts_with("custom")
                        || case == "no-program"
                        || case == "builtin-hard-missing"
                    {
                        "1"
                    } else {
                        "0"
                    },
                );
            if case.starts_with("custom") {
                command.env(
                    "ASB_BPFTRACE_PROGRAMS",
                    format!("{}?limit=5&unit=ns", program.display()),
                );
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{case}: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
        }
        fs::remove_dir_all(root).unwrap();
        return;
    };
    let root = PathBuf::from(std::env::var_os("ASB_BPFTRACE_CAPTURE_ROOT").unwrap());
    let selected: Vec<String> =
        if case.starts_with("custom") || case == "disabled" || case == "no-program" {
            vec![]
        } else {
            vec!["bpftrace:syscall-rate".into()]
        };
    let mut sudo = BTreeSet::new();
    let mut context = PreflightContext {
        project_root: None,
        measurement_dir: &root,
        selected: &selected,
        tool_dir: None,
        sudo_collectors: &mut sudo,
    };
    if !matches!(
        case.as_str(),
        "custom-missing-start" | "builtin-unavailable" | "builtin-hard-missing"
    ) {
        let result = ADAPTER.preflight(&mut context);
        match case.as_str() {
            "no-program" => {
                assert!(result.unwrap_err().contains("no program or profile"));
                return;
            }
            "custom-invalid" => {
                assert!(result.unwrap_err().contains("unscoped probe"));
                return;
            }
            "custom-missing" => {
                assert!(result.unwrap_err().contains("cannot read bpftrace program"));
                return;
            }
            "missing-executable" => {
                assert!(result.unwrap_err().contains("asb tool install bpftrace"));
                return;
            }
            "unexecutable" => {
                assert!(result.unwrap_err().contains("cannot execute bpftrace"));
                return;
            }
            "builtin-no-sudo" => {
                assert!(result.unwrap_err().contains("sudo"));
                return;
            }
            "builtin-sudo" => {
                result.unwrap();
                assert!(sudo.contains("bpftrace"));
                return;
            }
            _ => {
                result.unwrap();
            }
        }
    }
    let mut session = measurements::Session::start_in_execution_namespace(
        &root.join("measurements"),
        &["agent-memory".into()],
    )
    .unwrap();
    ADAPTER.configure(&mut session, &selected).unwrap();
    let result = ADAPTER.attach_pid(&mut session, std::process::id());
    match case.as_str() {
        "failed-ready" => {
            let error = result.unwrap_err();
            assert!(error.contains("did not become ready"));
            assert!(error.contains("fixture attachment failed"));
        }
        "custom-missing-start" => {
            assert!(
                result
                    .unwrap_err()
                    .contains("cannot start bpftrace program")
            );
        }
        "builtin-hard-missing" => {
            assert!(
                result
                    .unwrap_err()
                    .contains("cannot start bpftrace profile")
            );
        }
        "builtin-unavailable" => {
            result.unwrap();
            let output =
                fs::read_to_string(session.dir.join("bpftrace-syscall-rate.json")).unwrap();
            assert!(output.contains("unavailable"));
        }
        "builtin" => {
            result.unwrap();
            assert!(session.collectors[0].child.wait().unwrap().success());
            assert!(session.dir.join("bpftrace-syscall-rate.stdout").exists());
            assert!(session.dir.join("bpftrace-syscall-rate.stderr").exists());
        }
        "custom" => {
            result.unwrap();
            assert!(session.collectors[0].child.wait().unwrap().success());
            assert!(session.dir.join("bpftrace-0.stdout").exists());
            assert!(session.dir.join("bpftrace-0.stderr").exists());
            let arguments = fs::read_to_string(root.join("arguments")).unwrap();
            assert!(arguments.contains("-D\nlimit=5\n-D\nunit=ns"));
            assert!(arguments.contains(&format!("-p\n{}", std::process::id())));
            assert!(arguments.contains("@asb_tree["));
        }
        "disabled" => {
            result.unwrap();
            assert!(!session.dir.join("bpftrace-0.stdout").exists());
        }
        _ => panic!("unknown fixture case"),
    }
}
