//! Read-only grouped proof limits, selective payloads and publication races.
use super::*;

fn readonly_recording(
    fixture: &Fixture,
    id: &str,
    count: u64,
) -> (RecordingJournal, Vec<TrackEvidence>) {
    let mut identity = manifest(0);
    identity.recording_id = id.into();
    let mut journal = RecordingJournal::create(
        &fixture.0.join("readonly.zip"),
        identity,
        count,
        Limits::default(),
    )
    .unwrap();
    for track in manifest(count).tracks {
        journal.begin(track.batch, 1).unwrap();
        journal.commit(track, &mut &b"{}\n"[..]).unwrap();
    }
    let proofs = journal
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .map(|track| journal.track_evidence(track).unwrap())
        .collect();
    (journal, proofs)
}

fn proof_counts() -> std::collections::BTreeMap<&'static str, u64> {
    crate::cassette::journal::take_observations()
        .into_iter()
        .filter(|(event, _)| matches!(*event, "intent" | "archive" | "payload" | "staged"))
        .collect()
}

fn assert_no_track_reads() {
    let counts = proof_counts();
    assert_eq!(counts.get("payload").copied().unwrap_or(0), 0, "{counts:?}");
    assert_eq!(counts.get("staged").copied().unwrap_or(0), 0, "{counts:?}");
}

#[test]
fn readonly_batch_deduplicates_payloads_and_bounds_index_passes_per_recording() {
    let first = Fixture::new();
    let second = Fixture::new();
    let (mut a, mut proofs) = readonly_recording(&first, "readonly-a", 3);
    let (mut b, others) = readonly_recording(&second, "readonly-b", 2);
    // Immutable selected staging is readable while the writer still owns it.
    proof_counts();
    TrackEvidence::verify_batch(
        &[proofs[0].clone(), proofs[0].clone()],
        ProofLimits::default(),
    )
    .unwrap();
    assert_eq!(
        proof_counts(),
        std::collections::BTreeMap::from([("intent", 1), ("staged", 1)])
    );
    a.finalize().unwrap();
    b.finalize().unwrap();
    proofs.extend(others);
    proofs.push(proofs[0].clone());
    proof_counts();
    TrackEvidence::verify_batch(&proofs, ProofLimits::default()).unwrap();
    assert_eq!(
        proof_counts(),
        std::collections::BTreeMap::from([("intent", 2), ("archive", 4), ("payload", 5)])
    );
    for proof in &proofs {
        proof.verify(Limits::default()).unwrap();
    }
}

#[test]
fn readonly_batch_rejects_conflicting_references_and_global_bounds_before_artifact_reads() {
    let fixture = Fixture::new();
    let (_journal, proofs) = readonly_recording(&fixture, "readonly-bounds", 2);
    for mutate in 0..5 {
        let mut other = proofs[0].clone();
        match mutate {
            0 => other.track.statistics.tokens = Some(999),
            1 => other.compatibility.model = "different-model".into(),
            2 => other.recording_id = "different-recording".into(),
            3 => {
                other.concurrency = 2;
            }
            4 => other.destination = fixture.0.join("alias.zip"),
            _ => unreachable!(),
        }
        proof_counts();
        assert!(
            TrackEvidence::verify_batch(&[proofs[0].clone(), other], ProofLimits::default())
                .is_err(),
            "mutation {mutate}"
        );
        assert!(
            proof_counts().is_empty(),
            "mutation {mutate} opened artifacts"
        );
    }
    let reference_bytes: u64 = proofs
        .iter()
        .map(|proof| serde_json::to_vec(proof).unwrap().len() as u64)
        .sum();
    for limits in [
        ProofLimits {
            max_selected_bytes: 5,
            ..ProofLimits::default()
        },
        ProofLimits {
            max_reference_bytes: reference_bytes - 1,
            ..ProofLimits::default()
        },
        ProofLimits {
            max_references: 1,
            ..ProofLimits::default()
        },
    ] {
        proof_counts();
        assert!(TrackEvidence::verify_batch(&proofs, limits).is_err());
        assert!(proof_counts().is_empty());
    }
}

#[test]
fn readonly_batch_checks_late_recording_metadata_before_earlier_payloads() {
    let first = Fixture::new();
    let second = Fixture::new();
    let (mut a, mut proofs) = readonly_recording(&first, "readonly-first", 1);
    let (b, others) = readonly_recording(&second, "readonly-last", 1);
    a.finalize().unwrap();
    proofs.extend(others);
    let committed = b.partial_directory().join("committed-1.json");
    let original = fs::read(&committed).unwrap();
    for changed in [b"invalid-json".to_vec(), vec![b' '; 64 * 1024 + 1]] {
        fs::write(&committed, changed).unwrap();
        proof_counts();
        assert!(TrackEvidence::verify_batch(&proofs, ProofLimits::default()).is_err());
        assert_no_track_reads();
    }
    fs::write(committed, original).unwrap();
    TrackEvidence::verify_batch(&proofs, ProofLimits::default()).unwrap();
}

#[test]
fn readonly_batch_selected_integrity_does_not_decode_unselected_corruption() {
    let fixture = Fixture::new();
    let (mut journal, proofs) = readonly_recording(&fixture, "readonly-selective", 3);
    let publication = journal.finalize().unwrap();
    let mut archive = zip::ZipArchive::new(File::open(&publication.destination).unwrap()).unwrap();
    let offset = archive
        .by_name("tracks/3.jsonl")
        .unwrap()
        .data_start()
        .unwrap() as usize;
    let mut bytes = fs::read(&publication.destination).unwrap();
    bytes[offset] = 0xff;
    fs::write(&publication.destination, bytes).unwrap();
    proof_counts();
    TrackEvidence::verify_batch(&proofs[..2], ProofLimits::default()).unwrap();
    assert_eq!(proof_counts().get("payload"), Some(&2));
    assert!(TrackEvidence::verify_batch(&proofs, ProofLimits::default()).is_err());
}

#[test]
fn readonly_batch_switches_once_when_publication_removes_staged_payloads() {
    let fixture = Fixture::new();
    let (journal, proofs) = readonly_recording(&fixture, "readonly-race", 3);
    let mut journal = Some(journal);
    crate::cassette::journal::set_hook(Some(Box::new(move |event| {
        if event == "staged"
            && let Some(mut journal) = journal.take()
        {
            journal.finalize().unwrap();
        }
    })));
    proof_counts();
    let result = TrackEvidence::verify_batch(&proofs, ProofLimits::default());
    crate::cassette::journal::set_hook(None);
    result.unwrap();
    assert_eq!(
        proof_counts(),
        std::collections::BTreeMap::from([
            ("intent", 1),
            ("staged", 1),
            ("archive", 1),
            ("payload", 3)
        ])
    );
}

#[test]
fn readonly_batch_never_falls_back_to_staging_after_published_target_loss() {
    let fixture = Fixture::new();
    let (mut journal, proofs) = readonly_recording(&fixture, "readonly-lost", 2);
    let root = journal.partial_directory().to_path_buf();
    let publication = journal.finalize().unwrap();
    // Leftover valid JSONL must not recreate or substitute the confirmed target.
    for proof in &proofs {
        fs::write(
            root.join(format!("track-{}.jsonl", proof.track.id)),
            b"{}\n",
        )
        .unwrap();
    }
    let retained = fixture.0.join("retained.zip");
    fs::rename(&publication.destination, &retained).unwrap();
    proof_counts();
    assert!(TrackEvidence::verify_batch(&proofs, ProofLimits::default()).is_err());
    assert_no_track_reads();
    assert!(!publication.destination.exists());
    assert_eq!(fs::read(root.join("track-1.jsonl")).unwrap(), b"{}\n");
    assert!(retained.exists());
}

#[test]
fn readonly_batch_supports_more_than_256_recordings_without_retaining_archive_handles() {
    let mut fixtures = Vec::new();
    let mut proofs = Vec::new();
    for id in 0..257 {
        let fixture = Fixture::new();
        let (mut journal, selected) = readonly_recording(&fixture, &format!("many-{id}"), 1);
        journal.finalize().unwrap();
        proofs.extend(selected);
        fixtures.push(fixture);
    }
    proof_counts();
    TrackEvidence::verify_batch(&proofs, ProofLimits::default()).unwrap();
    assert_eq!(
        proof_counts(),
        std::collections::BTreeMap::from([("intent", 257), ("archive", 514), ("payload", 257)])
    );
}

#[test]
fn readonly_batch_uses_the_descriptor_checked_at_open_not_a_replaced_path() {
    let fixture = Fixture::new();
    let (mut journal, proofs) = readonly_recording(&fixture, "readonly-descriptor", 1);
    let publication = journal.finalize().unwrap();
    let destination = publication.destination;
    let original = fs::read(&destination).unwrap();
    let replacement = destination.clone();
    let retained = fixture.0.join("admitted-original.zip");
    let mut opens = 0;
    crate::cassette::journal::set_hook(Some(Box::new(move |event| {
        if event == "archive-opened" {
            opens += 1;
            if opens == 2 {
                fs::rename(&replacement, &retained).unwrap();
                fs::write(
                    &replacement,
                    b"foreign replacement after descriptor admission",
                )
                .unwrap();
            }
        }
    })));
    let result = TrackEvidence::verify_batch(&proofs, ProofLimits::default());
    crate::cassette::journal::set_hook(None);
    result.unwrap();
    assert_eq!(
        fs::read(fixture.0.join("admitted-original.zip")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"foreign replacement after descriptor admission"
    );
    assert!(TrackEvidence::verify_batch(&proofs, ProofLimits::default()).is_err());
}

#[test]
fn readonly_batch_rejects_changed_publication_between_metadata_and_payload_passes() {
    for mutation in 0..4 {
        let fixture = Fixture::new();
        let (mut journal, proofs) = readonly_recording(&fixture, "readonly-between-passes", 1);
        let marker = journal.partial_directory().join("published.json");
        let destination = journal.finalize().unwrap().destination;
        crate::cassette::journal::set_hook(Some(Box::new(move |event| {
            if event == "metadata-ready" {
                match mutation {
                    0 => {
                        let mut receipt: serde_json::Value =
                            serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
                        receipt["archive_sha256"] = serde_json::json!(digest(b"changed receipt"));
                        fs::write(&marker, serde_json::to_vec(&receipt).unwrap()).unwrap();
                    }
                    1 => {
                        fs::rename(&destination, destination.with_extension("retained")).unwrap();
                    }
                    2 => {
                        let mut file = fs::OpenOptions::new()
                            .append(true)
                            .open(&destination)
                            .unwrap();
                        file.write_all(b"changed length").unwrap();
                    }
                    3 => {
                        fs::write(&marker, b"malformed receipt").unwrap();
                    }
                    _ => unreachable!(),
                }
            }
        })));
        proof_counts();
        let result = TrackEvidence::verify_batch(&proofs, ProofLimits::default());
        crate::cassette::journal::set_hook(None);
        assert!(result.is_err(), "mutation {mutation}");
        assert_no_track_reads();
    }
}

#[test]
fn readonly_batch_global_archive_directory_and_metadata_limits_fail_before_payloads() {
    let first = Fixture::new();
    let second = Fixture::new();
    let mut proofs = Vec::new();
    let mut compressed = 0;
    let mut directories = 0;
    let mut metadata = 0;
    for (fixture, id) in [(&first, "aggregate-a"), (&second, "aggregate-b")] {
        let (mut journal, selected) = readonly_recording(fixture, id, 1);
        let root = journal.partial_directory().to_path_buf();
        let destination = journal.finalize().unwrap().destination;
        compressed += fs::metadata(&destination).unwrap().len();
        let archive = Archive::open(&destination, Limits::default()).unwrap();
        let (directory, manifest) = archive.metadata_bytes();
        directories += directory;
        metadata += manifest;
        for record in [
            "intent.json",
            "started-1.json",
            "committed-1.json",
            "published.json",
        ] {
            metadata += fs::metadata(root.join(record)).unwrap().len();
        }
        proofs.extend(selected);
    }
    let exact = ProofLimits {
        max_archive_bytes: compressed,
        max_directory_bytes: directories,
        max_metadata_bytes: metadata,
        ..ProofLimits::default()
    };
    TrackEvidence::verify_batch(&proofs, exact).unwrap();
    for limits in [
        ProofLimits {
            max_archive_bytes: compressed - 1,
            ..exact
        },
        ProofLimits {
            max_directory_bytes: directories - 1,
            ..exact
        },
        ProofLimits {
            max_metadata_bytes: metadata - 1,
            ..exact
        },
    ] {
        proof_counts();
        assert!(TrackEvidence::verify_batch(&proofs, limits).is_err());
        assert_no_track_reads();
    }
}

#[test]
fn readonly_reference_serialization_counts_without_allocating_an_encoded_copy() {
    let fixture = Fixture::new();
    let (_journal, mut proofs) = readonly_recording(&fixture, "reference-bytes", 1);
    proofs[0].destination = fixture.0.join("x".repeat(1024 * 1024));
    proof_counts();
    let error = TrackEvidence::verify_batch(
        &proofs,
        ProofLimits {
            max_reference_bytes: 256,
            ..ProofLimits::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("reference metadata"), "{error}");
    assert!(proof_counts().is_empty());
}

#[test]
fn readonly_single_proof_keeps_separate_journal_and_archive_manifest_allowances() {
    let fixture = Fixture::new();
    let (mut journal, proofs) = readonly_recording(&fixture, "single-boundary", 32);
    let root = journal.partial_directory().to_path_buf();
    let publication = journal.finalize().unwrap();
    let archive = Archive::open(&publication.destination, Limits::default()).unwrap();
    let (_, manifest_bytes) = archive.metadata_bytes();
    let limits = Limits {
        max_manifest_bytes: manifest_bytes,
        ..Limits::default()
    };
    let journal_bytes: u64 = [
        "intent.json",
        "started-1.json",
        "committed-1.json",
        "published.json",
    ]
    .iter()
    .map(|record| fs::metadata(root.join(record)).unwrap().len())
    .sum();
    assert!(journal_bytes < manifest_bytes);
    // A per-archive manifest boundary is not a combined operation budget.
    assert!(journal_bytes + manifest_bytes > limits.max_manifest_bytes);
    proofs[0].verify(limits).unwrap();
    TrackEvidence::verify_batch(
        &proofs[..1],
        ProofLimits {
            archive: limits,
            ..ProofLimits::default()
        },
    )
    .unwrap();
}

#[test]
fn readonly_staged_groups_keep_per_recording_limits_with_generous_global_budgets() {
    let fixture = Fixture::new();
    let (mut journal, proofs) = readonly_recording(&fixture, "local-budgets", 8);
    let root = journal.partial_directory();
    let mut journal_bytes = fs::metadata(root.join("intent.json")).unwrap().len();
    for id in 1..=8 {
        for kind in ["started", "committed"] {
            journal_bytes += fs::metadata(root.join(format!("{kind}-{id}.json")))
                .unwrap()
                .len();
        }
    }
    let exact = ProofLimits {
        archive: Limits {
            max_manifest_bytes: journal_bytes,
            max_total_bytes: 24,
            ..Limits::default()
        },
        ..ProofLimits::default()
    };
    for published in [false, true] {
        if published {
            journal.finalize().unwrap();
        }
        TrackEvidence::verify_batch(&proofs, exact).unwrap();
        for archive in [
            Limits {
                max_manifest_bytes: journal_bytes - 1,
                ..exact.archive
            },
            Limits {
                max_total_bytes: 23,
                ..exact.archive
            },
        ] {
            proof_counts();
            assert!(
                TrackEvidence::verify_batch(&proofs, ProofLimits { archive, ..exact }).is_err(),
                "published={published}"
            );
            assert_no_track_reads();
        }
    }
}
