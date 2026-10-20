//! The shared project Python used to create isolated Python tool environments.

use crate::{Error, project};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PYTHON_VERSION_REQUESTS: [&str; 2] = ["3.12", "3.13"];

fn version(executable: &Path) -> Option<(u32, u32)> {
    let result = Command::new(executable)
        .args([
            "-I",
            "-S",
            "-c",
            "import sys; print('%d.%d' % sys.version_info[:2])",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !result.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&result.stdout).ok()?.trim();
    let (major, minor) = text.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn compatible(executable: &Path) -> bool {
    matches!(version(executable), Some((3, 12 | 13)))
}

pub(crate) fn matches_project(executable: &Path, project_python: &Path) -> bool {
    compatible(executable) && version(executable) == version(project_python)
}

pub(crate) fn project_python(root: &Path) -> PathBuf {
    root.join(".asb/python/venv/bin/python")
}

/// Preserve a previous environment even when it contains manual work.
pub(crate) fn preserve_environment(path: &Path) -> Result<Option<PathBuf>, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::Io(format!(
                "cannot inspect {}: {error}",
                path.display()
            )));
        }
    }
    let parent = path.parent().expect("environment has a parent");
    for index in 1..=10_000 {
        let backup = parent.join(format!("venv.asb-backup-{index}"));
        match fs::create_dir(&backup) {
            Ok(()) => {
                let destination = backup.join("venv");
                fs::rename(path, &destination).map_err(|error| {
                    Error::Io(format!(
                        "cannot preserve Python environment {} at {}: {error}",
                        path.display(),
                        destination.display()
                    ))
                })?;
                return Ok(Some(destination));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(Error::Io(format!(
                    "cannot create environment backup {}: {error}",
                    backup.display()
                )));
            }
        }
    }
    Err(Error::Io(format!(
        "cannot reserve a Python environment backup in {}",
        parent.display()
    )))
}

fn installed_python() -> Option<PathBuf> {
    for name in ["python3.12", "python3.13", "python3"] {
        let candidate = PathBuf::from(name);
        if compatible(&candidate) {
            return Some(candidate);
        }
    }
    // uv can know about managed interpreters not present on PATH. This is a
    // lookup only; downloads happen exclusively in the explicit fallback.
    for request in PYTHON_VERSION_REQUESTS {
        let output = Command::new("uv")
            .args([
                "python",
                "find",
                "--no-project",
                "--no-config",
                "--no-python-downloads",
                request,
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(output) = output
            && output.status.success()
            && let Ok(text) = std::str::from_utf8(&output.stdout)
        {
            let path = PathBuf::from(text.trim());
            if path.is_absolute() && compatible(&path) {
                return Some(path);
            }
        }
    }
    None
}

/// Create the project environment, downloading Python through uv only when no
/// suitable local interpreter is found. Failed setup can be retried later.
pub(crate) fn ensure(root: &Path, output: &mut dyn std::io::Write) -> Result<PathBuf, Error> {
    let python = project_python(root);
    if compatible(&python) {
        return Ok(python);
    }
    let env_dir = root.join(".asb/python/venv");
    fs::create_dir_all(env_dir.parent().expect("venv parent"))
        .map_err(|error| Error::Io(format!("cannot create project Python directory: {error}")))?;
    if let Some(backup) = preserve_environment(&env_dir)? {
        writeln!(
            output,
            "Preserved the previous project Python environment at {}.",
            backup.display()
        )
        .map_err(crate::io_error)?;
    }
    let log_path = root.join(".asb/python/setup.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| Error::Io(format!("cannot open {}: {error}", log_path.display())))?;
    let log_stderr = log
        .try_clone()
        .map_err(|error| Error::Io(format!("cannot open Python setup log: {error}")))?;
    let status = if let Some(interpreter) = installed_python() {
        writeln!(
            output,
            "Preparing the project Python environment using {}.",
            interpreter.display()
        )
        .map_err(crate::io_error)?;
        Command::new(&interpreter)
            .args(["-m", "venv", "--without-pip"])
            .arg(&env_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_stderr))
            .status()
            .map_err(|error| {
                Error::Io(format!(
                    "cannot create project Python environment with {}: {error}",
                    interpreter.display()
                ))
            })?
    } else {
        let (_, config) = project::load(root)?;
        let tools_root = project::resolve(root, &config.tools_dir);
        crate::install::ensure_runtime_dependency("uv", root, &tools_root, output, &mut |_, _| {})
            .map_err(|error| Error::Io(format!("could not bootstrap project Python: {error}. Install uv, or install Python 3.12 or 3.13 and retry.")))?;
        writeln!(
            output,
            "No compatible Python found; using uv to download Python 3.12 for this project."
        )
        .map_err(crate::io_error)?;
        let mut command = Command::new("uv");
        crate::tools::apply_runtime_path(&mut command, root, &config.tools_dir)?;
        command
            .args(["venv", "--no-project", "--no-config", "--python", "3.12"])
            .arg(&env_dir)
            .env("UV_PYTHON_INSTALL_DIR", root.join(".asb/python/uv-python"))
            .env("UV_CACHE_DIR", root.join(".asb/python/uv-cache"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_stderr));
        project::apply_network_proxy(&mut command, root)?;
        command.status().map_err(|error| Error::Io(format!("cannot start uv to install Python 3.12: {error}. Install uv, or install Python 3.12 or 3.13 and retry.")))?
    };
    if !status.success() || !compatible(&python) {
        return Err(Error::Io(format!(
            "could not prepare a compatible Python 3.12 or 3.13 environment. Check {} for details, including Python/uv, project-directory access, network or proxy errors; then retry the tool installation.",
            log_path.display()
        )));
    }
    Ok(python)
}

pub(crate) fn add_to_path(command: &mut Command, root: &Path, program: &Path) -> Result<(), Error> {
    let python = project_python(root);
    if !python.is_file() {
        return Ok(());
    }
    let path = command
        .get_envs()
        .find(|(name, _)| *name == std::ffi::OsStr::new("PATH"))
        .and_then(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    // An installed Python tool must keep its own dependencies visible to
    // subprocesses. It uses the same project-selected Python version.
    let tool_bin = program.parent().filter(|bin| {
        bin.file_name().is_some_and(|name| name == "bin")
            && bin
                .parent()
                .is_some_and(|venv| venv.join("pyvenv.cfg").is_file())
    });
    let joined = std::env::join_paths(
        tool_bin
            .into_iter()
            .map(Path::to_path_buf)
            .chain(std::iter::once(
                python.parent().expect("Python bin").to_path_buf(),
            ))
            .chain(std::env::split_paths(&path)),
    )
    .map_err(|error| Error::Io(format!("cannot configure project Python PATH: {error}")))?;
    command.env("PATH", joined);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_python_precedes_existing_child_path() {
        let root = std::env::temp_dir().join(format!("asb-python-path-{}", std::process::id()));
        let bin = root.join(".asb/python/venv/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("python"), "fixture").unwrap();
        let mut command = Command::new("unused");
        command.env("PATH", "/custom/bin:/usr/bin");
        add_to_path(&mut command, &root, Path::new("/usr/bin/tool")).unwrap();
        let path = command
            .get_envs()
            .find(|(key, _)| *key == "PATH")
            .unwrap()
            .1
            .unwrap();
        let entries: Vec<_> = std::env::split_paths(path).collect();
        assert_eq!(
            entries,
            vec![bin, PathBuf::from("/custom/bin"), PathBuf::from("/usr/bin")]
        );
        let tool_bin = root.join("tool/venv/bin");
        fs::create_dir_all(&tool_bin).unwrap();
        fs::write(root.join("tool/venv/pyvenv.cfg"), "fixture").unwrap();
        let mut python_tool = Command::new("unused");
        python_tool.env("PATH", "/custom/bin");
        add_to_path(&mut python_tool, &root, &tool_bin.join("agent")).unwrap();
        let path = python_tool
            .get_envs()
            .find(|(key, _)| *key == "PATH")
            .unwrap()
            .1
            .unwrap();
        assert_eq!(
            std::env::split_paths(path).collect::<Vec<_>>(),
            vec![
                tool_bin,
                root.join(".asb/python/venv/bin"),
                PathBuf::from("/custom/bin")
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }
}
