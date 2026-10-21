//! Actual replay-only entry qualification, not a synthetic finalizer substitute.
use super::*;
use crate::evidence::{
    FaultPoint,
    tests::{Scope, WriteAction},
};

#[test]
fn actual_entry_publication_faults_preserve_grades_identity_and_independent_finalization() {
    isolated_full_entry_cases(&[
        "disposition-true-publication-log-short",
        "disposition-false-publication-log-short-client-output",
        "disposition-native-publication-log-sync",
        "disposition-pending-publication-log-sync",
        "disposition-true-publication-run-short",
        "disposition-native-publication-summary-malformed",
        "disposition-pending-publication-run-short-output",
        "disposition-false-publication-summary-sync",
        "disposition-false-publication-log-scratch-client-output",
        "disposition-native-publication-capture-rename",
        "disposition-pending-publication-capture-sync-output",
        "disposition-native-publication-capture-admission-output",
    ]);
}

const PREVIOUS_LOG: &[u8] = b"{\"previous\":\"immutable log sentinel\"}\n";
const PREVIOUS_RUN: &[u8] = b"{\"previous\":\"immutable measurement sentinel\"}";
pub(super) fn prepare(case: &str, root: &Path) -> Scope {
    std::fs::create_dir_all(root.join("measurements")).unwrap();
    let (path, point, writes) = if case.contains("log-") {
        let path = root.join("run.jsonl");
        std::fs::write(&path, PREVIOUS_LOG).unwrap();
        (
            path,
            if case.contains("scratch") {
                Some(FaultPoint::Cleanup)
            } else {
                case.contains("sync").then_some(FaultPoint::DirectorySync)
            },
            if case.contains("short") {
                vec![
                    WriteAction::Short(PREVIOUS_LOG.len()),
                    WriteAction::Short(5),
                    WriteAction::NoSpace,
                ]
            } else {
                Vec::new()
            },
        )
    } else if case.contains("capture") {
        (
            root.join("measurements/capture-fixture.json"),
            if case.contains("admission") {
                None
            } else {
                Some(if case.contains("sync") {
                    FaultPoint::DirectorySync
                } else {
                    FaultPoint::Rename
                })
            },
            Vec::new(),
        )
    } else if case.contains("run-short") {
        let path = root.join("measurements/run.json");
        std::fs::write(&path, PREVIOUS_RUN).unwrap();
        (
            path,
            None,
            vec![WriteAction::Short(5), WriteAction::NoSpace],
        )
    } else {
        let path = root.join("measurements/summary.json");
        if case.contains("malformed") {
            std::fs::write(&path, b"{historical summary sentinel").unwrap();
        }
        (path, Some(FaultPoint::DirectorySync), Vec::new())
    };
    Scope::new(path, point, writes)
}

pub(super) fn check(
    case: &str,
    root: &Path,
    result: Result<(), Error>,
    calls: &Rc<RefCell<Vec<&'static str>>>,
    output: &[u8],
    completions: usize,
) {
    let error = result.unwrap_err().to_string();
    let log_fault = case.contains("log-");
    let expected = if log_fault {
        "run log publication failed"
    } else {
        "measurement finalization failed"
    };
    assert!(error.contains(expected), "{case}: {error}");
    if case.contains("sync") {
        assert!(
            error.contains("was published, but durability is unconfirmed"),
            "{error}"
        );
    }
    if case.contains("scratch") {
        assert!(error.contains("was published and synchronized"), "{error}");
        assert!(!error.contains("durability is unconfirmed"), "{error}");
        assert!(std::fs::read_dir(root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".asb-evidence-stage-")
        }));
    }
    if case.contains("output") {
        assert!(
            error.contains("presentation") || error.contains("cannot write agent output"),
            "{error}"
        );
    }
    assert_eq!(
        calls.borrow().as_slice(),
        ["evaluate", "finish", "provider-finish"]
    );
    assert_eq!(completions, 0);
    assert!(!root.join(".asb-completions").exists());
    if case.contains("admission") {
        assert!(error.contains("raw path changed"), "{error}");
        assert_eq!(
            std::fs::read(root.join("measurements/admission.stdout")).unwrap(),
            b"A=99\n"
        );
        assert_eq!(
            std::fs::read(root.join("measurements/admission.saved")).unwrap(),
            b"A=17\n"
        );
        let capture: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("measurements/admission.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(capture["stdout"], "A=17\n");
        assert_eq!(capture["metrics"]["value"], 17.0);
        for name in ["stdout", "stderr"] {
            assert!(
                !root
                    .join(format!("measurements/capture-sibling.{name}"))
                    .exists()
            );
        }
        assert!(root.join("measurements/capture-sibling.json").is_file());
    } else if case.contains("capture") {
        for (name, expected) in [
            ("stdout", b"complete raw stdout".as_slice()),
            ("stderr", b"complete raw stderr".as_slice()),
        ] {
            let path = root.join(format!("measurements/capture-fixture.{name}"));
            assert_eq!(std::fs::read(&path).unwrap(), expected);
            assert!(error.contains(path.to_str().unwrap()), "{error}");
            assert!(
                !root
                    .join(format!("measurements/capture-sibling.{name}"))
                    .exists()
            );
        }
        assert!(root.join("measurements/capture-sibling.json").is_file());
    }
    // A failed run.json must not skip summary, components or lifecycle; a failed
    // log must not skip any measurement. No Drop rewrite retries publication.
    for file in ["components.json", "lifecycle.json"] {
        let path = root.join("measurements").join(file);
        let _: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    }
    let summary_path = root.join("measurements/summary.json");
    if case.contains("malformed") {
        assert_eq!(
            std::fs::read(&summary_path).unwrap(),
            b"{historical summary sentinel"
        );
        assert!(error.contains("historical summary JSON is malformed"));
    }
    let log_bytes = std::fs::read(root.join("run.jsonl")).unwrap();
    if log_fault && case.contains("short") {
        assert_eq!(log_bytes, PREVIOUS_LOG);
    }
    let measured_bytes = std::fs::read(root.join("measurements/run.json")).unwrap();
    if case.contains("run-short") {
        assert_eq!(measured_bytes, PREVIOUS_RUN);
    }
    let record: serde_json::Value = if case.contains("run-short") {
        serde_json::from_slice(
            log_bytes
                .split(|byte| *byte == b'\n')
                .rfind(|line| !line.is_empty())
                .unwrap(),
        )
        .unwrap()
    } else {
        serde_json::from_slice(&measured_bytes).unwrap()
    };
    if case.contains("false") || case.contains("output") {
        let reason = record["failure_reason"].as_str().unwrap();
        assert!(
            error.find(reason).unwrap() < error.find(expected).unwrap(),
            "primary outcome must precede publication: {error}"
        );
    }
    if !case.contains("malformed") {
        let aggregate: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&summary_path).unwrap()).unwrap();
        let aggregate = &aggregate["runs"][0];
        assert_eq!(aggregate["task"], record["task"]);
        assert_eq!(aggregate["benchmark_outcome"], record["benchmark_outcome"]);
        assert_eq!(
            aggregate["execution_plan_fingerprint"],
            record["execution_plan_fingerprint"]
        );
        assert_eq!(aggregate["execution_cohort"], record["execution_cohort"]);
        for field in ["workload_resource_disposition", "workload_artifacts"] {
            assert!(
                record.get(field).is_some(),
                "missing current run field {field}"
            );
            assert_eq!(aggregate[field], record[field]);
        }
    }
    assert_eq!(record["execution_cohort"]["selected_replicas"], 1);
    assert!(
        record["execution_plan_fingerprint"]
            .as_str()
            .unwrap()
            .starts_with("asb-protocol-v1:")
    );
    assert!(!record["task"]["attempt_id"].as_str().unwrap().is_empty());
    if case.contains("native") {
        assert_eq!(
            record["benchmark_outcome"]["evaluation"]["rewards"]["accuracy"],
            0.5
        );
        assert!(record["benchmark_outcome"]["evaluation"]["resolution"].is_null());
    } else if case.contains("pending") {
        assert_eq!(
            record["benchmark_outcome"]["evaluation"]["status"],
            "pending"
        );
        assert!(record["workload_success"].is_null());
    } else {
        assert_eq!(record["workload_success"], !case.contains("false"));
    }
    if !case.contains("output") {
        let rendered: serde_json::Value = serde_json::from_slice(output).unwrap();
        assert_eq!(rendered["task"], record["task"]);
        assert_eq!(rendered["benchmark_outcome"], record["benchmark_outcome"]);
    }
    let compared = crate::compare::generate(
        root.join("benchmark.toml").to_str().unwrap(),
        &mut Vec::new(),
    );
    if (log_fault && case.contains("short"))
        || case.contains("run-short")
        || case.contains("malformed")
    {
        assert!(
            compared.is_err(),
            "old unrelated sentinel cannot masquerade as this attempt"
        );
    } else {
        let report = compared.unwrap();
        let comparison: serde_json::Value = serde_json::from_slice(
            &std::fs::read(report.with_file_name("comparison.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(comparison["benchmarks"][0]["task"], record["task"]);
        assert_eq!(
            comparison["benchmarks"][0]["benchmark_outcome"],
            record["benchmark_outcome"]
        );
    }
}
