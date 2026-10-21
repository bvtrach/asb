//! Opened raw source custody. Consumption follows confirmed result publication.
use super::*;

pub(crate) struct Capture {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    stopped: Option<Metadata>,
}
impl Capture {
    /// Establish identity before any child/worker receives its output handle.
    /// Existing sources are never silently truncated/re-adopted as this attempt.
    pub(crate) fn create(path: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path).map_err(|error| {
            format!(
                "cannot create owned raw capture {}: {error}; preserve existing capture evidence",
                path.display()
            )
        })?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        Ok(Self {
            path: path.into(),
            file,
            metadata,
            stopped: None,
        })
    }
    pub(crate) fn writer(&self) -> Result<File, String> {
        self.file.try_clone().map_err(|error| {
            format!(
                "cannot prepare raw capture writer {}: {error}",
                self.path.display()
            )
        })
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn freeze(&mut self) -> Result<(), String> {
        self.stopped = Some(self.file.metadata().map_err(|error| {
            format!(
                "cannot freeze admitted raw source {}: {error}",
                self.path.display()
            )
        })?);
        Ok(())
    }
    pub(crate) fn reader(&self) -> Result<CaptureReader<'_>, String> {
        let length = self
            .stopped
            .as_ref()
            .ok_or("raw capture has not reached its stopped boundary")?
            .len();
        Ok(CaptureReader {
            file: &self.file,
            offset: 0,
            length,
        })
    }
    /// Startup acknowledgements also come from the admitted handle, while its
    /// producer is still running. This prefix is not a complete result reader.
    pub(crate) fn startup_diagnostic(&self, limit: usize) -> Result<String, String> {
        let length = self
            .file
            .metadata()
            .map_err(|error| error.to_string())?
            .len();
        let mut bytes = Vec::new();
        CaptureReader {
            file: &self.file,
            offset: 0,
            length,
        }
        .take(limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    pub(crate) fn verify_stopped(&self) -> Result<(), String> {
        let previous = self
            .stopped
            .as_ref()
            .ok_or("raw capture has not reached its stopped boundary")?;
        let now = self.file.metadata().map_err(|error| error.to_string())?;
        if previous.len() != now.len() || previous.modified().ok() != now.modified().ok() {
            return Err(format!(
                "admitted raw source {} changed after stop; no complete measurements or cleanup confirmed",
                self.path.display()
            ));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let file = open_regular(path)
            .map_err(|error| format!("cannot retain raw capture {}: {error}", path.display()))?
            .ok_or_else(|| {
                format!(
                    "raw capture {} is missing; cleanup ownership unavailable",
                    path.display()
                )
            })?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("cannot inspect raw capture {}: {error}", path.display()))?;
        Ok(Self {
            path: path.into(),
            file,
            stopped: Some(metadata.clone()),
            metadata,
        })
    }
    pub(crate) fn diagnostic(&self, limit: usize) -> Result<String, String> {
        let mut bytes = Vec::new();
        self.reader()?
            .take(limit as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!(
                    "cannot read raw capture {}: {error}; source not consumed",
                    self.path.display()
                )
            })?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    fn matches(&self, path: &Path) -> io::Result<bool> {
        let Some(file) = open_regular(path)? else {
            return Ok(false);
        };
        let now = file.metadata()?;
        let stopped = self
            .stopped
            .as_ref()
            .ok_or_else(|| io::Error::other("raw capture was not frozen after stop"))?;
        #[cfg(unix)]
        let same = {
            use std::os::unix::fs::MetadataExt;
            self.metadata.dev() == now.dev() && self.metadata.ino() == now.ino()
        };
        // Without a native file identity, retaining scratch is safer than
        // deleting a same-sized replacement. Publication itself remains usable.
        #[cfg(not(unix))]
        let same = false;
        Ok(same && stopped.len() == now.len() && stopped.modified()? == now.modified()?)
    }
    pub(crate) fn consume(&self) -> Result<(), String> {
        let operation = || -> io::Result<()> {
            if !self.matches(&self.path)? {
                return Err(io::Error::other(
                    "raw path changed or exact cleanup ownership is unavailable; no path was removed by cleanup",
                ));
            }
            let parent = self
                .path
                .parent()
                .ok_or_else(|| io::Error::other("raw capture has no parent"))?;
            #[cfg(test)]
            let _active = super::tests::Active::new(&self.path);
            let (stage, file) = Stage::create(parent)?;
            drop(file);
            // An atomic move, then a second identity check inside the private
            // directory, closes check/unlink's foreign-replacement gap.
            stage.settled.set(true);
            if let Err(error) = checkpoint(FaultPoint::CaptureMove) {
                let cleanup = stage.clean();
                return Err(io::Error::other(format!(
                    "{error}; original raw path retained{}",
                    cleanup.err().map_or(String::new(), |error| format!(
                        "; cleanup failed: {error}; inspect {}",
                        stage.directory.display()
                    ))
                )));
            }
            if let Err(error) = fs::rename(&self.path, &stage.path) {
                let cleanup = stage.clean();
                return Err(io::Error::other(format!(
                    "{error}; original raw path retained{}",
                    cleanup.err().map_or(String::new(), |error| format!(
                        "; cleanup failed: {error}; inspect {}",
                        stage.directory.display()
                    ))
                )));
            }
            if !self.matches(&stage.path).unwrap_or(false) {
                // Never overwrite a new arrival at the old pathname. Keep the
                // moved entry in private custody if no-replace restore fails.
                let restore = fs::symlink_metadata(&stage.path).and_then(|metadata| {
                    if !metadata.is_file() {
                        return Err(io::Error::other(
                            "moved entry is nonregular; automatic restore refused",
                        ));
                    }
                    fs::hard_link(&stage.path, &self.path)
                });
                match restore {
                    Ok(()) => {
                        return Err(io::Error::other(format!(
                            "raw identity changed during consumption; foreign entry restored and retained at {} and {}",
                            self.path.display(),
                            stage.path.display()
                        )));
                    }
                    Err(error) => {
                        return Err(io::Error::other(format!(
                            "raw identity changed during consumption; moved entry retained at {}; restore refused: {error}",
                            stage.path.display()
                        )));
                    }
                }
            }
            stage.clean().map_err(|error| {
                io::Error::other(format!(
                    "owned raw cleanup incomplete: {error}; inspect retained staging {}",
                    stage.directory.display()
                ))
            })
        };
        operation().map_err(|error| format!("raw capture {}: {error}", self.path.display()))
    }
}

/// Independent positional reads never seek a cloned stdio/sampler description.
/// The stopped length bounds EOF even if a descendant keeps an output open.
pub(crate) struct CaptureReader<'a> {
    file: &'a File,
    offset: u64,
    length: u64,
}
impl Read for CaptureReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = bytes.len().min(
            self.length
                .saturating_sub(self.offset)
                .min(usize::MAX as u64) as usize,
        );
        if count == 0 {
            return Ok(0);
        }
        #[cfg(unix)]
        let read = {
            use std::os::unix::fs::FileExt;
            self.file.read_at(&mut bytes[..count], self.offset)?
        };
        #[cfg(windows)]
        let read = {
            use std::os::windows::fs::FileExt;
            self.file.seek_read(&mut bytes[..count], self.offset)?
        };
        #[cfg(not(any(unix, windows)))]
        return Err(io::Error::other(
            "positional capture reads are unsupported on this platform",
        ));
        #[cfg(any(unix, windows))]
        {
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "admitted capture shortened after stop",
                ));
            }
            self.offset += read as u64;
            Ok(read)
        }
    }
}
