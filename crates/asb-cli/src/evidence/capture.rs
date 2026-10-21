//! Opened raw source custody. Consumption follows confirmed result publication.
use super::*;

pub(crate) struct Capture {
    path: PathBuf,
    file: File,
    metadata: Metadata,
}
impl Capture {
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
            metadata,
        })
    }
    pub(crate) fn diagnostic(&self, limit: usize) -> Result<String, String> {
        let mut bytes = Vec::new();
        (&self.file)
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
        #[cfg(unix)]
        let same = {
            use std::os::unix::fs::MetadataExt;
            self.metadata.dev() == now.dev() && self.metadata.ino() == now.ino()
        };
        // Without a native file identity, retaining scratch is safer than
        // deleting a same-sized replacement. Publication itself remains usable.
        #[cfg(not(unix))]
        let same = false;
        Ok(same
            && self.metadata.len() == now.len()
            && self.metadata.modified()? == now.modified()?)
    }
    pub(crate) fn consume(self) -> Result<(), String> {
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
