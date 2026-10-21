//! Read-only campaign layout and exact outcome references. No resume authority.
use super::{OutcomeReference, Plan, Slot};
use crate::{Error, io_error};
use serde_json::Value;
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_PLAN_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REFERENCE_BYTES: u64 = 16 * 1024;
const MAX_OUTCOME_BYTES: u64 = 1024 * 1024;
const MAX_OBSERVATION_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) struct Layout {
    pub plan: Plan,
    pub root: PathBuf,
    remaining: Cell<u64>,
}

pub(crate) struct ReferencedOutcome {
    pub path: PathBuf,
    pub value: Value,
}

fn invalid(reason: &str) -> Error {
    Error::Input(format!("invalid campaign report input: {reason}"))
}

pub(crate) fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>, Error> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io_error)?;
    bounded_file(file, limit)
}

fn bounded_file(file: File, limit: u64) -> Result<Vec<u8>, Error> {
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid("input must be a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(metadata.len() + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid("input length changed while reading"));
    }
    Ok(bytes)
}

impl Layout {
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = bounded(path, MAX_PLAN_BYTES)?;
        let plan: Plan =
            serde_json::from_slice(&bytes).map_err(|_| invalid("malformed execution plan"))?;
        plan.validate_layout()?;
        let root = std::path::absolute(
            path.parent()
                .ok_or_else(|| invalid("plan needs a campaign parent"))?,
        )
        .map_err(io_error)?;
        if root.file_name().and_then(|name| name.to_str()) != Some(plan.campaign.as_str())
            || !fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.is_dir())
        {
            return Err(invalid(
                "plan campaign does not match its real root directory",
            ));
        }
        let layout = Self {
            plan,
            root,
            remaining: Cell::new(MAX_OBSERVATION_BYTES),
        };
        layout.preflight()?;
        Ok(layout)
    }

    /// Bound the complete primary observation population before any outcome
    /// hash or recording payload verification. Actual reads share another budget
    /// so growth/replacement after this inspection cannot evade the cap.
    fn preflight(&self) -> Result<(), Error> {
        let mut total = 0u64;
        let mut inspect = |path: &Path, limit: u64| -> Result<(), Error> {
            let metadata = match fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(io_error(error)),
                Ok(metadata) => metadata,
            };
            if !metadata.is_file() || metadata.len() > limit {
                return Err(invalid("observation must be a bounded regular file"));
            }
            total = total
                .checked_add(metadata.len())
                .filter(|value| *value <= MAX_OBSERVATION_BYTES)
                .ok_or_else(|| {
                    invalid(
                        "aggregate primary observations exceed 256 MiB; select a smaller campaign",
                    )
                })?;
            Ok(())
        };
        for slot in &self.plan.slots {
            let reference_path = self.root.join(format!(
                "batch-{}/outcomes/{}.json",
                slot.batch, slot.replica
            ));
            inspect(&reference_path, MAX_REFERENCE_BYTES)?;
            if reference_path.exists() {
                let bytes = self.read_observation(&reference_path, MAX_REFERENCE_BYTES)?;
                let reference: OutcomeReference = serde_json::from_slice(&bytes)
                    .map_err(|_| invalid("malformed outcome reference"))?;
                self.validate_reference(slot, &reference)?;
                inspect(&self.root.join(reference.path), MAX_OUTCOME_BYTES)?;
            }
            inspect(&self.root.join(&slot.log), MAX_PLAN_BYTES)?;
            if let Some(path) = &slot.measurements {
                inspect(
                    &self
                        .plan
                        .measurement_root
                        .as_ref()
                        .expect("validated measurement root")
                        .join(path)
                        .join("run.json"),
                    MAX_PLAN_BYTES,
                )?;
            }
        }
        Ok(())
    }

    fn read_observation(&self, path: &Path, limit: u64) -> Result<Vec<u8>, Error> {
        let limit = limit.min(self.remaining.get());
        #[cfg(unix)]
        let bytes = {
            let (root, relative) = if let Ok(relative) = path.strip_prefix(&self.root) {
                (&self.root, relative)
            } else {
                let root = self
                    .plan
                    .measurement_root
                    .as_ref()
                    .ok_or_else(|| invalid("observation is outside campaign roots"))?;
                (
                    root,
                    path.strip_prefix(root)
                        .map_err(|_| invalid("observation is outside campaign roots"))?,
                )
            };
            bounded_file(rooted::open(root, relative)?, limit)?
        };
        #[cfg(not(unix))]
        let bytes = bounded(path, limit)?;
        self.remaining
            .set(self.remaining.get() - bytes.len() as u64);
        Ok(bytes)
    }

    pub fn measurement(&self, slot: &Slot) -> Result<Option<Value>, Error> {
        let Some(relative) = &slot.measurements else {
            return Ok(None);
        };
        let path = self
            .plan
            .measurement_root
            .as_ref()
            .expect("validated measurement root")
            .join(relative)
            .join("run.json");
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
            Ok(_) => (),
        }
        serde_json::from_slice(&self.read_observation(&path, MAX_PLAN_BYTES)?)
            .map(Some)
            .map_err(|_| invalid("malformed measurement record"))
    }

    /// Missing references remain missing observations; malformed/present ones
    /// must not silently fall back to an unrelated or newer log.
    pub fn outcome(&self, slot: &Slot) -> Result<Option<ReferencedOutcome>, Error> {
        let path = self.root.join(format!(
            "batch-{}/outcomes/{}.json",
            slot.batch, slot.replica
        ));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
            Ok(_) => (),
        }
        let bytes = self.read_observation(&path, MAX_REFERENCE_BYTES)?;
        let reference: OutcomeReference =
            serde_json::from_slice(&bytes).map_err(|_| invalid("malformed outcome reference"))?;
        self.validate_reference(slot, &reference)?;
        let path = self.root.join(&reference.path);
        let bytes = self.read_observation(&path, MAX_OUTCOME_BYTES)?;
        if bytes.len() as u64 != reference.bytes
            || crate::cassette::digest(&bytes) != reference.sha256
        {
            return Err(invalid(
                "referenced outcome bytes differ from immutable association",
            ));
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("malformed referenced outcome"))?;
        if value["schema_version"] != 2
            || value["replica"]
                != serde_json::json!({"index":slot.replica,"total":self.plan.concurrency})
            || value["summary"]["execution"]
                != serde_json::to_value(self.plan.execution(slot))
                    .map_err(|error| invalid(&error.to_string()))?
            || value["summary"]["task"] != reference.task
        {
            return Err(invalid(
                "outcome summary differs from planned execution or referenced task",
            ));
        }
        Ok(Some(ReferencedOutcome { path, value }))
    }

    fn validate_reference(&self, slot: &Slot, reference: &OutcomeReference) -> Result<(), Error> {
        if reference.schema_version != 1
            || reference.execution != self.plan.execution(slot)
            || reference.bytes > MAX_OUTCOME_BYTES
            || !crate::cassette::valid_digest(&reference.sha256)
        {
            return Err(invalid(
                "outcome reference identity or bounds differ from planned slot",
            ));
        }
        let name = reference
            .path
            .file_name()
            .ok_or_else(|| invalid("outcome reference has no filename"))?;
        let expected = PathBuf::from(format!("batch-{}/.asb-completions", slot.batch)).join(name);
        if reference.path.as_os_str() != expected.as_os_str()
            || reference
                .path
                .extension()
                .is_none_or(|value| value != "json")
            || name.to_str().is_none_or(|name| name.starts_with('.'))
        {
            return Err(invalid(
                "outcome reference is outside its batch completion namespace",
            ));
        }
        Ok(())
    }

    pub fn log(&self, slot: &Slot) -> Result<Option<(PathBuf, Value)>, Error> {
        let path = self.root.join(&slot.log);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
            Ok(_) => (),
        }
        let bytes = self.read_observation(&path, MAX_PLAN_BYTES)?;
        let mut lines = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace));
        let line = lines
            .next()
            .ok_or_else(|| invalid("empty execution-private log"))?;
        if lines.next().is_some() {
            return Err(invalid("multiple records claim one execution-private log"));
        }
        let value: Value =
            serde_json::from_slice(line).map_err(|_| invalid("malformed execution-private log"))?;
        if value["execution"]
            != serde_json::to_value(self.plan.execution(slot))
                .map_err(|error| invalid(&error.to_string()))?
        {
            return Err(invalid("execution log differs from planned slot identity"));
        }
        Ok(Some((path, value)))
    }
}

/// Descriptor-relative traversal rejects intermediate aliases as well as final
/// symlinks. This is path confinement, not historical artifact authentication.
#[cfg(unix)]
mod rooted {
    #![allow(unsafe_code)]
    use super::*;
    use std::ffi::{CString, OsStr};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};

    fn child(parent: &File, name: &OsStr, directory: bool) -> Result<File, Error> {
        let name =
            CString::new(name.as_bytes()).map_err(|_| invalid("invalid observation component"))?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: the borrowed parent remains open, name is NUL-terminated, and
        // successful openat yields a fresh descriptor owned only by this File.
        let descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }

    pub(super) fn open(root: &Path, relative: &Path) -> Result<File, Error> {
        let mut directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root)
            .map_err(io_error)?;
        let mut parts = relative.components().peekable();
        while let Some(part) = parts.next() {
            let std::path::Component::Normal(name) = part else {
                return Err(invalid("observation path is not relative and canonical"));
            };
            let file = child(&directory, name, parts.peek().is_some())?;
            if parts.peek().is_none() {
                return Ok(file);
            }
            directory = file;
        }
        Err(invalid("observation path has no filename"))
    }
}
