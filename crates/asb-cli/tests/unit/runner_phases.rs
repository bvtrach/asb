//! Offline evidence/grade/resource transition contracts, not SDK qualification.
use super::*;
use std::cell::RefCell;
use std::rc::Rc;
#[path = "../support/mod.rs"]
mod support;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runner-phase-fixtures")
            .join(workloads::TaskIdentity::local("phases", None).attempt_id);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Workload {
    invocation: workloads::session::TaskInvocation,
    operations: Rc<RefCell<Vec<&'static str>>>,
    grade: u8,
    cleanup_error: bool,
    finished: bool,
}
impl workloads::WorkloadSession for Workload {
    fn invocation(&self) -> &workloads::session::TaskInvocation {
        &self.invocation
    }
    fn evaluation_kind(&self) -> &'static str {
        "upstream-harness"
    }
    fn baseline_passed(&self) -> Option<bool> {
        Some(false)
    }
    fn evaluate(&mut self, response: &str) -> Result<workloads::Evaluation, String> {
        assert_eq!(response, "submission");
        self.operations.borrow_mut().push("evaluate");
        match self.grade {
            0 => Ok(workloads::Evaluation::Passed),
            1 => Ok(workloads::Evaluation::Failed(
                "official unresolved task".into(),
            )),
            _ => Err("official verifier unavailable".into()),
        }
    }
    fn finish(&mut self) -> Result<(), String> {
        self.operations.borrow_mut().push(if self.grade == 2 {
            "preserve"
        } else {
            "finish"
        });
        self.finished = true;
        if self.cleanup_error {
            Err("owned cleanup unavailable".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Workload {
    fn drop(&mut self) {
        if !self.finished {
            self.operations.borrow_mut().push("cancel-preserve");
        }
    }
}
fn prepared(
    fixture: &Fixture,
    grade: u8,
    cleanup_error: bool,
    measurements: Option<measurements::Session>,
) -> (PreparedAttempt, Rc<RefCell<Vec<&'static str>>>) {
    let operations = Rc::new(RefCell::new(Vec::new()));
    (
        PreparedAttempt {
            workload: Box::new(Workload {
                invocation: workloads::session::TaskInvocation {
                    identity: workloads::TaskIdentity::local("fixture", None),
                    prompt: "fixture".into(),
                    workspace: fixture.0.clone(),
                },
                operations: operations.clone(),
                grade,
                cleanup_error,
                finished: false,
            }),
            measurements,
            tape: None,
        },
        operations,
    )
}
fn evidence(exit: i32, guest: bool) -> ExecutionEvidence {
    use std::os::unix::process::ExitStatusExt;
    ExecutionEvidence {
        output: Output {
            status: std::process::ExitStatus::from_raw(exit << 8),
            stdout: b"submission".to_vec(),
            stderr: b"diagnostic".to_vec(),
        },
        run_dir: PathBuf::new(),
        runtime_output: None,
        elapsed_ms: 17,
        guest: guest.then(|| {
            serde_json::from_value(serde_json::json!({
                "schema_version":1,"exit_code":exit,"final_response":"submission",
                "token_usage":{},"steps":3,"execution_failure":null,"tree_quiesced":true
            }))
            .unwrap()
        }),
    }
}

#[test]
fn native_guest_grade_and_cleanup_matrix_preserves_independent_facts() {
    // Each completed process, even an unsuccessful one, reaches the evaluator
    // exactly once. Instrumentation and cleanup never rewrite its grade.
    for guest in [false, true] {
        for exit in [0, 7, 124] {
            for grade in [0, 1, 2] {
                for cleanup_error in [false, true] {
                    let fixture = Fixture::new();
                    let (prepared, operations) = prepared(&fixture, grade, cleanup_error, None);
                    let mut issues = AttemptIssues::default();
                    issues
                        .collector_errors
                        .push("capture shutdown failed".into());
                    let execution = evidence(exit, guest);
                    let agent = execution.agent_outcome("fixture", &fixture.0);
                    assert_eq!(agent.failure.is_none(), exit == 0);
                    let final_state = prepared
                        .executed(execution, issues)
                        .settle(None)
                        .evaluate("submission")
                        .finish();
                    assert_eq!(
                        final_state.outcome.success(),
                        match grade {
                            0 => Some(true),
                            1 => Some(false),
                            _ => None,
                        }
                    );
                    assert_eq!(final_state.cleanup_confirmed, !cleanup_error);
                    assert_eq!(final_state.evidence.output.status.code(), Some(exit));
                    assert_eq!(final_state.evidence.output.stdout, b"submission");
                    assert_eq!(final_state.evidence.guest.is_some(), guest);
                    assert_eq!(
                        final_state.issues.collector_failure().as_deref(),
                        Some("capture shutdown failed")
                    );
                    assert_eq!(final_state.issues.run_error.is_some(), cleanup_error);
                    assert_eq!(
                        operations.borrow().as_slice(),
                        ["evaluate", if grade == 2 { "preserve" } else { "finish" }]
                    );
                    drop(final_state);
                    assert_eq!(
                        operations.borrow().len(),
                        2,
                        "completed attempts cannot cancel/rerun on drop"
                    );
                }
            }
        }
    }
}

#[test]
fn required_prelaunch_lifecycle_errors_admit_no_client_process() {
    for event in [
        LifecycleEvent::MeasurementsReady,
        LifecycleEvent::AgentSpawn,
        LifecycleEvent::AgentExecuteBegin,
    ] {
        let fixture = Fixture::new();
        let marker = fixture.0.join("executed");
        let mut session =
            measurements::Session::start_with_collectors(&fixture.0.join("measure"), &[], None)
                .unwrap();
        session.test_event_failure = Some(event);
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf executed > \"$1\"", "fixture"])
            .arg(&marker)
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let error = run_measured_agent(
            &mut command,
            "/bin/sh",
            None,
            Some(&mut session),
            false,
            &mut Vec::new(),
        )
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("fixture collector event failure")
        );
        assert!(
            !marker.exists(),
            "required prelaunch failure must remain fail-closed"
        );
    }
}

#[test]
fn spawn_and_wait_errors_remain_primary_when_failure_markers_also_fail() {
    for spawn_failure in [false, true] {
        let fixture = Fixture::new();
        let mut session =
            measurements::Session::start_with_collectors(&fixture.0.join("measure"), &[], None)
                .unwrap();
        session.test_event_failure = Some(LifecycleEvent::AgentExecuteEnd);
        let mut command = if spawn_failure {
            Command::new(fixture.0.join("missing-client"))
        } else {
            Command::new("/bin/true")
        };
        // Deliberately leave pipes absent in the wait-failure case.
        let error = run_measured_agent(
            &mut command,
            "fixture",
            None,
            Some(&mut session),
            false,
            &mut Vec::new(),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains(if spawn_failure {
                "could not start fixture"
            } else {
                "agent stdout was not piped"
            }),
            "{error}"
        );
        assert!(error.contains("fixture collector event failure"));
    }
}

#[test]
fn native_and_guest_apply_the_same_adapter_terminal_answer_policy() {
    let fixture = Fixture::new();
    for guest in [false, true] {
        let mut execution = evidence(0, guest);
        execution.output.stdout = b"{\"type\":\"tool_call\"}".to_vec();
        if let Some(metadata) = execution.guest.as_mut() {
            metadata.final_response = None;
        }
        let outcome = execution.agent_outcome("openjiuwen", &fixture.0);
        assert!(
            outcome
                .failure
                .unwrap()
                .contains("exited without a final response")
        );
        assert!(outcome.final_response.is_none());
        if let Some(metadata) = execution.guest.as_mut() {
            metadata.final_response = Some("finished".into());
        } else {
            execution.output.stdout = b"{\"type\":\"answer\",\"payload\":\"finished\"}".to_vec();
        }
        let outcome = execution.agent_outcome("openjiuwen", &fixture.0);
        assert!(outcome.failure.is_none());
        assert_eq!(outcome.final_response.as_deref(), Some("finished"));
    }
}

#[test]
fn every_evaluation_and_cleanup_marker_failure_still_invokes_owned_operations() {
    for event in [
        LifecycleEvent::PhaseBegin(LifecyclePhase::WorkloadEvaluation),
        LifecycleEvent::PhaseEnd(LifecyclePhase::WorkloadEvaluation),
        LifecycleEvent::PhaseFailed(LifecyclePhase::WorkloadEvaluation),
        LifecycleEvent::PhaseBegin(LifecyclePhase::WorkloadCleanup),
        LifecycleEvent::PhaseEnd(LifecyclePhase::WorkloadCleanup),
        LifecycleEvent::PhaseFailed(LifecyclePhase::WorkloadCleanup),
    ] {
        for grade in [0, 1, 2] {
            let fixture = Fixture::new();
            let mut session =
                measurements::Session::start_with_collectors(&fixture.0.join("measure"), &[], None)
                    .unwrap();
            session.test_event_failure = Some(event);
            let (prepared, operations) = prepared(&fixture, grade, true, Some(session));
            let result = prepared
                .executed(evidence(0, false), AttemptIssues::default())
                .settle(None)
                .evaluate("submission")
                .finish();
            assert_eq!(
                result.outcome.success(),
                match grade {
                    0 => Some(true),
                    1 => Some(false),
                    _ => None,
                }
            );
            assert_eq!(operations.borrow().len(), 2);
            if event != LifecycleEvent::PhaseFailed(LifecyclePhase::WorkloadEvaluation)
                || grade != 0
            {
                assert!(
                    result
                        .issues
                        .collector_failure()
                        .unwrap()
                        .contains("fixture collector event failure")
                );
            }
            assert!(
                result
                    .issues
                    .run_error
                    .unwrap()
                    .contains("owned cleanup unavailable")
            );
        }
    }
}

#[test]
fn interrupted_prepared_or_executed_attempt_preserves_without_grading() {
    for executed in [false, true] {
        let fixture = Fixture::new();
        let (prepared, operations) = prepared(&fixture, 0, false, None);
        if executed {
            drop(prepared.executed(evidence(0, false), AttemptIssues::default()));
        } else {
            drop(prepared);
        }
        assert_eq!(operations.borrow().as_slice(), ["cancel-preserve"]);
    }
}

#[test]
fn empty_missing_records_and_replay_failure_never_skip_grading_or_invent_usage() {
    for mode in [tape::Mode::Record, tape::Mode::Replay] {
        for empty in [false, true] {
            let fixture = Fixture::new();
            let path = fixture.0.join("cassette");
            if empty {
                std::fs::write(&path, b"").unwrap();
            }
            let (prepared, operations) = prepared(&fixture, 0, false, None);
            let mut issues = AttemptIssues::default();
            if mode == tape::Mode::Replay {
                issues.run_error("partial replay mismatch".into());
            }
            let result = prepared
                .executed(evidence(0, false), issues)
                .settle(Some(&(mode, path)))
                .evaluate("submission")
                .finish();
            assert_eq!(result.outcome.success(), Some(true));
            assert_eq!(operations.borrow().as_slice(), ["evaluate", "finish"]);
            assert!(result.issues.run_error.is_some());
            assert_eq!(result.cassette_usage_verified, mode == tape::Mode::Record);
        }
    }
}

#[test]
fn delayed_postexecution_work_does_not_extend_frozen_execution_time() {
    let fixture = Fixture::new();
    let (prepared, _) = prepared(&fixture, 0, false, None);
    let executed = prepared.executed(evidence(0, false), AttemptIssues::default());
    std::thread::sleep(Duration::from_millis(30));
    let result = executed.settle(None).evaluate("submission").finish();
    assert_eq!(result.evidence.elapsed_ms, 17);
}

#[test]
fn actual_native_duration_excludes_slow_execution_end_collector_hook() {
    struct SlowEnd;
    impl crate::collectors::CollectorAdapter for SlowEnd {
        fn id(&self) -> &'static str {
            "fixture-slow-end"
        }
        fn lifecycle_event(
            &self,
            _: &mut measurements::Session,
            event: LifecycleEvent,
        ) -> Result<(), String> {
            if event == LifecycleEvent::AgentExecuteEnd {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        }
    }
    static SLOW: SlowEnd = SlowEnd;
    let fixture = Fixture::new();
    let mut session =
        measurements::Session::start_with_collectors(&fixture.0.join("measure"), &[], None)
            .unwrap();
    session.add_test_adapter(&SLOW);
    let mut command = Command::new("/bin/true");
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    let started = Instant::now();
    let output = run_measured_agent(
        &mut command,
        "/bin/true",
        None,
        Some(&mut session),
        false,
        &mut Vec::new(),
    )
    .unwrap();
    assert!(output.output.status.success());
    assert!(
        started.elapsed().as_millis() >= output.elapsed_ms + 90,
        "collector shutdown must not enter execution elapsed_ms"
    );
}

#[test]
fn broken_native_terminal_retains_capture_and_finishes_already_admitted_work() {
    struct Broken {
        writes: usize,
        flush_only: bool,
    }
    impl Write for Broken {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            if self.flush_only {
                Ok(bytes.len())
            } else {
                Err(std::io::Error::other("first presentation failure"))
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("first flush failure"))
        }
    }
    for flush_only in [false, true] {
        let fixture = Fixture::new();
        let mut writer = Broken {
            writes: 0,
            flush_only,
        };
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf submission; printf diagnostic >&2; exit 7"])
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let measured =
            run_measured_agent(&mut command, "/bin/sh", None, None, true, &mut writer).unwrap();
        assert_eq!(
            writer.writes, 1,
            "never retry a broken terminal for every chunk/stream"
        );
        assert_eq!(measured.output.stdout, b"submission");
        assert_eq!(measured.output.stderr, b"diagnostic");
        assert_eq!(measured.output.status.code(), Some(7));
        assert!(
            measured
                .issues
                .output_error
                .as_ref()
                .unwrap()
                .contains(if flush_only {
                    "first flush failure"
                } else {
                    "first presentation failure"
                })
        );
        let (prepared, operations) = prepared(&fixture, 0, false, None);
        let mut execution = evidence(7, false);
        execution.output = measured.output;
        let finished = prepared
            .executed(execution, measured.issues)
            .settle(None)
            .evaluate("submission")
            .finish();
        assert_eq!(finished.outcome.success(), Some(true));
        assert_eq!(operations.borrow().as_slice(), ["evaluate", "finish"]);
        assert!(finished.issues.output_error.is_some());
    }
}

#[test]
fn native_postexecution_event_failure_retains_actual_child_output_and_grade() {
    for event in [LifecycleEvent::AgentExecuteEnd, LifecycleEvent::AgentExited] {
        let fixture = Fixture::new();
        let mut session =
            measurements::Session::start_with_collectors(&fixture.0.join("measure"), &[], None)
                .unwrap();
        session.test_event_failure = Some(event);
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf submission; printf diagnostic >&2; exit 7"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        let measured = run_measured_agent(
            &mut command,
            "/bin/sh",
            None,
            Some(&mut session),
            false,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(measured.output.stdout, b"submission");
        assert_eq!(measured.output.status.code(), Some(7));
        assert!(
            measured
                .issues
                .collector_failure()
                .unwrap()
                .contains("fixture collector event failure")
        );
        let (prepared, operations) = prepared(&fixture, 0, false, Some(session));
        let mut evidence = evidence(7, false);
        evidence.output = measured.output;
        evidence.elapsed_ms = measured.elapsed_ms;
        let result = prepared
            .executed(evidence, measured.issues)
            .settle(None)
            .evaluate("submission")
            .finish();
        assert_eq!(result.outcome.success(), Some(true));
        assert_eq!(operations.borrow().as_slice(), ["evaluate", "finish"]);
    }
}

fn result() -> RunResult<'static> {
    RunResult {
        replica_index: Some(1),
        execution_cohort: None,
        schema_version: 2,
        status: "completed",
        agent: "fixture",
        workload: "fixture",
        task: workloads::TaskIdentity::local("fixture", None),
        provider: "fixture",
        model: "fixture",
        process_success: true,
        workload_success: Some(true),
        workload_evaluation: "upstream-harness",
        workload_baseline_passed: Some(false),
        workload_baseline: workloads::BaselineEvidence::Measured {
            passed: Some(false),
        },
        workload_failure: None,
        failure_kind: None,
        failure_reason: None,
        success_rate: Some(1.0),
        adapter_smoke_success: None,
        elapsed_ms: 17,
        exit_code: Some(0),
        token_usage: Default::default(),
        steps: Some(3),
        cassette: None,
        measurement_directory: None,
        stdout: "submission".into(),
        stderr: String::new(),
    }
}

#[test]
fn postgrade_log_publication_is_independent_and_diagnostics_keep_primary_order() {
    let fixture = Fixture::new();
    let config = fixture.0.join("benchmark.toml");
    let result = result();
    let before = serde_json::to_vec(&result).unwrap();
    let published = LogPublication::publish(&config, "nested/run.jsonl", &result);
    assert!(published.confirmed());
    assert!(final_outcome(None, published, MeasurementCompletion::Confirmed).is_ok());
    let logged: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("nested/run.jsonl")).unwrap())
            .unwrap();
    assert_eq!(logged["workload_success"], true);
    std::fs::create_dir(fixture.0.join("blocked.jsonl")).unwrap();
    for primary in [None, Some(("run_error", "original proxy failure".into()))] {
        let failed = LogPublication::publish(&config, "blocked.jsonl", &result);
        assert!(!failed.confirmed());
        let error = final_outcome(
            primary.clone(),
            failed,
            MeasurementCompletion::Incomplete(vec!["measurement write failure".into()]),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("cannot open log file"));
        assert!(error.contains(fixture.0.join("blocked.jsonl").to_str().unwrap()));
        assert!(error.contains("measurement write failure"));
        if primary.is_some() {
            assert!(
                error.find("original proxy failure").unwrap()
                    < error.find("run log publication failed").unwrap()
            );
        }
        assert!(
            error.find("run log publication failed").unwrap()
                < error.find("measurement finalization failed").unwrap()
        );
    }
    let failed = LogPublication::publish(&config, "blocked.jsonl", &result);
    assert!(
        final_outcome(None, failed, MeasurementCompletion::Confirmed)
            .unwrap_err()
            .to_string()
            .contains("run log publication failed after task evaluation")
    );
    assert_eq!(serde_json::to_vec(&result).unwrap(), before);
}

#[test]
fn final_measurement_errors_preserve_grade_and_disallow_completion_receipts() {
    for fault in [
        "run-failed",
        "run-finishing",
        "publication",
        "none",
        "disabled",
    ] {
        let fixture = Fixture::new();
        let dir = fixture.0.join("measurements");
        let mut session = measurements::Session::start_with_collectors(&dir, &[], None).unwrap();
        session.test_event_failure = match fault {
            "run-failed" => Some(LifecycleEvent::RunFailed),
            "run-finishing" => Some(LifecycleEvent::RunFinishing),
            _ => None,
        };
        if fault == "publication" {
            std::fs::create_dir(dir.join("run.json")).unwrap();
        }
        let result = result();
        let before = serde_json::to_vec(&result).unwrap();
        let selected = if fault == "disabled" {
            drop(session);
            None
        } else {
            Some(session)
        };
        let completed = finish_measurements(selected, &result, fault != "run-failed");
        assert_eq!(completed.confirmed(), matches!(fault, "none" | "disabled"));
        let error = completed.into_error();
        if !matches!(fault, "none" | "disabled") {
            let error = error.unwrap().to_string();
            assert!(error.contains("after task evaluation"));
            assert!(error.contains("do not repeat the agent"));
        }
        assert_eq!(
            serde_json::to_vec(&result).unwrap(),
            before,
            "a finalization fault cannot replace the grade"
        );
        let mut output = Vec::new();
        render_json(&result, &mut output).unwrap();
        let rendered: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(rendered["workload_success"], true);
        assert_eq!(rendered["success_rate"], 1.0);
        if matches!(fault, "run-failed" | "run-finishing" | "none") {
            let measured: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("run.json")).unwrap()).unwrap();
            assert_eq!(
                measured["workload_success"], true,
                "finalization must still attempt publication after marker failure"
            );
        }
    }
}

#[test]
fn full_runner_entry_keeps_real_fixture_grade_and_primary_error_after_lifecycle_failure() {
    const CASE: &str = "ASB_RUNNER_ENTRY_TEST_CASE";
    if let Ok(case) = std::env::var(CASE) {
        full_entry_child(&case);
        return;
    }
    isolated_full_entry_cases(&[
        "execution-end",
        "run-finishing",
        "blocked-log",
        "blocked-log-and-measurements",
    ]);
}

#[test]
fn full_runner_entry_keeps_grade_and_publication_errors_when_result_writer_also_breaks() {
    isolated_full_entry_cases(&["blocked-log-broken-output"]);
}

#[test]
fn full_runner_entry_observer_warning_is_redacted_and_broken_warning_writer_keeps_primary() {
    isolated_full_entry_cases(&["observer-failure", "observer-warning-writer"]);
}

fn isolated_full_entry_cases(cases: &[&str]) {
    use support::IsolatedEnvironment;
    let executable = std::env::current_exe().unwrap();
    let runtime = executable.parent().unwrap().parent().unwrap().join("asb");
    assert!(
        runtime.is_file(),
        "build the actual ASB binary before this full-entry test"
    );
    for case in cases {
        let child = Command::new(&executable)
            .args(["--exact", "runner::phases::tests::full_runner_entry_keeps_real_fixture_grade_and_primary_error_after_lifecycle_failure", "--nocapture"])
            .isolated_env()
            .env("ASB_RUNNER_ENTRY_TEST_CASE", case)
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("ASB_RUNTIME_EXECUTABLE", &runtime)
            .output().unwrap();
        assert!(
            child.status.success(),
            "{case}: {}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

fn full_entry_child(case: &str) {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Observer {
        started: AtomicUsize,
        completed: AtomicUsize,
        fail: bool,
    }
    impl replica::CompletionObserver for Observer {
        fn starting(
            &self,
            _: replica::Identity,
            _: Option<&measurements::ExecutionCohort>,
        ) -> Result<(), Error> {
            self.started.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn completed(&self, _: replica::Identity, record: &Path) -> Result<(), Error> {
            assert!(record.is_file());
            self.completed.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(Error::Io(
                    "private-observer-secret-must-not-be-exposed".into(),
                ))
            } else {
                Ok(())
            }
        }
    }
    let fixture = Fixture::new();
    let root = fixture.0.canonicalize().unwrap();
    let project = project::ProjectConfig {
        tools_dir: "tools".into(),
        workloads_dir: "workloads".into(),
        workspace_dir: "workspace".into(),
        ..Default::default()
    };
    std::fs::create_dir(root.join(".asb")).unwrap();
    std::fs::write(
        root.join(".asb/project.toml"),
        toml::to_string(&project).unwrap(),
    )
    .unwrap();
    std::fs::create_dir(root.join("workloads")).unwrap();
    workloads::populate(&root.join("workloads")).unwrap();
    let agent = root.join("tools/pi/bin/pi");
    std::fs::create_dir_all(agent.parent().unwrap()).unwrap();
    let counter = root.join("agent-executions");
    let quoted_counter = counter.to_string_lossy().replace('\'', "'\\''");
    std::fs::write(&agent, format!("#!/bin/sh\nprintf 'called\\n' >> '{quoted_counter}'\nprintf '%s\\n' '{{\"type\":\"agent_end\",\"messages\":[{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"src/cli.py src/registry.py lookup\"}}]}}]}}'\n")).unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cassette = root.join("cassette.jsonl");
    let body = serde_json::json!({"model":"fixture","tools":[{"type":"function","function":{"name":"fixture","parameters":{"type":"object"}}}],"messages":[]});
    let row = serde_json::json!({"request":{"method":"POST","url":"/v1/chat/completions","body":body.to_string()},"response":{"status":200,"headers":{"content-type":"application/json"},"body":"{}"}});
    std::fs::write(&cassette, format!("{row}\n")).unwrap();
    let config: BenchmarkConfig = toml::from_str("agent='pi'\nprovider='openrouter'\nmodel='fixture'\nworkload='repository-navigation'\nworkspace='workspace'\nlog_file='run.jsonl'\n[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=[]\n").unwrap();
    let config_path = root.join("benchmark.toml");
    std::fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let blocked_log = case.starts_with("blocked-log");
    if blocked_log {
        std::fs::create_dir(root.join("run.jsonl")).unwrap();
    }
    let calls = Rc::new(RefCell::new(Vec::new()));
    let event = if matches!(
        case,
        "execution-end"
            | "blocked-log"
            | "blocked-log-broken-output"
            | "observer-failure"
            | "observer-warning-writer"
    ) {
        LifecycleEvent::AgentExecuteEnd
    } else {
        LifecycleEvent::RunFinishing
    };
    let _scope = entry_test_support::Scope::new(&root, event, calls.clone());
    let observer = Observer {
        started: AtomicUsize::new(0),
        completed: AtomicUsize::new(0),
        fail: case.starts_with("observer-"),
    };
    struct Presentation {
        bytes: Vec<u8>,
        fail_result: bool,
        fail_warning: bool,
        summary_done: bool,
        writes: usize,
        failures: usize,
    }
    impl Write for Presentation {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            if self.fail_result || (self.fail_warning && self.summary_done) {
                self.failures += 1;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "combined fixture presentation closed",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            self.summary_done |= bytes.contains(&b'\n');
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Presentation {
        bytes: Vec::new(),
        fail_result: case == "blocked-log-broken-output",
        fail_warning: case == "observer-warning-writer",
        summary_done: false,
        writes: 0,
        failures: 0,
    };
    let options = RunOptions {
        json: true,
        mode: Some(tape::Mode::Replay),
        cassette: Some(cassette.to_str().unwrap()),
        on_completion: Some(&observer),
        ..Default::default()
    };
    let error = run_mode(
        &config_path,
        config,
        options,
        &mut output,
        &Mutex::new(BTreeSet::new()),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("replay consumed 0 of 1"),
        "primary proxy validation must not be replaced: {error}"
    );
    if matches!(case, "run-finishing" | "blocked-log-and-measurements") {
        assert!(
            error.contains("measurement finalization failed after task evaluation"),
            "secondary finalization diagnostic must remain available: {error}"
        );
    }
    if blocked_log {
        assert!(
            error.contains("run log publication failed after task evaluation"),
            "{error}"
        );
        assert!(error.contains("cannot open log file"), "{error}");
        assert!(
            error.contains(root.join("run.jsonl").to_str().unwrap()),
            "{error}"
        );
        assert!(
            error.find("replay consumed 0 of 1").unwrap()
                < error.find("run log publication failed").unwrap(),
            "primary error must precede ancillary publication failure: {error}"
        );
        if case == "blocked-log-and-measurements" {
            assert!(
                error.find("run log publication failed").unwrap()
                    < error.find("measurement finalization failed").unwrap(),
                "both independent publication failures must be retained: {error}"
            );
        }
    }
    if matches!(
        case,
        "blocked-log-broken-output" | "observer-warning-writer"
    ) {
        assert!(
            error.contains("combined fixture presentation closed"),
            "{error}"
        );
        assert!(
            error.contains("result or continuation diagnostic output also failed"),
            "{error}"
        );
        assert_eq!(
            output.failures, 1,
            "never repeat output after the writer fails"
        );
        if case == "blocked-log-broken-output" {
            assert_eq!(output.writes, 1);
            assert!(output.bytes.is_empty());
            assert!(
                error.find("run log publication failed").unwrap()
                    < error
                        .find("result or continuation diagnostic output also failed")
                        .unwrap()
            );
        }
    }
    assert_eq!(std::fs::read_to_string(counter).unwrap(), "called\n");
    assert_eq!(calls.borrow().as_slice(), ["evaluate", "finish"]);
    assert_eq!(observer.started.load(Ordering::SeqCst), 1);
    let complete = case == "execution-end" || observer.fail;
    assert_eq!(
        observer.completed.load(Ordering::SeqCst),
        usize::from(complete)
    );
    assert_eq!(root.join(".asb-completions").exists(), complete);
    let log: Option<serde_json::Value> = (!blocked_log)
        .then(|| serde_json::from_slice(&std::fs::read(root.join("run.jsonl")).unwrap()).unwrap());
    let lines: Vec<serde_json::Value> = std::str::from_utf8(&output.bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let summary = lines.iter().find(|line| line.get("event").is_none());
    assert_eq!(summary.is_some(), case != "blocked-log-broken-output");
    for value in log.iter().chain(summary) {
        assert_eq!(
            value["workload_success"], true,
            "real bundled evaluator grade is independent of proxy/collector failure"
        );
        assert_eq!(value["process_success"], true);
        assert_eq!(value["success_rate"], 1.0);
        assert_eq!(value["failure_kind"], "run_error");
    }
    let measured: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("measurements/run.json")).unwrap())
            .unwrap();
    assert_eq!(measured["workload_success"], true);
    assert_eq!(measured["process_success"], true);
    assert_eq!(measured["success_rate"], 1.0);
    assert!(
        error.starts_with(measured["failure_reason"].as_str().unwrap()),
        "original primary must remain the returned error prefix: {error}"
    );
    if let Some(summary) = summary {
        assert_eq!(measured["failure_reason"], summary["failure_reason"]);
    }
    let warnings: Vec<_> = lines
        .iter()
        .filter(|line| line["event"] == "continuation_warning")
        .collect();
    if case == "observer-failure" {
        assert_eq!(
            lines.len(),
            2,
            "one unchanged summary then one typed warning"
        );
        assert!(lines[0].get("event").is_none());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0]["code"], "replica_receipt_unavailable");
        assert_eq!(warnings[0]["path_role"], "source_outcome");
        let source = Path::new(warnings[0]["path"].as_str().unwrap());
        assert!(source.is_file());
        assert_eq!(
            source.parent(),
            Some(root.join(".asb-completions").as_path())
        );
    } else {
        assert!(warnings.is_empty());
    }
    assert!(!error.contains("private-observer-secret-must-not-be-exposed"));
    assert!(
        !String::from_utf8_lossy(&output.bytes)
            .contains("private-observer-secret-must-not-be-exposed")
    );
    if blocked_log {
        assert!(
            root.join("run.jsonl").is_dir(),
            "never replace the blocking user directory"
        );
    }
}
