//! OS-owned sweep output exclusion, independent of archived result trees.
use crate::Error;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

/// The persistent lock file is never removed: unlinking it could split owners
/// across different inodes. Closing this guard releases the operating-system
/// lock, including when the process exits unexpectedly.
pub(super) struct OutputLock {
    _file: File,
}

impl OutputLock {
    pub(super) fn acquire(parent: &Path, stem: &str) -> Result<Self, Error> {
        let path = parent.join(format!(".{stem}-asb-sweep.lock"));
        let failure = |error| {
            Error::Run(format!(
                "cannot lock sweep output at {}: {error}",
                path.display()
            ))
        };
        // Do not truncate an existing file or follow a lock-path symlink.
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(Error::Run(format!(
                    "sweep output lock {} must be a regular file, not a symlink or directory",
                    path.display()
                )));
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
            // O_NONBLOCK also avoids hanging if a racing path replacement is
            // a FIFO. The opened descriptor is checked before taking the lock.
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(&path).map_err(failure)?;
        if !file.metadata().map_err(failure)?.is_file() {
            return Err(Error::Run(format!(
                "sweep output lock {} is not a regular file",
                path.display()
            )));
        }
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(Error::Run(format!(
                "another sweep owns the output for `{stem}`; wait for it to finish or use a different manifest filename (lock: {})",
                path.display()
            ))),
            Err(TryLockError::Error(error)) => Err(failure(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_child() {
        let Some(root) = std::env::var_os("ASB_SWEEP_LOCK_TEST_PARENT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let _owner = OutputLock::acquire(&root, "sweep").unwrap();
        std::fs::write(root.join("ready"), "ready").unwrap();
        let mut byte = [0];
        let _ = std::io::Read::read(&mut std::io::stdin(), &mut byte);
    }

    #[test]
    fn abrupt_writer_exit_releases_only_its_output_lock() {
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("sweep-lock-process", None).attempt_id);
        std::fs::create_dir_all(&root).unwrap();
        let unrelated = OutputLock::acquire(&root, "other").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "sweep::lock::tests::lock_child", "--nocapture"])
            .env("ASB_SWEEP_LOCK_TEST_PARENT", &root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !root.join("ready").is_file() && std::time::Instant::now() < deadline {
            assert!(
                child.try_wait().unwrap().is_none(),
                "lock fixture exited early"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !root.join("ready").is_file() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("lock fixture did not become ready");
        }
        assert!(OutputLock::acquire(&root, "sweep").is_err());
        child.kill().unwrap();
        child.wait().unwrap();
        let successor = OutputLock::acquire(&root, "sweep").unwrap();
        assert!(OutputLock::acquire(&root, "other").is_err());
        drop((successor, unrelated));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sweep_output_ownership_excludes_competitors_and_releases_on_drop() {
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("sweep-lock", None).attempt_id);
        std::fs::create_dir_all(root.join("other")).unwrap();
        let path = root.join(".sweep-asb-sweep.lock");
        std::fs::write(&path, "existing contents must not be truncated").unwrap();
        let owner = OutputLock::acquire(&root, "sweep").unwrap();
        let conflict = OutputLock::acquire(&root, "sweep").err().unwrap();
        assert!(conflict.to_string().contains("another sweep owns"));
        let unrelated_stem = OutputLock::acquire(&root, "different").unwrap();
        let unrelated_parent = OutputLock::acquire(&root.join("other"), "sweep").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "existing contents must not be truncated"
        );
        drop((owner, unrelated_stem, unrelated_parent));
        let successor = OutputLock::acquire(&root, "sweep").unwrap();
        drop(successor);
        assert!(path.is_file(), "do not unlink and split future lock owners");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn sweep_output_lock_rejects_symlinks_and_non_regular_files() {
        let root = std::env::temp_dir()
            .join(crate::workloads::TaskIdentity::local("sweep-lock-path", None).attempt_id);
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("untouched");
        std::fs::write(&target, "keep").unwrap();
        std::os::unix::fs::symlink(&target, root.join(".linked-asb-sweep.lock")).unwrap();
        std::fs::create_dir(root.join(".directory-asb-sweep.lock")).unwrap();
        for stem in ["linked", "directory"] {
            assert!(OutputLock::acquire(&root, stem).is_err());
        }
        assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
        std::fs::remove_dir_all(root).unwrap();
    }
}
