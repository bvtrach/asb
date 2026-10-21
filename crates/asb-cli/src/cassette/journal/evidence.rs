//! Read-only immutable track evidence; never acquires writer/recovery custody.
use super::*;

/// Exact committed track identity, independent of growing archive/staging bytes.
/// The journal retains immutable metadata after payload cleanup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrackEvidence {
    /// Evidence format, distinct from the manifest schema.
    pub schema_version: u32,
    /// Canonical final destination; no implicit recovery or recreation.
    pub destination: PathBuf,
    /// Stable journal/manifest identity.
    pub recording_id: String,
    /// Full replay compatibility, not scheduling settings.
    pub compatibility: super::super::Compatibility,
    /// Recording batch width, for stable slot identity validation.
    pub concurrency: u64,
    /// Immutable committed metadata and exact uncompressed payload identity.
    pub track: Track,
}

impl TrackEvidence {
    /// Validate bounded metadata without opening artifacts.
    pub fn validate(&self, limits: Limits) -> Result<(), Error> {
        if self.schema_version != 1
            || !self.destination.is_absolute()
            || self.recording_id.len() > 128
        {
            return Err(Error("invalid immutable recording track reference".into()));
        }
        self.compatibility.validate()?;
        Manifest {
            schema_version: super::super::SCHEMA_VERSION,
            recording_id: self.recording_id.clone(),
            compatibility: self.compatibility.clone(),
            concurrency: self.concurrency,
            tracks: vec![self.track.clone()],
        }
        .validate(limits)
    }

    /// Verify immutable commit metadata plus staged bytes or a selected published
    /// ZIP entry. Never acquires a writer lock, recovers state or starts a client.
    /// A disappeared published target cannot fall back to leftover staging.
    pub fn verify(&self, limits: Limits) -> Result<(), Error> {
        self.validate(limits)?;
        let (destination, root, name) = recording_paths(&self.destination)?;
        if destination != self.destination {
            return Err(Error("recording destination identity changed".into()));
        }
        let original = fs::symlink_metadata(&root).map_err(io_error)?;
        check_root_identity(&root, &original)?;
        let mut remaining = limits.max_manifest_bytes;
        let intent: Intent = read_json(
            &root.join("intent.json"),
            limits.max_manifest_bytes,
            &mut remaining,
        )?;
        intent.manifest.validate(limits)?;
        let count = validate_count(intent.repeat, intent.manifest.concurrency, limits)?;
        if !intent.manifest.tracks.is_empty()
            || intent.destination_name != name
            || intent.manifest.recording_id != self.recording_id
            || intent.manifest.compatibility != self.compatibility
            || intent.manifest.concurrency != self.concurrency
            || self.track.id > count
        {
            return Err(Error("track evidence differs from recording intent".into()));
        }
        let started: Started = read_json(
            &root.join(format!("started-{}.json", self.track.id)),
            MAX_RECORD_BYTES,
            &mut remaining,
        )?;
        let committed: Committed = read_json(
            &root.join(format!("committed-{}.json", self.track.id)),
            MAX_RECORD_BYTES,
            &mut remaining,
        )?;
        if started.recording_id != self.recording_id
            || started.id != self.track.id
            || started.batch != self.track.batch
            || started.replica != self.track.replica
            || committed.recording_id != self.recording_id
            || committed.track != self.track
        {
            return Err(Error(
                "track evidence differs from immutable start/commit metadata".into(),
            ));
        }
        let published = root.join("published.json");
        let result = if exists(&published)? {
            super::publication::verify_selected_publication(&root, self, limits)
        } else {
            let staged = hash_file(
                &root.join(format!("track-{}.jsonl", self.track.id)),
                self.track.bytes,
            );
            // Publication may remove staging between metadata inspection and
            // open. Switch only after the durable publication marker exists.
            if exists(&published)? {
                super::publication::verify_selected_publication(&root, self, limits)
            } else if staged? == (self.track.bytes, self.track.sha256.clone()) {
                Ok(())
            } else {
                Err(Error("immutable staged track bytes differ".into()))
            }
        };
        result?;
        check_root_identity(&root, &original)
    }
}

impl RecordingJournal {
    /// Issue a reference only after independently verifying committed bytes.
    pub fn track_evidence(&self, track: &Track) -> Result<TrackEvidence, Error> {
        self.verify_track_evidence(track)?;
        Ok(TrackEvidence {
            schema_version: 1,
            destination: self.destination.clone(),
            recording_id: self.intent.manifest.recording_id.clone(),
            compatibility: self.intent.manifest.compatibility.clone(),
            concurrency: self.intent.manifest.concurrency,
            track: track.clone(),
        })
    }
}
