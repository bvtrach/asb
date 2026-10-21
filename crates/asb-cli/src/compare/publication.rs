//! Report-specific ownership and coherent Linux generation publication.
use super::*;
use crate::output_lock::OutputLock;

pub(super) struct Publisher {
    destination: PathBuf,
    stage: PathBuf,
    files: Vec<String>,
    committed: bool,
    _lock: OutputLock,
}
pub(super) struct Published {
    pub report: PathBuf,
    pub backup: Option<PathBuf>,
}
impl Published {
    pub(super) fn confirmation_error(&self, error: impl std::fmt::Display) -> Error {
        Error::Io(format!(
            "Report was published at {}, but confirmation output failed: {error}. Previous report backup: {}",
            self.report.display(),
            self.backup.as_ref().map_or_else(
                || "none (first report)".into(),
                |path| path.display().to_string()
            )
        ))
    }
}
fn check_destination(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(Error::Input(format!(
            "report destination {} must be a real directory, not a symlink or file; preserve it and choose another report location",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(Error::Io(format!(
            "cannot inspect report destination {}: {error}",
            path.display()
        ))),
    }
}
impl Publisher {
    /// Called only after all source validation, aggregation and preview reads.
    pub(super) fn begin(destination: &Path, files: Vec<String>) -> Result<Self, Error> {
        check_destination(destination)?;
        let parent = destination
            .parent()
            .ok_or_else(|| Error::Input("report destination has no parent".into()))?;
        fs::create_dir_all(parent).map_err(crate::io_error)?;
        let name = destination
            .file_name()
            .ok_or_else(|| Error::Input("report destination has no filename".into()))?
            .to_string_lossy();
        let lock = OutputLock::acquire(
            &parent.join(format!(".{name}.asb-report.lock")),
            "another comparison report writer is active",
        )
        .map_err(Error::Io)?;
        check_destination(destination)?;
        let stage = parent.join(format!(
            ".{name}.asb-report-backup-{}",
            crate::tools::runtime::unique_id()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&stage).map_err(crate::io_error)?;
        Ok(Self {
            destination: destination.into(),
            stage,
            files,
            committed: false,
            _lock: lock,
        })
    }
    pub(super) fn stage(&self) -> &Path {
        &self.stage
    }
    pub(super) fn commit(mut self) -> Result<Published, Error> {
        // Synchronize every known generated file before making the generation
        // visible. Unknown entries are never removed by cleanup.
        for name in &self.files {
            fs::File::open(self.stage.join(name))
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    Error::Io(format!(
                        "report preparation was not published: cannot synchronize {name}: {error}"
                    ))
                })?;
        }
        fs::File::open(&self.stage)
            .and_then(|file| file.sync_all())
            .map_err(crate::io_error)?;
        let previous = check_destination(&self.destination)?;
        exchange(&self.stage, &self.destination, previous)?;
        self.committed = true;
        let published = Published {
            report: self.destination.join("report.html"),
            backup: previous.then(|| self.stage.clone()),
        };
        let parent = self.destination.parent().expect("validated report parent");
        if let Err(error) = fs::File::open(parent).and_then(|file| file.sync_all()) {
            return Err(Error::Io(format!(
                "Report was published at {}, but directory durability is unconfirmed: {error}. Previous report remains at {}",
                published.report.display(),
                published.backup.as_ref().map_or_else(
                    || "no previous generation".into(),
                    |path| path.display().to_string()
                )
            )));
        }
        Ok(published)
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for name in &self.files {
            let _ = fs::remove_file(self.stage.join(name));
        }
        let _ = fs::remove_dir(&self.stage);
    }
}
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn exchange(stage: &Path, destination: &Path, previous: bool) -> Result<(), Error> {
    use std::os::unix::ffi::OsStrExt;
    let stage = std::ffi::CString::new(stage.as_os_str().as_bytes())
        .map_err(|_| Error::Input("report staging path contains a NUL byte".into()))?;
    let destination_c = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| Error::Input("report destination contains a NUL byte".into()))?;
    let flags = if previous {
        libc::RENAME_EXCHANGE
    } else {
        libc::RENAME_NOREPLACE
    };
    // SAFETY: both C strings are live, NUL-terminated owned filesystem paths;
    // the report lock serializes ASB writers. Neither path is a broad root.
    if unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            stage.as_ptr(),
            libc::AT_FDCWD,
            destination_c.as_ptr(),
            flags,
        )
    } == 0
    {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    Err(Error::Io(format!(
        "report was not published; existing report at {} is preserved: atomic report replacement failed: {error}. If this filesystem lacks atomic exchange, manually back up/move the existing report directory before retrying",
        destination.display()
    )))
}
#[cfg(not(target_os = "linux"))]
fn exchange(_: &Path, destination: &Path, _: bool) -> Result<(), Error> {
    Err(Error::Input(format!(
        "report was not published; existing report at {} is preserved. Coherent report publication currently requires Linux atomic rename/exchange support; copy the raw results to a supported host. No non-atomic replacement was attempted",
        destination.display()
    )))
}
