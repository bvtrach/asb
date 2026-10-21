//! Negative source-install tests retain real SHA-256 verification. No fixture
//! impersonates a valid pinned binary, and no downloaded code is executed.
use super::*;
use std::process::Command;
#[cfg(unix)]
#[test]
fn isolated_perfetto_install_rejects_unverified_source_and_missing_dependencies() {
    use std::os::unix::fs::PermissionsExt;
    const CASE: &str = "ASB_PERFETTO_INSTALL_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/perfetto-install-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let sha = crate::tools::host_executable(Path::new("sha256sum")).unwrap();
        let name = format!(
            "{}::isolated_perfetto_install_rejects_unverified_source_and_missing_dependencies",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "bad-checksum",
            "download-fails",
            "bin-error",
            "missing-sha",
            "absent-curl",
        ] {
            let directory = root.join(case);
            fs::create_dir_all(directory.join("commands")).unwrap();
            if case != "missing-sha" {
                std::os::unix::fs::symlink(&sha, directory.join("commands/sha256sum")).unwrap();
            }
            if case != "absent-curl" {
                let curl = directory.join("commands/curl");
                fs::write(
                    &curl,
                    r#"#!/bin/sh
[ "$ASB_PERFETTO_INSTALL_CASE" != download-fails ] || exit 2
while [ "$#" -gt 0 ]; do
 if [ "$1" = --output ]; then shift; printf 'not a pinned binary' > "$1"; fi
 shift
done
"#,
                )
                .unwrap();
                fs::set_permissions(curl, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .current_dir(&directory)
                .env_clear()
                .env(CASE, case)
                .env("HOME", &directory)
                .env("PATH", directory.join("commands"));
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
    let root = PathBuf::from(std::env::var_os("HOME").unwrap()).join("tool");
    fs::create_dir_all(&root).unwrap();
    if case == "bin-error" {
        fs::write(root.join("bin"), "blocked").unwrap();
    }
    let mut progress = Vec::new();
    let error = install_perfetto(&root, &mut |percent, _| progress.push(percent))
        .unwrap_err()
        .to_string();
    let expected = match case.as_str() {
        "bad-checksum" => "checksum mismatch",
        "download-fails" => "could not download Perfetto Tracebox",
        "bin-error" => "cannot create",
        "missing-sha" => "cannot inspect Perfetto Tracebox checksum",
        "absent-curl" => "curl",
        _ => panic!("unknown fixture case"),
    };
    assert!(error.contains(expected), "{error}");
    assert!(!progress.contains(&90));
    assert!(!root.join("bin/trace_processor_shell").exists());
    if case == "bad-checksum" {
        assert!(
            !root.join("bin/tracebox").exists(),
            "unverified bytes must not be published"
        );
    }
}
