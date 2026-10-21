//! Explicit installer intent, shared deadlines and non-swallowable custody errors.
use crate::project::{InstallationConfig, ProjectConfig};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod native;

pub(crate) type InstallResult<T> = Result<T, InstallFailure>;

/// Only the outer command boundary may render/erase this private failure type.
#[derive(Debug)]
pub(crate) enum InstallFailure {
    Ordinary(crate::Error),
    Abort(AbortInstallation),
}
impl From<crate::Error> for InstallFailure {
    fn from(error: crate::Error) -> Self {
        Self::Ordinary(error)
    }
}
impl InstallFailure {
    pub(crate) fn into_public(self) -> crate::Error {
        match self {
            Self::Ordinary(error) => error,
            Self::Abort(abort) => crate::Error::Io(abort.to_string()),
        }
    }
}
impl std::fmt::Display for InstallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ordinary(error) => error.fmt(f),
            Self::Abort(error) => error.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Purpose {
    Probe,
    Version,
    Download,
    Extract,
    Configure,
    Package,
    Build,
}
impl Purpose {
    fn budget(self, config: &InstallationConfig) -> (&'static str, u64) {
        match self {
            Self::Probe => ("probe_timeout_seconds", config.probe_timeout_seconds),
            Self::Version => ("version_timeout_seconds", config.version_timeout_seconds),
            Self::Download => ("download_timeout_seconds", config.download_timeout_seconds),
            Self::Extract => ("extract_timeout_seconds", config.extract_timeout_seconds),
            Self::Configure => (
                "configure_timeout_seconds",
                config.configure_timeout_seconds,
            ),
            Self::Package => ("package_timeout_seconds", config.package_timeout_seconds),
            Self::Build => ("build_timeout_seconds", config.build_timeout_seconds),
        }
    }
}

#[derive(Clone)]
pub(crate) struct OperationContext {
    root: PathBuf,
    config_path: PathBuf,
    budgets: InstallationConfig,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
}
impl OperationContext {
    pub(crate) fn new(root: &Path, config: &ProjectConfig) -> InstallResult<Self> {
        let config_path = root.join(".asb/project.toml");
        config.installation.validate().map_err(|error| {
            crate::Error::Input(format!("invalid {}: {error}", config_path.display()))
        })?;
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(
                config.installation.operation_timeout_seconds,
            ))
            .ok_or_else(|| {
                crate::Error::Input(format!(
                    "invalid {}: installation.operation_timeout_seconds cannot be represented",
                    config_path.display()
                ))
            })?;
        Ok(Self {
            root: root.to_owned(),
            config_path,
            budgets: config.installation.clone(),
            deadline,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    fn step_deadline(&self, purpose: Purpose) -> InstallResult<(Instant, &'static str)> {
        let (key, seconds) = purpose.budget(&self.budgets);
        let requested = Instant::now()
            .checked_add(Duration::from_secs(seconds))
            .ok_or_else(|| {
                crate::Error::Input(format!(
                    "invalid {}: installation.{key} cannot be represented",
                    self.config_path.display()
                ))
            })?;
        Ok(if requested < self.deadline {
            (requested, key)
        } else {
            (self.deadline, "operation_timeout_seconds")
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AbortCause {
    Cancelled,
    Deadline,
    CaptureRead,
    CaptureOverflow,
    Custody,
    Worker,
}

#[derive(Debug)]
pub(crate) struct AbortInstallation {
    pub(crate) cause: AbortCause,
    pub(crate) cleanup_confirmed: bool,
    pub(crate) capture_complete: bool,
    label: String,
    config_path: PathBuf,
    limit_key: &'static str,
    pub(crate) evidence: Option<PathBuf>,
}
impl std::fmt::Display for AbortInstallation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "installation step {} aborted ({:?}); cleanup {}; required capture {}; configuration {} [installation].{}",
            self.label,
            self.cause,
            if self.cleanup_confirmed {
                "confirmed"
            } else {
                "unconfirmed; owned worker retains producer resources"
            },
            if self.capture_complete {
                "complete"
            } else {
                "incomplete"
            },
            self.config_path.display(),
            self.limit_key
        )?;
        if let Some(path) = &self.evidence {
            write!(f, "; retained evidence {}", path.display())?;
        }
        Ok(())
    }
}

/// Actual producer dependencies, held by the worker until matching quiescence.
/// The owner must move its real staging guard here, not a copy of its pathname.
#[derive(Clone, Default)]
pub(crate) struct ProducerLease {
    _owner: Option<Arc<dyn Send + Sync>>,
    evidence: Option<PathBuf>,
}
impl ProducerLease {
    pub(crate) fn new<T: Send + Sync + 'static>(owner: Arc<T>, evidence: &Path) -> Self {
        Self {
            _owner: Some(owner),
            evidence: Some(evidence.to_owned()),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Capture {
    Status,
    Inspect {
        stream_bytes: usize,
        total_bytes: usize,
    },
}
impl Capture {
    pub(crate) fn inspection() -> Self {
        Self::Inspect {
            stream_bytes: 1024 * 1024,
            total_bytes: 2 * 1024 * 1024,
        }
    }
}

pub(crate) struct SettledStep {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    #[cfg(target_os = "linux")]
    pub(crate) log: native::LogReport,
}
impl SettledStep {
    pub(crate) fn status(&self) -> std::process::ExitStatus {
        self.status
    }
    pub(crate) fn stdout(&self) -> &[u8] {
        &self.stdout
    }
    pub(crate) fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

/// Materialize first; configure this original Command, then consume the route.
pub(crate) struct PreparedStep {
    command: std::process::Command,
    route: Option<crate::launch::CommandRoute>,
    context: OperationContext,
    purpose: Purpose,
    label: String,
    capture: Capture,
    #[cfg(test)]
    log_writer: Option<native::WriterFactory>,
    #[cfg(test)]
    hold_observation: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    before_continue: Option<native::AdmissionPause>,
}
impl PreparedStep {
    pub(crate) fn new(
        context: &OperationContext,
        program: impl AsRef<std::ffi::OsStr>,
        purpose: Purpose,
        label: &str,
    ) -> InstallResult<Self> {
        let launch = crate::launch::ProcessLaunch::new(program);
        Self::from_launch(context, &launch, purpose, label)
    }
    pub(crate) fn from_launch(
        context: &OperationContext,
        launch: &crate::launch::ProcessLaunch,
        purpose: Purpose,
        label: &str,
    ) -> InstallResult<Self> {
        #[cfg(target_os = "linux")]
        let (mut command, route) = launch.native(Some(&runtime().map_err(crate::io_error)?));
        #[cfg(not(target_os = "linux"))]
        let (mut command, route) = launch.native(None);
        crate::project::apply_network_proxy(&mut command, &context.root)?;
        Ok(Self {
            command,
            route: Some(route),
            context: context.clone(),
            purpose,
            label: label.to_owned(),
            capture: Capture::Status,
            #[cfg(test)]
            log_writer: None,
            #[cfg(test)]
            hold_observation: None,
            #[cfg(test)]
            before_continue: None,
        })
    }
    pub(crate) fn command_mut(&mut self) -> &mut std::process::Command {
        &mut self.command
    }
    pub(crate) fn capture(&mut self, capture: Capture) {
        self.capture = capture;
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn run(self, lease: ProducerLease) -> InstallResult<SettledStep> {
        native::run(self, lease)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;

#[cfg(target_os = "linux")]
fn runtime() -> std::io::Result<PathBuf> {
    #[cfg(test)]
    return Ok(std::env::current_exe()?
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| std::io::Error::other("missing actual ASB binary directory"))?
        .join("asb"));
    #[cfg(not(test))]
    crate::process_gate::executable()
}
