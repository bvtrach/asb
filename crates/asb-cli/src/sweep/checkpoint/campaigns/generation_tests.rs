//! Publication-only qualification. No agent/provider/evaluator is started.
use super::*;
use crate::sweep::checkpoint::campaigns::tests::settlement_tests::fixture_concurrency;
use crate::sweep::checkpoint::campaigns::tests::{Fixture, published, reservations_tests};

#[path = "generation_resolution_tests.rs"]
mod resolution;

fn inputs(results: &Path, sweep: &SweepPlan) -> Inputs {
    let mut inputs = reservations_tests::inputs(results, sweep);
    // Match real initial/final input hashing, including environment. The
    // reservation-only fixture deliberately omits environment from its hashes.
    inputs.initial = inputs
        .snapshotter
        .fingerprint(&inputs.roots, &inputs.context, true, &[])
        .unwrap();
    inputs
}

fn prepared() -> (Fixture, SweepPlan, PathBuf, Prepared) {
    let (fixture, sweep, results) = published(crate::tape::Mode::Replay);
    let state = inputs(&results, &sweep);
    state.publish("pending", None).unwrap();
    state.reserve_campaigns(&results, &sweep).unwrap();
    let classification = state
        .classify_campaigns(&fixture.0, &results, &sweep)
        .unwrap();
    let prepared = Prepared::plan(state, classification, &fixture.0, &results, &sweep).unwrap();
    (fixture, sweep, results, prepared)
}

fn no_starts(sweep: &SweepPlan, results: &Path) {
    for layout in sweep.read_campaigns(results).unwrap() {
        assert!(!layout.root.join("batch-1").exists());
        assert!(!layout.root.join("batch-2").exists());
        assert!(!layout.root.join(".asb-generations").exists());
    }
}

#[test]
fn consuming_publication_freezes_all_manifests_before_returning_send_capabilities() {
    fn assert_send<T: Send>() {}
    assert_send::<AdmittedCampaign>();
    let (_fixture, sweep, results, prepared) = prepared();
    let seal = fs::read(results.join(".checkpoint/inputs.json")).unwrap();
    let directory = root(&prepared.guard.inputs);
    let generation = prepared.admission.generation.clone();
    let frozen: Vec<_> = prepared
        .campaigns
        .iter()
        .map(|campaign| campaign.bytes.clone())
        .collect();
    assert!(!directory.exists());
    no_starts(&sweep, &results);
    let (guard, workers) = prepared
        .publish_with(&results, &sweep, |index| {
            assert!(directory.join(manifest_path(index)).is_file());
            assert!(!directory.join("admitted.json").exists());
            if index == 0 {
                assert!(!directory.join(manifest_path(1)).exists());
            }
            no_starts(&sweep, &results);
            Ok(())
        })
        .unwrap();
    assert_eq!(workers.len(), 2);
    assert_eq!(guard.owners.len(), 2);
    let admission: Admission =
        serde_json::from_slice(&fs::read(directory.join("admitted.json")).unwrap()).unwrap();
    assert_eq!(admission.manifests.len(), 2);
    assert_eq!(admission.generation, generation);
    for (index, worker) in workers.iter().enumerate() {
        assert!(worker.completed.is_empty());
        assert_eq!(worker.manifest.generation, generation);
        let bytes = fs::read(directory.join(manifest_path(index))).unwrap();
        assert_eq!(bytes, frozen[index]);
        assert!(worker.manifest_reference.matches(&bytes));
        assert!(admission.manifests[index].content.matches(&bytes));
        assert_eq!(worker.manifest.pending.len(), 2);
        for batch in &worker.manifest.pending {
            assert_eq!(batch.cohort.admitted_slots, [1]);
            assert_eq!(batch.cohort.selected_replicas, 1);
            let dispatch = batch.cohort.dispatch.unwrap();
            assert_eq!((dispatch.configured(), dispatch.admitted()), (1, 1));
            assert_eq!(
                batch.directory(&generation),
                PathBuf::from(".asb-generations")
                    .join(&generation)
                    .join(format!("batch-{}", batch.batch))
            );
        }
        assert!(
            crate::output_lock::OutputLock::acquire_existing(
                &worker.root.join(".owner.lock"),
                "test busy"
            )
            .is_err()
        );
    }
    assert_eq!(
        fs::read(results.join(".checkpoint/inputs.json")).unwrap(),
        seal
    );
    no_starts(&sweep, &results);
    drop(guard);
    for worker in workers {
        assert!(
            crate::output_lock::OutputLock::acquire_existing(
                &worker.root.join(".owner.lock"),
                "test free"
            )
            .is_ok()
        );
    }
}

#[test]
fn interrupted_publication_retains_exact_partial_generation_and_never_mints_another() {
    for stop in [0, 1] {
        let (fixture, sweep, results, prepared) = prepared();
        let directory = root(&prepared.guard.inputs);
        let frozen = prepared.campaigns[0].bytes.clone();
        let error = prepared
            .publish_with(&results, &sweep, |index| {
                if index == stop {
                    Err(Error::Io("injected publication interruption".into()))
                } else {
                    Ok(())
                }
            })
            .err()
            .expect("fault must return no capabilities");
        assert!(error.to_string().contains("injected"));
        assert_eq!(fs::read(directory.join(manifest_path(0))).unwrap(), frozen);
        assert!(!directory.join("admitted.json").exists());
        no_starts(&sweep, &results);
        let state = inputs(&results, &sweep);
        assert!(
            state
                .classify_campaigns(&fixture.0, &results, &sweep)
                .is_err()
        );
        assert_eq!(fs::read(directory.join(manifest_path(0))).unwrap(), frozen);
    }
}

#[test]
fn late_manifest_or_pending_mutation_cannot_publish_admission_or_replace_foreign_bytes() {
    for mutation in ["manifest", "pending", "marker"] {
        let (_fixture, sweep, results, prepared) = prepared();
        let directory = root(&prepared.guard.inputs);
        let layout = sweep.read_campaigns(&results).unwrap().pop().unwrap();
        let changed = if mutation == "manifest" {
            directory.join(manifest_path(0))
        } else if mutation == "marker" {
            directory.join("admitted.json")
        } else {
            layout.root.join(&layout.plan.slots[1].log)
        };
        let result = prepared.publish_with(&results, &sweep, |index| {
            if index == 1 {
                if let Some(parent) = changed.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&changed, b"foreign sentinel").unwrap();
            }
            Ok(())
        });
        assert!(result.is_err(), "{mutation}");
        assert_eq!(fs::read(&changed).unwrap(), b"foreign sentinel");
        if mutation != "marker" {
            assert!(!directory.join("admitted.json").exists());
        }
    }
}

#[test]
fn classification_cannot_be_consumed_with_another_input_owner_or_changed_index() {
    for mutation in ["owner", "index", "invocation"] {
        let (fixture, sweep, results, prepared) = prepared();
        let Prepared { guard, .. } = prepared;
        // Reclassify while retaining this original owner only after releasing
        // the previous classification's campaign locks.
        let Guard { mut inputs, owners } = guard;
        drop(owners);
        let classification = inputs
            .classify_campaigns(&fixture.0, &results, &sweep)
            .unwrap();
        if mutation == "owner" {
            inputs.initial.digest = "0".repeat(64);
        } else if mutation == "index" {
            let mut bytes = fs::read(results.join("plan.json")).unwrap();
            bytes.push(b'\n');
            fs::write(results.join("plan.json"), bytes).unwrap();
        }
        let parent = if mutation == "invocation" {
            fixture.0.join("foreign-project")
        } else {
            fixture.0.clone()
        };
        assert!(Prepared::plan(inputs, classification, &parent, &results, &sweep).is_err());
        assert!(!results.join(".checkpoint").join(ROOT).exists());
        no_starts(&sweep, &results);
    }
}

fn mixed() -> (Fixture, SweepPlan, PathBuf, Prepared) {
    let (fixture, sweep, results, inputs) =
        fixture_concurrency(crate::tape::Mode::Replay, false, false, 4);
    let layouts = sweep.read_campaigns(&results).unwrap();
    let layout = &layouts[0];
    // Synthetic evidence fixtures, not real past executions. Return two
    // original slots to the positively untouched state BEFORE initial sealing.
    for replica in [2, 4] {
        let slot = &layout.plan.slots[replica - 1];
        fs::remove_dir_all(layout.root.join(&slot.workspace)).unwrap();
        for path in [
            slot.log.clone(),
            PathBuf::from(format!("batch-1/starts/{replica}.json")),
            PathBuf::from(format!("batch-1/outcomes/{replica}.json")),
            PathBuf::from(format!("batch-1/.asb-completions/fixture-{replica}.json")),
        ] {
            fs::remove_file(layout.root.join(path)).unwrap();
        }
    }
    inputs.seal_campaigns(&fixture.0, &results, &sweep).unwrap();
    let classification = inputs
        .classify_campaigns(&fixture.0, &results, &sweep)
        .unwrap();
    let prepared = Prepared::plan(inputs, classification, &fixture.0, &results, &sweep).unwrap();
    (fixture, sweep, results, prepared)
}

#[test]
fn mixed_c4_generation_preserves_original_slots_receipts_and_wholly_completed_configuration() {
    let (fixture, sweep, results, prepared) = mixed();
    assert_eq!(
        prepared.campaigns[0]
            .manifest
            .completed
            .iter()
            .map(|receipt| receipt.slot)
            .collect::<Vec<_>>(),
        [1, 3, 5, 6, 7, 8]
    );
    assert_eq!(prepared.campaigns[1].manifest.completed.len(), 8);
    assert!(prepared.campaigns[1].manifest.pending.is_empty());
    let batch = &prepared.campaigns[0].manifest.pending[0];
    assert_eq!(batch.batch, 1);
    assert_eq!(batch.cohort.admitted_slots, [2, 4]);
    let cohort = batch.cohort.clone();
    assert_eq!(cohort.selected_replicas, 4);
    assert_eq!(cohort.dispatch.unwrap().admitted(), 2);
    let old: Vec<_> = prepared
        .campaigns
        .iter()
        .flat_map(|campaign| {
            campaign.manifest.completed.iter().map(|receipt| {
                let path = prepared
                    .guard
                    .inputs
                    .campaign_completion_path(campaign.manifest.configuration, receipt.slot);
                let bytes = fs::read(&path).unwrap();
                assert!(receipt.receipt.matches(&bytes));
                (path, bytes)
            })
        })
        .collect();
    let (_guard, workers) = prepared.publish(&results, &sweep).unwrap();
    assert_eq!(workers[0].manifest.pending[0].cohort, cohort);
    assert_eq!(
        workers[0].completed.keys().copied().collect::<Vec<_>>(),
        [1, 3, 5, 6, 7, 8]
    );
    for (index, worker) in workers.iter().enumerate() {
        assert_eq!(
            worker.invocation,
            fixture.0.join(&sweep.entries[index].reference)
        );
        for value in worker.completed.values() {
            assert_eq!(value["workload_success"], false);
        }
    }
    for (path, bytes) in old {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn completed_receipt_and_actual_evidence_are_rechecked_after_manifest_publication() {
    for mutation in ["receipt", "actual", "workspace"] {
        let (_fixture, sweep, results, prepared) = mixed();
        let directory = root(&prepared.guard.inputs);
        let changed = if mutation == "receipt" {
            prepared.guard.inputs.campaign_completion_path(1, 8)
        } else if mutation == "actual" {
            prepared.campaigns[1]
                .root
                .join("batch-2/.asb-completions/fixture-4.json")
        } else {
            prepared.campaigns[1]
                .root
                .join(&prepared.campaigns[1].plan.slots[7].workspace)
                .join("late-edit")
        };
        let result = prepared.publish_with(&results, &sweep, |index| {
            if index == 1 {
                fs::write(&changed, b"changed retained evidence").unwrap();
            }
            Ok(())
        });
        assert!(result.is_err(), "{mutation}");
        assert!(!directory.join("admitted.json").exists());
        assert_eq!(fs::read(changed).unwrap(), b"changed retained evidence");
    }
}

#[test]
fn bare_filename_parent_and_explicit_dot_bind_the_same_invocation_directory() {
    let (_fixture, sweep, results, prepared) = prepared();
    let inputs = &prepared.guard.inputs;
    let empty_parent = Path::new("sweep.toml").parent().unwrap();
    assert!(empty_parent.as_os_str().is_empty());
    let bare = Origin::read(inputs, empty_parent, &results, &sweep).unwrap();
    let explicit = Origin::read(inputs, Path::new("."), &results, &sweep).unwrap();
    assert_eq!(bare, explicit);
    assert_eq!(bare.invocation_parent, std::env::current_dir().unwrap());
}
