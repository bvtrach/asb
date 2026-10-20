//! Private bounded command output, drained one configuration at a time.
use crate::{Error, benchmark};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

pub(super) struct Output {
    file: File,
    remaining: u64,
}

fn limit(replicas: usize) -> u64 {
    replicas as u64 * 4 * 1024 * 1024 + 65536
}

impl Output {
    pub(super) fn create(path: &Path, replicas: usize) -> Result<Self, Error> {
        if !(1..=benchmark::MAX_CONCURRENCY).contains(&replicas) {
            return Err(Error::Input("invalid output spool replica count".into()));
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(Self {
            file: options.open(path).map_err(|error| {
                Error::Io(format!(
                    "cannot create private sweep output {}: {error}",
                    path.display()
                ))
            })?,
            remaining: limit(replicas),
        })
    }
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other(
                "sweep command output exceeded its per-attempt limit; inspect the agent log instead of --show-output",
            ));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(super) fn drain(path: &Path, replicas: usize, output: &mut dyn Write) -> Result<(), Error> {
    if !(1..=benchmark::MAX_CONCURRENCY).contains(&replicas) {
        return Err(Error::Input("invalid output spool replica count".into()));
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(crate::io_error(error)),
    };
    if !metadata.is_file() || metadata.len() > limit(replicas) {
        return Err(Error::Io(
            "private sweep output is not a bounded regular file".into(),
        ));
    }
    let mut file = OpenOptions::new();
    file.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let result = file.open(path).and_then(|file| {
        let copied = io::copy(&mut file.take(limit(replicas) + 1), output)?;
        if copied > limit(replicas) {
            Err(io::Error::other(
                "private sweep output grew beyond its bound",
            ))
        } else {
            Ok(())
        }
    });
    let cleanup = std::fs::remove_file(path);
    result.and(cleanup).map_err(crate::io_error)
}

pub(super) fn discard(path: &Path) -> Result<(), Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(crate::io_error(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(crate::workloads::TaskIdentity::local("spool", None).attempt_id);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn output_is_private_bounded_non_overwriting_and_drained_once() {
        let root = Directory::new();
        let path = root.0.join("output");
        let mut spool = Output::create(&path, 1).unwrap();
        spool.remaining = 3;
        spool.write_all(b"abc").unwrap();
        assert!(spool.write_all(b"more").is_err());
        spool.flush().unwrap();
        drop(spool);
        assert!(Output::create(&path, 1).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let mut output = Vec::new();
        drain(&path, 1, &mut output).unwrap();
        assert_eq!(output, b"abc");
        assert!(!path.exists());
        drain(&path, 1, &mut output).unwrap();
        assert_eq!(output, b"abc");
    }

    #[test]
    fn broken_terminal_and_discard_remove_only_the_temporary_duplicate() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("broken output fixture"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let root = Directory::new();
        let path = root.0.join("output");
        std::fs::write(root.0.join("agent.log"), "retained").unwrap();
        Output::create(&path, 1)
            .unwrap()
            .write_all(b"result")
            .unwrap();
        assert!(drain(&path, 1, &mut Broken).is_err());
        assert!(!path.exists());
        Output::create(&path, 1).unwrap();
        discard(&path).unwrap();
        discard(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.0.join("agent.log")).unwrap(),
            "retained"
        );
        assert!(Output::create(&path, 0).is_err());
        assert!(Output::create(&path, 65).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn draining_does_not_follow_links_or_read_oversized_files() {
        let root = Directory::new();
        let target = root.0.join("unrelated");
        std::fs::write(&target, "private").unwrap();
        let link = root.0.join("linked");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(drain(&link, 1, &mut Vec::new()).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "private");
        let path = root.0.join("oversized");
        let file = File::create(&path).unwrap();
        file.set_len(limit(1) + 1).unwrap();
        assert!(drain(&path, 1, &mut Vec::new()).is_err());
        assert!(
            path.exists(),
            "retain unexpected evidence instead of following/deleting it"
        );
    }
}
