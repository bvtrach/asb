//! Argument/ownership/namespace/path rejection only. No test can reach mount.
use super::*;
#[cfg(target_os = "linux")]
#[test]
fn isolated_bootstrap_refuses_foreign_owners_leases_namespaces_and_trace_paths() {
    use std::fs::{self, File};
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    const CASE: &str = "ASB_BOOTSTRAP_REJECT_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/perfetto-bootstrap-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let lease = File::create(root.join("lease")).unwrap();
        let name = format!(
            "{}::isolated_bootstrap_refuses_foreign_owners_leases_namespaces_and_trace_paths",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "owner",
            "fd-format",
            "owner-race",
            "closed-fd",
            "same-user",
            "bad-user",
            "same-mount",
            "bad-mount",
            "missing-parent",
            "missing-instance",
            "missing-root",
            "foreign-root",
            "foreign-instance",
            "wrong-filename",
            "unsafe-name",
            "missing-owned-instance",
        ] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .env(CASE, case)
                .env("ASB_BOOTSTRAP_PARENT", std::process::id().to_string())
                .env("ASB_BOOTSTRAP_LEASE", lease.as_raw_fd().to_string());
            retain_owned_fixture_lease(&mut command, lease.as_raw_fd());
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
    let user = fs::read_link("/proc/self/ns/user").unwrap();
    let mount = fs::read_link("/proc/self/ns/mnt").unwrap();
    let mut args: Vec<OsString> = vec![
        "never-executed".into(),
        "/tmp/instances/asb-fixture/trace".into(),
        std::env::var_os("ASB_BOOTSTRAP_PARENT").unwrap(),
        std::env::var_os("ASB_BOOTSTRAP_LEASE").unwrap(),
        "user:[0]".into(),
        "mnt:[0]".into(),
    ];
    let expected = match case.as_str() {
        "owner" => {
            args[2] = "1".into();
            "invalid owner PID"
        }
        "fd-format" => {
            args[3] = "2".into();
            "invalid instance lease FD"
        }
        "owner-race" => {
            args[2] = std::process::id().to_string().into();
            "owner exited"
        }
        "closed-fd" => {
            args[3] = i32::MAX.to_string().into();
            "lease was not retained"
        }
        "same-user" => {
            args[4] = user.into_os_string();
            "outside a private user"
        }
        "bad-user" => {
            args[4] = "invalid".into();
            "outside a private user"
        }
        "same-mount" => {
            args[5] = mount.into_os_string();
            "outside a private mnt"
        }
        "bad-mount" => {
            args[5] = "invalid".into();
            "outside a private mnt"
        }
        "missing-parent" => {
            args[1] = "".into();
            "invalid instance trace path"
        }
        "missing-instance" => {
            args[1] = "trace".into();
            "invalid instance name"
        }
        "missing-root" => {
            args[1] = "asb-fixture/trace".into();
            "invalid tracefs root"
        }
        "foreign-root" => {
            args[1] = "/tmp/instances/asb-fixture/trace".into();
            "refusing to mount"
        }
        "foreign-instance" => {
            args[1] = "/sys/kernel/tracing/instances/foreign/trace".into();
            "refusing to mount"
        }
        "wrong-filename" => {
            args[1] = "/sys/kernel/tracing/instances/asb-fixture/other".into();
            "refusing to mount"
        }
        "unsafe-name" => {
            args[1] = "/sys/kernel/tracing/instances/asb-unsafe_name/trace".into();
            "refusing to mount"
        }
        "missing-owned-instance" => {
            let instance = format!(
                "/sys/kernel/tracing/instances/asb-fixture-{}-missing/trace",
                std::process::id()
            );
            assert!(
                !PathBuf::from(&instance).exists(),
                "must reject before any mount"
            );
            args[1] = instance.into();
            "cannot inspect instance mount policy"
        }
        _ => panic!("invalid fixture case"),
    };
    assert!(run(&args).unwrap_err().contains(expected), "{case}");
    assert_eq!(entry(&[]), ExitCode::from(2));
}
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn retain_owned_fixture_lease(command: &mut std::process::Command, fd: i32) {
    use std::os::unix::process::CommandExt;
    // SAFETY: this private child inherits one still-owned fixture descriptor.
    // The hook performs only async-signal-safe fcntl and never alters the parent.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}
