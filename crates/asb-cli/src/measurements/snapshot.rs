//! Cancellable, bounded observations; the worker retains any uncertain custody.
use super::{ProcessIdentity, TreeSnapshotArguments, measurement_interval_ms, process_tree_pids};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub(super) struct ProcessTreeSnapshot {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<Result<(), String>>>,
}
impl ProcessTreeSnapshot {
    pub(super) fn start(
        root: ProcessIdentity,
        executable: PathBuf,
        arguments: TreeSnapshotArguments,
        mut stdout: File,
        mut stderr: File,
    ) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("asb-process-tree-snapshot".into())
            .spawn(move || {
                let started = Instant::now();
                while !cancelled.load(Ordering::Acquire) && root.matches() {
                    let members = process_tree_pids(root.pid)
                        .into_iter()
                        .filter_map(ProcessIdentity::read)
                        .filter(|member| member.in_tree(root))
                        .collect::<Vec<_>>();
                    if !root.matches() || members.is_empty() {
                        break;
                    }
                    let pids = members.iter().map(|member| member.pid).collect::<Vec<_>>();
                    let request = arguments(&pids);
                    let Some(observation) = observe(&executable, request, &cancelled)? else {
                        break;
                    };
                    if !root.matches() || members.iter().any(|member| !member.in_tree(root)) {
                        continue;
                    }
                    writeln!(
                        stdout,
                        "# asb-snapshot-elapsed-seconds {:.6}",
                        started.elapsed().as_secs_f64()
                    )
                    .map_err(|error| {
                        format!("cannot write process-tree snapshot timestamp: {error}")
                    })?;
                    stdout.write_all(&observation.stdout).map_err(|error| {
                        format!("cannot write process-tree snapshot stdout: {error}")
                    })?;
                    stderr.write_all(&observation.stderr).map_err(|error| {
                        format!("cannot write process-tree snapshot stderr: {error}")
                    })?;
                    stdout.flush().map_err(|error| {
                        format!("cannot flush process-tree snapshot stdout: {error}")
                    })?;
                    stderr.flush().map_err(|error| {
                        format!("cannot flush process-tree snapshot stderr: {error}")
                    })?;
                    let deadline =
                        Instant::now() + Duration::from_millis(measurement_interval_ms());
                    while !cancelled.load(Ordering::Acquire) && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                Ok(())
            })
            .ok()?;
        Some(Self {
            stop,
            handle: Some(handle),
        })
    }
    pub(super) fn stop(mut self) -> Result<(), String> {
        self.settle()
    }
    #[cfg(test)]
    pub(super) fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(|handle| handle.is_finished())
    }
    fn settle(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        let deadline = Instant::now() + Duration::from_secs(6);
        while !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if !handle.is_finished() {
            // The existing worker still owns its files and guardian. Do not
            // join it, publish a complete capture, or create another worker.
            return Err("process-tree snapshot settlement is unconfirmed; existing worker retains producer custody".into());
        }
        handle
            .join()
            .map_err(|_| "process-tree snapshot worker failed")?
    }
}
impl Drop for ProcessTreeSnapshot {
    fn drop(&mut self) {
        let _ = self.settle();
    }
}

/// Generic command limits are supplied by the owning adapter, never inferred
/// from a collector ID in the measurement runtime.
pub(crate) struct SnapshotCommand {
    pub arguments: Vec<String>,
    pub runtime: Duration,
    pub stream_bytes: usize,
}

struct Observation {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(target_os = "linux")]
fn runtime() -> io::Result<PathBuf> {
    #[cfg(test)]
    return Ok(std::env::current_exe()?
        .parent()
        .ok_or_else(|| io::Error::other("missing test binary parent"))?
        .parent()
        .ok_or_else(|| io::Error::other("missing actual ASB binary parent"))?
        .join("asb"));
    #[cfg(not(test))]
    crate::process_gate::executable()
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn observe(
    executable: &std::path::Path,
    request: SnapshotCommand,
    cancelled: &AtomicBool,
) -> Result<Option<Observation>, String> {
    if request.runtime.is_zero()
        || request.runtime > Duration::from_secs(3600)
        || request.stream_bytes == 0
        || request.stream_bytes > 256 * 1024 * 1024
    {
        return Err("process-tree snapshot command has invalid capture limits".into());
    }
    if cancelled.load(Ordering::Acquire) {
        return Ok(None);
    }
    let mut launch = crate::launch::ProcessLaunch::new(executable);
    for argument in &request.arguments {
        launch.arg(argument);
    }
    let (mut command, route) = launch.native(Some(&runtime().map_err(|error| error.to_string())?));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut gate = crate::process_gate::Admission::with_principal_control(
        route
            .validate_gate(&mut command)
            .map_err(|error| error.to_string())?,
    )
    .and_then(|admission| admission.spawn())
    .map_err(|error| error.to_string())?;
    let mut out = gate.take_stdout().ok_or("missing snapshot stdout")?;
    let mut err = gate.take_stderr().ok_or("missing snapshot stderr")?;
    nonblocking(&out)
        .and_then(|()| nonblocking(&err))
        .map_err(|error| error.to_string())?;
    gate.resume().map_err(|error| error.to_string())?;
    let deadline = Instant::now() + request.runtime;
    let mut output = [Vec::new(), Vec::new()];
    let mut eof = [false; 2];
    let mut exited = false;
    let mut drain = None;
    loop {
        if cancelled.load(Ordering::Acquire) {
            if !exited {
                let grace = Instant::now() + Duration::from_millis(100);
                let _ = gate.interrupt_principal(grace);
                while Instant::now() < grace {
                    if gate
                        .try_wait()
                        .map_err(|error| error.to_string())?
                        .is_some()
                    {
                        exited = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                // Delivery acknowledgement is never used as quiescence proof.
                let _acknowledged = gate.interruption_acknowledged();
                if !exited {
                    gate.stop().map_err(|error| error.to_string())?;
                }
            }
            return Ok(None);
        }
        if !exited
            && gate
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
        {
            let status = gate
                .principal_exit()
                .ok_or("snapshot principal termination is unknown")?
                .status();
            if !status.success() {
                return Err("process-tree snapshot command failed".into());
            }
            exited = true;
            drain = Some(Instant::now() + Duration::from_secs(1));
        }
        if !exited && Instant::now() >= deadline {
            return Err("process-tree snapshot command exceeded its deadline".into());
        }
        for (index, pipe) in [
            (0, &mut out as &mut dyn Read),
            (1, &mut err as &mut dyn Read),
        ] {
            let mut count = 0;
            while !eof[index] && count < 64 * 1024 {
                let mut buffer = [0; 4096];
                match pipe.read(&mut buffer) {
                    Ok(0) => eof[index] = true,
                    Ok(length) => {
                        if output[index].len() + length > request.stream_bytes {
                            return Err(
                                "process-tree snapshot output exceeded its capture limit".into()
                            );
                        }
                        output[index].extend_from_slice(&buffer[..length]);
                        count += length;
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => return Err("cannot read process-tree snapshot output".into()),
                }
            }
        }
        if exited && eof.iter().all(|eof| *eof) {
            let [out, err] = output;
            return Ok(Some(Observation {
                stdout: out,
                stderr: err,
            }));
        }
        if drain.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(
                "process-tree snapshot output remained open after confirmed tree quiescence".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(not(target_os = "linux"))]
fn observe(
    _: &std::path::Path,
    _: SnapshotCommand,
    _: &AtomicBool,
) -> Result<Option<Observation>, String> {
    Err("bounded process-tree snapshot supervision requires Linux native gate support".into())
}
