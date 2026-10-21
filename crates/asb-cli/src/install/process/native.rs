//! Linux producer custody. File logging never owns cancellation authority.
#![allow(unsafe_code)] // fcntl only configures owned pipe descriptors, never PIDs.
use super::*;
use crate::process_gate::{Admission, ManagedGate};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::process::Stdio;
use std::sync::{Mutex, mpsc};

const CLEANUP: Duration = Duration::from_secs(6);
const LOG_BYTES: usize = 16 * 1024 * 1024;
const CHUNK: usize = 8192;

#[cfg(test)]
pub(super) type WriterFactory = Box<dyn FnOnce(File) -> Box<dyn Write + Send> + Send>;

#[derive(Clone, Debug, Default)]
pub(crate) struct LogReport {
    pub(crate) path: Option<PathBuf>,
    pub(crate) retained: usize,
    pub(crate) accepted: usize,
    pub(crate) discarded: usize,
    pub(crate) fault: Option<io::ErrorKind>,
    pub(crate) settled: bool,
}

struct LogSink {
    queue: Option<mpsc::SyncSender<Vec<u8>>>,
    report: Arc<Mutex<LogReport>>,
}
impl LogSink {
    fn new(root: &Path, #[cfg(test)] factory: Option<WriterFactory>) -> Self {
        let report = Arc::new(Mutex::new(LogReport::default()));
        let prepared = private_log(root);
        let (file, path) = match prepared {
            Ok(value) => value,
            Err(error) => {
                report.lock().unwrap().fault = Some(error.kind());
                return Self {
                    queue: None,
                    report,
                };
            }
        };
        report.lock().unwrap().path = Some(path);
        let (queue, receive) = mpsc::sync_channel::<Vec<u8>>(16);
        let state = Arc::clone(&report);
        let spawned = std::thread::Builder::new()
            .name("asb-install-log".into())
            .spawn(move || {
                // This thread owns only the log file, never the gate or source stage.
                // An uninterruptible regular-file write cannot block cancellation.
                #[cfg(test)]
                let mut file: Box<dyn Write + Send> = match factory {
                    Some(factory) => factory(file),
                    None => Box::new(file),
                };
                #[cfg(not(test))]
                let mut file = file;
                let mut failed = false;
                for bytes in receive {
                    let mut offset = 0;
                    let mut interruptions = 0;
                    while !failed && offset < bytes.len() {
                        match file.write(&bytes[offset..]) {
                            Ok(0) => {
                                state.lock().unwrap().fault = Some(io::ErrorKind::WriteZero);
                                failed = true;
                            }
                            Ok(count) => {
                                offset += count;
                                interruptions = 0;
                                state.lock().unwrap().retained += count;
                            }
                            Err(error)
                                if error.kind() == io::ErrorKind::Interrupted
                                    && interruptions < 32 =>
                            {
                                interruptions += 1;
                            }
                            Err(error) => {
                                state.lock().unwrap().fault = Some(error.kind());
                                failed = true;
                            }
                        }
                    }
                    let mut report = state.lock().unwrap();
                    report.discarded = report.discarded.saturating_add(bytes.len() - offset);
                }
                if !failed && let Err(error) = file.flush() {
                    state.lock().unwrap().fault = Some(error.kind());
                }
                state.lock().unwrap().settled = true;
            });
        if let Err(error) = spawned {
            report.lock().unwrap().fault = Some(error.kind());
            return Self {
                queue: None,
                report,
            };
        }
        Self {
            queue: Some(queue),
            report,
        }
    }
    fn push(&self, bytes: &[u8]) {
        let mut report = self.report.lock().unwrap();
        let count = bytes.len().min(LOG_BYTES.saturating_sub(report.accepted));
        if count == 0 || self.queue.is_none() || report.fault.is_some() {
            report.discarded = report.discarded.saturating_add(bytes.len());
            return;
        }
        // Only bounded in-memory queue operations run under this short lock.
        match self
            .queue
            .as_ref()
            .unwrap()
            .try_send(bytes[..count].to_vec())
        {
            Ok(()) => {
                report.accepted += count;
                report.discarded = report.discarded.saturating_add(bytes.len() - count);
            }
            Err(mpsc::TrySendError::Full(_)) => {
                report.discarded = report.discarded.saturating_add(bytes.len())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                report.fault.get_or_insert(io::ErrorKind::BrokenPipe);
                report.discarded = report.discarded.saturating_add(bytes.len());
            }
        }
    }
    fn finish(mut self) -> LogReport {
        self.queue.take();
        // Never join a potentially blocked regular-file writer. The report
        // explicitly distinguishes pending diagnostics from complete capture.
        self.report.lock().unwrap().clone()
    }
}

fn private_log(root: &Path) -> io::Result<(File, PathBuf)> {
    private_log_at(
        root,
        &format!(".asb-install-step-{}", crate::tools::runtime::unique_id()),
    )
}

fn private_log_at(root: &Path, attempt: &str) -> io::Result<(File, PathBuf)> {
    // Walk each existing directory without symlink following. The new private
    // namespace is unique and create-new; foreign logs are never touched.
    let root = if root.is_absolute() {
        root.to_owned()
    } else {
        std::env::current_dir()?.join(root)
    };
    let mut current = PathBuf::from("/");
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&current)?;
    for component in root.components() {
        use std::path::Component;
        match component {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => current.push(name),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "installer project path must not contain parent traversal",
                ));
            }
        }
        directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(
                PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
                    .join(component.as_os_str()),
            )?;
    }
    let parent = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(parent.join(attempt))?;
    let pinned = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(parent.join(attempt))?;
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(PathBuf::from(format!("/proc/self/fd/{}", pinned.as_raw_fd())).join("output.log"))?;
    Ok((log, root.join(attempt).join("output.log")))
}

struct Pipe {
    reader: Box<dyn Read + Send>,
    output: Vec<u8>,
    ended: bool,
}
impl Pipe {
    fn read_turn(
        &mut self,
        capture: Capture,
        other_bytes: usize,
        log: &LogSink,
    ) -> Result<(), AbortCause> {
        if self.ended {
            return Ok(());
        }
        let mut buffer = [0; CHUNK];
        for _ in 0..8 {
            match self.reader.read(&mut buffer) {
                Ok(0) => {
                    self.ended = true;
                    return Ok(());
                }
                Ok(count) => {
                    log.push(&buffer[..count]);
                    if let Capture::Inspect {
                        stream_bytes,
                        total_bytes,
                    } = capture
                    {
                        if count > stream_bytes.saturating_sub(self.output.len())
                            || count
                                > total_bytes
                                    .saturating_sub(self.output.len().saturating_add(other_bytes))
                        {
                            return Err(AbortCause::CaptureOverflow);
                        }
                        self.output.extend_from_slice(&buffer[..count]);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(_) => {
                    self.ended = true;
                    return Err(AbortCause::CaptureRead);
                }
            }
        }
        Ok(())
    }
}

fn nonblocking(file: &impl AsRawFd) -> io::Result<()> {
    // SAFETY: the descriptor is borrowed from a live, uniquely owned pipe.
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn run(step: PreparedStep, lease: ProducerLease) -> InstallResult<SettledStep> {
    let (deadline, limit_key) = step.context.step_deadline(step.purpose)?;
    if step.context.cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
        let cause = if Instant::now() >= deadline {
            AbortCause::Deadline
        } else {
            AbortCause::Cancelled
        };
        return Err(InstallFailure::Abort(abort(
            &step, limit_key, cause, true, true, None,
        )));
    }
    let cancel = Arc::clone(&step.context.cancel);
    let fallback = abort(
        &step,
        limit_key,
        AbortCause::Worker,
        false,
        false,
        lease.evidence.clone(),
    );
    let (send, receive) = mpsc::sync_channel(1);
    // The thread exists before Admission.spawn. If allocation fails, no target
    // has been admitted and no producer can hold a descriptor into the stage.
    std::thread::Builder::new()
        .name("asb-install-process".into())
        .spawn(move || supervise(step, lease, deadline, limit_key, send))
        .map_err(|error| {
            crate::Error::Io(format!(
                "cannot establish installation custody worker: {}",
                error.kind()
            ))
        })?;
    let mut cancellation_end = None;
    loop {
        match receive.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                cancel.store(true, Ordering::Release);
                return Err(InstallFailure::Abort(fallback));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            cancel.store(true, Ordering::Release);
            let end = *cancellation_end.get_or_insert_with(|| Instant::now() + CLEANUP);
            if Instant::now() >= end {
                return Err(InstallFailure::Abort(AbortInstallation {
                    cause: if Instant::now() >= deadline {
                        AbortCause::Deadline
                    } else {
                        AbortCause::Cancelled
                    },
                    ..fallback
                }));
            }
        }
    }
}

fn abort(
    step: &PreparedStep,
    key: &'static str,
    cause: AbortCause,
    cleanup: bool,
    capture: bool,
    evidence: Option<PathBuf>,
) -> AbortInstallation {
    AbortInstallation {
        cause,
        cleanup_confirmed: cleanup,
        capture_complete: capture,
        label: step.label.clone(),
        config_path: step.context.config_path.clone(),
        limit_key: key,
        evidence,
    }
}

fn supervise(
    mut step: PreparedStep,
    lease: ProducerLease,
    deadline: Instant,
    key: &'static str,
    send: mpsc::SyncSender<InstallResult<SettledStep>>,
) {
    // These resources remain outside every fallible/unwinding pump turn.
    let _lease = lease;
    let log = LogSink::new(
        &step.context.root,
        #[cfg(test)]
        step.log_writer.take(),
    );
    step.command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let admitted = step
        .route
        .take()
        .expect("one admission route")
        .validate_gate(&mut step.command)
        .and_then(Admission::new)
        .and_then(Admission::spawn);
    let mut gate: ManagedGate = match admitted {
        Ok(gate) => gate,
        Err(error) => {
            let _ = send.send(Err(crate::Error::Io(format!(
                "cannot start installation step {}: {}",
                step.label,
                error.kind()
            ))
            .into()));
            return;
        }
    };
    let mut cause = None;
    let stdout = gate.take_stdout().expect("configured stdout pipe");
    let stderr = gate.take_stderr().expect("configured stderr pipe");
    let unusable = nonblocking(&stdout)
        .and_then(|_| nonblocking(&stderr))
        .is_err();
    if unusable {
        cause = Some(AbortCause::CaptureRead);
    }
    let mut pipes = [
        Pipe {
            reader: Box::new(stdout),
            output: Vec::new(),
            ended: unusable,
        },
        Pipe {
            reader: Box::new(stderr),
            output: Vec::new(),
            ended: unusable,
        },
    ];
    if cause.is_none() && gate.resume().is_err() {
        cause = Some(AbortCause::Custody);
    }
    let mut status = None;
    let mut cleanup_deadline = None;
    let mut sent = false;
    loop {
        if Instant::now() >= deadline {
            cause.get_or_insert(AbortCause::Deadline);
        }
        if step.context.cancel.load(Ordering::Acquire) {
            cause.get_or_insert(AbortCause::Cancelled);
        }
        let turn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for index in 0..2 {
                let other = pipes[1 - index].output.len();
                if let Err(error) = pipes[index].read_turn(step.capture, other, &log) {
                    cause.get_or_insert(error);
                }
            }
        }));
        if turn.is_err() {
            cause.get_or_insert(AbortCause::Worker);
        }
        #[cfg(test)]
        let hold_observation = step
            .hold_observation
            .as_ref()
            .is_some_and(|hold| hold.load(Ordering::Acquire));
        #[cfg(not(test))]
        let hold_observation = false;
        match if hold_observation {
            Ok(None)
        } else {
            gate.try_wait()
        } {
            Ok(Some(exit)) => status = Some(exit),
            Ok(None) => {}
            Err(_) => {
                cause.get_or_insert(AbortCause::Custody);
            }
        }
        if let Some(reason) = cause {
            let end = *cleanup_deadline.get_or_insert_with(|| Instant::now() + CLEANUP);
            if status.is_none() {
                let _ = gate.request_cancel();
            }
            if Instant::now() >= end && !sent {
                let evidence = _lease
                    .evidence
                    .clone()
                    .or_else(|| log.report.lock().unwrap().path.clone());
                let _ = send.send(Err(InstallFailure::Abort(abort(
                    &step,
                    key,
                    reason,
                    status.is_some(),
                    false,
                    evidence,
                ))));
                sent = true;
                // Keep exact gate, pumps, and the actual producer dependency in
                // this existing worker until proof, regardless of parent wait.
            }
        }
        if let Some(status) = status
            && pipes.iter().all(|pipe| pipe.ended)
        {
            if !sent {
                let log = log.finish();
                let result = match cause {
                    Some(cause) => Err(InstallFailure::Abort(abort(
                        &step,
                        key,
                        cause,
                        true,
                        false,
                        _lease.evidence.clone().or(log.path),
                    ))),
                    None => {
                        let [stdout, stderr] = pipes;
                        Ok(SettledStep {
                            status,
                            stdout: stdout.output,
                            stderr: stderr.output,
                            log,
                        })
                    }
                };
                let _ = send.send(result);
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(if sent { 25 } else { 2 }));
    }
}

#[cfg(test)]
mod tests;
