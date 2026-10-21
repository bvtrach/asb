//! Shared pinned original source archive for all three SWE-bench populations.
use super::SweBenchAdapter;
use crate::Error;
use crate::install::LoggedStatus;
use crate::workloads::{InstallationContext, TaskIdentity};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

const ARCHIVE: &str = "swe-bench-tasks-3d07b464b7b311a0cbfb5ed5b2d8a3b96f84a33d.tar.gz";
const URL: &str = "https://codeload.github.com/SWE-bench/swe-bench-tasks/tar.gz/3d07b464b7b311a0cbfb5ed5b2d8a3b96f84a33d";

pub(super) fn install(
    adapter: &SweBenchAdapter,
    context: &InstallationContext<'_>,
    output: &mut dyn Write,
    progress: &mut dyn FnMut(u8, &str),
) -> Result<(), Error> {
    let tool = context.tools_root.join("swebench");
    let catalog = context.workloads_root.join(adapter.id).join("catalog.json");
    let verify = |archive: &Path, catalog: Option<&Path>| -> Result<bool, Error> {
        let mut command = Command::new(tool.join("venv/bin/python"));
        command
            .arg(tool.join("asb_swebench_sources.py"))
            .arg("--archive")
            .arg(archive)
            .env("PYTHONDONTWRITEBYTECODE", "1");
        if let Some(catalog) = catalog {
            command.arg("--catalog").arg(catalog);
        }
        Ok(command
            .logged_status()
            .map_err(|error| {
                Error::Io(format!(
                    "cannot validate original SWE-bench source: {error}"
                ))
            })?
            .success())
    };
    let destination = tool.join(ARCHIVE);
    if destination.is_file() && verify(&destination, Some(&catalog))? {
        return Ok(());
    }
    let cache = context.project_root.join(".asb-cache/workload-sources");
    fs::create_dir_all(&cache).map_err(crate::io_error)?;
    let archive = cache.join(ARCHIVE);
    if fs::symlink_metadata(&archive).is_ok() && !verify(&archive, None)? {
        preserve(&archive, output)?;
    }
    if !archive.is_file() {
        progress(80, "downloading pinned original SWE-bench Dockerfiles");
        let temporary = archive.with_extension(format!(
            "part-{}",
            TaskIdentity::local(adapter.id, None).attempt_id
        ));
        let status = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--max-time",
                "90",
                "--max-filesize",
                "67108864",
                "--output",
            ])
            .arg(&temporary)
            .arg(URL)
            .logged_status()
            .map_err(|error| {
                Error::Io(format!(
                    "cannot download original SWE-bench Dockerfiles: {error}"
                ))
            })?;
        if !status.success() || !verify(&temporary, None)? {
            return Err(Error::Input("original SWE-bench source download failed or did not match its pinned checksum; inspect .asb/install.log, check network/proxy access and retry workload installation".into()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(crate::io_error)?;
        }
        fs::rename(&temporary, &archive).map_err(crate::io_error)?;
    }
    progress(
        90,
        "matching original Dockerfiles to every installed dataset instance",
    );
    if !verify(&archive, Some(&catalog))? {
        return Err(Error::Input("original SWE-bench source definitions do not match the installed dataset; reinstall the pinned workload and inspect .asb/install.log".into()));
    }
    if fs::symlink_metadata(&destination).is_ok() {
        preserve(&destination, output)?;
    }
    let temporary = destination.with_extension(format!(
        "part-{}",
        TaskIdentity::local(adapter.id, None).attempt_id
    ));
    fs::copy(&archive, &temporary).map_err(crate::io_error)?;
    fs::rename(&temporary, &destination).map_err(crate::io_error)?;
    Ok(())
}

fn preserve(path: &Path, output: &mut dyn Write) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path).map_err(crate::io_error)?;
    if !metadata.is_file() {
        return Err(Error::Input(format!(
            "official task source {} must be a regular file, not a symlink or directory",
            path.display()
        )));
    }
    let backup = path.with_extension(format!(
        "asb-backup-{}",
        TaskIdentity::local("swebench-source", None).attempt_id
    ));
    fs::rename(path, &backup).map_err(crate::io_error)?;
    writeln!(
        output,
        "Preserved the changed original task archive at {} before reinstalling.",
        backup.display()
    )
    .map_err(crate::io_error)
}

#[cfg(test)]
#[path = "sources/coverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_pins_match_the_published_helper_contract() {
        let helper = include_str!("../../../../assets/benchmarks/asb_swebench_sources.py");
        let revision = ARCHIVE
            .strip_prefix("swe-bench-tasks-")
            .unwrap()
            .strip_suffix(".tar.gz")
            .unwrap();
        assert!(URL.ends_with(revision));
        assert!(helper.contains(&format!("SOURCE_REVISION = \"{revision}\"")));
    }

    #[test]
    fn changed_source_archives_are_preserved_without_overwriting_their_contents() {
        let root = std::env::temp_dir().join(TaskIdentity::local("source-backup", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("source.tar.gz");
        fs::write(&archive, "changed source evidence").unwrap();
        let mut output = Vec::new();
        preserve(&archive, &mut output).unwrap();
        assert!(!archive.exists());
        let backups: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(&backups[0]).unwrap(),
            "changed source evidence"
        );
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Preserved the changed")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn source_backup_does_not_follow_symlinks_or_move_directories() {
        let root = std::env::temp_dir().join(TaskIdentity::local("source-link", None).attempt_id);
        fs::create_dir_all(root.join("directory")).unwrap();
        let target = root.join("unrelated");
        fs::write(&target, "untouched").unwrap();
        let link = root.join("linked.tar.gz");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        for path in [&link, &root.join("directory")] {
            assert!(preserve(path, &mut Vec::new()).is_err());
            assert!(fs::symlink_metadata(path).is_ok());
        }
        assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
        fs::remove_dir_all(root).unwrap();
    }
}
