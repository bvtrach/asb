//! Run-owned recording coordination; providers and proxies see only JSONL.
mod raw;
use super::{RunResult, io_error, replica};
use crate::cassette::{
    Compatibility, Limits, Manifest, Publication, RecordingJournal, Statistics, Track,
    TrackEvidence,
};
use crate::{Error, proxy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Historical producer association plus immutable track proof. Temporary raw
/// files are not completion closure inputs after their exact bytes are committed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub campaign_root: PathBuf,
    pub producer: PathBuf,
    pub attempt_id: String,
    pub track: TrackEvidence,
}

impl Evidence {
    pub fn validate_summary(
        &self,
        summary: &serde_json::Value,
        captures: &[proxy::capture::CaptureEvidence],
    ) -> Result<(), Error> {
        self.validate_summary_metadata(summary, captures)?;
        self.track
            .verify(Limits::default())
            .map_err(recording_error)
    }

    /// Validate the association and capture declarations without reading payloads.
    /// Reports must subsequently verify the collected tracks as one batch.
    pub fn validate_summary_metadata(
        &self,
        summary: &serde_json::Value,
        captures: &[proxy::capture::CaptureEvidence],
    ) -> Result<(), Error> {
        self.validate_report_metadata(summary)?;
        let track = &self.track.track;
        let matching: Vec<_> = captures
            .iter()
            .filter(|capture| capture.output == self.producer)
            .collect();
        if matching.len() > 1
            || matching.iter().any(|capture| {
                capture.bytes != track.bytes
                    || capture.sha256 != track.sha256
                    || capture.complete != track.complete
            })
            || (track.complete && matching.is_empty())
            || track.statistics.requests
                != matching.first().map(|capture| capture.inference_requests)
        {
            return Err(Error::Input(
                "recorded track differs from its capture evidence".into(),
            ));
        }
        Ok(())
    }

    /// A report-only summary lacks completion capture receipts. Validate its
    /// identity and immutable producer association before grouped payload proof.
    /// This does not establish cleanup or authorize execution continuation.
    pub fn validate_report_metadata(&self, summary: &serde_json::Value) -> Result<(), Error> {
        self.track
            .validate(Limits::default())
            .map_err(recording_error)?;
        let track = &self.track.track;
        let execution = &summary["execution"];
        if !self.campaign_root.is_absolute()
            || self
                .campaign_root
                .file_name()
                .and_then(|name| name.to_str())
                != Some(self.track.recording_id.as_str())
            || execution["campaign"] != self.track.recording_id
            || execution["batch"].as_u64() != Some(track.batch)
            || execution["replica"].as_u64() != Some(track.replica)
            || execution["slot"].as_u64() != Some(track.id)
            || summary["task"]["attempt_id"] != self.attempt_id
            || summary["agent"] != self.track.compatibility.agent
            || summary["provider"] != self.track.compatibility.provider
            || summary["model"] != self.track.compatibility.model
            || summary["workload"] != self.track.compatibility.workload
            || summary["task"]["task_id"] != self.track.compatibility.task
            || track.process_success != summary["process_success"].as_bool()
            || track.workload_success != summary["workload_success"].as_bool()
            || track.statistics.tokens != summary["token_usage"]["total_tokens"].as_u64()
            || track.statistics.duration_ms != summary["elapsed_ms"].as_u64()
            || track.statistics.steps != summary["steps"].as_u64()
            || self.producer
                != producer(
                    &self.campaign_root,
                    track.batch,
                    track.replica as usize,
                    self.track.concurrency as usize,
                )
        {
            return Err(Error::Input(
                "recorded track does not belong to this execution/producer".into(),
            ));
        }
        // Bind the association itself, not just a self-consistent claim copied
        // into a different summary. This immutable receipt precedes completion.
        let association = read_association(&self.track)?;
        let digest = crate::cassette::digest(
            &serde_json::to_vec(self).map_err(|error| Error::Io(error.to_string()))?,
        );
        if association.schema_version != 1 || association.evidence_sha256 != digest {
            return Err(Error::Input(
                "recording producer/attempt association changed".into(),
            ));
        }
        Ok(())
    }
}

struct State {
    journal: Option<RecordingJournal>,
    compatibility: Option<Compatibility>,
    committed: std::collections::BTreeSet<u64>,
    association_bytes: u64,
    raw_identities: std::collections::BTreeMap<u64, fs::Metadata>,
}

pub(crate) struct Recorder {
    destination: PathBuf,
    root: PathBuf,
    campaign: String,
    repeat: u64,
    concurrency: usize,
    failed: AtomicBool,
    state: Mutex<State>,
}

/// Read-only destination planning. This observes names, never reserves them.
/// Activation and the journal must independently recheck no-overwrite conditions.
pub(super) fn inspect_destination(destination: &Path) -> Result<PathBuf, Error> {
    let absolute = std::path::absolute(destination).map_err(io_error)?;
    let name = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Input("recording cassette needs a UTF-8 filename".into()))?;
    let requested_parent = absolute
        .parent()
        .ok_or_else(|| Error::Input("recording cassette needs a parent".into()))?;
    // Resolve existing directory/symlink prefixes without creating missing
    // parents. Parent components after a missing name behave as create_dir_all
    // followed by canonicalization did, but planning has no filesystem writes.
    let mut parent = PathBuf::new();
    for component in requested_parent.components() {
        match component {
            std::path::Component::CurDir => (),
            std::path::Component::ParentDir => {
                parent.pop();
            }
            component => {
                parent.push(component.as_os_str());
                match fs::symlink_metadata(&parent) {
                    Ok(_) => {
                        parent = parent.canonicalize().map_err(io_error)?;
                        if !parent.is_dir() {
                            return Err(Error::Input(
                                "recording cassette parent must be a directory".into(),
                            ));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(io_error(error)),
                }
            }
        }
    }
    let absolute = parent.join(name);
    for path in [&absolute, &parent.join(format!(".{name}.partial"))] {
        match fs::symlink_metadata(path) {
            Ok(_) => return Err(Error::Input("recording destination or partial artifact already exists; choose a new cassette or explicitly recover it".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(absolute)
}

/// Refuse final/partial destinations before workload/tool preparation. The
/// journal independently owns the authoritative no-replace publication boundary.
pub(super) fn preflight(destination: &Path) -> Result<PathBuf, Error> {
    let expected = inspect_destination(destination)?;
    fs::create_dir_all(expected.parent().expect("inspected parent")).map_err(io_error)?;
    let observed = inspect_destination(&expected)?;
    if expected != observed {
        return Err(Error::Input(
            "recording cassette parent changed during activation".into(),
        ));
    }
    Ok(observed)
}

impl Recorder {
    pub fn new(
        destination: PathBuf,
        root: PathBuf,
        campaign: String,
        repeat: u64,
        concurrency: usize,
    ) -> Self {
        Self {
            destination,
            root,
            campaign,
            repeat,
            concurrency,
            failed: AtomicBool::new(false),
            state: Mutex::new(State {
                journal: None,
                compatibility: None,
                committed: Default::default(),
                association_bytes: 0,
                raw_identities: Default::default(),
            }),
        }
    }

    /// Called outside the material gate's mutex after all material agrees. These
    /// are recording-slot intents, not claims that a client process was launched.
    pub fn prepare(&self, compatibility: &Compatibility, batch: u64) -> Result<(), Error> {
        association_budget(self.repeat, self.concurrency)?;
        self.operation(|state| {
            if let Some(identity) = &state.compatibility {
                identity
                    .require_match(compatibility)
                    .map_err(recording_error)?;
            } else {
                state.journal = Some(
                    RecordingJournal::create(
                        &self.destination,
                        Manifest {
                            schema_version: crate::cassette::SCHEMA_VERSION,
                            recording_id: self.campaign.clone(),
                            compatibility: compatibility.clone(),
                            concurrency: self.concurrency as u64,
                            tracks: Vec::new(),
                        },
                        self.repeat,
                        Limits::default(),
                    )
                    .map_err(recording_error)?,
                );
            }
            state.compatibility = Some(compatibility.clone());
            let journal = state.journal.as_mut().expect("initialized journal");
            for replica in 1..=self.concurrency {
                journal
                    .begin(batch, replica as u64)
                    .map_err(recording_error)?;
            }
            Ok(())
        })
    }

    /// Single writer held only during bounded payload commit/verification.
    pub(super) fn commit(
        &self,
        batch: u64,
        replica: replica::Identity,
        result: &RunResult<'_>,
        proxy: &proxy::Evidence,
    ) -> Result<Evidence, Error> {
        self.operation(|state| {
            let output = producer(&self.root, batch, replica.index, self.concurrency);
            let mut matching = proxy
                .captures
                .iter()
                .filter(|capture| capture.output == output);
            let capture = matching.next();
            if matching.next().is_some() {
                return Err(Error::Io(
                    "multiple recorder captures claim one producer".into(),
                ));
            }
            if let Some(capture) = capture {
                capture.validate()?;
            }
            let mut file = open_payload(&output)?;
            let raw_identity = file
                .as_ref()
                .map(File::metadata)
                .transpose()
                .map_err(io_error)?;
            if file.is_none() && capture.is_some() {
                return Err(Error::Io("captured recording payload is missing".into()));
            }
            let (bytes, sha256) = if let Some(capture) = capture {
                (capture.bytes, capture.sha256.clone())
            } else if let Some(file) = &mut file {
                payload_identity(file)?
            } else {
                (0, crate::cassette::digest(b""))
            };
            let track = Track {
                id: (batch - 1) * self.concurrency as u64 + replica.index as u64,
                batch,
                replica: replica.index as u64,
                complete: capture.is_some_and(|capture| capture.complete),
                process_success: Some(result.process_success),
                workload_success: result.workload_success,
                bytes,
                sha256,
                statistics: Statistics {
                    tokens: result.token_usage.total_tokens,
                    duration_ms: u64::try_from(result.elapsed_ms).ok(),
                    requests: capture.map(|capture| capture.inference_requests),
                    steps: result.steps,
                },
            };
            let journal = state
                .journal
                .as_mut()
                .ok_or_else(|| Error::Io("recording has no verified material identity".into()))?;
            if let Some(file) = &mut file {
                file.seek(SeekFrom::Start(0)).map_err(io_error)?;
                journal
                    .commit(track.clone(), file)
                    .map_err(recording_error)?;
            } else {
                journal
                    .commit(track.clone(), &mut std::io::empty())
                    .map_err(recording_error)?;
            }
            let evidence = Evidence {
                campaign_root: self.root.clone(),
                producer: output,
                attempt_id: result.task.attempt_id.clone(),
                track: journal.track_evidence(&track).map_err(recording_error)?,
            };
            let evidence_bytes =
                serde_json::to_vec(&evidence).map_err(|error| Error::Io(error.to_string()))?;
            if evidence_bytes.len() > 64 * 1024 {
                return Err(Error::Io("recording evidence exceeds bound".into()));
            }
            let bytes = serde_json::to_vec(&Association {
                schema_version: 1,
                evidence_sha256: crate::cassette::digest(&evidence_bytes),
            })
            .map_err(|error| Error::Io(error.to_string()))?;
            let total = state
                .association_bytes
                .checked_add(bytes.len() as u64)
                .filter(|total| {
                    *total <= Limits::default().max_manifest_bytes
                        && bytes.len() <= ASSOCIATION_BOUND as usize
                })
                .ok_or_else(|| Error::Io("recording association metadata exceeds bounds".into()))?;
            super::repeated::publish_bytes(&association_path(&evidence.track)?, &bytes)?;
            state.association_bytes = total;
            if let Some(identity) = raw_identity {
                state.raw_identities.insert(track.id, identity);
            }
            state.committed.insert(track.id);
            Ok(evidence)
        })
    }

    /// Called only after every admitted producer was joined. Unknown producer
    /// cleanup retains raw files even when publication itself can be confirmed.
    pub fn finalize(&self, producers_quiescent: bool) -> Result<Option<Publication>, Error> {
        self.operation(|state| {
            let Some(journal) = state.journal.as_mut() else { return Ok(None) };
            // Preserve partial producer bytes from setup/launch failure without
            // fabricating an execution outcome or positive completeness.
            for mut track in journal.snapshot().map_err(recording_error)?.tracks {
                if state.committed.contains(&track.id) { continue }
                let output = producer(&self.root, track.batch, track.replica as usize, self.concurrency);
                let mut file = open_payload(&output)?;
                let raw_identity=file.as_ref().map(File::metadata).transpose().map_err(io_error)?;
                let id=track.id;
                let (bytes, sha256) = if let Some(file) = &mut file { payload_identity(file)? } else { (0, crate::cassette::digest(b"")) };
                track.bytes=bytes; track.sha256=sha256; track.complete=false;
                if let Some(file) = &mut file { file.seek(SeekFrom::Start(0)).map_err(io_error)?; journal.commit(track,file).map_err(recording_error)?; }
                else { journal.commit(track,&mut std::io::empty()).map_err(recording_error)?; }
                if let Some(identity)=raw_identity { state.raw_identities.insert(id,identity); }
            }
            let mut published = journal.finalize().map_err(recording_error)?;
            if producers_quiescent {
                let mut cleanup_failures=0;
                for track in &published.manifest.tracks {
                    let output = producer(&self.root,track.batch,track.replica as usize,self.concurrency);
                    match cleanup_raw(&output,track,state.raw_identities.get(&track.id)) {
                        Ok(()) => (),
                        Err(error) => {
                            published.cleanup_complete=false;
                            cleanup_failures+=1;
                            if cleanup_failures<=16 { published.warnings.push(error.to_string()); }
                        }
                    }
                }
                if cleanup_failures>0 { published.warnings.push(format!("published cassette is usable; {cleanup_failures} raw capture cleanups were incomplete or durability-unconfirmed; inspect artifacts (at most 16 details retained)")); }
            } else { published.cleanup_complete=false; published.warnings.push("published cassette is usable; raw captures retained because producer cleanup is unconfirmed".into()); }
            Ok(Some(published))
        })
    }

    fn operation<T>(
        &self,
        action: impl FnOnce(&mut State) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.failed.load(Ordering::Acquire) {
            return Err(Error::Io(
                "recording state requires explicit recovery; partial evidence retained".into(),
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Io("recording writer state was interrupted".into()))?;
        if self.failed.load(Ordering::Acquire) {
            return Err(Error::Io(
                "recording state requires explicit recovery; partial evidence retained".into(),
            ));
        }
        let result = action(&mut state);
        if result.is_err() {
            self.failed.store(true, Ordering::Release);
        }
        result
    }
}

pub(super) fn base(root: &Path, batch: u64) -> PathBuf {
    root.join(format!("batch-{batch}/capture.jsonl"))
}
fn producer(root: &Path, batch: u64, replica: usize, concurrency: usize) -> PathBuf {
    let base = base(root, batch);
    if concurrency == 1 {
        base
    } else {
        crate::benchmark_paths::replica_path(&base, replica)
    }
}
const ASSOCIATION_BOUND: u64 = 128;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Association {
    schema_version: u32,
    evidence_sha256: String,
}

fn association_budget(repeat: u64, concurrency: usize) -> Result<u64, Error> {
    repeat
        .checked_mul(concurrency as u64)
        .filter(|count| {
            repeat > 0
                && (1..=64).contains(&concurrency)
                && *count <= Limits::default().max_tracks as u64
        })
        .and_then(|count| count.checked_mul(ASSOCIATION_BOUND))
        .filter(|bytes| *bytes <= Limits::default().max_manifest_bytes)
        .ok_or_else(|| {
            Error::Input("recording population exceeds association metadata budget".into())
        })
}
fn association_path(evidence: &TrackEvidence) -> Result<PathBuf, Error> {
    let name = evidence
        .destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Input("invalid recording destination".into()))?;
    let parent = evidence
        .destination
        .parent()
        .ok_or_else(|| Error::Input("invalid recording parent".into()))?;
    Ok(parent.join(format!(
        ".{name}.partial/association-{}.json",
        evidence.track.id
    )))
}
fn read_association(evidence: &TrackEvidence) -> Result<Association, Error> {
    let path = association_path(evidence)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > ASSOCIATION_BOUND {
        return Err(Error::Input(
            "recording association is invalid or oversized".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(ASSOCIATION_BOUND + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > ASSOCIATION_BOUND as usize {
        return Err(Error::Input(
            "recording association grew beyond bound".into(),
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| Error::Input("recording association is malformed".into()))
}
#[cfg(test)]
std::thread_local! {
    static BEFORE_RAW_UNLINK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
        std::cell::RefCell::new(None)
    };
}

fn cleanup_raw(output: &Path, track: &Track, original: Option<&fs::Metadata>) -> Result<(), Error> {
    raw::cleanup(output, track, original)
}
fn open_payload(output: &Path) -> Result<Option<File>, Error> {
    match fs::symlink_metadata(output) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
        Ok(metadata) if !metadata.is_file() => {
            return Err(Error::Io(
                "recording producer must be a regular non-symlink file".into(),
            ));
        }
        Ok(_) => (),
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(output).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > Limits::default().max_track_bytes {
        return Err(Error::Io(
            "recording producer is invalid or oversized".into(),
        ));
    }
    Ok(Some(file))
}
fn payload_identity(file: &mut File) -> Result<(u64, String), Error> {
    let limit = Limits::default().max_track_bytes;
    if file.metadata().map_err(io_error)?.len() > limit {
        return Err(Error::Io("recording payload exceeds limit".into()));
    }
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .filter(|bytes| *bytes <= limit)
            .ok_or_else(|| Error::Io("recording payload grew beyond limit".into()))?;
        hash.update(&buffer[..count]);
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}
fn recording_error(error: crate::cassette::Error) -> Error {
    Error::Io(format!("recording: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn raw_cleanup_preserves_foreign_entries_replaced_after_final_check() {
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                BEFORE_RAW_UNLINK.with_borrow_mut(|hook| {
                    hook.take();
                });
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let mut failures = Vec::new();
        for mode in ["regular", "symlink", "parent"] {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/raw-cleanup-race-fixtures")
                .join(crate::workloads::TaskIdentity::local("raw-cleanup-race", None).attempt_id);
            fs::create_dir_all(root.join("producer")).unwrap();
            let _owned = Owned(root.clone());
            let output = root.join("producer/capture.jsonl");
            fs::write(&output, b"{}\n").unwrap();
            let original = fs::metadata(&output).unwrap();
            let track = Track {
                id: 1,
                batch: 1,
                replica: 1,
                complete: false,
                process_success: None,
                workload_success: None,
                bytes: 3,
                sha256: crate::cassette::digest(b"{}\n"),
                statistics: Statistics::default(),
            };
            let replacement_root = root.clone();
            BEFORE_RAW_UNLINK.with_borrow_mut(|hook| {
                *hook = Some(Box::new(move || {
                    let output = replacement_root.join("producer/capture.jsonl");
                    if mode == "parent" {
                        fs::rename(
                            replacement_root.join("producer"),
                            replacement_root.join("saved-producer"),
                        )
                        .unwrap();
                        fs::create_dir(replacement_root.join("producer")).unwrap();
                    } else {
                        fs::rename(&output, replacement_root.join("saved-original")).unwrap();
                    }
                    if mode == "symlink" {
                        let target = replacement_root.join("foreign-target");
                        fs::write(&target, b"foreign content").unwrap();
                        std::os::unix::fs::symlink(target, &output).unwrap();
                    } else {
                        fs::write(&output, b"foreign content").unwrap();
                    }
                }));
            });
            let cleaned = cleanup_raw(&output, &track, Some(&original));
            let Ok(metadata) = fs::symlink_metadata(&output) else {
                failures.push(format!(
                    "{mode}: removed a foreign pathname after validating the original: {cleaned:?}"
                ));
                continue;
            };
            if fs::read(&output).unwrap() != b"foreign content" {
                failures.push(format!("{mode}: changed foreign bytes"));
            }
            if mode == "symlink" && !metadata.file_type().is_symlink() {
                failures.push("symlink: replaced the foreign link".into());
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("; "));
    }

    #[test]
    fn destination_inspection_is_read_only_and_activation_rechecks_foreign_outputs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/recording-destination-fixtures")
            .join(crate::workloads::TaskIdentity::local("destination", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let destination = root.join("missing/nested/recording.asb.zip");
        assert_eq!(inspect_destination(&destination).unwrap(), destination);
        assert!(!root.join("missing").exists());
        assert_eq!(preflight(&destination).unwrap(), destination);
        assert!(destination.parent().unwrap().is_dir());
        fs::write(&destination, b"foreign").unwrap();
        assert!(preflight(&destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"foreign");
        let other = root.join("other.asb.zip");
        inspect_destination(&other).unwrap();
        let partial = root.join(".other.asb.zip.partial");
        fs::create_dir(&partial).unwrap();
        assert!(preflight(&other).is_err());
        assert!(partial.is_dir());
        let non_directory = root.join("file");
        fs::write(&non_directory, b"sentinel").unwrap();
        assert!(inspect_destination(&non_directory.join("new.zip")).is_err());
        assert_eq!(fs::read(&non_directory).unwrap(), b"sentinel");
        let with_parent = root.join("never-created/../next/archive.zip");
        assert_eq!(
            inspect_destination(&with_parent).unwrap(),
            root.join("next/archive.zip")
        );
        assert!(!root.join("never-created").exists());
        assert!(!root.join("next").exists());
        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(root.join("missing"), &alias).unwrap();
            let candidate = alias.join("future/next.zip");
            assert_eq!(
                inspect_destination(&candidate).unwrap(),
                root.join("missing/future/next.zip")
            );
            assert!(!root.join("missing/future").exists());
            let nested_alias = root.join("nested-alias");
            std::os::unix::fs::symlink(root.join("missing/nested"), &nested_alias).unwrap();
            assert_eq!(
                inspect_destination(&nested_alias.join("../sibling/next.zip")).unwrap(),
                root.join("missing/sibling/next.zip")
            );
            assert!(!root.join("missing/sibling").exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compact_association_budget_fits_maximum_population_before_launch() {
        assert_eq!(
            association_budget(65_533, 1).unwrap(),
            65_533 * ASSOCIATION_BOUND
        );
        for (repeat, concurrency) in [(0, 1), (1, 0), (1, 65), (1024, 64), (u64::MAX, 2)] {
            assert!(association_budget(repeat, concurrency).is_err());
        }
        let association = Association {
            schema_version: 1,
            evidence_sha256: "f".repeat(64),
        };
        assert!(serde_json::to_vec(&association).unwrap().len() as u64 <= ASSOCIATION_BOUND);
    }

    #[cfg(unix)]
    #[test]
    fn raw_cleanup_retains_mutation_replacement_links_and_directories() {
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        for mode in ["unchanged", "mutated", "replaced", "symlink", "directory"] {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/raw-cleanup-fixtures")
                .join(crate::workloads::TaskIdentity::local("raw-cleanup", None).attempt_id);
            fs::create_dir_all(&root).unwrap();
            let _owned = Owned(root.clone());
            let output = root.join("capture.jsonl");
            fs::write(&output, b"{}\n").unwrap();
            let original = fs::metadata(&output).unwrap();
            let track = Track {
                id: 1,
                batch: 1,
                replica: 1,
                complete: false,
                process_success: None,
                workload_success: None,
                bytes: 3,
                sha256: crate::cassette::digest(b"{}\n"),
                statistics: Statistics::default(),
            };
            match mode {
                "mutated" => fs::write(&output, b"[]\n").unwrap(),
                "replaced" => {
                    fs::rename(&output, root.join("preserved-original")).unwrap();
                    fs::write(&output, b"{}\n").unwrap();
                }
                "symlink" => {
                    fs::rename(&output, root.join("sentinel")).unwrap();
                    std::os::unix::fs::symlink(root.join("sentinel"), &output).unwrap();
                }
                "directory" => {
                    fs::rename(&output, root.join("sentinel")).unwrap();
                    fs::create_dir(&output).unwrap();
                }
                _ => (),
            }
            let cleaned = cleanup_raw(&output, &track, Some(&original));
            if mode == "unchanged" {
                cleaned.unwrap();
                assert!(!output.exists());
            } else {
                assert!(cleaned.is_err(), "{mode}");
                assert!(fs::symlink_metadata(&output).is_ok());
            }
            if root.join("sentinel").exists() {
                assert_eq!(fs::read(root.join("sentinel")).unwrap(), b"{}\n");
            }
        }
    }
}
