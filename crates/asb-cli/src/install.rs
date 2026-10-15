//! Small local installers for agent executables and workload prompt files.

use crate::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Install an agent executable copied from an explicit source or the current PATH.
pub fn tool(args: &[String], output: &mut dyn std::io::Write) -> Result<(), Error> {
    let (id, source, directory) = parse_tool(args)?;
    let source = source.or_else(|| find_on_path(&id));
    let source = source.ok_or_else(|| {
        Error::Input(format!(
            "agent {id} was not found; install it first or pass --source PATH"
        ))
    })?;
    let metadata = fs::metadata(&source).map_err(|error| {
        Error::Input(format!(
            "cannot read agent source {}: {error}",
            source.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(Error::Input(format!(
            "agent source is not a file: {}",
            source.display()
        )));
    }
    fs::create_dir_all(&directory)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", directory.display())))?;
    let destination = directory.join(&id);
    fs::copy(&source, &destination).map_err(|error| {
        Error::Io(format!(
            "cannot install agent {id} to {}: {error}",
            destination.display()
        ))
    })?;
    make_executable(&destination)?;
    writeln!(output, "Installed agent {id} at {}", destination.display())
        .map_err(super::io_error)?;
    Ok(())
}

/// Install one external workload prompt file into a workload directory.
pub fn workload(args: &[String], output: &mut dyn std::io::Write) -> Result<(), Error> {
    let (source, name, directory) = parse_workload(args)?;
    let source_metadata = fs::metadata(&source).map_err(|error| {
        Error::Input(format!(
            "cannot read workload source {}: {error}",
            source.display()
        ))
    })?;
    if !source_metadata.is_file() {
        return Err(Error::Input(format!(
            "workload source is not a file: {}",
            source.display()
        )));
    }
    fs::create_dir_all(&directory)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", directory.display())))?;
    let destination = directory.join(format!("{name}.txt"));
    fs::copy(&source, &destination).map_err(|error| {
        Error::Io(format!(
            "cannot install workload {name} to {}: {error}",
            destination.display()
        ))
    })?;
    writeln!(
        output,
        "Installed workload {name} at {}",
        destination.display()
    )
    .map_err(super::io_error)?;
    Ok(())
}

fn parse_tool(args: &[String]) -> Result<(String, Option<PathBuf>, PathBuf), Error> {
    let id = args
        .first()
        .filter(|value| valid_name(value))
        .cloned()
        .ok_or(Error::Usage("tool install requires an agent name"))?;
    let mut source = None;
    let mut directory = default_tool_directory();
    parse_options(&args[1..], |flag, value| match flag {
        "--source" => Ok(source = Some(PathBuf::from(value))),
        "--dir" => Ok(directory = PathBuf::from(value)),
        _ => {
            return Err(Error::Usage(
                "tool install accepts --source PATH and --dir DIR",
            ));
        }
    })?;
    Ok((id, source, directory))
}

fn parse_workload(args: &[String]) -> Result<(PathBuf, String, PathBuf), Error> {
    let source = args
        .first()
        .map(PathBuf::from)
        .ok_or(Error::Usage("workload install requires a source .txt file"))?;
    let default_name = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| valid_name(value))
        .ok_or(Error::Input(
            "workload source must have a simple file name".into(),
        ))?
        .to_owned();
    let mut name = default_name;
    let mut directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("workloads"));
    parse_options(&args[1..], |flag, value| match flag {
        "--name" if valid_name(value) => Ok(name = value.to_owned()),
        "--name" => {
            return Err(Error::Input(
                "workload name must contain only letters, numbers, ., _, or -".into(),
            ));
        }
        "--dir" => Ok(directory = PathBuf::from(value)),
        _ => {
            return Err(Error::Usage(
                "workload install accepts --name NAME and --dir DIR",
            ));
        }
    })?;
    Ok((source, name, directory))
}

fn parse_options(
    args: &[String],
    mut apply: impl FnMut(&str, &str) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or(Error::Usage("install options require values"))?;
        apply(flag, value)?;
        index += 2;
    }
    Ok(())
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn default_tool_directory() -> PathBuf {
    std::env::var_os("ASB_TOOL_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin")))
        .unwrap_or_else(|| PathBuf::from(".asb-tools/bin"))
}

fn find_on_path(id: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(Path::new)
        .map(|directory| directory.join(id))
        .find(|candidate| candidate.is_file())
}

fn make_executable(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| Error::Io(format!("cannot inspect {}: {error}", path.display())))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(|error| {
            Error::Io(format!(
                "cannot make {} executable: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}
