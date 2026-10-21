//! Complete, validate and synchronize private source bytes before atomic publication.
use super::*;
use std::io::Read;
use std::time::{Duration, Instant};

pub(super) const MAX_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) struct Options<'a> {
    pub kind: &'a str,
    pub limit: u64,
    pub checksum: Option<&'a str>,
    pub executable: bool,
}

struct Staging {
    directory: PathBuf,
    file: PathBuf,
}

impl Staging {
    fn new(destination: &Path) -> Result<Self, Error> {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let directory = parent.join(format!(".asb-install-{}", tools::runtime::unique_id()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(|error| {
            Error::Io(format!(
                "cannot stage installation beside {}: {error}",
                destination.display()
            ))
        })?;
        let staging = Self {
            // Multicall executables may choose their applet from argv[0]. Keep
            // that basename while the containing directory remains private.
            file: directory.join(
                destination
                    .file_name()
                    .unwrap_or(std::ffi::OsStr::new("source")),
            ),
            directory,
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&staging.file).map_err(|error| {
            Error::Io(format!(
                "cannot create private installation source: {error}"
            ))
        })?;
        Ok(staging)
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // Never recursively delete unknown entries or any published destination.
        let _ = fs::remove_file(&self.file);
        let _ = fs::remove_dir(&self.directory);
    }
}

struct DownloadChild(std::process::Child);
impl Drop for DownloadChild {
    fn drop(&mut self) {
        // Every early failure owns exactly this subprocess, not a process group.
        let _ = self.0.kill();
        let _ = self.0.wait();
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
    let mut child = DownloadChild(
        command
            .logged_spawn()
            .map_err(|error| super::missing_dependency("curl", options.kind, error))?,
    );
    let started = Instant::now();
    loop {
        let status = child.0.try_wait().map_err(|error| {
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
    }
}

/// Check a staged pinned executable without an unbounded version subprocess.
pub(crate) fn check_executable_version(path: &Path, label: &str) -> Result<(), Error> {
    check_version_with_timeout(path, label, Duration::from_secs(10))
}

fn check_version_with_timeout(path: &Path, label: &str, timeout: Duration) -> Result<(), Error> {
    let mut child = DownloadChild(
        Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .logged_spawn()
            .map_err(|error| {
                Error::Io(format!(
                    "cannot start staged {label}: {error}; previous installation was preserved"
                ))
            })?,
    );
    let started = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().map_err(crate::io_error)? {
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
    }
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
    let staging = Staging::new(destination)?;
    if source.starts_with("https://") || source.starts_with("http://") {
        download(source, &staging.file, &options, timeout)?;
    } else {
        let mut input_options = OpenOptions::new();
        input_options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Inspect the opened object without blocking on a named pipe.
            input_options.custom_flags(libc::O_NONBLOCK);
        }
        let input = input_options.open(source).map_err(|error| {
            Error::Input(format!(
                "cannot read {} source {source}: {error}",
                options.kind
            ))
        })?;
        let metadata = input.metadata().map_err(crate::io_error)?;
        if !metadata.is_file() {
            return Err(Error::Input(format!(
                "{} source is not a file: {source}",
                options.kind
            )));
        }
        if metadata.len() > options.limit {
            return Err(size_error(options.kind, options.limit));
        }
        let mut output = OpenOptions::new()
            .write(true)
            .open(&staging.file)
            .map_err(crate::io_error)?;
        let copied = io::copy(
            &mut input.take(options.limit.saturating_add(1)),
            &mut output,
        )
        .map_err(|error| Error::Io(format!("cannot copy {} source: {error}", options.kind)))?;
        if copied > options.limit {
            return Err(size_error(options.kind, options.limit));
        }
        fs::set_permissions(&staging.file, metadata.permissions()).map_err(crate::io_error)?;
    }
    validate_file(&staging.file, &options)?;
    if let Some(expected) = options.checksum {
        super::verify_sha256_preserving(&staging.file, expected, options.kind)?;
    }
    if options.executable {
        super::make_executable(&staging.file)?;
    }
    validate(&staging.file)?;
    fs::File::open(&staging.file)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            Error::Io(format!(
                "cannot synchronize staged {}: {error}",
                options.kind
            ))
        })?;
    // This is the only operation that changes the destination. The source is
    // on the same filesystem; readers see either the old or complete new file.
    fs::rename(&staging.file, destination).map_err(|error| {
        Error::Io(format!(
            "cannot install {} at {}: {error}",
            options.kind,
            destination.display()
        ))
    })
}

#[cfg(all(test, unix))]
mod publication_tests;
