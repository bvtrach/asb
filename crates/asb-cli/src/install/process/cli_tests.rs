//! Real public CLI and owning tool adapters; every external command is inert.
use super::*;
use crate::process_gate::{Admission, ManagedGate};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

const OLD: &[u8] = b"#!/bin/sh\nprintf 'previous tool\\n'\n";
const EXACT_CASE: &str = "ASB_INSTALLER_EXACT_CLI_CASE";

fn isolated_case(test: &str, case: &str) {
    // Parent libtest creates directories only, never executable inodes whose
    // writable descriptions could be inherited by another parallel test fork.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/installer-cli-entry")
        .join(crate::tools::runtime::unique_id());
    fs::create_dir_all(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let mut config = ProjectConfig::default();
    config.installation.operation_timeout_seconds = 12;
    let context = OperationContext::new(&root, &config).unwrap();
    let owner = ProducerDirectory::open(&root).unwrap();
    let mut scope = owner.scope(&context);
    let mut step = scope
        .prepare(
            std::env::current_exe().unwrap(),
            Purpose::Probe,
            "exact installer CLI fixture child",
        )
        .unwrap();
    step.command_mut()
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .current_dir(&root)
        .env_clear()
        .env("HOME", &root)
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env(EXACT_CASE, case)
        .stdin(Stdio::null());
    step.capture(Capture::inspection());
    let result = scope.run(step).unwrap();
    // The exact child (including its nested CLI and sentinel) must actually
    // settle before interpreting any status or captured assertion text.
    owner.consume(scope.finish().unwrap()).unwrap();
    fs::write(
        root.join("matching-exact-child-cleanup-confirmed"),
        "exact child guardian quiesced and matching actual directory proof consumed",
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{case}: exact fixture child failed\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

struct Fixture {
    root: PathBuf,
    gate: Option<ManagedGate>,
    sentinel: Option<ManagedGate>,
}
impl Fixture {
    fn new(case: &str, tool: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/installer-cli-faults")
            .join(crate::tools::runtime::unique_id());
        fs::create_dir_all(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        fs::create_dir(root.join(".asb")).unwrap();
        fs::write(root.join(".asb/project.toml"), "tools_dir='tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n[installation]\noperation_timeout_seconds=5\nprobe_timeout_seconds=1\nversion_timeout_seconds=1\ndownload_timeout_seconds=1\n").unwrap();
        let destination = root.join("tools").join(tool).join("bin").join(tool);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        executable(&destination, OLD);
        fs::write(
            root.join("tools")
                .join(tool)
                .join("bpftrace-compatibility.json"),
            "previous metadata",
        )
        .unwrap();
        executable(
            &root.join("bin/uname"),
            br##"#!/bin/sh
printf called >> "$ASB_FIXTURE_ROOT/uname.called"
printf '6.12-fixture\n'
"##,
        );
        executable(&root.join("bin/apt-cache"), br##"#!/bin/sh
printf called >> "$ASB_FIXTURE_ROOT/apt-cache.called"
if test "$ASB_FIXTURE_CASE" = catalog-overflow; then exec "$ASB_FIXTURE_PYTHON" -c 'import sys; sys.stdout.buffer.write(b"x"*(2*1024*1024)); sys.stdout.buffer.flush()'; fi
printf 'Depends: linux-tools-fixture\n'
"##);
        executable(
            &root.join("bin/apt-get"),
            br##"#!/bin/sh
printf called >> "$ASB_FIXTURE_ROOT/apt-get.called"
printf partial-package > fixture.deb
if test "$ASB_FIXTURE_CASE" = download-timeout; then exec /bin/sleep 30; fi
"##,
        );
        executable(
            &root.join("bin/dpkg-deb"),
            br##"#!/bin/sh
printf called >> "$ASB_FIXTURE_ROOT/dpkg-deb.called"
/bin/mkdir -p "$3/usr/bin"
/bin/cp "$ASB_FIXTURE_ROOT/stub-perf" "$3/usr/bin/perf"
"##,
        );
        executable(&root.join("stub-perf"), br##"#!/bin/sh
printf called >> "$ASB_FIXTURE_ROOT/perf-version.called"
if test "$ASB_FIXTURE_CASE" = runnable-timeout; then exec /bin/sleep 30; fi
if test "$ASB_FIXTURE_CASE" = runnable-overflow; then exec "$ASB_FIXTURE_PYTHON" -c 'import sys; sys.stdout.buffer.write(b"x"*(2*1024*1024)); sys.stdout.buffer.flush()'; fi
printf 'perf fixture\n'
"##);
        executable(&root.join("bin/bpftrace"), br##"#!/bin/sh
printf called >> "$ASB_FIXTURE_ROOT/bpf-version.called"
if test "$ASB_FIXTURE_CASE" = bpf-timeout; then exec /bin/sleep 30; fi
exec "$ASB_FIXTURE_PYTHON" -c 'import sys; sys.stdout.buffer.write(b"x"*(2*1024*1024)); sys.stdout.buffer.flush()'
"##);
        for name in ["curl", "tar", "make", "cmake", "pkg-config"] {
            executable(
                &root.join("bin").join(name),
                br##"#!/bin/sh
printf forbidden >> "$ASB_FIXTURE_ROOT/forbidden-fallback.called"
exit 99
"##,
            );
        }
        let mut fixture = Self {
            root,
            gate: None,
            sentinel: None,
        };
        let binary = runtime().unwrap();
        let (mut sentinel, route) =
            crate::launch::ProcessLaunch::new("/bin/sleep").native(Some(&binary));
        sentinel
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut sentinel = Admission::new(route.validate_gate(&mut sentinel).unwrap())
            .unwrap()
            .spawn()
            .unwrap();
        sentinel.resume().unwrap();
        fixture.sentinel = Some(sentinel);
        let (mut command, route) = crate::launch::ProcessLaunch::new(&binary).native(Some(&binary));
        command
            .args(["tool", "install", tool])
            .current_dir(&fixture.root)
            .env_clear()
            .env("HOME", &fixture.root)
            .env("PATH", fixture.root.join("bin"))
            .env("ASB_FIXTURE_ROOT", &fixture.root)
            .env("ASB_FIXTURE_CASE", case)
            .env(
                "ASB_FIXTURE_PYTHON",
                crate::tools::host_executable(Path::new("python3")).unwrap(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                fs::File::create(fixture.root.join("stdout")).unwrap(),
            ))
            .stderr(Stdio::from(
                fs::File::create(fixture.root.join("stderr")).unwrap(),
            ));
        let mut gate = Admission::new(route.validate_gate(&mut command).unwrap())
            .unwrap()
            .spawn()
            .unwrap();
        gate.resume().unwrap();
        fixture.gate = Some(gate);
        fixture
    }
    fn finish(&mut self) -> (std::process::ExitStatus, String) {
        let end = Instant::now() + Duration::from_secs(8);
        let mut exceeded = false;
        let observed = loop {
            if let Some(status) = self.gate.as_mut().unwrap().try_wait().unwrap() {
                break Some(status);
            }
            if ["stdout", "stderr"]
                .iter()
                .any(|name| fs::metadata(self.root.join(name)).unwrap().len() > 65536)
            {
                exceeded = true;
                break None;
            }
            if Instant::now() >= end {
                break None;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let main = self.gate.as_mut().unwrap().stop();
        let sentinel_alive = self
            .sentinel
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none();
        let sentinel = self.sentinel.as_mut().unwrap().stop();
        assert!(
            main.is_ok() && sentinel.is_ok(),
            "fixture cleanup unconfirmed; retain {}",
            self.root.display()
        );
        assert!(sentinel_alive, "installer changed an unrelated process");
        fs::write(
            self.root.join("matching-cli-cleanup-confirmed"),
            "CLI guardian quiesced; unrelated sentinel survived and was separately stopped",
        )
        .unwrap();
        exceeded |= ["stdout", "stderr"]
            .iter()
            .any(|name| fs::metadata(self.root.join(name)).unwrap().len() > 65536);
        assert!(!exceeded, "public CLI exceeded its output budget");
        let status =
            observed.expect("public CLI exceeded its owned fixture deadline after cleanup");
        let mut output = Vec::new();
        for name in ["stdout", "stderr"] {
            fs::File::open(self.root.join(name))
                .unwrap()
                .take(65537)
                .read_to_end(&mut output)
                .unwrap();
        }
        (status, String::from_utf8(output).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for gate in [&mut self.gate, &mut self.sentinel].into_iter().flatten() {
            let _ = gate.stop();
        }
    }
}
fn executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn public_perf_install_abort_never_falls_back_retries_or_publishes() {
    let cases = [
        "catalog-overflow",
        "download-timeout",
        "runnable-overflow",
        "runnable-timeout",
    ];
    let selected = std::env::var(EXACT_CASE).ok();
    assert!(selected.as_deref().is_none_or(|case| cases.contains(&case)));
    for case in cases {
        if let Some(selected) = &selected {
            if selected != case {
                continue;
            }
        } else {
            isolated_case(
                "install::process::cli_tests::public_perf_install_abort_never_falls_back_retries_or_publishes",
                case,
            );
            continue;
        }
        let mut fixture = Fixture::new(case, "perf");
        let (status, output) = fixture.finish();
        assert!(!status.success(), "{case}: {output}");
        assert!(
            output.contains(if case.ends_with("overflow") {
                "CaptureOverflow"
            } else {
                "Deadline"
            }),
            "{case}: {output}"
        );
        assert!(
            !output.contains("Installed collector perf"),
            "{case}: {output}"
        );
        assert!(
            !fixture.root.join("forbidden-fallback.called").exists(),
            "{case}: uncertainty became source fallback"
        );
        assert_eq!(
            fs::read(fixture.root.join("tools/perf/bin/perf")).unwrap(),
            OLD
        );
        if case == "catalog-overflow" {
            assert!(!fixture.root.join("apt-get.called").exists());
        } else {
            assert_eq!(
                fs::read(fixture.root.join("apt-get.called")).unwrap(),
                b"called",
                "{case}: later package retry ran"
            );
        }
        assert!(
            fs::read_dir(fixture.root.join("tools/perf"))
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".install-")),
            "{case}: Abort erased actual package evidence"
        );
    }
}

#[test]
fn public_bpftrace_install_abort_preserves_old_binary_metadata_and_error_class() {
    let cases = ["bpf-overflow", "bpf-timeout"];
    let selected = std::env::var(EXACT_CASE).ok();
    assert!(selected.as_deref().is_none_or(|case| cases.contains(&case)));
    for case in cases {
        if let Some(selected) = &selected {
            if selected != case {
                continue;
            }
        } else {
            isolated_case(
                "install::process::cli_tests::public_bpftrace_install_abort_preserves_old_binary_metadata_and_error_class",
                case,
            );
            continue;
        }
        let mut fixture = Fixture::new(case, "bpftrace");
        let (status, output) = fixture.finish();
        assert!(!status.success(), "{case}: {output}");
        assert!(
            output.contains(if case == "bpf-overflow" {
                "CaptureOverflow"
            } else {
                "Deadline"
            }),
            "{case}: {output}"
        );
        assert!(!output.contains("Install a matching host tool"));
        assert!(!fixture.root.join("forbidden-fallback.called").exists());
        assert!(
            !fixture.root.join("uname.called").exists(),
            "{case}: execution continued after failed required version capture"
        );
        assert_eq!(
            fs::read(fixture.root.join("tools/bpftrace/bin/bpftrace")).unwrap(),
            OLD
        );
        assert_eq!(
            fs::read(
                fixture
                    .root
                    .join("tools/bpftrace/bpftrace-compatibility.json")
            )
            .unwrap(),
            b"previous metadata"
        );
    }
}
