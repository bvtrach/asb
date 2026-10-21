//! Delegation commands are intercepted, never executed against host tracefs.
use super::*;
use std::fs;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
#[test]
fn isolated_delegation_consent_logs_scoped_commands_without_host_mutation() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};
    const CASE: &str = "ASB_PERFETTO_DELEGATION_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/perfetto-delegation-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let name = format!(
            "{}::isolated_delegation_consent_logs_scoped_commands_without_host_mutation",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "declined",
            "approved",
            "sudo-fails",
            "metadata-fails",
            "chown-fails",
        ] {
            let directory = root.join(case);
            fs::create_dir_all(directory.join("commands")).unwrap();
            let sudo = directory.join("commands/sudo");
            fs::write(
                &sudo,
                r#"#!/bin/sh
printf 'sudo' >> "$HOME/calls"
for value in "$@"; do printf ' [%s]' "$value" >> "$HOME/calls"; done
printf '\n' >> "$HOME/calls"
[ "$ASB_PERFETTO_DELEGATION_CASE" != sudo-fails ] || exit 1
if [ "$ASB_PERFETTO_DELEGATION_CASE" = metadata-fails ] && [ "$2" = o+r ]; then exit 1; fi
if [ "$ASB_PERFETTO_DELEGATION_CASE" = chown-fails ] && [ "$1" = chown ]; then exit 1; fi
exit 0
"#,
            )
            .unwrap();
            fs::set_permissions(sudo, fs::Permissions::from_mode(0o700)).unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .env_clear()
                .env(CASE, case)
                .env("PATH", directory.join("commands"))
                .env("HOME", &directory)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
                command.env("LLVM_PROFILE_FILE", profile);
            }
            let mut child = command.spawn().unwrap();
            child.stdin.take().unwrap().write_all(b"n\n").unwrap();
            let output = child.wait_with_output().unwrap();
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
    let names = instance_names(&root).unwrap();
    let parent = Path::new("/sys/kernel/tracing/instances");
    for name in &names {
        assert!(
            !parent.join(name).exists(),
            "never select an existing host tracing instance"
        );
    }
    let mut output = Vec::new();
    let result = configure(&root, &mut output, case != "declined").unwrap();
    assert!(result.is_empty());
    for name in &names {
        assert!(
            !parent.join(name).exists(),
            "fake sudo must never create an instance"
        );
    }
    if parent.is_dir() {
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("project-specific tracefs instances"));
        if case == "declined" {
            assert!(!root.join("calls").exists());
        } else {
            let calls = fs::read_to_string(root.join("calls")).unwrap();
            assert!(calls.lines().all(|line| line.starts_with("sudo [")));
            assert!(!calls.contains("sysctl"));
            assert!(!calls.contains("tracing_on"));
            assert!(!calls.contains("sudoers"));
            assert!(
                output.contains("Could not delegate")
                    || output.contains("Did not adopt")
                    || output.contains("could not be enabled")
            );
        }
    }
}
#[cfg(unix)]
#[test]
fn admission_and_reset_reject_symlinked_coordination_without_touching_targets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/perfetto-lock-path-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(root.join(".asb/perfetto")).unwrap();
    let root = root.canonicalize().unwrap();
    let target = root.join("unrelated");
    fs::write(&target, "unchanged").unwrap();
    let admission = root.join(".asb-perfetto-admission.lock");
    std::os::unix::fs::symlink(&target, &admission).unwrap();
    assert!(
        lock_admission(&root)
            .unwrap_err()
            .to_string()
            .contains("cannot open Perfetto admission lock")
    );
    fs::remove_file(&admission).unwrap();
    let instance = instance_names(&root).unwrap().remove(0);
    let lease = root.join(".asb/perfetto").join(format!("{instance}.lease"));
    std::os::unix::fs::symlink(&target, &lease).unwrap();
    assert!(
        lock_for_reset(&root)
            .unwrap_err()
            .to_string()
            .contains("cannot inspect Perfetto lease before reset")
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "unchanged");
    assert!(
        instance_names(&root.join("missing"))
            .unwrap_err()
            .contains("cannot identify benchmark project")
    );
    fs::remove_dir_all(root).unwrap();
}
