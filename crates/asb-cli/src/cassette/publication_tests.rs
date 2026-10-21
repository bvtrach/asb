use super::*;
use std::path::Path;

fn recording(fixture: &Fixture, count: u64) -> RecordingJournal {
    let mut journal = RecordingJournal::create(
        &fixture.0.join("trials.zip"),
        manifest(0),
        count,
        Limits::default(),
    )
    .unwrap();
    for id in 1..=count {
        journal.begin(id, 1).unwrap();
        let mut track = manifest(count).tracks[(id - 1) as usize].clone();
        if id == 1 {
            track.process_success = Some(false);
            track.workload_success = Some(false);
        }
        journal.commit(track, &mut &b"{}\n"[..]).unwrap();
    }
    journal
}

fn candidate(root: &Path) -> PathBuf {
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("prepared.json")).unwrap()).unwrap();
    root.join(receipt["candidate"].as_str().unwrap())
}

#[test]
fn failed_complete_and_incomplete_attempts_publish_and_recover_after_cleanup() {
    let fixture = Fixture::new();
    let destination = fixture.0.join("trials.zip");
    let mut journal =
        RecordingJournal::create(&destination, manifest(0), 4, Limits::default()).unwrap();
    journal.begin(3, 1).unwrap();
    journal.begin(1, 1).unwrap();
    journal.begin(2, 1).unwrap();
    let mut track = manifest(1).tracks[0].clone();
    track.process_success = Some(false);
    track.workload_success = Some(false);
    journal.commit(track.clone(), &mut &b"{}\n"[..]).unwrap();
    journal.verify_track_evidence(&track).unwrap();
    let mut bad = manifest(3).tracks[2].clone();
    bad.sha256 = digest(b"incorrect");
    assert!(journal.commit(bad, &mut &b"{}\n"[..]).is_err());
    let expected = journal.snapshot().unwrap();
    let root = journal.partial_directory().to_path_buf();
    fs::write(root.join("diagnostic-user-file"), b"retain").unwrap();
    let committed_bytes = fs::read(root.join("committed-1.json")).unwrap();
    let publication = journal.finalize().unwrap();
    assert!(publication.cleanup_complete, "{:?}", publication.warnings);
    assert_eq!(publication.manifest, expected);
    assert_eq!(publication.manifest.tracks.len(), 3);
    assert!(publication.manifest.tracks[0].automatically_selectable());
    assert!(!publication.manifest.tracks[1].complete);
    assert!(!publication.manifest.tracks[2].complete);
    assert_eq!(publication.manifest.tracks[2].bytes, 3);
    assert!(!root.join("track-1.jsonl").exists());
    assert!(!root.join("track-3.jsonl").exists());
    assert!(!candidate(&root).exists());
    assert_eq!(
        fs::read(root.join("committed-1.json")).unwrap(),
        committed_bytes
    );
    assert_eq!(
        fs::read(root.join("diagnostic-user-file")).unwrap(),
        b"retain"
    );
    let mut archive = Archive::open(&destination, Limits::default()).unwrap();
    assert_eq!(archive.read_track(1).unwrap(), b"{}\n");
    assert!(archive.read_track(2).unwrap().is_empty());
    assert_eq!(archive.read_track(3).unwrap(), b"{}\n");
    journal.verify_track_evidence(&track).unwrap();
    let mut forged = track.clone();
    forged.statistics.tokens = Some(9999);
    assert!(journal.verify_track_evidence(&forged).is_err());
    assert!(journal.begin(4, 1).is_err());
    assert!(journal.commit(track.clone(), &mut &b"{}\n"[..]).is_err());
    let bytes = fs::read(&destination).unwrap();
    drop(journal);
    let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
    assert_eq!(recovered.snapshot().unwrap(), expected);
    recovered.verify_track_evidence(&track).unwrap();
    assert!(recovered.finalize().unwrap().cleanup_complete);
    assert_eq!(fs::read(&destination).unwrap(), bytes);
}

#[test]
fn publication_fault_matrix_retains_recoverable_data_and_no_new_attempts() {
    let points = [
        "before-freeze",
        "finalizing.json:before-write",
        "finalizing.json:after-link",
        "after-freeze",
        "before-zip-create",
        "before-zip-track",
        "before-zip-finish",
        "before-zip-sync",
        "after-zip-sync",
        "after-candidate-directory-sync",
        "before-prepared",
        "prepared.json:before-write",
        "prepared.json:after-link",
        "after-prepared",
        "before-final-link",
        "after-final-link",
        "before-parent-sync",
        "after-parent-sync",
        "before-published",
        "published.json:before-write",
        "published.json:after-link",
    ];
    for point in points {
        let fixture = Fixture::new();
        let mut journal = recording(&fixture, 2);
        let expected = journal.snapshot().unwrap();
        let destination = journal.destination().to_path_buf();
        let root = journal.partial_directory().to_path_buf();
        let start_before = fs::read(root.join("started-1.json")).unwrap();
        journal.inject_publication_failure(point);
        let error = journal.finalize().expect_err("fault must fire");
        assert!(
            error.to_string().contains(point),
            "wrong error at {point}: {error}"
        );
        assert!(
            journal.snapshot().is_err(),
            "ambiguous owner must require recovery at {point}"
        );
        assert!(root.join("track-1.jsonl").is_file());
        let prepared_before = fs::read(root.join("prepared.json")).ok();
        let linked_before = fs::read(&destination).ok();
        drop(journal);
        let mut recovered = RecordingJournal::recover(&destination, Limits::default())
            .unwrap_or_else(|error| panic!("recover {point}: {error}"));
        assert_eq!(
            recovered.snapshot().unwrap(),
            expected,
            "changed attempts at {point}"
        );
        let published = recovered
            .finalize()
            .unwrap_or_else(|error| panic!("finalize {point}: {error}"));
        assert!(
            published.cleanup_complete,
            "{point}: {:?}",
            published.warnings
        );
        assert_eq!(published.manifest, expected);
        assert_eq!(fs::read(root.join("started-1.json")).unwrap(), start_before);
        if let Some(before) = prepared_before {
            assert_eq!(
                fs::read(root.join("prepared.json")).unwrap(),
                before,
                "must reuse exact prepared candidate"
            );
        }
        if let Some(before) = linked_before {
            assert_eq!(
                fs::read(&destination).unwrap(),
                before,
                "must not replace linked destination"
            );
        }
    }
}

#[test]
fn cleanup_faults_disclose_usable_publication_and_resume_individually() {
    for (point, skip, remaining) in [
        ("after-published", 0, 3),
        ("before-payload-cleanup", 0, 3),
        ("after-payload-cleanup", 0, 2),
        ("after-payload-cleanup", 1, 1),
        ("after-payload-cleanup", 2, 0),
        ("before-candidate-cleanup", 0, 0),
        ("after-candidate-cleanup", 0, 0),
    ] {
        let fixture = Fixture::new();
        let mut journal = recording(&fixture, 3);
        let root = journal.partial_directory().to_path_buf();
        let destination = journal.destination().to_path_buf();
        journal.inject_publication_failure_after(point, skip);
        let publication = journal.finalize().unwrap();
        assert!(!publication.cleanup_complete, "fault not observed: {point}");
        assert!(!publication.warnings.is_empty());
        assert!(destination.is_file());
        let actual = (1..=3)
            .filter(|id| root.join(format!("track-{id}.jsonl")).exists())
            .count();
        assert_eq!(
            actual, remaining,
            "wrong cleanup boundary for {point}/{skip}"
        );
        let bytes = fs::read(&destination).unwrap();
        drop(journal);
        let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
        assert!(recovered.finalize().unwrap().cleanup_complete);
        assert_eq!(fs::read(&destination).unwrap(), bytes);
    }
}

#[test]
fn output_limit_is_enforced_during_writes_and_can_be_recovered_with_more_capacity() {
    let fixture = Fixture::new();
    let destination = fixture.0.join("limited.zip");
    let limits = Limits {
        max_archive_bytes: 100,
        ..Limits::default()
    };
    let mut journal = RecordingJournal::create(&destination, manifest(0), 1, limits).unwrap();
    journal.begin(1, 1).unwrap();
    journal
        .commit(manifest(1).tracks[0].clone(), &mut &b"{}\n"[..])
        .unwrap();
    assert!(journal.finalize().is_err());
    let root = journal.partial_directory().to_path_buf();
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().starts_with(".archive-") {
            assert!(entry.metadata().unwrap().len() <= 100);
        }
    }
    assert_eq!(fs::read(root.join("track-1.jsonl")).unwrap(), b"{}\n");
    assert!(!destination.exists());
    drop(journal);
    assert!(
        RecordingJournal::recover(&destination, Limits::default())
            .unwrap()
            .finalize()
            .unwrap()
            .cleanup_complete
    );
}

#[test]
fn existing_foreign_or_changed_targets_are_never_overwritten() {
    let fixture = Fixture::new();
    let mut journal = recording(&fixture, 1);
    let destination = journal.destination().to_path_buf();
    fs::write(&destination, b"foreign data").unwrap();
    assert!(journal.finalize().is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"foreign data");
    fs::remove_file(&destination).unwrap();
    journal.inject_publication_failure("before-final-link");
    assert!(journal.finalize().is_err());
    drop(journal);
    fs::write(&destination, b"racing foreign data").unwrap();
    assert!(RecordingJournal::recover(&destination, Limits::default()).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"racing foreign data");
}

#[test]
fn prepared_candidate_and_target_recovery_uses_exact_bytes_without_rebuilding() {
    for (target_present, candidate_present) in
        [(false, true), (true, true), (true, false), (false, false)]
    {
        let fixture = Fixture::new();
        let mut journal = recording(&fixture, 1);
        let destination = journal.destination().to_path_buf();
        let root = journal.partial_directory().to_path_buf();
        journal.inject_publication_failure(if target_present {
            "after-final-link"
        } else {
            "before-final-link"
        });
        assert!(journal.finalize().is_err());
        let prepared_before = fs::read(root.join("prepared.json")).unwrap();
        if !candidate_present {
            fs::remove_file(candidate(&root)).unwrap();
        }
        // A verified candidate or final target supplies immutable track bytes;
        // redundant staging is not a prerequisite once prepared bytes exist.
        fs::remove_file(root.join("track-1.jsonl")).unwrap();
        drop(journal);
        let recovered = RecordingJournal::recover(&destination, Limits::default());
        if !target_present && !candidate_present {
            assert!(recovered.is_err());
            continue;
        }
        let mut recovered = recovered.unwrap();
        assert!(recovered.finalize().unwrap().cleanup_complete);
        assert_eq!(
            fs::read(root.join("prepared.json")).unwrap(),
            prepared_before
        );
    }
}

#[test]
fn frozen_mapping_tampering_and_uncommitted_promotion_are_rejected() {
    for mutation in 0..4 {
        let fixture = Fixture::new();
        let destination = fixture.0.join("freeze.zip");
        let mut journal =
            RecordingJournal::create(&destination, manifest(0), 2, Limits::default()).unwrap();
        journal.begin(1, 1).unwrap();
        let root = journal.partial_directory().to_path_buf();
        journal.inject_publication_failure("after-freeze");
        assert!(journal.finalize().is_err());
        drop(journal);
        let path = root.join("finalizing.json");
        let mut frozen: Manifest = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match mutation {
            0 => frozen.tracks.clear(),
            1 => frozen.tracks[0].complete = true,
            2 => frozen.tracks[0].workload_success = Some(true),
            _ => frozen.tracks[0].statistics.tokens = Some(99),
        }
        fs::write(path, serde_json::to_vec(&frozen).unwrap()).unwrap();
        assert!(
            RecordingJournal::recover(&destination, Limits::default()).is_err(),
            "accepted mutation {mutation}"
        );
    }
}

#[cfg(unix)]
#[test]
fn cleanup_preserves_replaced_payloads_and_dangling_destinations() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let mut journal = recording(&fixture, 1);
    let destination = journal.destination().to_path_buf();
    symlink(fixture.0.join("missing"), &destination).unwrap();
    assert!(journal.finalize().is_err());
    assert!(
        fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&destination).unwrap();
    journal.inject_publication_failure("after-published");
    assert!(!journal.finalize().unwrap().cleanup_complete);
    let root = journal.partial_directory().to_path_buf();
    let unrelated = fixture.0.join("unrelated");
    fs::write(&unrelated, b"user bytes").unwrap();
    fs::remove_file(root.join("track-1.jsonl")).unwrap();
    symlink(&unrelated, root.join("track-1.jsonl")).unwrap();
    drop(journal);
    let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
    let publication = recovered.finalize().unwrap();
    assert!(!publication.cleanup_complete);
    assert!(
        fs::symlink_metadata(root.join("track-1.jsonl"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(unrelated).unwrap(), b"user bytes");
    assert!(Archive::open(&destination, Limits::default()).is_ok());
}

#[test]
fn published_archive_loss_never_authorizes_same_guard_or_recovery_to_recreate_it() {
    let fixture = Fixture::new();
    let mut journal = recording(&fixture, 1);
    journal.inject_publication_failure("after-published");
    let result = journal.finalize().unwrap();
    assert!(!result.cleanup_complete);
    let destination = journal.destination().to_path_buf();
    let root = journal.partial_directory().to_path_buf();
    assert!(candidate(&root).is_file());
    fs::remove_file(&destination).unwrap();
    assert!(journal.snapshot().is_err());
    assert!(
        journal
            .verify_track_evidence(&result.manifest.tracks[0])
            .is_err()
    );
    assert!(
        journal
            .finalize()
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    assert!(!destination.exists());
    drop(journal);
    assert!(RecordingJournal::recover(&destination, Limits::default()).is_err());
    assert!(!destination.exists());
    assert!(candidate(&root).is_file());
}

#[test]
fn abrupt_owner_child() {
    let Some(destination) = std::env::var_os("ASB_CASSETTE_OWNER_DEATH_DESTINATION") else {
        return;
    };
    let destination = PathBuf::from(destination);
    let stage = std::env::var("ASB_CASSETTE_OWNER_DEATH_STAGE").unwrap();
    let mut journal =
        RecordingJournal::create(&destination, manifest(0), 1, Limits::default()).unwrap();
    journal.begin(1, 1).unwrap();
    let mut track = manifest(1).tracks[0].clone();
    track.process_success = Some(false);
    track.workload_success = Some(false);
    journal.commit(track, &mut &b"{}\n"[..]).unwrap();
    match stage.as_str() {
        "recording" => (),
        "prepared" => {
            journal.inject_publication_failure("before-final-link");
            assert!(journal.finalize().is_err());
        }
        "linked" => {
            journal.inject_publication_failure("after-final-link");
            assert!(journal.finalize().is_err());
        }
        "published" => {
            journal.inject_publication_failure("after-published");
            assert!(!journal.finalize().unwrap().cleanup_complete);
        }
        _ => panic!("unknown test stage"),
    }
    fs::write(destination.with_extension("ready"), b"ready").unwrap();
    let mut byte = [0];
    let _ = std::io::stdin().read(&mut byte);
}

#[test]
fn abrupt_owner_death_at_recording_prepared_linked_and_published_stages_is_recoverable() {
    use std::process::{Command, Stdio};
    for stage in ["recording", "prepared", "linked", "published"] {
        let fixture = Fixture::new();
        let destination = fixture.0.join("trials.zip");
        let ready = destination.with_extension("ready");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cassette::tests::publication::abrupt_owner_child",
                "--nocapture",
            ])
            .env("ASB_CASSETTE_OWNER_DEATH_DESTINATION", &destination)
            .env("ASB_CASSETTE_OWNER_DEATH_STAGE", stage)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready.is_file() && std::time::Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("owner exited early at {stage}: {status}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !ready.is_file() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owner did not reach {stage}");
        }
        let busy = RecordingJournal::recover(&destination, Limits::default())
            .err()
            .map(|error| error.to_string());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            busy.is_some_and(|error| error.contains("another writer")),
            "live owner must be excluded"
        );
        let mut recovered = RecordingJournal::recover(&destination, Limits::default()).unwrap();
        let result = recovered.finalize().unwrap();
        assert!(result.cleanup_complete);
        assert_eq!(result.manifest.tracks.len(), 1);
        assert_eq!(result.manifest.tracks[0].process_success, Some(false));
        assert_eq!(result.manifest.tracks[0].workload_success, Some(false));
        assert!(result.manifest.tracks[0].complete);
    }
}

#[test]
fn batch_track_evidence_is_selective_while_group_verification_checks_the_whole_archive() {
    let fixture = Fixture::new();
    let mut journal = recording(&fixture, 3);
    let publication = journal.finalize().unwrap();
    let track = publication.manifest.tracks[0].clone();
    journal
        .verify_track_evidence_batch(&[track.clone(), track.clone()])
        .unwrap();
    let mut forged = track.clone();
    forged.sha256 = digest(b"forged");
    assert!(
        journal
            .verify_track_evidence_batch(&[track.clone(), forged])
            .is_err()
    );
    let mut archive = zip::ZipArchive::new(File::open(&publication.destination).unwrap()).unwrap();
    let offset = archive.by_index(3).unwrap().data_start().unwrap() as usize;
    let mut bytes = fs::read(&publication.destination).unwrap();
    bytes[offset] = 0xff;
    fs::write(&publication.destination, bytes).unwrap();
    // Per-track evidence binds only the requested immutable track. A changed
    // unrelated track must still invalidate final whole-bundle evidence.
    journal.verify_track_evidence_batch(&[track]).unwrap();
    assert!(journal.snapshot().is_err());
    assert!(
        journal
            .verify_track_evidence_batch(&publication.manifest.tracks)
            .is_err()
    );
}
