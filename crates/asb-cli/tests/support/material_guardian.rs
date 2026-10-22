//! Test-only outer process custody. Never terminate an admitted guardian.
use super::IsolatedEnvironment;
use crate::process_cleanup::OwnedReaper;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Explicit images survive frozen libtest reexec outside target/debug/deps.
pub(crate) fn actual_asb() -> Result<PathBuf, String> {
    let candidate = if let Some(path) = std::env::var_os("ASB_RUNTIME_EXECUTABLE") {
        PathBuf::from(path)
    } else if let Some(target) = option_env!("CARGO_TARGET_DIR") {
        Path::new(target).join("debug/asb")
    } else {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = if manifest.ends_with("crates/asb-cli") {
            manifest
                .parent()
                .and_then(Path::parent)
                .ok_or("invalid workspace fixture root")?
        } else {
            manifest
        };
        root.join("target/debug/asb")
    };
    let image = candidate.canonicalize().map_err(
        |_| "build actual ASB first or provide its explicit ASB_RUNTIME_EXECUTABLE path",
    )?;
    if !image.is_file() {
        return Err("actual ASB image is not a regular file".into());
    }
    Ok(image)
}

struct Guardian {
    child: Option<Child>,
    keeper: Option<OwnedReaper>,
    namespace: PathBuf,
}

impl Guardian {
    fn spawn(command: &mut Command, namespace: &Path) -> Result<Self, String> {
        let keeper = OwnedReaper::new("material-fixture-guardian")?;
        let mut owner = Self {
            child: None,
            keeper: Some(keeper),
            namespace: namespace.to_owned(),
        };
        // Immediate ownership after spawn; no callback or fallible publication
        // is allowed between receiving Child and installing this guard.
        owner.child = Some(
            command
                .spawn()
                .map_err(|error| format!("cannot start fixture guardian: {error}"))?,
        );
        Ok(owner)
    }

    fn retain(&mut self, reason: &str) -> String {
        if let Some(child) = self.child.take() {
            self.keeper
                .as_ref()
                .expect("keeper predates spawn")
                .adopt_wait_only(child);
            self.keeper.take();
        }
        format!(
            "{reason}; fixture guardian remains in wait-only custody; retain namespace {}",
            self.namespace.display()
        )
    }

    fn wait(&mut self, deadline: Instant) -> Result<ExitStatus, String> {
        loop {
            match self
                .child
                .as_mut()
                .expect("owned fixture guardian")
                .try_wait()
            {
                Ok(Some(status)) => {
                    self.child.take();
                    return Ok(status);
                }
                Err(_) => return Err(self.retain("cannot observe fixture guardian exit")),
                Ok(None) => {}
            }
            if self.namespace.join("guardian-error.json").exists() {
                return Err(
                    self.retain("fixture cleanup reported uncertainty, not completed settlement")
                );
            }
            if Instant::now() >= deadline {
                return Err(self.retain("fixture guardian exceeded caller deadline"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Guardian {
    fn drop(&mut self) {
        self.retain("fixture owner unwound");
    }
}

pub(crate) fn run(root: &Path, test: &str, scenario: &str) -> Result<ExitStatus, String> {
    let actual = actual_asb()?;
    let mut command = Command::new("python3");
    command
        .isolated_env()
        .env("PATH", "/usr/bin:/bin")
        .env("ASB_RUNTIME_EXECUTABLE", &actual)
        .args(["-I", "-S", "-B"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/material-transport-custody.py"),
        )
        .arg("guardian")
        .arg(root)
        .arg(std::env::current_exe().map_err(|error| error.to_string())?)
        .arg(test)
        .arg(scenario)
        .arg(&actual)
        .stdin(Stdio::null());
    for (name, stdout) in [("guardian.stdout", true), ("guardian.stderr", false)] {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .map_err(|error| error.to_string())?;
        if stdout {
            command.stdout(file);
        } else {
            command.stderr(file);
        }
    }
    let mut guardian = Guardian::spawn(&mut command, root)?;
    guardian.wait(Instant::now() + Duration::from_secs(55))
}
