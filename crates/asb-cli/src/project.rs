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

pub fn init(
    root: Option<PathBuf>,
    reset: bool,
    output: &mut dyn std::io::Write,
) -> Result<(), Error> {
    let root = root.unwrap_or_else(|| PathBuf::from("."));
    if is_asb_source_checkout(&root) {
        return Err(Error::Usage(
            "refusing to initialize the ASB source checkout; choose a separate benchmark project directory",
        ));
    }
    let path = root.join(CONFIG_DIR).join(CONFIG_FILE);
    if path.is_file() {
        if !reset {
            writeln!(
                output,
                "Warning: this project is already initialized at {}.\nNo files were changed. Rerun `asb init --reset` to remove managed state and initialize fresh.",
                path.display()
            )
            .map_err(super::io_error)?;
            return Err(Error::Usage(
                "project already initialized; use `asb init --reset` for a fresh initialization",
            ));
        }
        reset_project(&root, &path)?;
    }
    fs::create_dir_all(path.parent().expect("config parent"))
        .map_err(|error| Error::Io(format!("cannot create project configuration: {error}")))?;
    let config = ProjectConfig::default();
    fs::create_dir_all(resolve(&root, &config.tools_dir))
        .map_err(|error| Error::Io(format!("cannot create tools directory: {error}")))?;
    fs::create_dir_all(resolve(&root, &config.workloads_dir))
        .map_err(|error| Error::Io(format!("cannot create workloads directory: {error}")))?;
    let bundled = crate::workloads::populate(&resolve(&root, &config.workloads_dir))
        .map_err(|error| Error::Io(format!("cannot install bundled workloads: {error}")))?;
    fs::create_dir_all(resolve(&root, &config.workspace_dir))
        .map_err(|error| Error::Io(format!("cannot create workspace directory: {error}")))?;
    let fixtures = crate::workloads::populate_fixtures(&root.join(CONFIG_DIR).join("fixtures"))
        .map_err(|error| Error::Io(format!("cannot install workload fixtures: {error}")))?;
    let text = toml::to_string_pretty(&config).map_err(|error| Error::Input(error.to_string()))?;
    fs::write(&path, text)
        .map_err(|error| Error::Io(format!("cannot write {}: {error}", path.display())))?;
    writeln!(
        output,
        "Initialized ASB project at {}\nBundled workloads available: {bundled}\nFixture files available: {fixtures}",
        path.display()
    )
    .map_err(super::io_error)
}

fn reset_project(root: &Path, config_path: &Path) -> Result<(), Error> {
    let state = root.join(CONFIG_DIR);
    reject_symlink(&state)?;
    reject_symlink(config_path)?;
    let existing = fs::read_to_string(config_path)
        .map_err(|error| Error::Io(format!("cannot read {}: {error}", config_path.display())))?;
    let config: ProjectConfig = toml::from_str(&existing).map_err(|error| {
        Error::Input(format!(
            "cannot parse {} before reset: {error}",
            config_path.display()
        ))
    })?;
    // Only direct children can be removed safely with std::fs::remove_dir_all:
    // it does not follow the final symlink, but does follow intermediate ones.
    // Validate everything before changing anything, including the default paths
    // that initialization will recreate after the reset.
    let defaults = ProjectConfig::default();
    for directory in [
        &config.tools_dir,
        &config.workloads_dir,
        &config.workspace_dir,
        &defaults.tools_dir,
        &defaults.workloads_dir,
        &defaults.workspace_dir,
    ] {
        if !is_safe_project_relative_path(directory) {
            return Err(Error::Input(format!(
                "cannot reset configured directory {directory:?}: reset requires a direct child of the project; move its contents manually and update .asb/project.toml before retrying"
            )));
        }
        reject_symlink(&root.join(directory))?;
    }
    if state.exists() {
        fs::remove_dir_all(&state).map_err(|error| {
            Error::Io(format!("cannot remove {}: {error}", config_path.display()))
        })?;
    }
    for directory in [
        &config.tools_dir,
        &config.workloads_dir,
        &config.workspace_dir,
    ] {
        let path = root.join(directory);
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .map_err(|error| Error::Io(format!("cannot remove {}: {error}", path.display())))?;
        }
    }
    Ok(())
}

fn is_safe_project_relative_path(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn reject_symlink(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Error::Input(format!(
            "cannot reset: {} is a symbolic link; move or remove the link before retrying",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::Io(format!(
            "cannot inspect {}: {error}",
            path.display()
        ))),
    }
}

fn is_asb_source_checkout(root: &Path) -> bool {
    root.join("Cargo.toml").is_file()
        && root.join("crates/asb-cli/Cargo.toml").is_file()
        && root.join("workloads").is_dir()
}

pub fn load(start: &Path) -> Result<(PathBuf, ProjectConfig), Error> {
    let mut directory = if start.is_dir() {
        start.to_path_buf()
    } else {
        start
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
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
        init(Some(root.clone()), false, &mut output).expect("init succeeds");
        let (loaded_root, config) = load(&root).expect("config loads");
        assert_eq!(loaded_root, root);
        assert!(root.join(".asb/project.toml").is_file());
        assert!(resolve(&root, &config.tools_dir).is_dir());
        assert!(resolve(&root, &config.workloads_dir).is_dir());
        assert!(resolve(&root, &config.workspace_dir).is_dir());
        assert!(root.join("workloads/bug-fix.txt").is_file());
        assert!(root.join(".asb/fixtures/bug-fix/src/parser.py").is_file());
        assert!(
            root.join(".asb/fixtures/test-generation/tests/test_temperature.py")
                .is_file()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn init_rejects_the_asb_source_checkout_layout() {
        let root = std::env::temp_dir().join(format!("asb-source-checkout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("crates/asb-cli")).unwrap();
        fs::create_dir_all(root.join("workloads")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(root.join("crates/asb-cli/Cargo.toml"), "[package]\n").unwrap();
        let error = init(Some(root.clone()), false, &mut Vec::new())
            .expect_err("source checkout must be rejected");
        assert!(error.to_string().contains("source checkout"));
        assert!(!root.join(".asb").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn init_warns_without_reset_and_preserves_existing_state() {
        let root = std::env::temp_dir().join(format!("asb-init-warning-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut output = Vec::new();
        init(Some(root.clone()), false, &mut output).expect("initial init succeeds");
        fs::write(root.join("workloads/custom.txt"), "keep").unwrap();
        let error =
            init(Some(root.clone()), false, &mut output).expect_err("second init requires reset");
        assert!(error.to_string().contains("already initialized"));
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("Warning:"));
        assert!(text.contains("asb init --reset"));
        assert_eq!(
            fs::read_to_string(root.join("workloads/custom.txt")).unwrap(),
            "keep"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn init_reset_removes_managed_state_before_recreating_it() {
        let root = std::env::temp_dir().join(format!("asb-init-reset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        init(Some(root.clone()), false, &mut Vec::new()).expect("initial init succeeds");
        fs::write(root.join("workloads/custom.txt"), "remove").unwrap();
        let mut output = Vec::new();
        init(Some(root.clone()), true, &mut output).expect("reset succeeds");
        assert!(!root.join("workloads/custom.txt").exists());
        assert!(root.join("workloads/bug-fix.txt").is_file());
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Bundled workloads available")
        );
        let _ = fs::remove_dir_all(root);
    }
}
