//! Baseline-before-refactor: literal wire and scoring contracts, not a bug repro.
use super::*;

#[test]
fn known_producers_preserve_literal_kinds_and_prompt_trait_contract() {
    let directory = Path::new(".");
    for (name, expected) in [
        ("bug-fix", "bundled-fixture"),
        ("repository-navigation", "response-review"),
        ("agentbench", "adapter-smoke"),
        ("not-a-registered-workload", "ungraded"),
    ] {
        assert_eq!(evaluation_kind(name, directory).as_str(), expected);
    }
    // `find` returns the actual registry trait object: stronger owning proof
    // than an unused dead-code assertion in the prompt adapter module.
    let prompt: &dyn adapters::WorkloadAdapter = adapters::find("bug-fix").unwrap();
    assert_eq!(prompt.id(), "prompt");
    assert_eq!(
        prompt.evaluation_kind("bug-fix").as_str(),
        "bundled-fixture"
    );
    assert_eq!(
        prompt.evaluation_kind("repository-navigation").as_str(),
        "response-review"
    );
}

#[test]
fn literal_resolution_and_baseline_matrix_keeps_unknown_extensions_unscored() {
    let measured = BaselineEvidence::Measured {
        passed: Some(false),
    };
    let unavailable = BaselineEvidence::Unavailable;
    for kind in ["bundled-fixture", "response-review", "upstream-harness"] {
        assert!(resolution_eligible(kind));
        assert_eq!(scored_kind(kind, &measured), kind);
        assert_eq!(scored_kind(kind, &unavailable), "invalid-baseline");
    }
    for kind in [
        "adapter-smoke",
        "ungraded",
        "invalid-baseline",
        "unknown",
        "extension-v7",
        "Upstream-Harness",
        "",
    ] {
        assert!(!resolution_eligible(kind));
        assert_eq!(scored_kind(kind, &measured), kind);
        assert_eq!(scored_kind(kind, &unavailable), kind);
        // This low-level predicate alone has NEVER granted resolution credit.
        assert!(measured.supports(kind));
        let mut summary = ResolutionSummary::default();
        summary.observe(kind, true);
        assert_eq!(summary.task_count, 0);
        assert_eq!(summary.unscored_count, 1);
        assert_eq!(summary.success_rate, None);
    }
}

#[test]
fn official_baselines_keep_hash_requirements_and_local_distinction() {
    for baseline in [
        BaselineEvidence::UpstreamTaskDefinition {
            base_commit: "a".repeat(40),
        },
        BaselineEvidence::UpstreamEnvironmentDefinition {
            source_revision: "b".repeat(40),
            definition_sha256: "c".repeat(64),
        },
    ] {
        assert!(baseline.supports("upstream-harness"));
        for local in [
            "bundled-fixture",
            "response-review",
            "adapter-smoke",
            "extension-v7",
        ] {
            assert!(!baseline.supports(local));
        }
    }
    assert!(
        !BaselineEvidence::UpstreamTaskDefinition {
            base_commit: "z".repeat(40)
        }
        .supports("upstream-harness")
    );
    assert!(
        !BaselineEvidence::UpstreamEnvironmentDefinition {
            source_revision: "b".repeat(40),
            definition_sha256: "c".repeat(63)
        }
        .supports("upstream-harness")
    );
}

#[test]
fn historical_extension_selection_roundtrips_without_closing_the_wire() {
    for kind in ["extension-v7", "unknown", "unscored", "fixture-grader"] {
        let raw = serde_json::json!({"workload":"historical-suite", "task_id":"task", "dataset":null, "evaluation_kind":kind, "population_size":null});
        let selection: TaskSelection = serde_json::from_value(raw.clone()).unwrap();
        assert!(selection.valid());
        assert_eq!(serde_json::to_value(selection).unwrap(), raw);
    }
}

#[test]
fn selection_descriptor_and_fingerprint_inputs_are_literal_and_stable() {
    use sha2::{Digest, Sha256};
    let selection = selection::describe("bug-fix", None, None, Path::new("."), None).unwrap();
    let expected = serde_json::json!({
        "module": {"schema_version":1,"id":"bug-fix","family":"asb-local","suite_version":"1","support":"local-task","dataset":null,"harness":null,
            "protocol":{"id":"bundled-fixture","revision":1,"environment":"native","submission":"workspace","evaluation":"local","score_policy":"task-resolution","input_modalities":["text"]}},
        "workload":"bug-fix","task_id":"bug-fix","dataset":null,"evaluation_kind":"bundled-fixture","population_size":null
    });
    let actual = serde_json::to_value(&selection).unwrap();
    assert_eq!(actual, expected);
    // Selection is embedded in checkpoint/planning fingerprint context. Lock
    // its exact canonical JSON bytes, not a new hash algorithm or wire field.
    assert_eq!(
        Sha256::digest(serde_json::to_vec(&actual).unwrap()),
        Sha256::digest(serde_json::to_vec(&expected).unwrap())
    );
    assert_eq!(
        descriptor("repository-navigation").unwrap().protocol.id,
        "response-review"
    );
    assert_eq!(
        descriptor("agentbench").unwrap().protocol.id,
        "adapter-smoke"
    );
}

#[test]
fn typed_producers_and_borrowed_saved_classifier_preserve_every_category() {
    use KnownEvaluationKind as Kind;
    let measured = BaselineEvidence::Measured {
        passed: Some(false),
    };
    let unavailable = BaselineEvidence::Unavailable;
    for (kind, wire, eligible) in [
        (Kind::BundledFixture, "bundled-fixture", true),
        (Kind::ResponseReview, "response-review", true),
        (Kind::AdapterSmoke, "adapter-smoke", false),
        (Kind::UpstreamHarness, "upstream-harness", true),
        (Kind::Ungraded, "ungraded", false),
        (Kind::InvalidBaseline, "invalid-baseline", false),
    ] {
        assert_eq!(kind.as_str(), wire);
        assert_eq!(
            SavedEvaluationKind::classify(wire),
            SavedEvaluationKind::Known(kind)
        );
        assert_eq!(kind.resolution_eligible(), eligible);
        assert_eq!(kind.scored(&measured), kind);
        assert_eq!(
            kind.scored(&unavailable),
            if eligible {
                Kind::InvalidBaseline
            } else {
                kind
            }
        );
    }
    for wire in [
        "custom-future-evaluator",
        "extension-v7",
        "Upstream-Harness",
        "",
        "原样类别",
    ] {
        let owned = wire.to_owned();
        let saved = SavedEvaluationKind::classify(&owned);
        assert!(
            matches!(saved, SavedEvaluationKind::Extension(value) if std::ptr::eq(value.as_ptr(), owned.as_ptr()))
        );
        assert_eq!(saved.as_str(), wire);
        assert!(!saved.resolution_eligible());
        assert!(measured.supports_evaluation(saved));
        assert_eq!(saved.scored(&unavailable), saved);
    }
}
