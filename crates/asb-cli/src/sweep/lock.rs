//! Sweep-specific paths and diagnostics over shared output ownership.
use crate::Error;
use std::path::Path;

pub(super) struct OutputLock {
    _owner: crate::output_lock::OutputLock,
}

impl OutputLock {
    pub(super) fn acquire(parent: &Path, stem: &str) -> Result<Self, Error> {
        let path = parent.join(format!(".{stem}-asb-sweep.lock"));
        let busy = format!(
            "another sweep owns the output for `{stem}`; wait for it to finish or use a different manifest filename"
        );
        crate::output_lock::OutputLock::acquire(&path, &busy)
            .map(|owner| Self { _owner: owner })
            .map_err(Error::Run)
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
        let inherited_descriptor = owner._owner.cloned_descriptor();
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
        drop(inherited_descriptor);
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
