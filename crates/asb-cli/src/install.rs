//! Project-local installers for agents and workload prompt files.

use crate::Error;
use crate::project;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy)]
enum Recipe {
    Npm {
        package: &'static str,
        executable: &'static str,
    },
    Python {
        package: &'static str,
        executable: &'static str,
    },
}

fn recipe(name: &str) -> Option<Recipe> {
    match name {
        "codex" => Some(Recipe::Npm {
            package: "@openai/codex@0.162.1",
            executable: "codex",
        }),
        "opencode" => Some(Recipe::Npm {
            package: "opencode-ai@1.19.0",
            executable: "opencode",
        }),
        "codeagent" => Some(Recipe::Npm {
            package: "@bryanhu/code-agent@1.14.1",
            executable: "code-agent",
        }),
        "workbuddy" => Some(Recipe::Npm {
            package: "@tencent-ai/codebuddy-code@2.167.0",
            executable: "codebuddy",
        }),
        "gemini" => Some(Recipe::Npm {
            package: "@google/gemini-cli@0.64.0",
            executable: "gemini",
        }),
        "qwen-code" => Some(Recipe::Npm {
            package: "@qwen-code/qwen-code@0.25.1",
            executable: "qwen",
        }),
        "aider" => Some(Recipe::Python {
            package: "aider-chat==0.86.2",
            executable: "aider",
        }),
        "mini-swe" => Some(Recipe::Python {
            package: "mini-swe-agent==2.4.6",
            executable: "mini",
        }),
        "openhands" => Some(Recipe::Python {
            package: "openhands==1.16.0",
            executable: "openhands",
        }),
        "openjiuwen" => Some(Recipe::Python {
            package: "openjiuwen[cli]==0.1.19.post2",
            executable: "openjiuwen",
        }),
        "cli2key" => Some(Recipe::Npm {
            package: "codex-anthropic-bridge@0.1.10",
            executable: "codex-bridge",
        }),
        _ => None,
    }
}

pub fn tool(args: &[String], output: &mut dyn std::io::Write) -> Result<(), Error> {
    let (id, source, root, dry_run) = parse_tool(args)?;
    let root = root.join(&id);
    if let Some(source) = source {
        let destination = root.join("bin").join(executable_name(&id));
        if dry_run {
            writeln!(output, "download {source} -> {}", destination.display())
                .map_err(super::io_error)?;
            return Ok(());
        }
        fs::create_dir_all(destination.parent().expect("bin parent"))
            .map_err(|error| Error::Io(format!("cannot create {}: {error}", root.display())))?;
        install_source(&source, &destination, "agent")?;
        make_executable(&destination)?;
        writeln!(output, "Installed agent {id} at {}", destination.display())
            .map_err(super::io_error)?;
        return Ok(());
    }
    let plan = recipe(&id).ok_or(Error::Usage("unknown agent; pass --source URL or PATH"))?;
    if dry_run {
        writeln!(output, "{}", describe(plan, &root)).map_err(super::io_error)?;
        return Ok(());
    }
    let executable = install_recipe(plan, &root)?;
    writeln!(output, "Installed agent {id} at {}", executable.display())
        .map_err(super::io_error)?;
    Ok(())
}

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

fn install_recipe(recipe: Recipe, root: &Path) -> Result<PathBuf, Error> {
    fs::create_dir_all(root)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", root.display())))?;
    let status = match recipe {
        Recipe::Npm { package, .. } => Command::new("npm")
            .args(["install", "--prefix"])
            .arg(root)
            .args(["--no-save", package])
            .status(),
        Recipe::Python { package, .. } => {
            let venv = root.join("venv");
            let created = Command::new("python3")
                .args(["-m", "venv"])
                .arg(&venv)
                .status()
                .map_err(|error| Error::Io(format!("cannot start python3: {error}")))?;
            if !created.success() {
                return Err(Error::Io(
                    "python3 could not create the agent environment".into(),
                ));
            }
            Command::new(venv.join("bin/python"))
                .args(["-m", "pip", "install", package])
                .status()
        }
    }
    .map_err(|error| Error::Io(format!("cannot start agent installer: {error}")))?;
    if !status.success() {
        return Err(Error::Io("agent package installation failed".into()));
    }
    Ok(executable(recipe, root))
}

fn executable(recipe: Recipe, root: &Path) -> PathBuf {
    match recipe {
        Recipe::Npm { executable, .. } => root.join("node_modules/.bin").join(executable),
        Recipe::Python { executable, .. } => root.join("venv/bin").join(executable),
    }
}

fn describe(recipe: Recipe, root: &Path) -> String {
    match recipe {
        Recipe::Npm { package, .. } => format!("npm install --prefix {} {package}", root.display()),
        Recipe::Python { package, .. } => format!(
            "python3 -m venv {}/venv && pip install {package}",
            root.display()
        ),
    }
}

fn parse_tool(args: &[String]) -> Result<(String, Option<String>, PathBuf, bool), Error> {
    let id = args
        .first()
        .filter(|value| valid_name(value))
        .cloned()
        .ok_or(Error::Usage("tool install requires an agent name"))?;
    let mut source = None;
    let (project_root, project_config) = project::load(Path::new("."))?;
    let mut root = project::resolve(&project_root, &project_config.tools_dir);
    let mut dry_run = false;
    parse_options(&args[1..], |flag, value| match flag {
        "--source" => Ok(source = Some(value.to_owned())),
        "--dir" => Ok(root = project::resolve(&project_root, value)),
        "--dry-run" if value.is_empty() => Ok(dry_run = true),
        _ => Err(Error::Usage(
            "tool install accepts --source URL|PATH, --dir DIR, and --dry-run",
        )),
    })?;
    Ok((id, source, root, dry_run))
}

fn parse_workload(args: &[String]) -> Result<(String, String, PathBuf), Error> {
    let source = args.first().cloned().ok_or(Error::Usage(
        "workload install requires a local path or URL",
    ))?;
    let filename = source
        .split('?')
        .next()
        .and_then(|value| value.rsplit('/').next())
        .unwrap_or(&source);
    let default_name = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| valid_name(value))
        .ok_or(Error::Input(
            "workload source must have a simple file name".into(),
        ))?
        .to_owned();
    let mut name = default_name;
    let (project_root, project_config) = project::load(Path::new("."))?;
    let mut directory = std::env::var_os("ASB_WORKLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| project::resolve(&project_root, &project_config.workloads_dir));
    parse_options(&args[1..], |flag, value| match flag {
        "--name" if valid_name(value) => Ok(name = value.to_owned()),
        "--dir" => Ok(directory = project::resolve(&project_root, value)),
        _ => Err(Error::Usage(
            "workload install accepts --name NAME and --dir DIR",
        )),
    })?;
    Ok((source, name, directory))
}

fn executable_name(agent: &str) -> &str {
    match agent {
        "codeagent" => "code-agent",
        "workbuddy" => "codebuddy",
        "qwen-code" => "qwen",
        "mini-swe" => "mini",
        other => other,
    }
}

fn parse_options(
    args: &[String],
    mut apply: impl FnMut(&str, &str) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if flag == "--dry-run" {
            apply(flag, "")?;
            index += 1;
            continue;
        }
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

fn install_source(source: &str, destination: &Path, kind: &str) -> Result<(), Error> {
    if source.starts_with("https://") || source.starts_with("http://") {
        let status = Command::new("curl")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_recipes_cover_all_documented_agents() {
        for agent in [
            "codex",
            "opencode",
            "codeagent",
            "workbuddy",
            "gemini",
            "qwen-code",
            "aider",
            "mini-swe",
            "openhands",
            "openjiuwen",
            "cli2key",
        ] {
            let recipe = recipe(agent).expect("documented agent has a recipe");
            assert!(!describe(recipe, Path::new(".asb-tools")).is_empty());
            assert!(!executable_name(agent).is_empty());
        }
    }
}
