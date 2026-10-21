//! Fake source/build commands never download or run a host collector.
use super::*;
#[cfg(unix)]
#[test]
fn isolated_bpftrace_build_and_install_contracts() {
    use std::os::unix::fs::PermissionsExt;
    const CASE: &str = "ASB_BPFTRACE_INSTALL_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/bpftrace-installer-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let name = format!(
            "{}::isolated_bpftrace_build_and_install_contracts",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "build",
            "cached",
            "curl-fails",
            "tar-fails",
            "configure-fails",
            "compile-fails",
            "no-output",
            "missing-curl",
            "missing-tar",
            "missing-cmake",
            "source-blocked",
            "install",
            "version-fails",
            "kernel-fails",
            "arch-fails",
            "bin-blocked",
            "copy-blocked",
            "metadata-blocked",
        ] {
            let path = root.join(case);
            fs::create_dir_all(path.join("commands")).unwrap();
            let result = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name, "--nocapture"])
                .current_dir(&path)
                .env(CASE, case)
                .env("ASB_INSTALL_FIXTURE_ROOT", &path)
                .env("PATH", path.join("commands"))
                .output()
                .unwrap();
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
    let root = PathBuf::from(std::env::var_os("ASB_INSTALL_FIXTURE_ROOT").unwrap());
    let commands = root.join("commands");
    let tool = root.join("tool");
    let executable = |name: &str, body: &str| {
        let path = commands.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    };
    executable(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$ASB_INSTALL_FIXTURE_ROOT/calls"
[ "$ASB_BPFTRACE_INSTALL_CASE" != curl-fails ] || exit 2
while [ "$#" -gt 0 ]; do
 if [ "$1" = --output ]; then shift; printf 'archive fixture' > "$1"; fi
 shift
done"#,
    );
    executable(
        "tar",
        r#"printf 'tar %s\n' "$*" >> "$ASB_INSTALL_FIXTURE_ROOT/calls"
[ "$ASB_BPFTRACE_INSTALL_CASE" != tar-fails ]"#,
    );
    executable(
        "cmake",
        r#"printf 'cmake %s\n' "$*" >> "$ASB_INSTALL_FIXTURE_ROOT/calls"
if [ "$1" = --build ]; then
 [ "$ASB_BPFTRACE_INSTALL_CASE" != compile-fails ] || exit 2
 [ "$ASB_BPFTRACE_INSTALL_CASE" != no-output ] || exit 0
 /bin/mkdir -p "$2/src"
 printf 'fixture binary' > "$2/src/bpftrace"
else
 [ "$ASB_BPFTRACE_INSTALL_CASE" != configure-fails ] || exit 2
fi"#,
    );
    let mut progress = Vec::new();
    if matches!(
        case.as_str(),
        "install"
            | "version-fails"
            | "kernel-fails"
            | "arch-fails"
            | "bin-blocked"
            | "copy-blocked"
            | "metadata-blocked"
    ) {
        executable(
            "bpftrace",
            r#"[ "$1" = --version ] || exit 3
[ "$ASB_BPFTRACE_INSTALL_CASE" != version-fails ] || exit 2
printf 'fixture bpftrace "quoted"\n'"#,
        );
        executable(
            "uname",
            r#"case "$1" in
-r) [ "$ASB_BPFTRACE_INSTALL_CASE" != kernel-fails ] || exit 2; printf 'fixture-kernel\n';;
-m) [ "$ASB_BPFTRACE_INSTALL_CASE" != arch-fails ] || exit 2; printf 'fixture-arch\n';;
*) exit 3;;
esac"#,
        );
        fs::create_dir_all(&tool).unwrap();
        match case.as_str() {
            "bin-blocked" => fs::write(tool.join("bin"), "blocked").unwrap(),
            "copy-blocked" => fs::create_dir_all(tool.join("bin/bpftrace")).unwrap(),
            "metadata-blocked" => {
                fs::create_dir_all(tool.join("bpftrace-compatibility.json")).unwrap()
            }
            _ => {}
        }
        let result = install_bpftrace(&tool, &mut |percent, _| progress.push(percent));
        if case == "install" {
            let binary = result.unwrap();
            assert_eq!(binary, tool.join("bin/bpftrace"));
            assert_eq!(
                fs::read(&binary).unwrap(),
                fs::read(commands.join("bpftrace")).unwrap()
            );
            assert_ne!(
                fs::metadata(binary).unwrap().permissions().mode() & 0o111,
                0
            );
            let json: serde_json::Value = serde_json::from_slice(
                &fs::read(tool.join("bpftrace-compatibility.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(json["schema_version"], 1);
            assert_eq!(json["tool"], "bpftrace");
            assert_eq!(json["version"], "fixture bpftrace \"quoted\"");
            assert_eq!(json["kernel_release"], "fixture-kernel");
            assert_eq!(json["kernel_architecture"], "fixture-arch");
            assert_eq!(progress, [10, 95, 100]);
        } else {
            let error = result.unwrap_err().to_string();
            let expected = match case.as_str() {
                "version-fails" => "cannot inspect bpftrace version",
                "kernel-fails" => "cannot inspect kernel release",
                "arch-fails" => "cannot inspect kernel architecture",
                "bin-blocked" => "cannot create",
                "copy-blocked" => "cannot copy host bpftrace",
                "metadata-blocked" => "cannot write bpftrace compatibility metadata",
                _ => unreachable!(),
            };
            assert!(error.contains(expected), "{error}");
            assert!(!progress.contains(&100));
        }
        return;
    }
    if case == "cached" {
        fs::create_dir_all(tool.join("source")).unwrap();
    }
    if case == "source-blocked" {
        fs::create_dir_all(&tool).unwrap();
        fs::write(tool.join("source"), "blocked").unwrap();
    }
    for (name, command) in [
        ("missing-curl", "curl"),
        ("missing-tar", "tar"),
        ("missing-cmake", "cmake"),
    ] {
        if case == name {
            fs::remove_file(commands.join(command)).unwrap();
        }
    }
    let result = build_bpftrace(&tool, &mut |percent, _| progress.push(percent));
    if case == "build" || case == "cached" {
        assert_eq!(result.unwrap(), tool.join("build/src/bpftrace"));
        assert_eq!(
            progress,
            if case == "build" {
                vec![25, 40, 60, 90]
            } else {
                vec![60, 90]
            }
        );
        let log = fs::read_to_string(root.join("calls")).unwrap();
        assert!(log.contains("-DBUILD_TESTING=OFF -DBPFTRACE_BUILD_TESTING=OFF"));
        assert!(log.contains("--target bpftrace --parallel"));
        if case == "build" {
            assert!(log.contains(BPFTRACE_SOURCE));
            assert!(log.contains("--strip-components=1"));
        } else {
            assert!(!log.lines().any(|line| line.starts_with("curl ")));
            assert!(!log.lines().any(|line| line.starts_with("tar ")));
        }
    } else {
        let error = result.unwrap_err().to_string();
        let expected = match case.as_str() {
            "curl-fails" => "could not download pinned bpftrace source",
            "tar-fails" => "could not extract",
            "configure-fails" => "cmake could not configure",
            "compile-fails" => "source compilation failed",
            "no-output" => "without a discoverable executable",
            "missing-curl" | "missing-tar" | "missing-cmake" => "source build needs",
            "source-blocked" => "cannot create bpftrace source directory",
            _ => unreachable!(),
        };
        assert!(error.contains(expected), "{error}");
    }
}
