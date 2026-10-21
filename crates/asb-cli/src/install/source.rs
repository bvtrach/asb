//! Complete, validate and synchronize private source bytes before atomic publication.
use super::*;
use std::io::Read;
use std::time::{Duration, Instant};

pub(super) const MAX_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

mod prepared;
pub(super) use prepared::Input;
pub(super) use prepared::Quarantined;
pub(in crate::install) use prepared::Staging;
pub(super) use prepared::same_object;
pub(super) use prepared::settle_directory;
pub(super) use prepared::verify_pinned_checksum;
pub(super) use prepared::{open_named, same_named};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FaultPoint {
    Copy,
    Mode,
    Sync,
    Commit,
    CommitTake,
    CommitCopy,
    CommitSync,
    CommitRename,
    Cleanup,
    CleanupTake,
    GooseCleanupTake,
    DirectoryTake,
    GooseDirectoryTake,
    Recovery,
}
fn checkpoint(point: FaultPoint, path: &Path) -> Result<(), Error> {
    #[cfg(test)]
    faults::run(point, path)?;
    #[cfg(not(test))]
    let _ = (point, path);
    Ok(())
}
#[cfg(test)]
pub(super) mod faults {
    use super::*;
    type Action = Box<dyn FnOnce(&Path) -> Result<(), Error>>;
    std::thread_local! {static HOOK:std::cell::RefCell<Option<(FaultPoint,Action)>>=const {std::cell::RefCell::new(None)};}
    pub(in crate::install) struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            HOOK.with_borrow_mut(|hook| *hook = None);
        }
    }
    pub(in crate::install) fn set(
        point: FaultPoint,
        action: impl FnOnce(&Path) -> Result<(), Error> + 'static,
    ) -> Reset {
        arm(point, action);
        Reset
    }
    pub(in crate::install) fn arm(
        point: FaultPoint,
        action: impl FnOnce(&Path) -> Result<(), Error> + 'static,
    ) {
        HOOK.with_borrow_mut(|hook| {
            assert!(hook.is_none());
            *hook = Some((point, Box::new(action)));
        });
    }
    pub(super) fn run(point: FaultPoint, path: &Path) -> Result<(), Error> {
        let action = HOOK.with_borrow_mut(|hook| {
            if hook.as_ref().is_some_and(|value| value.0 == point) {
                hook.take().map(|value| value.1)
            } else {
                None
            }
        });
        action.map_or(Ok(()), |action| action(path))
    }
}

pub(super) struct Options<'a> {
    pub kind: &'a str,
    pub limit: u64,
    pub checksum: Option<&'a str>,
    pub executable: bool,
}

struct DownloadChild {
    child: Option<std::process::Child>,
    reaper: crate::process_cleanup::OwnedReaper,
}

impl DownloadChild {
    fn spawn(command: &mut Command, label: &str, dependency: Option<&str>) -> Result<Self, Error> {
        // Establish fallback custody before admitting any external process.
        let reaper = crate::process_cleanup::OwnedReaper::new("installation").map_err(Error::Io)?;
        let child = command.logged_spawn().map_err(|error| match dependency {
            Some(dependency) => super::missing_dependency(dependency, label, error),
            None => Error::Io(format!("cannot start {label}: {error}")),
        })?;
        let owned = Self {
            child: Some(child),
            reaper,
        };
        #[cfg(all(test, target_os = "linux"))]
        lifecycle_tests::record_admission(owned.child.as_ref().expect("admitted child").id());
        Ok(owned)
    }

    fn child(&mut self) -> &mut std::process::Child {
        self.child.as_mut().expect("owned installation child")
    }

    fn cleanup(&mut self) -> Result<(), Error> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if matches!(cleanup_poll(&mut child), Ok(Some(_))) {
            return Ok(());
        }
        let _ = child.kill();
        let timeout = Duration::from_secs(5);
        #[cfg(all(test, target_os = "linux"))]
        let timeout = lifecycle_tests::cleanup_timeout_override().unwrap_or(timeout);
        let deadline = Instant::now() + timeout;
        loop {
            let reason = match cleanup_poll(&mut child) {
                Ok(Some(_)) => return Ok(()),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Ok(None) => "child did not exit within five seconds".to_owned(),
                Err(error) => format!("cannot confirm child exit: {error}"),
            };
            self.reaper.adopt(child);
            return Err(Error::Io(format!(
                "installation process cleanup was not confirmed: {reason}; an owned reaper retains the child"
            )));
        }
    }

    fn finish<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        match (result, self.cleanup()) {
            (result, Ok(())) => result,
            (Ok(_), Err(cleanup)) => Err(cleanup),
            (Err(error), Err(cleanup)) => Err(Error::Io(format!("{error}; {cleanup}"))),
        }
    }
}

fn cleanup_poll(
    child: &mut std::process::Child,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    #[cfg(all(test, target_os = "linux"))]
    if let Some(result) = lifecycle_tests::cleanup_poll_override() {
        return result;
    }
    child.try_wait()
}

impl Drop for DownloadChild {
    fn drop(&mut self) {
        // Every early failure owns exactly this subprocess, not a process group.
        // Unwinding has the same bounded ownership handoff as ordinary errors.
        let _ = self.cleanup();
    }
}

fn size_error(kind: &str, limit: u64) -> Error {
    Error::Input(format!(
        "{kind} exceeds the installation size limit ({limit} bytes)"
    ))
}

fn validate_file(path: &Path, options: &Options<'_>) -> Result<fs::Metadata, Error> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| Error::Io(format!("cannot inspect staged {}: {error}", options.kind)))?;
    if !metadata.is_file() {
        return Err(Error::Input(format!(
            "staged {} is not a regular file",
            options.kind
        )));
    }
    if metadata.len() > options.limit {
        return Err(size_error(options.kind, options.limit));
    }
    if options.executable && metadata.len() == 0 {
        return Err(Error::Input(format!(
            "downloaded {} is empty; previous installation was preserved",
            options.kind
        )));
    }
    Ok(metadata)
}

fn download(
    source: &str,
    staging: &Path,
    options: &Options<'_>,
    timeout: Duration,
) -> Result<(), Error> {
    let mut command = Command::new("curl");
    // Ignore ~/.curlrc: inherited curl options must not redirect the output or
    // turn a failed HTTP response into a successfully installed executable.
    command
        .args([
            "--disable",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-time",
            "300",
            "--max-filesize",
        ])
        .arg(options.limit.to_string())
        .arg("--output")
        .arg(staging)
        .arg(source);
    command.stdin(Stdio::null());
    let mut child = DownloadChild::spawn(&mut command, options.kind, Some("curl"))?;
    let started = Instant::now();
    let result = (|| loop {
        let status = child.child().try_wait().map_err(|error| {
            Error::Io(format!(
                "cannot wait for {} download: {error}",
                options.kind
            ))
        })?;
        validate_file(
            staging,
            &Options {
                executable: false,
                ..*options
            },
        )?;
        if let Some(status) = status {
            return if status.success() {
                Ok(())
            } else {
                Err(Error::Io(format!(
                    "could not download {} from {source}: curl exited with {status}; previous installation was preserved",
                    options.kind
                )))
            };
        }
        if started.elapsed() >= timeout {
            return Err(Error::Io(format!(
                "{} download timed out; previous installation was preserved",
                options.kind
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    })();
    child.finish(result)
}

/// Check a staged pinned executable without an unbounded version subprocess.
pub(crate) fn check_executable_version(path: &Path, label: &str) -> Result<(), Error> {
    check_version_with_timeout(path, label, Duration::from_secs(10))
}

pub(super) fn command_status(
    command: &mut Command,
    label: &str,
    dependency: &str,
) -> Result<ExitStatus, Error> {
    let mut child = DownloadChild::spawn(command, label, Some(dependency))?;
    let deadline = Instant::now() + DOWNLOAD_TIMEOUT;
    let result = (|| loop {
        if let Some(status) = child.child().try_wait().map_err(crate::io_error)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(Error::Io(format!("{label} timed out")));
        }
        std::thread::sleep(Duration::from_millis(25));
    })();
    child.finish(result)
}

fn check_version_with_timeout(path: &Path, label: &str, timeout: Duration) -> Result<(), Error> {
    let mut child = DownloadChild::spawn(
        Command::new(path).arg("--version").stdin(Stdio::null()),
        &format!("staged {label}"),
        None,
    )
    .map_err(|error| Error::Io(format!("{error}; previous installation was preserved")))?;
    let started = Instant::now();
    let result = (|| loop {
        if let Some(status) = child.child().try_wait().map_err(crate::io_error)? {
            return if status.success() {
                Ok(())
            } else {
                Err(Error::Io(format!(
                    "staged {label} did not report a usable version ({status}); previous installation was preserved"
                )))
            };
        }
        if started.elapsed() >= timeout {
            return Err(Error::Io(format!(
                "staged {label} version probe timed out; previous installation was preserved"
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    })();
    child.finish(result)
}

pub(super) fn install(
    source: &str,
    destination: &Path,
    options: Options<'_>,
    validate: impl FnOnce(&Path) -> Result<(), Error>,
) -> Result<(), Error> {
    install_with_timeout(source, destination, options, validate, DOWNLOAD_TIMEOUT)
}

fn install_with_timeout(
    source: &str,
    destination: &Path,
    options: Options<'_>,
    validate: impl FnOnce(&Path) -> Result<(), Error>,
    timeout: Duration,
) -> Result<(), Error> {
    install_input_with_timeout(
        &Input::from_os(source.as_ref()),
        destination,
        options,
        validate,
        timeout,
        Publication::Replace,
    )
}

#[derive(Clone, Copy)]
pub(super) enum Publication {
    Replace,
    NoClobber,
}

pub(super) fn install_input(
    source: &Input,
    destination: &Path,
    options: Options<'_>,
    validate: impl FnOnce(&Path) -> Result<(), Error>,
    publication: Publication,
) -> Result<(), Error> {
    install_input_with_timeout(
        source,
        destination,
        options,
        validate,
        DOWNLOAD_TIMEOUT,
        publication,
    )
}

fn install_input_with_timeout(
    source: &Input,
    destination: &Path,
    options: Options<'_>,
    validate: impl FnOnce(&Path) -> Result<(), Error>,
    timeout: Duration,
    publication: Publication,
) -> Result<(), Error> {
    let mut staging = Staging::new(destination)?;
    let source_input = source;
    let result = (|| {
        if let Input::Remote(url) = source {
            download(url, &staging.file, &options, timeout)?;
        } else if let Input::Local(source) | Input::Pinned { path: source, .. } = source {
            let mut input_options = OpenOptions::new();
            input_options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                // Inspect the opened object without blocking on a named pipe.
                input_options.custom_flags(libc::O_NONBLOCK);
            }
            let mut input = if let Input::Pinned { handle, .. } = source_input {
                use std::io::{Seek, SeekFrom};
                let mut file = handle.try_clone().map_err(crate::io_error)?;
                file.seek(SeekFrom::Start(0)).map_err(crate::io_error)?;
                file
            } else {
                input_options.open(source).map_err(|error| {
                    Error::Input(format!(
                        "cannot read {} source {}: {error}",
                        options.kind,
                        source.display()
                    ))
                })?
            };
            let metadata = input.metadata().map_err(crate::io_error)?;
            if !metadata.is_file() {
                return Err(Error::Input(format!(
                    "{} source is not a file: {}",
                    options.kind,
                    source.display()
                )));
            }
            if metadata.len() > options.limit {
                return Err(size_error(options.kind, options.limit));
            }
            let mut output = staging.writer().map_err(crate::io_error)?;
            let copied = io::copy(
                &mut (&mut input).take(options.limit.saturating_add(1)),
                &mut output,
            )
            .map_err(|error| Error::Io(format!("cannot copy {} source: {error}", options.kind)))?;
            checkpoint(FaultPoint::Copy, &staging.file)?;
            if copied > options.limit {
                return Err(size_error(options.kind, options.limit));
            }
            if copied != metadata.len()
                || !prepared::unchanged_metadata(
                    &metadata,
                    &input.metadata().map_err(crate::io_error)?,
                )
            {
                return Err(Error::Input(
                    "local installation source changed during bounded copying".into(),
                ));
            }
            output
                .set_permissions(metadata.permissions())
                .map_err(crate::io_error)?;
        }
        staging.file_owned().map_err(crate::io_error)?;
        validate_file(&staging.file, &options)?;
        if options.executable {
            checkpoint(FaultPoint::Mode, &staging.file)?;
            staging.make_executable()?;
        }
        let pinned = staging.snapshot(&options)?;
        validate(&staging.file)?;
        let validated = staging.snapshot(&options)?;
        if !pinned.matches(&validated) {
            return Err(Error::Input("staged installation source changed after its version probe; previous installation was preserved".into()));
        }
        staging.synchronize()?;
        checkpoint(FaultPoint::Commit, &staging.file)?;
        if !pinned.matches(&staging.snapshot(&options)?) {
            return Err(Error::Input("staged installation source changed before commit; previous installation was preserved".into()));
        }
        staging.file_owned().map_err(crate::io_error)?;
        // This is the only operation that changes the destination. The source is
        // on the same filesystem; readers see either the old or complete new file.
        staging.publish_copy(destination, &options, &pinned, publication)
    })();
    staging.finish(result)
}

#[cfg(all(test, unix))]
mod publication_tests;
#[cfg(all(test, unix))]
mod repair_tests;

#[cfg(all(test, target_os = "linux"))]
mod lifecycle_tests;
