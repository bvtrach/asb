//! Real owned regression fixtures: no providers, Docker, credentials or sudo.
use super::*;
use crate::test_support::IsolatedEnvironment;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;

const CASE: &str = "ASB_CHECKPOINT_TRANSPORT_CASE";
const DIRECTORY: &str = "ASB_CHECKPOINT_TRANSPORT_ROOT";

#[test]
fn unread_request_obeys_checkpoint_operation_deadline() {
    bounded_case(
        "unread-request",
        "unread_request_obeys_checkpoint_operation_deadline",
    );
}

#[test]
fn duplex_pressure_obeys_checkpoint_operation_deadline() {
    bounded_case(
        "duplex-pressure",
        "duplex_pressure_obeys_checkpoint_operation_deadline",
    );
}

#[test]
fn inherited_stdout_obeys_checkpoint_operation_deadline() {
    bounded_case(
        "inherited-stdout",
        "inherited_stdout_obeys_checkpoint_operation_deadline",
    );
}

#[test]
fn borrowed_request_matches_legacy_bytes_and_literal_digest() {
    let context = json!({"z":"fixture", "a":[1,true,null,"é"]});
    let roots = [
        Root::path("zeta", PathBuf::from("/fixture/é\"")),
        Root {
            name: "alpha".into(),
            path: "/fixture/alpha".into(),
            bind_location: false,
            direct_file: true,
        },
    ];
    let key = [0x42; 32];
    let key_hex = "42".repeat(32);
    for environment in [false, true] {
        let legacy = serde_json::to_vec(&json!({"key":key_hex,"roots":roots,"context":context,
            "environment":environment,"exclude":[".git","cache"]}))
        .unwrap();
        assert_eq!(
            encode_request(&key, &roots, &context, environment, &[".git", "cache"]).unwrap(),
            legacy
        );
    }
    let snapshotter = Snapshotter {
        key,
        python: "python3".into(),
    };
    let actual = snapshotter
        .fingerprint_with_timeout(&[], &context, false, &[], Duration::from_secs(3))
        .unwrap();
    // Independent literal HMAC over the unchanged prefix, sorted Python JSON
    // context length and context bytes; no filesystem-dependent expectation.
    assert_eq!(
        actual.digest,
        "b5429558ed5fd5c5739d7009dd9982e5bb2cd17be83f1fe500a889112c0a946c"
    );
    assert_eq!((actual.bytes, actual.nodes), (0, 0));
}

fn helper(source: &str) -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/checkpoint49-fixtures")
        .join(crate::workloads::TaskIdentity::local("focused", None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    let python = root.join("fake-python");
    std::fs::write(&python, format!("#!/usr/bin/python3\n{source}\n")).unwrap();
    std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o700)).unwrap();
    (root, python)
}

#[test]
fn exact_request_limit_is_delivered_and_excess_never_admits_helper() {
    let (root, python) = helper(
        "import pathlib,sys\nb=sys.stdin.buffer.read()\nassert len(b)==16*1024*1024\npathlib.Path(__file__+'.received').write_text('complete')\nprint('{\"schema_version\":1,\"digest\":\"'+'a'*64+'\",\"bytes\":0,\"nodes\":0}')",
    );
    let snapshotter = Snapshotter {
        key: [0x42; 32],
        python: python.clone(),
    };
    let empty = encode_request(&snapshotter.key, &[], &json!(""), false, &[])
        .unwrap()
        .len();
    for excess in [0, 1] {
        let context = json!("x".repeat(MAX_REQUEST - empty + excess));
        let result =
            snapshotter.fingerprint_with_timeout(&[], &context, false, &[], Duration::from_secs(3));
        assert_eq!(result.is_ok(), excess == 0, "{result:?}");
        let marker = root.join("fake-python.received");
        if excess == 0 {
            assert!(marker.exists());
            std::fs::remove_file(marker).unwrap();
        } else {
            assert!(!marker.exists());
            assert!(result.unwrap_err().to_string().contains("16 MiB"));
        }
    }
}

#[test]
fn response_limits_status_schema_and_ignored_stderr_preserve_contracts() {
    for (body, accepted) in [
        (
            "import sys\nsys.stdin.buffer.read()\nsys.stderr.write('PRIVATE_HELPER_DIAGNOSTIC')\ns='{\"schema_version\":1,\"digest\":\"'+'a'*64+'\",\"bytes\":0,\"nodes\":0}'\nsys.stdout.write(s+' '*(2048-len(s)))",
            true,
        ),
        (
            "import sys\nsys.stdin.buffer.read()\nsys.stdout.write('x'*2049)",
            false,
        ),
        (
            "import sys\nsys.stdin.buffer.read()\nprint('{broken PRIVATE_HELPER_DIAGNOSTIC')",
            false,
        ),
        ("import sys\nsys.stdin.buffer.read()\nprint('{}')", false),
        ("import sys\nsys.stdin.buffer.read()\nsys.exit(2)", false),
        ("import sys\nprint('{}')\nsys.exit(0)", false),
    ] {
        let (_, python) = helper(body);
        let snapshotter = Snapshotter {
            key: [0x42; 32],
            python,
        };
        let result = snapshotter.fingerprint_with_timeout(
            &[],
            &json!("x".repeat(1024 * 1024)),
            false,
            &[],
            Duration::from_secs(3),
        );
        assert_eq!(result.is_ok(), accepted, "{result:?}");
        if let Err(error) = result {
            assert!(!error.to_string().contains("PRIVATE_HELPER_DIAGNOSTIC"));
        }
    }
}

#[test]
fn invalid_path_and_helper_faults_fail_without_leaking_private_inputs_or_custody() {
    use std::os::unix::ffi::OsStringExt;
    let bad = Root::path(
        "private-root",
        PathBuf::from(std::ffi::OsString::from_vec(b"/PRIVATE_PATH_\xff".to_vec())),
    );
    let error = encode_request(&[0x42; 32], &[bad], &json!({}), false, &[]).unwrap_err();
    assert!(error.to_string().contains("cannot encode"));
    assert!(!error.to_string().contains("PRIVATE_PATH"));
    let (root, python) = helper(
        "import pathlib,os,time\npathlib.Path(__file__+'.pid').write_text(str(os.getpid()))\ntime.sleep(30)",
    );
    let snapshotter = Snapshotter {
        key: [0x42; 32],
        python,
    };
    let receipts = crate::process_cleanup::test_support::Scope::new();
    crate::process_cleanup::test_support::set_creation_failure(true);
    assert!(
        snapshotter
            .fingerprint_with_timeout(&[], &json!({}), false, &[], Duration::from_millis(200))
            .is_err()
    );
    crate::process_cleanup::test_support::set_creation_failure(false);
    assert!(!root.join("fake-python.pid").exists());
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::helper_exchange::faults::set(0);
        }
    }
    for fault in [1, 2, 3] {
        let reset = Reset;
        crate::helper_exchange::faults::set(fault);
        let result = std::panic::catch_unwind(|| {
            snapshotter.fingerprint_with_timeout(
                &[],
                &json!({"key":"PRIVATE_REQUEST"}),
                false,
                &[],
                Duration::from_millis(150),
            )
        });
        if fault == 2 {
            assert!(result.is_err());
        } else {
            let message = result.unwrap().unwrap_err().to_string();
            assert!(!message.contains("PRIVATE_REQUEST"));
            assert!(message.contains(if fault == 3 {
                "cleanup remains unconfirmed"
            } else {
                "read failed"
            }));
        }
        drop(reset);
        receipts.wait_finished();
        if let Ok(pid) = std::fs::read_to_string(root.join("fake-python.pid")) {
            assert!(!Path::new(&format!("/proc/{}", pid.trim())).exists());
            std::fs::remove_file(root.join("fake-python.pid")).unwrap();
        }
    }
}

fn bounded_case(case: &str, test: &str) {
    if std::env::var(CASE).as_deref() == Ok(case) {
        let root = PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
        let snapshotter = Snapshotter {
            key: [0x42; 32],
            python: root.join("fake-python"),
        };
        // Larger than any ordinary pipe: delivery itself must be cancellable.
        let context = json!({"private-context": "PRIVATE_CHECKPOINT_VALUE".repeat(32768)});
        let started = Instant::now();
        let result = snapshotter.fingerprint_with_timeout(
            &[],
            &context,
            false,
            &[],
            Duration::from_millis(150),
        );
        if let Err(error) = &result {
            assert!(!error.to_string().contains("PRIVATE_CHECKPOINT_VALUE"));
        }
        // A complete valid response may succeed after descendant quiescence.
        // Blocked input and excessive required output must never earn a digest.
        if case != "inherited-stdout" {
            assert!(result.is_err());
        }
        std::fs::write(
            root.join("returned.json"),
            serde_json::to_vec(&json!({
                "returned": true, "elapsed_ms": started.elapsed().as_millis(),
                "success": result.is_ok()
            }))
            .unwrap(),
        )
        .unwrap();
        return;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/checkpoint49-fixtures")
        .join(crate::workloads::TaskIdentity::local(case, None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let helper = root.join("fake-python");
    let script = format!(
        "#!/usr/bin/python3\nimport os,pathlib,sys,time\nroot=pathlib.Path({})\ncase={}\n(root/'helper.pid').write_text(str(os.getpid()))\nif case=='unread-request':\n time.sleep(30)\nelif case=='duplex-pressure':\n os.write(1,b'x'*131072)\n sys.stdin.buffer.read()\nelse:\n sys.stdin.buffer.read()\n pid=os.fork()\n if pid==0:\n  os.setsid()\n  os.close(0)\n  time.sleep(30)\n  os._exit(0)\n (root/'descendant.pid').write_text(str(pid))\n print('{{\"schema_version\":1,\"digest\":\"'+'a'*64+'\",\"bytes\":0,\"nodes\":0}}',flush=True)\n",
        serde_json::to_string(&root).unwrap(),
        serde_json::to_string(case).unwrap()
    );
    std::fs::write(&helper, script).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let executable = std::env::current_exe().unwrap();
    let binary = executable.parent().unwrap().parent().unwrap().join("asb");
    assert!(binary.is_file(), "build the actual ASB binary first");
    let spawn = |program: &OsStr, arguments: &[&str], fixture: bool| {
        let mut launch = crate::launch::ProcessLaunch::new(program);
        for argument in arguments {
            launch.arg(argument);
        }
        let (mut command, route) = launch.native(Some(&binary));
        command
            .isolated_env()
            .env("PATH", "/usr/bin:/bin")
            .env("ASB_RUNTIME_EXECUTABLE", &binary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if fixture {
            command
                .env(CASE, case)
                .env(DIRECTORY, &root)
                .stdout(File::create(root.join("fixture.stdout")).unwrap())
                .stderr(File::create(root.join("fixture.stderr")).unwrap());
        }
        let mut gate =
            crate::process_gate::Admission::new(route.validate_gate(&mut command).unwrap())
                .unwrap()
                .spawn()
                .unwrap();
        gate.resume().unwrap();
        gate
    };
    let name = format!("sweep::checkpoint::transport_tests::{test}");
    let mut sentinel = spawn(OsStr::new("/bin/sleep"), &["30"], false);
    let mut fixture = spawn(
        executable.as_os_str(),
        &["--exact", &name, "--nocapture"],
        true,
    );
    let deadline = Instant::now() + Duration::from_secs(4);
    let status = loop {
        if let Some(status) = fixture.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Finish actual ownership before any intended RED assertion. This also
    // contains a baseline helper whose stdin or descendant stdout never settles.
    fixture
        .stop()
        .expect("watchdog must quiesce its whole owned fixture tree");
    let sentinel_alive = sentinel.try_wait().unwrap().is_none();
    sentinel.stop().unwrap();
    std::fs::write(
        root.join("outer-cleanup-confirmed"),
        "fixture quiesced; sentinel stopped separately",
    )
    .unwrap();
    assert!(sentinel_alive, "unrelated sentinel was affected");
    for marker in ["helper.pid", "descendant.pid"] {
        if root.join(marker).exists() {
            let pid: u32 = std::fs::read_to_string(root.join(marker))
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "owned {marker} remained after watchdog cleanup"
            );
        } else if marker == "helper.pid" {
            panic!("helper never started; fixture {}", root.display());
        }
    }
    assert!(
        status.is_some_and(|status| status.success()) && root.join("returned.json").is_file(),
        "checkpoint exchange exceeded its short operation allowance ({case}); actual outer cleanup confirmed; evidence {}",
        root.display()
    );
    let returned: Value =
        serde_json::from_slice(&std::fs::read(root.join("returned.json")).unwrap()).unwrap();
    assert!(
        returned["elapsed_ms"].as_u64().unwrap() < 3000,
        "exchange did not return within its bounded caller allowance: {returned}"
    );
}
