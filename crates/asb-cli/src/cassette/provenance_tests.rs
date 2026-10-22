//! Artifact-only union imports, immutable old facts and explicit recovery.
use super::*;

fn origin(tracks: &[Track], population: u64) -> OriginPlan {
    OriginPlan {
        schema_version: 1,
        population,
        sources: vec![RecordingSource {
            recording_id: "historical-source".into(),
            manifest_sha256: digest(b"source manifest"),
        }],
        imports: tracks
            .iter()
            .map(|track| ImportedTrack {
                slot: track.id,
                source: 0,
                metadata_sha256: track_metadata_digest(track).unwrap(),
                bytes: track.bytes,
            })
            .collect(),
    }
}

fn journal(fixture: &Fixture, tracks: &[Track], population: u64) -> RecordingJournal {
    let mut identity = manifest(0);
    identity.recording_id = "new-physical-union".into();
    identity.provenance = Some(origin(tracks, population));
    RecordingJournal::create(
        &fixture.0.join("union.zip"),
        identity,
        population,
        Limits::default(),
    )
    .unwrap()
}

struct NoRead;
impl Read for NoRead {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("invalid metadata must fail before reading imported bytes")
    }
}

#[test]
fn exact_union_import_is_self_contained_and_preserves_failed_unknown_observations() {
    let fixture = Fixture::new();
    let mut old = manifest(1).tracks.remove(0);
    old.process_success = Some(false);
    old.workload_success = None;
    old.statistics.tokens = None;
    old.statistics.steps = None;
    let mut historical = manifest(0);
    historical.recording_id = "historical-source".into();
    let old_path = fixture.0.join("old.zip");
    let mut writer = RecordingJournal::create(&old_path, historical, 1, Limits::default()).unwrap();
    writer.begin(1, 1).unwrap();
    writer.commit(old.clone(), &mut &b"{}\n"[..]).unwrap();
    writer.finalize().unwrap();
    let old_bytes = fs::read(&old_path).unwrap();
    let mut source = Archive::open(&old_path, Limits::default()).unwrap();
    let mut identity = manifest(0);
    identity.recording_id = "new-physical-union".into();
    let mut plan = origin(std::slice::from_ref(&old), 2);
    plan.sources[0].manifest_sha256 = digest(&serde_json::to_vec(source.manifest()).unwrap());
    identity.provenance = Some(plan.clone());
    let destination = fixture.0.join("union.zip");
    let mut union = RecordingJournal::create(&destination, identity, 2, Limits::default()).unwrap();
    union
        .import_track(
            old.clone(),
            0,
            &mut source.read_track(1).unwrap().as_slice(),
        )
        .unwrap();
    let new = manifest(2).tracks.remove(1);
    union.begin(2, 1).unwrap();
    union.commit(new.clone(), &mut &b"{}\n"[..]).unwrap();
    let proof = union.track_evidence(&old).unwrap();
    proof.verify(Limits::default()).unwrap();
    union.finalize().unwrap();
    proof.verify(Limits::default()).unwrap();
    assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
    drop(source);
    drop(writer);
    fs::rename(&old_path, fixture.0.join("ancestor-not-available.zip")).unwrap();
    let mut standalone = Archive::open(&destination, Limits::default()).unwrap();
    assert_eq!(standalone.manifest().provenance, Some(plan));
    assert_eq!(standalone.manifest().tracks, vec![old, new]);
    assert_eq!(standalone.read_track(1).unwrap(), b"{}\n");
    assert_eq!(standalone.read_track(2).unwrap(), b"{}\n");
}

#[test]
fn imports_reject_changed_metadata_foreign_slots_and_local_execution_before_payload() {
    let fixture = Fixture::new();
    let old = manifest(1).tracks.remove(0);
    let mut union = journal(&fixture, std::slice::from_ref(&old), 2);
    assert!(union.begin(1, 1).is_err());
    assert!(union.commit(old.clone(), &mut NoRead).is_err());
    assert!(union.import_track(old.clone(), 1, &mut NoRead).is_err());
    for mutation in 0..7 {
        let mut changed = old.clone();
        match mutation {
            0 => changed.statistics.tokens = None,
            1 => changed.workload_success = Some(false),
            2 => changed.complete = false,
            3 => changed.sha256 = digest(b"other"),
            4 => changed.bytes += 1,
            5 => changed.id = 2,
            _ => changed.replica = 2,
        }
        assert!(
            union.import_track(changed, 0, &mut NoRead).is_err(),
            "mutation {mutation}"
        );
    }
    assert!(!union.partial_directory().join("started-1.json").exists());
    union
        .import_track(old.clone(), 0, &mut &b"{}\n"[..])
        .unwrap();
    assert!(union.import_track(old.clone(), 0, &mut NoRead).is_err());
    assert!(union.resume_import(old, 0, &mut NoRead).is_err());
}

#[test]
fn import_intent_fault_recovers_without_execution_and_retains_partial_payloads() {
    for retained_payload in [false, true] {
        let fixture = Fixture::new();
        let old = manifest(1).tracks.remove(0);
        let mut union = journal(&fixture, std::slice::from_ref(&old), 1);
        union.inject_publication_failure("started-1.json:after-link");
        assert!(union.import_track(old.clone(), 0, &mut NoRead).is_err());
        let root = union.partial_directory().to_path_buf();
        let destination = union.destination().to_path_buf();
        assert!(root.join("started-1.json").is_file());
        assert!(!root.join("track-1.jsonl").exists());
        drop(union);
        if retained_payload {
            fs::write(root.join("track-1.jsonl"), b"{").unwrap();
        }
        let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
        assert!(recovered.import_track(old.clone(), 0, &mut NoRead).is_err());
        assert!(recovered.begin(1, 1).is_err());
        if retained_payload {
            assert!(recovered.resume_import(old, 0, &mut NoRead).is_err());
            assert_eq!(fs::read(root.join("track-1.jsonl")).unwrap(), b"{");
        } else {
            recovered
                .resume_import(old.clone(), 0, &mut &b"{}\n"[..])
                .unwrap();
            assert!(
                recovered
                    .resume_import(old.clone(), 0, &mut NoRead)
                    .is_err()
            );
            assert_eq!(recovered.finalize().unwrap().manifest.tracks, vec![old]);
        }
    }
}

#[test]
fn absent_import_cannot_be_frozen_or_published() {
    let fixture = Fixture::new();
    let old = manifest(1).tracks.remove(0);
    let mut union = journal(&fixture, &[old], 2);
    union.begin(2, 1).unwrap();
    union
        .commit(manifest(2).tracks.remove(1), &mut &b"{}\n"[..])
        .unwrap();
    assert!(union.finalize().is_err());
    assert!(!union.destination().exists());
    assert!(!union.partial_directory().join("finalizing.json").exists());
}

#[test]
fn imported_payload_commit_faults_recover_exact_bytes_without_reading_source() {
    for point in [
        "committed-1.json:before-write",
        "committed-1.json:before-link",
        "committed-1.json:after-link",
    ] {
        let fixture = Fixture::new();
        let old = manifest(1).tracks.remove(0);
        let mut union = journal(&fixture, std::slice::from_ref(&old), 1);
        union.inject_publication_failure(point);
        assert!(
            union
                .import_track(old.clone(), 0, &mut &b"{}\n"[..])
                .is_err()
        );
        let destination = union.destination().to_path_buf();
        let root = union.partial_directory().to_path_buf();
        let payload = fs::read(root.join("track-1.jsonl")).unwrap();
        let start = fs::read(root.join("started-1.json")).unwrap();
        drop(union);
        let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
        if point.ends_with("after-link") {
            assert!(
                recovered
                    .resume_import(old.clone(), 0, &mut NoRead)
                    .is_err()
            );
        } else {
            recovered
                .resume_import(old.clone(), 0, &mut NoRead)
                .unwrap();
        }
        assert_eq!(fs::read(root.join("track-1.jsonl")).unwrap(), payload);
        assert_eq!(fs::read(root.join("started-1.json")).unwrap(), start);
        assert_eq!(recovered.finalize().unwrap().manifest.tracks, vec![old]);
    }
}

#[test]
fn altered_import_intent_and_frozen_origin_are_rejected() {
    for mutation in 0..3 {
        let fixture = Fixture::new();
        let old = manifest(1).tracks.remove(0);
        let mut union = journal(&fixture, std::slice::from_ref(&old), 1);
        union
            .import_track(old.clone(), 0, &mut &b"{}\n"[..])
            .unwrap();
        let proof = union.track_evidence(&old).unwrap();
        let root = union.partial_directory().to_path_buf();
        let destination = union.destination().to_path_buf();
        if mutation < 2 {
            let path = root.join("started-1.json");
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if mutation == 0 {
                value.as_object_mut().unwrap().remove("import_source");
            } else {
                value["import_source"] = 1.into();
            }
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            crate::cassette::journal::take_observations();
            assert!(proof.verify(Limits::default()).is_err());
            assert!(
                !crate::cassette::journal::take_observations()
                    .iter()
                    .any(|(event, _)| matches!(*event, "staged" | "payload"))
            );
        } else {
            union.finalize().unwrap();
            let path = root.join("intent.json");
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            value["manifest"]["provenance"]["sources"][0]["manifest_sha256"] =
                digest(b"edited ancestor").into();
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(proof.verify(Limits::default()).is_err());
        }
        drop(union);
        assert!(
            RecordingJournal::recover(&destination, Limits::default()).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn recovered_import_sync_failure_cannot_publish_commit_or_consume_source() {
    let fixture = Fixture::new();
    let old = manifest(1).tracks.remove(0);
    let mut union = journal(&fixture, std::slice::from_ref(&old), 1);
    union.inject_publication_failure("committed-1.json:before-write");
    assert!(
        union
            .import_track(old.clone(), 0, &mut &b"{}\n"[..])
            .is_err()
    );
    let destination = union.destination().to_path_buf();
    let root = union.partial_directory().to_path_buf();
    drop(union);
    let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
    recovered.inject_publication_failure("before-import-recovery-sync");
    assert!(
        recovered
            .resume_import(old.clone(), 0, &mut NoRead)
            .unwrap_err()
            .0
            .contains("before-import-recovery-sync")
    );
    assert!(!root.join("committed-1.json").exists());
    assert_eq!(fs::read(root.join("track-1.jsonl")).unwrap(), b"{}\n");
    drop(recovered);
    let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
    recovered
        .resume_import(old.clone(), 0, &mut NoRead)
        .unwrap();
    assert_eq!(recovered.finalize().unwrap().manifest.tracks, vec![old]);
}

#[test]
fn origin_validation_rejects_identity_order_population_and_budget_errors() {
    let tracks = manifest(2).tracks;
    let original = origin(&tracks, 2);
    original.validate(1, Limits::default()).unwrap();
    for mutation in 0..9 {
        let mut plan = original.clone();
        match mutation {
            0 => plan.schema_version += 1,
            1 => plan.population = 0,
            2 => plan.imports.swap(0, 1),
            3 => plan.imports[1].slot = 1,
            4 => plan.imports[0].source = 1,
            5 => plan.sources.push(plan.sources[0].clone()),
            6 => plan.imports[0].metadata_sha256 = "bad".into(),
            7 => plan.imports[0].bytes = Limits::default().max_track_bytes + 1,
            _ => plan.sources[0].recording_id = "invalid/source".into(),
        }
        assert!(
            plan.validate(1, Limits::default()).is_err(),
            "mutation {mutation}"
        );
    }
    assert!(original.validate(3, Limits::default()).is_err());
    assert!(
        original
            .validate(
                1,
                Limits {
                    max_total_bytes: 5,
                    ..Limits::default()
                }
            )
            .is_err()
    );
    assert!(
        original
            .validate(
                1,
                Limits {
                    max_manifest_bytes: 1,
                    ..Limits::default()
                }
            )
            .is_err()
    );
    let mut identity = manifest(0);
    identity.recording_id = original.sources[0].recording_id.clone();
    identity.provenance = Some(original);
    assert!(identity.validate(Limits::default()).is_err());
}

#[test]
fn readonly_proof_rejects_different_origin_population_before_any_payload_read() {
    let fixture = Fixture::new();
    let old = manifest(1).tracks.remove(0);
    let mut union = journal(&fixture, std::slice::from_ref(&old), 2);
    union
        .import_track(old.clone(), 0, &mut &b"{}\n"[..])
        .unwrap();
    let proof = union.track_evidence(&old).unwrap();
    let intent = union.partial_directory().join("intent.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&intent).unwrap()).unwrap();
    value["manifest"]["provenance"]["population"] = 3.into();
    fs::write(intent, serde_json::to_vec(&value).unwrap()).unwrap();
    crate::cassette::journal::take_observations();
    assert!(
        proof
            .verify(Limits::default())
            .unwrap_err()
            .0
            .contains("intent")
    );
    let observed = crate::cassette::journal::take_observations();
    assert!(
        !observed
            .iter()
            .any(|(event, _)| matches!(*event, "staged" | "payload"))
    );
}

#[test]
fn origin_commitment_respects_custom_limits_and_matches_canonical_wire() {
    // Large but valid metadata exceeds the default 16 MiB policy. Hashing must
    // not silently reapply that policy after the owner admits a larger limit.
    let count = 50_000u64;
    let mut plan = OriginPlan {
        schema_version: 1,
        population: count,
        sources: Vec::new(),
        imports: Vec::new(),
    };
    for slot in 1..=count {
        plan.sources.push(RecordingSource {
            recording_id: format!("source-{slot:06}-{}", "x".repeat(100)),
            manifest_sha256: digest(b"source"),
        });
        plan.imports.push(ImportedTrack {
            slot,
            source: (slot - 1) as usize,
            metadata_sha256: digest(b"metadata"),
            bytes: 0,
        });
    }
    let encoded = serde_json::to_vec(&plan).unwrap();
    assert!(encoded.len() as u64 > Limits::default().max_manifest_bytes);
    assert!(plan.validate(1, Limits::default()).is_err());
    plan.validate(
        1,
        Limits {
            max_manifest_bytes: 32 * 1024 * 1024,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(plan.commitment().unwrap(), digest(&encoded));
}

#[test]
fn legacy_manifest_and_start_wire_still_omit_origin_metadata() {
    let original = manifest(1);
    let encoded = serde_json::to_string(&original).unwrap();
    assert!(!encoded.contains("provenance"));
    let decoded: Manifest = serde_json::from_str(&encoded).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    let fixture = Fixture::new();
    let mut legacy = RecordingJournal::create(
        &fixture.0.join("old.zip"),
        manifest(0),
        1,
        Limits::default(),
    )
    .unwrap();
    legacy.begin(1, 1).unwrap();
    assert_eq!(
        fs::read_to_string(legacy.partial_directory().join("started-1.json")).unwrap(),
        r#"{"recording_id":"fixture-recording-1","id":1,"batch":1,"replica":1}"#
    );
}
