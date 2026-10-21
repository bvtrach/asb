//! Offline runner admission and owned-child stream handling.
use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "asb-runner-boundaries-{}",
            workloads::TaskIdentity::local("boundary", None).attempt_id
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn config(&self) -> PathBuf {
        let path = self.0.join("benchmark.toml");
        std::fs::write(
            &path,
            "agent='fixture'\nprovider='fixture'\nmodel='fixture'\nworkload='bug-fix'\n",
        )
        .unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn admission_rejects_bad_replica_reuse_and_cohort_before_any_execution() {
    let root = Fixture::new();
    let path = root.config();
    let options = RunOptions {
        replica: replica::Identity { index: 0, total: 1 },
        ..Default::default()
    };
    assert!(
        run_with_tape(path.to_str().unwrap(), options, &mut Vec::new())
            .unwrap_err()
            .to_string()
            .contains("invalid benchmark replica identity")
    );
    let reused = std::collections::BTreeMap::from([(2, serde_json::json!({}))]);
    let options = RunOptions {
        reused: Some(&reused),
        ..Default::default()
    };
    assert!(
        run_with_tape(path.to_str().unwrap(), options, &mut Vec::new())
            .unwrap_err()
            .to_string()
            .contains("outside the selected population")
    );
    let cohort = measurements::ExecutionCohort {
        id: "fixture".into(),
        selected_replicas: 1,
        admitted_slots: Vec::new(),
    };
    let options = RunOptions {
        execution_cohort: Some(&cohort),
        ..Default::default()
    };
    let config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        run_mode(
            &path,
            config,
            options,
            &mut Vec::new(),
            &Mutex::new(BTreeSet::new())
        )
        .unwrap_err()
        .to_string()
        .contains("does not admit the selected replica")
    );
    assert_eq!(
        std::fs::read_dir(&root.0).unwrap().count(),
        1,
        "no workspace or artifact may be created"
    );
}

#[test]
fn single_reused_result_renders_without_provider_project_or_agent_startup() {
    let root = Fixture::new();
    let path = root.config();
    let summary = serde_json::json!({"task":{"task_id":"retained"},"process_success":true,"workload_success":true,"token_usage":{"total_tokens":123}});
    let reused = std::collections::BTreeMap::from([(1, summary.clone())]);
    for json in [false, true] {
        let options = RunOptions {
            json,
            reused: Some(&reused),
            ..Default::default()
        };
        let mut output = Vec::new();
        run_with_tape(path.to_str().unwrap(), options, &mut output).unwrap();
        if json {
            let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(value["reused"], true);
            assert_eq!(value["token_usage"]["total_tokens"], 123);
        } else {
            assert_eq!(
                String::from_utf8(output).unwrap(),
                "Reused completed task retained: resolved.\n"
            );
        }
    }
    assert_eq!(
        reused[&1], summary,
        "rendering must not rewrite persisted outcome"
    );
}

#[cfg(unix)]
#[test]
fn gate_target_checks_absolute_interpreters_without_launching_anything() {
    let root = Fixture::new();
    assert!(!gate_target_available(
        root.0.join("missing").to_str().unwrap()
    ));
    assert!(gate_target_available("fixture-name-on-path"));
    let path = root.0.join("program");
    for (bytes, available) in [
        (b"ELF-fixture".as_slice(), true),
        (b"#!/missing/asb-fixture-interpreter\n".as_slice(), false),
        (b"#!/bin/sh\n".as_slice(), true),
        (b"#!relative-interpreter\n".as_slice(), true),
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(gate_target_available(path.to_str().unwrap()), available);
    }
    assert_eq!(safe_name("model:a/b Ω"), "model-a-b--");
}

#[cfg(unix)]
fn child(script: &str) -> OwnedChild {
    OwnedChild::Direct(
        Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

#[cfg(unix)]
#[test]
fn timeout_stops_only_owned_child_and_reports_budget_once() {
    // exec replaces the shell: there are no untracked descendants or sleep races.
    let child = child("exec /bin/sleep 10");
    let pid = child.id();
    let mut callbacks = 0;
    let result = wait_with_output_streams(
        child,
        false,
        &mut Vec::new(),
        None,
        Duration::from_millis(10),
        &mut || {
            callbacks += 1;
            Ok(())
        },
    )
    .unwrap();
    assert!(!result.status.success());
    assert_eq!(callbacks, 1);
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "agent execution exceeded 0.01 seconds\n"
    );
    #[cfg(target_os = "linux")]
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "owned child must be reaped"
    );
}

#[cfg(unix)]
#[test]
fn both_streams_are_forwarded_and_captured_without_losing_exit_status() {
    let mut combined = Vec::new();
    let result = wait_with_output_streams(
        child("printf out; printf err >&2; exit 7"),
        true,
        &mut combined,
        None,
        Duration::from_secs(2),
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(result.status.code(), Some(7));
    assert_eq!(result.stdout, b"out");
    assert_eq!(result.stderr, b"err");
    let text = String::from_utf8(combined).unwrap();
    assert!(text == "outerr" || text == "errout", "{text}");
}

#[test]
fn stream_read_errors_and_dropped_receivers_are_bounded() {
    struct BrokenRead;
    impl Read for BrokenRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("fixture read failure"))
        }
    }
    let (sender, receiver) = mpsc::channel();
    read_stream(BrokenRead, OutputStream::Stdout, sender);
    assert!(matches!(receiver.recv().unwrap(), OutputChunk::Finished));
    let (sender, receiver) = mpsc::channel();
    drop(receiver);
    read_stream(
        std::io::Cursor::new(b"unconsumed"),
        OutputStream::Stderr,
        sender,
    );
}

#[test]
fn unknown_agent_command_retains_generic_contract_without_adapter_guessing() {
    let config:BenchmarkConfig=toml::from_str("agent='fixture-unique-executable'\nprovider='fixture'\nmodel='fixture'\nworkload='bug-fix'\n").unwrap();
    let command = command_for_with_interface(&config, None, "prompt", None).unwrap();
    assert_eq!(command.program, config.agent);
    assert!(command.args.is_empty());
    assert!(command.append_prompt);
}
