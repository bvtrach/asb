//! Passive metadata tests: synthetic receipt state, never agent execution.
use super::*;
use crate::runner::campaign::StartIntent;
use crate::runner::campaign::generation::{Catalog, Context, Expected};

fn load(guard: &Guard, sweep: &SweepPlan, results: &Path) -> Result<Catalog, Error> {
    let layouts = sweep.read_campaigns(results)?;
    let expected = sweep
        .entries
        .iter()
        .zip(&layouts)
        .map(|(entry, layout)| Expected {
            reference: entry.campaign.as_ref().unwrap(),
            plan: &layout.plan,
        })
        .collect::<Vec<_>>();
    Catalog::read(
        &guard.inputs.directory,
        Context {
            run_id: &sweep.run_id,
            input_digest: &guard.inputs.initial.digest,
            original_index: Blob::of(&fs::read(results.join("plan.json")).unwrap()),
        },
        &expected,
    )
}

fn published_mixed() -> (Fixture, SweepPlan, PathBuf, Guard, Vec<AdmittedCampaign>) {
    let (fixture, sweep, results, prepared) = mixed();
    let (guard, workers) = prepared.publish(&results, &sweep).unwrap();
    (fixture, sweep, results, guard, workers)
}

fn rewrite_manifest(results: &Path, configuration: usize, mutate: impl FnOnce(&mut Value)) {
    let directory = results.join(".checkpoint").join(ROOT);
    let path = directory.join(manifest_path(configuration));
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut value);
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(path, &bytes).unwrap();
    let path = directory.join("admitted.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["manifests"][configuration]["content"] = serde_json::to_value(Blob::of(&bytes)).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn exact_start_reference_resolves_only_pending_members_and_preserves_old_bindings() {
    let (_fixture, sweep, results, guard, workers) = published_mixed();
    let catalog = load(&guard, &sweep, &results).unwrap();
    let worker = &workers[0];
    let reference = catalog.reference(0).unwrap();
    let batch = &worker.manifest.pending[0];
    for id in [2, 4] {
        let slot = &worker.plan.slots[id - 1];
        let resolved = catalog
            .resolve(Some(&reference), 0, &worker.plan, slot, Some(&batch.cohort))
            .unwrap();
        assert_eq!(
            resolved.prepared,
            batch.directory(&reference.generation).join("prepared.json")
        );
        assert_eq!(
            resolved.evidence,
            [
                root(&guard.inputs).join("000000.json"),
                root(&guard.inputs).join("admitted.json")
            ]
        );
        assert!(
            catalog
                .resolve(None, 0, &worker.plan, slot, Some(&batch.cohort))
                .is_err()
        );
        assert!(
            catalog
                .resolve(Some(&reference), 0, &worker.plan, slot, None)
                .is_err()
        );
    }
    for id in [1, 3, 5, 6, 7, 8] {
        let slot = &worker.plan.slots[id - 1];
        let old = catalog.resolve(None, 0, &worker.plan, slot, None).unwrap();
        assert!(old.evidence.is_empty());
        assert_eq!(
            old.prepared,
            PathBuf::from(format!("batch-{}/prepared.json", slot.batch))
        );
        assert!(
            catalog
                .resolve(Some(&reference), 0, &worker.plan, slot, Some(&batch.cohort))
                .is_err()
        );
    }
    let slot = &worker.plan.slots[1];
    for field in ["generation", "configuration", "manifest", "admission"] {
        let mut wrong = reference.clone();
        match field {
            "generation" => wrong.generation.push('a'),
            "configuration" => wrong.configuration = 1,
            "manifest" => wrong.manifest.sha256 = "0".repeat(64),
            _ => wrong.admission.bytes += 1,
        }
        assert!(
            catalog
                .resolve(Some(&wrong), 0, &worker.plan, slot, Some(&batch.cohort))
                .is_err(),
            "{field}"
        );
    }
    let mut wrong = batch.cohort.clone();
    wrong.id.push('b');
    assert!(
        catalog
            .resolve(Some(&reference), 0, &worker.plan, slot, Some(&wrong))
            .is_err()
    );
    let mut wrong_slot: crate::runner::campaign::Slot =
        serde_json::from_value(serde_json::to_value(slot).unwrap()).unwrap();
    wrong_slot.id = 4;
    assert!(
        catalog
            .resolve(
                Some(&reference),
                0,
                &worker.plan,
                &wrong_slot,
                Some(&batch.cohort)
            )
            .is_err()
    );
}

#[test]
fn global_metadata_admits_no_partial_foreign_aliased_or_changed_index() {
    for case in [
        "missing-last",
        "extra",
        "pending",
        "alias",
        "index",
        "membership",
        "scope",
        "unknown",
        "digest",
    ] {
        let (_fixture, sweep, results, guard, _workers) = published_mixed();
        let directory = root(&guard.inputs);
        match case {
            "missing-last" => fs::remove_file(directory.join("000001.json")).unwrap(),
            "extra" | "pending" => fs::write(
                directory.join(if case == "extra" {
                    "foreign.json"
                } else {
                    ".interrupted.pending"
                }),
                b"retained",
            )
            .unwrap(),
            "alias" => {
                fs::rename(directory.join("000001.json"), directory.join("retained")).unwrap();
                std::os::unix::fs::symlink("retained", directory.join("000001.json")).unwrap();
            }
            "index" => {
                let path = results.join("plan.json");
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(b'\n');
                fs::write(path, bytes).unwrap();
            }
            _ => {
                let path = directory.join("admitted.json");
                let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                match case {
                    "membership" => value["manifests"][1]["configuration"] = json!(0),
                    "scope" => value["scope"] = json!("foreign"),
                    "unknown" => value["extra"] = json!(true),
                    _ => value["manifests"][1]["content"]["sha256"] = json!("0".repeat(64)),
                }
                fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            }
        }
        assert!(load(&guard, &sweep, &results).is_err(), "{case}");
    }
}

#[test]
fn manifest_partition_and_frozen_cohort_validation_survive_recomputed_file_hashes() {
    for case in [
        "completed-overlap",
        "missing-slot",
        "duplicate",
        "unsorted",
        "dispatch",
        "unknown-dispatch",
        "wrong-c",
        "receipt",
        "batch",
        "original",
        "generation",
        "late-original",
    ] {
        let (_fixture, sweep, results, guard, _workers) = published_mixed();
        rewrite_manifest(
            &results,
            usize::from(case == "late-original"),
            |value| match case {
                "completed-overlap" => {
                    value["pending"][0]["cohort"]["admitted_slots"] = json!([1, 4])
                }
                "missing-slot" => {
                    value["completed"].as_array_mut().unwrap().pop();
                }
                "duplicate" => value["pending"][0]["cohort"]["admitted_slots"] = json!([2, 2]),
                "unsorted" => value["pending"][0]["cohort"]["admitted_slots"] = json!([4, 2]),
                "dispatch" => value["pending"][0]["cohort"]["dispatch"]["admitted"] = json!(4),
                "unknown-dispatch" => value["pending"][0]["cohort"]["dispatch"] = Value::Null,
                "wrong-c" => value["pending"][0]["cohort"]["selected_replicas"] = json!(2),
                "receipt" => value["completed"][0]["receipt"]["bytes"] = json!(0),
                "batch" => value["pending"][0]["batch"] = json!(3),
                "generation" => value["generation"] = json!("../escape"),
                _ => value["original"]["sha256"] = json!("0".repeat(64)),
            },
        );
        assert!(load(&guard, &sweep, &results).is_err(), "{case}");
    }
}

#[test]
fn start_resolution_selects_generation_material_without_changing_old_receipt_roots() {
    let (_fixture, sweep, results, guard, workers) = published_mixed();
    let catalog = load(&guard, &sweep, &results).unwrap();
    let layouts = sweep.read_campaigns(&results).unwrap();
    let layout = &layouts[0];
    let slot = &layout.plan.slots[1];
    let cohort = &workers[0].manifest.pending[0].cohort;
    let reference = catalog.reference(0).unwrap();
    let path = layout.root.join("batch-1/starts/2.json");
    let start = StartIntent {
        schema_version: 1,
        scope: "execution-start-intent-not-actual-attempt".into(),
        execution: layout.plan.execution(slot),
        execution_cohort: Some(cohort.clone()),
        generation: Some(reference),
    };
    fs::write(&path, serde_json::to_vec(&start).unwrap()).unwrap();
    let resolved = super::super::super::resolve_start(layout, slot, 0, Some(&catalog)).unwrap();
    assert!(resolved.binding.prepared.starts_with(".asb-generations"));
    assert_eq!(resolved.binding.evidence.len(), 2);
    let mut stripped = start;
    stripped.generation = None;
    fs::write(&path, serde_json::to_vec(&stripped).unwrap()).unwrap();
    assert!(super::super::super::resolve_start(layout, slot, 0, Some(&catalog)).is_err());
    let old = super::super::super::resolve_start(layout, &layout.plan.slots[0], 0, Some(&catalog))
        .unwrap();
    assert_eq!(old.binding.prepared, Path::new("batch-1/prepared.json"));
    assert!(old.binding.evidence.is_empty());
}

#[test]
fn historical_receipts_remain_byte_frozen_after_admission_and_are_not_republished() {
    for rewrite in [false, true] {
        let (fixture, sweep, results, guard, _workers) = published_mixed();
        let path = guard.inputs.campaign_completion_path(1, 8);
        let original = fs::read(&path).unwrap();
        if rewrite {
            let mut equivalent = original.clone();
            equivalent.push(b'\n');
            assert_eq!(
                serde_json::from_slice::<Value>(&original).unwrap(),
                serde_json::from_slice::<Value>(&equivalent).unwrap()
            );
            fs::write(&path, &equivalent).unwrap();
            let error = guard
                .inputs
                .seal_campaigns(&fixture.0, &results, &sweep)
                .unwrap_err();
            assert!(
                error.to_string().contains("receipt bytes differ"),
                "{error}"
            );
            assert_eq!(fs::read(&path).unwrap(), equivalent);
        } else {
            // Pending members have no results yet; only previously completed
            // peers are verified. Do not manufacture receipts for pending slots.
            guard
                .inputs
                .seal_campaigns(&fixture.0, &results, &sweep)
                .unwrap();
            assert_eq!(fs::read(path).unwrap(), original);
            for id in [2, 4] {
                assert!(!guard.inputs.campaign_completion_path(0, id).exists());
            }
        }
    }
}

#[test]
fn resolved_generation_prepared_records_are_bounded_before_receipt_publication() {
    let (fixture, sweep, results, guard, workers) = published_mixed();
    let catalog = load(&guard, &sweep, &results).unwrap();
    let layout = sweep.read_campaigns(&results).unwrap().remove(0);
    let slot = &layout.plan.slots[1];
    let cohort = &workers[0].manifest.pending[0].cohort;
    let reference = catalog.reference(0).unwrap();
    let start = StartIntent {
        schema_version: 1,
        scope: "execution-start-intent-not-actual-attempt".into(),
        execution: layout.plan.execution(slot),
        execution_cohort: Some(cohort.clone()),
        generation: Some(reference.clone()),
    };
    fs::write(
        layout.root.join("batch-1/starts/2.json"),
        serde_json::to_vec(&start).unwrap(),
    )
    .unwrap();
    let bound = catalog
        .resolve(Some(&reference), 0, &layout.plan, slot, Some(cohort))
        .unwrap();
    let path = layout.root.join(bound.prepared);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 + 1)
        .unwrap();
    let original = fs::read(guard.inputs.campaign_completion_path(0, 1)).unwrap();
    let error = guard
        .inputs
        .seal_campaigns(&fixture.0, &results, &sweep)
        .unwrap_err();
    assert!(
        error.to_string().contains("bounded regular file"),
        "{error}"
    );
    assert_eq!(
        fs::read(guard.inputs.campaign_completion_path(0, 1)).unwrap(),
        original
    );
    assert!(!guard.inputs.campaign_completion_path(0, 2).exists());
}

#[test]
fn generation_slots_seal_beside_historical_peers_without_replacing_their_evidence() {
    let (fixture, sweep, results, guard, workers) = published_mixed();
    let catalog = load(&guard, &sweep, &results).unwrap();
    let layouts = sweep.read_campaigns(&results).unwrap();
    let layout = &layouts[0];
    let reference = catalog.reference(0).unwrap();
    let cohort = &workers[0].manifest.pending[0].cohort;
    let old: Vec<_> = workers
        .iter()
        .flat_map(|worker| {
            worker.manifest.completed.iter().map(|receipt| {
                let path = guard
                    .inputs
                    .campaign_completion_path(worker.manifest.configuration, receipt.slot);
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
        })
        .collect();
    let source: Value = serde_json::from_slice(
        &fs::read(layout.root.join("batch-1/.asb-completions/fixture-1.json")).unwrap(),
    )
    .unwrap();
    for replica in [2, 4] {
        let slot = &layout.plan.slots[replica - 1];
        let task = crate::workloads::TaskIdentity::local("bug-fix", None);
        let workspace = layout.root.join(&slot.workspace);
        let log = layout.root.join(&slot.log);
        fs::create_dir_all(&workspace).unwrap();
        fs::write(&log, "new generation fixture log").unwrap();
        let start = StartIntent {
            schema_version: 1,
            scope: "execution-start-intent-not-actual-attempt".into(),
            execution: layout.plan.execution(slot),
            execution_cohort: Some(cohort.clone()),
            generation: Some(reference.clone()),
        };
        fs::write(
            layout.root.join(format!("batch-1/starts/{replica}.json")),
            serde_json::to_vec(&start).unwrap(),
        )
        .unwrap();
        let binding = catalog
            .resolve(
                start.generation.as_ref(),
                0,
                &layout.plan,
                slot,
                Some(cohort),
            )
            .unwrap();
        let prepared = layout.root.join(binding.prepared);
        if !prepared.exists() {
            fs::create_dir_all(prepared.parent().unwrap()).unwrap();
            fs::copy(layout.root.join("batch-1/prepared.json"), &prepared).unwrap();
        }
        // This is synthetic current-protocol outcome evidence, not an agent
        // execution. Use the real fingerprint constructor instead of disabling
        // the dispatch/hash invariant or guessing a historical fingerprint.
        let mut config = layout.plan.configuration.clone();
        config
            .planning
            .get_or_insert_with(Default::default)
            .repetition = crate::planning::Repetition { index: 1, total: 2 };
        let policy = crate::planning::resolve(&config).unwrap();
        let effective = crate::planning::EffectiveBudgets {
            execution_seconds: policy
                .budgets
                .execution_seconds
                .or(policy.default_budgets.execution_seconds)
                .map(|value| value as f64),
            environment_preparation_seconds: policy
                .budgets
                .environment_preparation_seconds
                .or(policy.default_budgets.environment_preparation_seconds),
            evaluation_seconds: policy
                .budgets
                .evaluation_seconds
                .or(policy.default_budgets.evaluation_seconds),
            ..Default::default()
        };
        let fingerprint = crate::planning::protocol_fingerprint_with_dispatch(
            &config,
            &policy,
            effective,
            4,
            Some(layout.plan.admission),
            cohort.dispatch,
        )
        .unwrap();
        let mut outcome = source.clone();
        outcome["replica"] = json!({"index":replica,"total":4});
        outcome["outputs"] = json!([workspace, log]);
        let summary = &mut outcome["summary"];
        summary["task"] = json!(task);
        summary["execution"] = json!(layout.plan.execution(slot));
        summary["execution_cohort"] = json!(cohort);
        summary["replica_index"] = json!(replica);
        summary["planning"] = json!(policy);
        summary["effective_budgets"] = json!(effective);
        summary["execution_plan_fingerprint"] = json!(fingerprint.as_str());
        let bytes = serde_json::to_vec(&outcome).unwrap();
        let path = PathBuf::from(format!("batch-1/.asb-completions/new-{replica}.json"));
        fs::write(layout.root.join(&path), &bytes).unwrap();
        fs::write(layout.root.join(format!("batch-1/outcomes/{replica}.json")), serde_json::to_vec(&json!({
            "schema_version":1,"execution":layout.plan.execution(slot),"path":path,"bytes":bytes.len(),
            "sha256":crate::cassette::digest(&bytes),"task":task
        })).unwrap()).unwrap();
    }
    guard
        .inputs
        .seal_campaigns(&fixture.0, &results, &sweep)
        .unwrap();
    for (path, bytes) in old {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    for id in [2, 4] {
        let value: Value = serde_json::from_slice(
            &fs::read(guard.inputs.campaign_completion_path(0, id)).unwrap(),
        )
        .unwrap();
        let roots = value["roots"].as_array().unwrap();
        assert!(roots.iter().any(|root| root["name"] == "generation-0"));
        assert!(roots.iter().any(|root| root["name"] == "generation-1"));
        assert!(roots.iter().any(|root| root["name"] == "prepared"
            && root["path"].as_str().unwrap().contains(".asb-generations")));
    }
    let Guard { inputs, owners } = guard;
    drop(owners);
    let reused = inputs
        .resume_campaign_outcomes(&fixture.0, &results, &sweep)
        .unwrap();
    assert_eq!(reused.groups.len(), 2);
    assert!(reused.groups.values().all(|rows| rows.len() == 8));
}
