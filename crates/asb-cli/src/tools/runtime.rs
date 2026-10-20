//! Shared publication of pinned, project-local language runtime archives.
use crate::install::{LoggedStatus, install_source, verify_sha256};
use crate::{Error, workloads::TaskIdentity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn archive(
    root: &Path,
    url: &str,
    checksum: &str,
    programs: &[(&str, &str)],
    progress: &mut dyn FnMut(u8, &str),
) -> Result<PathBuf, Error> {
    if !cfg!(target_os = "linux") {
        return Err(Error::Input(
            "project-local bootstrap runtimes currently support Linux".into(),
        ));
    }
    let cache = root.join("cache");
    fs::create_dir_all(&cache).map_err(|error| Error::Io(error.to_string()))?;
    let filename = url.rsplit('/').next().expect("runtime archive name");
    let download = cache.join(filename);
    if !download.is_file() || verify_sha256(&download, checksum, "runtime archive").is_err() {
        if download.exists() {
            fs::rename(
                &download,
                download.with_extension(format!(
                    "backup-{}",
                    TaskIdentity::local("runtime", None).attempt_id
                )),
            )
            .map_err(|error| {
                Error::Io(format!("cannot preserve damaged runtime archive: {error}"))
            })?;
        }
        progress(15, "downloading pinned runtime");
        install_source(url, &download, "language runtime")?;
    }
    verify_sha256(&download, checksum, "runtime archive")?;
    let runtime = root.join("runtime");
    let receipt = root.join("ASB-RUNTIME");
    if fs::read_to_string(&receipt).ok().as_deref() != Some(checksum)
        || programs
            .iter()
            .any(|(_, path)| !runtime.join(path).is_file())
    {
        let stage = root.join(format!(
            ".runtime-{}",
            TaskIdentity::local("runtime", None).attempt_id
        ));
        fs::create_dir(&stage).map_err(|error| Error::Io(error.to_string()))?;
        progress(50, "extracting project runtime");
        let status = Command::new("tar")
            .args(["--extract", "--file"])
            .arg(&download)
            .arg("--directory")
            .arg(&stage)
            .args([
                "--strip-components=1",
                "--no-same-owner",
                "--no-same-permissions",
            ])
            .logged_status()
            .map_err(|error| Error::Io(format!("cannot extract language runtime: {error}")))?;
        if !status.success() || programs.iter().any(|(_, path)| !stage.join(path).is_file()) {
            return Err(Error::Io(format!(
                "runtime archive extraction failed; retained evidence at {}",
                stage.display()
            )));
        }
        if runtime.exists() {
            fs::rename(
                &runtime,
                root.join(format!(
                    "runtime.asb-backup-{}",
                    TaskIdentity::local("runtime", None).attempt_id
                )),
            )
            .map_err(|error| Error::Io(format!("cannot preserve previous runtime: {error}")))?;
        }
        fs::rename(&stage, &runtime).map_err(|error| Error::Io(error.to_string()))?;
        fs::write(&receipt, checksum).map_err(|error| Error::Io(error.to_string()))?;
    }
    let bin = root.join("bin");
    fs::create_dir_all(&bin).map_err(|error| Error::Io(error.to_string()))?;
    for (name, relative) in programs {
        let target = bin.join(name);
        let temporary = bin.join(format!(
            ".{name}-{}",
            TaskIdentity::local("runtime", None).attempt_id
        ));
        #[cfg(unix)]
        std::os::unix::fs::symlink(Path::new("../runtime").join(relative), &temporary)
            .map_err(|error| Error::Io(error.to_string()))?;
        #[cfg(not(unix))]
        fs::copy(runtime.join(relative), &temporary)
            .map_err(|error| Error::Io(error.to_string()))?;
        fs::rename(&temporary, &target).map_err(|error| {
            Error::Io(format!(
                "cannot publish runtime executable {}: {error}",
                target.display()
            ))
        })?;
    }
    progress(100, "runtime ready");
    Ok(bin.join(programs[0].0))
}
