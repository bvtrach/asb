//! Content-based source discovery without fabricated legacy completion metadata.
use super::{Archive, Error, Limits, Manifest};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Observed legacy payload identity. Legacy JSONL has no positive completeness,
/// grading, token or timing receipt; callers must display those as unknown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyTrack {
    /// One for a single file, or the original sparse concurrency suffix.
    pub id: u64,
    /// Source display path, never an extraction destination.
    pub path: PathBuf,
    /// Exact uncompressed bytes observed at source discovery.
    pub bytes: u64,
    /// SHA-256 of observed bytes, independently checked again on selected read.
    pub sha256: String,
}

enum Storage {
    Bundle(Box<Archive>),
    Legacy {
        tracks: Vec<LegacyTrack>,
        files: Vec<File>,
    },
}

/// Open cassette source. Discovery and reads start no providers or agents.
/// Archive selection remains separate; callers own private JSONL staging.
pub struct Source {
    storage: Storage,
    limits: Limits,
}

impl Source {
    /// Detect ZIP by bytes, not suffix. If the base is absent, inspect at most
    /// the supported 64 legacy concurrency paths, preserving sparse track IDs.
    /// A present base takes precedence, as in historical single-file replay.
    pub fn open(path: &Path, limits: Limits) -> Result<Self, Error> {
        limits.validate()?;
        let mut files = Vec::new();
        let mut tracks = Vec::new();
        let mut candidates = Vec::new();
        if let Some(mut file) = open_regular(path)? {
            let mut prefix = [0; 4];
            let count = file.read(&mut prefix).map_err(io_error)?;
            if count >= 2 && prefix[..2] == *b"PK" {
                return Ok(Self {
                    storage: Storage::Bundle(Box::new(Archive::open(path, limits)?)),
                    limits,
                });
            }
            candidates.push((file, path.to_path_buf(), 1));
        } else {
            for id in 1..=crate::benchmark::MAX_CONCURRENCY {
                let candidate = crate::benchmark_paths::replica_path(path, id);
                let Some(file) = open_regular(&candidate)? else {
                    continue;
                };
                candidates.push((file, candidate, id as u64));
            }
        }
        if candidates.is_empty() {
            return Err(Error(
                "cassette source is missing and no legacy concurrency tracks exist".into(),
            ));
        }
        if candidates.len() > limits.max_tracks {
            return Err(Error("legacy cassette layout exceeds track limit".into()));
        }
        // Bound the entire discovered layout before hashing even its first
        // payload. Recheck each file against the remaining budget below.
        let mut total = 0u64;
        for (file, _, _) in &candidates {
            let bytes = file.metadata().map_err(io_error)?.len();
            if bytes > limits.max_track_bytes {
                return Err(Error("legacy cassette exceeds byte limit".into()));
            }
            total = total
                .checked_add(bytes)
                .filter(|total| *total <= limits.max_total_bytes)
                .ok_or_else(|| Error("legacy cassette layout exceeds total byte limit".into()))?;
        }
        let mut remaining = limits.max_total_bytes;
        for (mut file, path, id) in candidates {
            let track = observe(
                &mut file,
                &path,
                id,
                Limits {
                    max_total_bytes: remaining,
                    ..limits
                },
            )?;
            remaining -= track.bytes;
            tracks.push(track);
            files.push(file);
        }
        Ok(Self {
            storage: Storage::Legacy { tracks, files },
            limits,
        })
    }

    /// Bundle metadata only; `None` means legacy unknown-metadata JSONL.
    pub fn manifest(&self) -> Option<&Manifest> {
        match &self.storage {
            Storage::Bundle(archive) => Some(archive.manifest()),
            Storage::Legacy { .. } => None,
        }
    }

    /// Observed legacy identities; empty for a bundle. No synthetic manifest or
    /// `complete=true` assertion is manufactured for an older file.
    pub fn legacy_tracks(&self) -> &[LegacyTrack] {
        match &self.storage {
            Storage::Legacy { tracks, .. } => tracks,
            Storage::Bundle(_) => &[],
        }
    }

    /// Read only the requested payload and verify its frozen observed identity.
    /// This does not parse provider requests or weaken strict proxy matching.
    pub fn read_track(&mut self, id: u64) -> Result<Vec<u8>, Error> {
        match &mut self.storage {
            Storage::Bundle(archive) => archive.read_track(id),
            Storage::Legacy { tracks, files } => {
                let index = tracks
                    .binary_search_by_key(&id, |track| track.id)
                    .map_err(|_| Error(format!("legacy cassette track {id} does not exist")))?;
                let expected = &tracks[index];
                let file = &mut files[index];
                file.seek(SeekFrom::Start(0)).map_err(io_error)?;
                let bound = expected
                    .bytes
                    .min(self.limits.max_track_bytes)
                    .saturating_add(1);
                let capacity = usize::try_from(bound).map_err(|_| {
                    Error("selected legacy cassette exceeds addressable memory".into())
                })?;
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(capacity).map_err(|_| {
                    Error("cannot allocate bounded selected legacy cassette".into())
                })?;
                let mut selected = file.take(bound);
                let mut buffer = [0; 64 * 1024];
                loop {
                    let count = selected.read(&mut buffer).map_err(io_error)?;
                    if count == 0 {
                        break;
                    }
                    // Capacity was reserved fallibly for the entire bounded
                    // read, so extending cannot invoke infallible allocation.
                    bytes.extend_from_slice(&buffer[..count]);
                }
                if bytes.len() as u64 != expected.bytes || super::digest(&bytes) != expected.sha256
                {
                    return Err(Error(
                        "legacy cassette changed after source discovery".into(),
                    ));
                }
                Ok(bytes)
            }
        }
    }
}

fn open_regular(path: &Path) -> Result<Option<File>, Error> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
        Ok(_) => (),
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io_error)?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(Error("cassette source must be a regular file".into()));
    }
    Ok(Some(file))
}

fn observe(file: &mut File, path: &Path, id: u64, limits: Limits) -> Result<LegacyTrack, Error> {
    let declared = file.metadata().map_err(io_error)?.len();
    if declared > limits.max_track_bytes || declared > limits.max_total_bytes {
        return Err(Error("legacy cassette exceeds byte limit".into()));
    }
    #[cfg(test)]
    OBSERVATIONS.with(|count| count.set(count.get() + 1));
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .filter(|length| *length <= declared)
            .ok_or_else(|| Error("legacy cassette grew during discovery".into()))?;
        hash.update(&buffer[..count]);
    }
    if length != declared || file.metadata().map_err(io_error)?.len() != declared {
        return Err(Error(
            "legacy cassette length changed during discovery".into(),
        ));
    }
    Ok(LegacyTrack {
        id,
        path: path.into(),
        bytes: length,
        sha256: format!("{:x}", hash.finalize()),
    })
}

#[cfg(test)]
thread_local! { static OBSERVATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[cfg(test)]
pub(super) fn take_observation_count() -> usize {
    OBSERVATIONS.with(|count| count.replace(0))
}

fn io_error(error: std::io::Error) -> Error {
    Error(format!("cannot read cassette source: {error}"))
}
