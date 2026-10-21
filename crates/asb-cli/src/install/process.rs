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
    proxy: crate::project::NetworkProxy,
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
            proxy: config.proxy.clone(),
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

#[derive(Clone, Debug)]
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
pub(crate) trait ProducerOwner: Send + Sync {
    /// The immutable state belonging to this actual owner. A lease cannot
    /// substitute fresh state while retaining the same producer resources.
    fn producer_state(&self) -> &ProducerState;
    /// Disable destructive settlement before any potentially admitted producer.
    fn producer_started(&self);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProducerProof {
    NoNewAdmission,
    Quiescent,
}

struct OwnerState {
    generation: u64,
    active: bool,
    proof: Option<ProducerProof>,
    consumed: bool,
    poisoned: bool,
    operation: Option<Arc<AtomicBool>>,
}
/// Identity plus private admission/settlement facts, never just pointer identity.
#[derive(Clone)]
pub(crate) struct ProducerState(Arc<std::sync::Mutex<OwnerState>>);
impl Default for ProducerState {
    fn default() -> Self {
        Self(Arc::new(std::sync::Mutex::new(OwnerState {
            generation: 0,
            active: false,
            proof: Some(ProducerProof::NoNewAdmission),
            consumed: false,
            poisoned: false,
            operation: None,
        })))
    }
}
impl ProducerState {
    fn lock(&self) -> std::sync::MutexGuard<'_, OwnerState> {
        match self.0.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.poisoned = true;
                state
            }
        }
    }
    fn permission(
        &self,
        context: &OperationContext,
        evidence: Option<PathBuf>,
    ) -> Option<ProducerPermission> {
        let mut state = self.lock();
        if state.active || state.poisoned || state.consumed {
            return None;
        }
        if let Some(operation) = &state.operation {
            if !Arc::ptr_eq(operation, &context.cancel) {
                return None;
            }
        } else {
            state.operation = Some(Arc::clone(&context.cancel));
        }
        Some(ProducerPermission {
            owner: self.clone(),
            generation: state.generation,
            proof: state.proof?,
            context: context.clone(),
            evidence,
        })
    }
}

/// Non-cloneable one-use permission; identity, generation, operation and actual
/// settlement facts are all revalidated before ANY destructive owner action.
pub(crate) struct ProducerPermission {
    owner: ProducerState,
    generation: u64,
    proof: ProducerProof,
    context: OperationContext,
    evidence: Option<PathBuf>,
}
impl ProducerPermission {
    fn current(&self) -> bool {
        let state = self.owner.lock();
        state.generation == self.generation
            && !state.active
            && !state.poisoned
            && !state.consumed
            && state.proof == Some(self.proof)
            && state
                .operation
                .as_ref()
                .is_some_and(|operation| Arc::ptr_eq(operation, &self.context.cancel))
    }
    pub(crate) fn consume_for(self, owner: &ProducerState) -> InstallResult<()> {
        let mut state = owner.lock();
        let valid = Arc::ptr_eq(&self.owner.0, &owner.0)
            && state.generation == self.generation
            && !state.active
            && !state.poisoned
            && !state.consumed
            && state.proof == Some(self.proof)
            && state
                .operation
                .as_ref()
                .is_some_and(|operation| Arc::ptr_eq(operation, &self.context.cancel));
        if !valid {
            return Err(authority_failure(
                &self.context,
                "stale or different producer permission",
                self.evidence,
            ));
        }
        state.consumed = true;
        Ok(())
    }
}

fn authority_failure(
    context: &OperationContext,
    label: &str,
    evidence: Option<PathBuf>,
) -> InstallFailure {
    InstallFailure::Abort(AbortInstallation {
        cause: AbortCause::Custody,
        cleanup_confirmed: false,
        capture_complete: false,
        label: label.into(),
        config_path: context.config_path.clone(),
        limit_key: "operation_timeout_seconds",
        evidence,
    })
}

#[derive(Clone, Default)]
pub(crate) struct ProducerLease {
    _owner: Option<Arc<dyn ProducerOwner>>,
    state: ProducerState,
    evidence: Option<PathBuf>,
}
impl ProducerLease {
    pub(crate) fn new<T: ProducerOwner + 'static>(owner: Arc<T>, evidence: &Path) -> Self {
        let state = owner.producer_state().clone();
        Self {
            _owner: Some(owner),
            state,
            evidence: Some(evidence.to_owned()),
        }
    }
    fn begin(&self, context: &OperationContext) -> InstallResult<AdmissionTicket> {
        // The actual owner disables destructive Drop before target exposure,
        // including every pre-admission failure and unwind path.
        if let Some(owner) = &self._owner {
            owner.producer_started();
        }
        let mut state = self.state.lock();
        let same_operation = state
            .operation
            .as_ref()
            .is_none_or(|operation| Arc::ptr_eq(operation, &context.cancel));
        let next = state.generation.checked_add(1);
        if state.active || state.poisoned || state.consumed || !same_operation || next.is_none() {
            state.poisoned = true;
            return Err(authority_failure(
                context,
                "producer lane is active, settled, or belongs to another operation",
                self.evidence.clone(),
            ));
        }
        state.operation = Some(Arc::clone(&context.cancel));
        state.generation = next.unwrap();
        state.active = true;
        state.proof = None; // Invalidate every previously issued permission.
        Ok(AdmissionTicket {
            state: self.state.clone(),
            generation: state.generation,
        })
    }
}

/// Private worker/parent bookkeeping, not a public settlement permission.
#[derive(Clone)]
struct AdmissionTicket {
    state: ProducerState,
    generation: u64,
}
impl AdmissionTicket {
    fn settle(&self, proof: ProducerProof) {
        let mut state = self.state.lock();
        if state.generation != self.generation || !state.active {
            state.poisoned = true;
            return;
        }
        state.active = false;
        state.proof = Some(proof);
    }
    fn poison(&self) {
        let mut state = self.state.lock();
        state.poisoned = true;
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
    lease: ProducerLease,
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

/// Captures/status have no publication authority; only ProcessScope owns it.
pub(crate) struct CompletedStep {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) log: native::LogReport,
}

/// A single sequential producer lane for one actual private stage. Neither this
/// scope nor its permission can be cloned or extracted by validation callbacks.
pub(crate) struct ProcessScope {
    context: OperationContext,
    lease: ProducerLease,
    permission: Option<ProducerPermission>,
    terminal: Option<AbortInstallation>,
}
impl ProcessScope {
    pub(crate) fn new(context: &OperationContext, lease: ProducerLease) -> Self {
        // This initial token means no producer was admitted by this new scope,
        // not a manufactured process exit or guardian receipt.
        let permission = lease.state.permission(context, lease.evidence.clone());
        Self {
            context: context.clone(),
            lease,
            permission,
            terminal: None,
        }
    }
    pub(crate) fn prepare(
        &self,
        program: impl AsRef<std::ffi::OsStr>,
        purpose: Purpose,
        label: &str,
    ) -> InstallResult<PreparedStep> {
        PreparedStep::new(&self.context, program, purpose, label)
    }
    pub(crate) fn run(&mut self, step: PreparedStep) -> InstallResult<CompletedStep> {
        if let Some(abort) = &self.terminal {
            return Err(InstallFailure::Abort(abort.clone()));
        }
        self.permission.take(); // Invalidate before EVERY attempt.
        if !Arc::ptr_eq(&self.context.cancel, &step.context.cancel) {
            let abort = AbortInstallation {
                cause: AbortCause::Custody,
                cleanup_confirmed: true,
                capture_complete: false,
                label: "producer operation mismatch".into(),
                config_path: self.context.config_path.clone(),
                limit_key: "operation_timeout_seconds",
                evidence: self.lease.evidence.clone(),
            };
            self.terminal = Some(abort.clone());
            return Err(InstallFailure::Abort(abort));
        }
        match step.run(self.lease.clone()) {
            Ok(step) => {
                if !Arc::ptr_eq(&step.lease.state.0, &self.lease.state.0) {
                    let abort = AbortInstallation {
                        cause: AbortCause::Custody,
                        cleanup_confirmed: false,
                        capture_complete: false,
                        label: "producer owner mismatch".into(),
                        config_path: self.context.config_path.clone(),
                        limit_key: "operation_timeout_seconds",
                        evidence: self.lease.evidence.clone(),
                    };
                    self.terminal = Some(abort.clone());
                    return Err(InstallFailure::Abort(abort));
                }
                self.permission = step
                    .lease
                    .state
                    .permission(&self.context, self.lease.evidence.clone());
                if self.permission.is_none() {
                    let failure = authority_failure(
                        &self.context,
                        "producer proof is missing or invalidated",
                        self.lease.evidence.clone(),
                    );
                    if let InstallFailure::Abort(abort) = &failure {
                        self.terminal = Some(abort.clone());
                    }
                    return Err(failure);
                }
                Ok(CompletedStep {
                    status: step.status,
                    stdout: step.stdout,
                    stderr: step.stderr,
                    log: step.log,
                })
            }
            // The native boundary emits Ordinary only before admission. Earlier
            // settled facts remain valid when this attempt admitted no producer.
            Err(InstallFailure::Ordinary(error)) => {
                self.permission = self
                    .lease
                    .state
                    .permission(&self.context, self.lease.evidence.clone());
                Err(InstallFailure::Ordinary(error))
            }
            Err(InstallFailure::Abort(abort)) => {
                self.terminal = Some(abort.clone());
                Err(InstallFailure::Abort(abort))
            }
        }
    }
    pub(crate) fn finish(mut self) -> InstallResult<ProducerPermission> {
        if let Some(abort) = self.terminal.take() {
            return Err(InstallFailure::Abort(abort));
        }
        match self.permission.take() {
            Some(permission) if permission.current() => Ok(permission),
            _ => Err(authority_failure(
                &self.context,
                "producer settlement is unavailable or stale",
                self.lease.evidence.clone(),
            )),
        }
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
        context
            .proxy
            .apply(&mut command, &|name| std::env::var(name).ok());
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
        let ticket = lease.begin(&self.context)?;
        native::run(self, lease, ticket)
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
