//! Bounded canonical identity of adapter-owned initial files, before evaluation.
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

const MAX_ENTRIES: usize = 100_000;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 128;

pub(super) fn fingerprint(root: &Path) -> Result<String, String> {
    fingerprint_with_limits(root, MAX_ENTRIES, MAX_BYTES, MAX_NAME_BYTES)
}

fn fingerprint_with_limits(
    root: &Path,
    entries: usize,
    bytes: u64,
    names: usize,
) -> Result<String, String> {
    let mut state = State {
        hash: Sha256::new(),
        entries,
        bytes,
        names,
    };
    state.hash.update(b"asb-initial-file-tree-v1\0");
    state.visit(root, root, 0)?;
    Ok(format!("{:x}", state.hash.finalize()))
}

struct State {
    hash: Sha256,
    entries: usize,
    bytes: u64,
    names: usize,
}

impl State {
    fn visit(&mut self, root: &Path, path: &Path, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH || self.entries == 0 {
            return Err("initial material exceeds entry/depth bounds".into());
        }
        self.entries -= 1;
        let metadata = fs::symlink_metadata(path).map_err(io_error)?;
        // Do not follow material outside the adapter-owned tree or read devices/FIFOs.
        if !metadata.is_file() && !metadata.is_dir() {
            return Err("initial material contains an unsupported symlink or special file".into());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "invalid material root")?;
        let name = relative
            .to_str()
            .ok_or("initial material names must be UTF-8")?;
        self.hash.update((name.len() as u64).to_le_bytes());
        self.hash.update(name.as_bytes());
        self.hash.update([u8::from(metadata.is_dir())]);
        if metadata.is_dir() {
            let mut children = Vec::new();
            for child in fs::read_dir(path).map_err(io_error)? {
                if children.len() >= self.entries {
                    return Err("initial material exceeds entry bounds".into());
                }
                let child = child.map_err(io_error)?.path();
                let relative = child
                    .strip_prefix(root)
                    .map_err(|_| "invalid material root")?;
                let name = relative
                    .to_str()
                    .ok_or("initial material names must be UTF-8")?;
                // Charge before retaining pending paths, not after sorting/allocation.
                self.names = self
                    .names
                    .checked_sub(name.len())
                    .ok_or("initial material names exceed bounds")?;
                children.push(child);
            }
            children.sort_unstable();
            self.hash.update((children.len() as u64).to_le_bytes());
            for child in children {
                self.visit(root, &child, depth + 1)?;
            }
        } else {
            self.bytes = self
                .bytes
                .checked_sub(metadata.len())
                .ok_or("initial material exceeds byte bound")?;
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            self.hash.update([u8::from(executable)]);
            self.hash.update(metadata.len().to_le_bytes());
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let mut file = options.open(path).map_err(io_error)?;
            let opened = file.metadata().map_err(io_error)?;
            if !opened.is_file() || opened.len() != metadata.len() {
                return Err("initial material changed while fingerprinting".into());
            }
            let mut remaining = metadata.len();
            let mut buffer = [0u8; 64 * 1024];
            while remaining > 0 {
                let bound = remaining.min(buffer.len() as u64) as usize;
                let count = file.read(&mut buffer[..bound]).map_err(io_error)?;
                if count == 0 {
                    return Err("initial material changed while fingerprinting".into());
                }
                self.hash.update(&buffer[..count]);
                remaining -= count as u64;
            }
            if file.read(&mut buffer[..1]).map_err(io_error)? != 0
                || file.metadata().map_err(io_error)?.modified().ok() != opened.modified().ok()
            {
                return Err("initial material changed while fingerprinting".into());
            }
        }
        Ok(())
    }
}

fn io_error(error: std::io::Error) -> String {
    format!("cannot establish initial material identity: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_excludes_root_and_creation_order_but_binds_names_bytes_and_empty_directories() {
        let root = std::env::temp_dir()
            .join(super::super::session::TaskIdentity::local("material", None).attempt_id);
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(a.join("empty")).unwrap();
        fs::create_dir_all(b.join("empty")).unwrap();
        fs::write(a.join("z"), "one").unwrap();
        fs::write(a.join("a"), "two").unwrap();
        fs::write(b.join("a"), "two").unwrap();
        fs::write(b.join("z"), "one").unwrap();
        let original = fingerprint(&a).unwrap();
        assert_eq!(original, fingerprint(&b).unwrap());
        fs::write(b.join("z"), "ONE").unwrap();
        assert_ne!(original, fingerprint(&b).unwrap());
        fs::write(b.join("z"), "one").unwrap();
        fs::rename(b.join("z"), b.join("y")).unwrap();
        assert_ne!(original, fingerprint(&b).unwrap());
        fs::rename(b.join("y"), b.join("z")).unwrap();
        fs::remove_dir(b.join("empty")).unwrap();
        assert_ne!(original, fingerprint(&b).unwrap());
        assert!(fingerprint_with_limits(&a, 1, MAX_BYTES, MAX_NAME_BYTES).is_err());
        assert!(fingerprint_with_limits(&a, MAX_ENTRIES, 1, MAX_NAME_BYTES).is_err());
        assert!(fingerprint_with_limits(&a, MAX_ENTRIES, MAX_BYTES, 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn executable_bit_is_bound_and_symlinks_are_not_followed() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = std::env::temp_dir()
            .join(super::super::session::TaskIdentity::local("material", None).attempt_id);
        fs::create_dir(&root).unwrap();
        let path = root.join("file");
        fs::write(&path, "content").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let original = fingerprint(&root).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        assert_ne!(original, fingerprint(&root).unwrap());
        symlink("/does/not/exist", root.join("link")).unwrap();
        assert!(fingerprint(&root).unwrap_err().contains("symlink"));
        fs::remove_dir_all(root).unwrap();
    }
}
