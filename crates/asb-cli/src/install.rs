//! Small local installers for agent executables and workload prompt files.

use crate::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Install an agent executable from a URL or local source.
pub fn tool(args: &[String], output: &mut dyn std::io::Write) -> Result<(), Error> {
    let (id, source, directory) = parse_tool(args)?;
    let source = source.ok_or(Error::Usage(
        "tool install requires --source URL (or a local executable path)",
    ))?;
    fs::create_dir_all(&directory)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", directory.display())))?;
    let destination = directory.join(&id);
    install_source(&source, &destination, "agent")?;
    make_executable(&destination)?;
    writeln!(output, "Installed agent {id} at {}", destination.display())
        .map_err(super::io_error)?;
    Ok(())
}

/// Install one external workload prompt file from a URL or local source.
pub fn workload(args: &[String], output: &mut dyn std::io::Write) -> Result<(), Error> {
    let (source, name, directory) = parse_workload(args)?;
    fs::create_dir_all(&directory)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", directory.display())))?;
    let destination = directory.join(format!("{name}.txt"));
    install_source(&source, &destination, "workload")?;
    writeln!(
        output,
        "Installed workload {name} at {}",
        destination.display()
    )
    .map_err(super::io_error)?;
    Ok(())
}

fn parse_tool(args: &[String]) -> Result<(String, Option<String>, PathBuf), Error> {
    let id = args
        .first()
        .filter(|value| valid_name(value))
        .cloned()
        .ok_or(Error::Usage("tool install requires an agent name"))?;
    let mut source = None;
    let mut directory = default_tool_directory();
    parse_options(&args[1..], |flag, value| match flag {
        "--source" => Ok(source = Some(value.to_owned())),
        "--dir" => Ok(directory = PathBuf::from(value)),
        _ => {
            return Err(Error::Usage(
                "tool install accepts --source PATH and --dir DIR",
            ));
        }
    })?;
    Ok((id, source, directory))
}

fn parse_workload(args: &[String]) -> Result<(String, String, PathBuf), Error> {
    let source = args.first().cloned().ok_or(Error::Usage(
        "workload install requires a local path or URL",
    ))?;
    let default_name = source
        .rsplit('/')
        .next()
        .unwrap_or(&source)
        .split('?')
        .next()
        .unwrap_or(&source)
        .strip_suffix(".txt")
        .unwrap_or_else(|| source.rsplit('/').next().unwrap_or(&source))
        .strip_suffix(".md")
        .unwrap_or_else(|| source.rsplit('/').next().unwrap_or(&source))
        .to_owned();
    let default_name = Path::new(&default_name)
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
        .unwrap_or_else(|| PathBuf::from(".asb-tools/bin"))
}

fn install_source(source: &str, destination: &Path, kind: &str) -> Result<(), Error> {
    if source.starts_with("https://") || source.starts_with("http://") {
        let status = std::process::Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(destination)
            .arg(source)
            .status()
            .map_err(|error| {
                Error::Io(format!("cannot start curl for {kind} download: {error}"))
            })?;
        if !status.success() {
            return Err(Error::Io(format!(
                "could not download {kind} from {source}"
            )));
        }
        return Ok(());
    }
    let source_path = Path::new(source);
    let metadata = fs::metadata(source_path)
        .map_err(|error| Error::Input(format!("cannot read {kind} source {source}: {error}")))?;
    if !metadata.is_file() {
        return Err(Error::Input(format!(
            "{kind} source is not a file: {source}"
        )));
    }
    fs::copy(source_path, destination).map_err(|error| {
        Error::Io(format!(
            "cannot install {kind} at {}: {error}",
            destination.display()
        ))
    })?;
    Ok(())
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
