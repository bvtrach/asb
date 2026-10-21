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
