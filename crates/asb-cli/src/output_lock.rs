//! Private persistent-inode output ownership shared by sweeps and cassettes.
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

pub(crate) struct OutputLock {
    file: File,
}

impl Drop for OutputLock {
    fn drop(&mut self) {
        // A fork can retain a descriptor before exec: explicit unlock releases
        // ownership even during that window. Never unlink the persistent inode.
        let _ = self.file.unlock();
    }
}

impl OutputLock {
    pub(crate) fn acquire(path: &Path, busy: &str) -> Result<Self, String> {
        let failure = |error| format!("cannot lock output at {}: {error}", path.display());
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(format!(
                    "output lock {} must be a regular file, not a symlink or directory",
                    path.display()
                ));
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(failure(error)),
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path).map_err(failure)?;
        if !file.metadata().map_err(failure)?.is_file() {
            return Err(format!(
                "output lock {} is not a regular file",
                path.display()
            ));
        }
        match file.try_lock() {
            Ok(()) => Ok(Self { file }),
            Err(TryLockError::WouldBlock) => Err(format!("{busy} (lock: {})", path.display())),
            Err(TryLockError::Error(error)) => Err(failure(error)),
        }
    }

    #[cfg(test)]
    pub(crate) fn cloned_descriptor(&self) -> File {
        self.file
            .try_clone()
            .expect("clone locked fixture descriptor")
    }
}
