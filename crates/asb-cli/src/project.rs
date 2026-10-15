//! Project-local configuration shared by every command.

use crate::Error;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_DIR: &str = ".asb";
const CONFIG_FILE: &str = "project.toml";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub tools_dir: String,
    pub workloads_dir: String,
    pub workspace_dir: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            tools_dir: ".asb-tools".into(),
            workloads_dir: "workloads".into(),
            workspace_dir: "asb-workspace".into(),
        }
    }
}

pub fn init(args: &[String], output: &mut dyn std::io::Write) -> Result<(), Error> {
    if args.len() > 1 {
        return Err(Error::Usage("init accepts an optional project directory"));
    }
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let path = root.join(CONFIG_DIR).join(CONFIG_FILE);
    fs::create_dir_all(path.parent().expect("config parent"))
        .map_err(|error| Error::Io(format!("cannot create project configuration: {error}")))?;
    let config = ProjectConfig::default();
    fs::create_dir_all(resolve(&root, &config.tools_dir))
        .map_err(|error| Error::Io(format!("cannot create tools directory: {error}")))?;
    fs::create_dir_all(resolve(&root, &config.workloads_dir))
        .map_err(|error| Error::Io(format!("cannot create workloads directory: {error}")))?;
    fs::create_dir_all(resolve(&root, &config.workspace_dir))
        .map_err(|error| Error::Io(format!("cannot create workspace directory: {error}")))?;
    let text = toml::to_string_pretty(&config).map_err(|error| Error::Input(error.to_string()))?;
    fs::write(&path, text)
        .map_err(|error| Error::Io(format!("cannot write {}: {error}", path.display())))?;
    writeln!(output, "Initialized ASB project at {}", path.display()).map_err(super::io_error)
}

pub fn load(start: &Path) -> Result<(PathBuf, ProjectConfig), Error> {
    let mut directory = if start.is_dir() {
        start.to_path_buf()
    } else {
        start
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    };
    loop {
        let path = directory.join(CONFIG_DIR).join(CONFIG_FILE);
        if path.is_file() {
            let text = fs::read_to_string(&path).map_err(|error| {
                Error::Input(format!("cannot read {}: {error}", path.display()))
            })?;
            let config = toml::from_str(&text).map_err(|error| {
                Error::Input(format!("cannot parse {}: {error}", path.display()))
            })?;
            return Ok((directory, config));
        }
        if !directory.pop() {
            break;
        }
    }
    Ok((PathBuf::from("."), ProjectConfig::default()))
}

pub fn resolve(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_config_and_directories() {
        let root = std::env::temp_dir().join(format!("asb-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut output = Vec::new();
        init(&[root.to_string_lossy().into_owned()], &mut output).expect("init succeeds");
        let (loaded_root, config) = load(&root).expect("config loads");
        assert_eq!(loaded_root, root);
        assert!(root.join(".asb/project.toml").is_file());
        assert!(resolve(&root, &config.tools_dir).is_dir());
        assert!(resolve(&root, &config.workloads_dir).is_dir());
        assert!(resolve(&root, &config.workspace_dir).is_dir());
        let _ = fs::remove_dir_all(&root);
    }
}
