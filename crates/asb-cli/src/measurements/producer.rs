//! Collector-independent ownership and capture facts, never metric/tool policy.
use super::snapshot::ProcessTreeSnapshot;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::{Command, ExitStatus};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct Completion {
    pub exit: Option<ExitStatus>,
    pub capture_complete: bool,
    pub error: Option<String>,
}
impl Completion {
    fn failed(exit: Option<ExitStatus>, reason: impl Into<String>) -> Self {
        Self { exit, capture_complete: false, error: Some(reason.into()) }
    }
}

/// Output and graceful-flush policy is supplied by the owning adapter.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub stream_bytes: u64,
    pub graceful: Option<Duration>,
}

pub(crate) fn command(launch: &mut crate::launch::ProcessLaunch) -> Result<(Command, crate::launch::CommandRoute), String> {
    #[cfg(target_os = "linux")]
    return Ok(launch.native(Some(&super::snapshot::runtime().map_err(|error| error.to_string())?)));
    #[cfg(not(target_os = "linux"))]
    Ok(launch.native(None))
}

pub(crate) struct Producer {
    active: Option<Active>,
    completion: Option<Completion>,
}
enum Active {
    #[cfg(target_os = "linux")]
    Process(Process),
    Snapshot(ProcessTreeSnapshot),
}
impl Producer {
    pub fn snapshot(snapshot: ProcessTreeSnapshot) -> Self {
        Self { active: Some(Active::Snapshot(snapshot)), completion: None }
    }
    pub fn spawn(command: &mut Command, route: crate::launch::CommandRoute, policy: Policy, files: [File; 2]) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            if policy.stream_bytes == 0 || policy.stream_bytes > 256 * 1024 * 1024
                || policy.graceful.is_some_and(|grace| grace.is_zero() || grace > Duration::from_secs(60)) {
                return Err("invalid collector capture policy".into());
            }
            command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
            let gate = crate::process_gate::Admission::with_principal_control(route.validate_gate(command).map_err(|error| error.to_string())?)
                .and_then(|admission| admission.spawn()).map_err(|error| error.to_string())?;
            let stopped = Arc::new(AtomicBool::new(false));
            let request = Arc::clone(&stopped);
            // Allocate the worker before it can resume this stopped guardian.
            // Failed spawn drops only the unadmitted gate into existing custody.
            let handle = std::thread::Builder::new().name("asb-collector-producer".into())
                .spawn(move || capture(gate, files, policy, &request))
                .map_err(|error| format!("cannot start collector capture worker: {error}"))?;
            Ok(Self { active: Some(Active::Process(Process { stop: stopped, handle: Some(handle), policy })), completion: None })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (command, route, policy, files);
            Err("owned collector process capture requires Linux native gate supervision".into())
        }
    }
    pub fn poll(&mut self) -> Result<Option<Completion>, String> {
        if let Some(completion) = &self.completion { return Ok(Some(completion.clone())); }
        #[cfg(target_os = "linux")]
        if let Some(Active::Process(process)) = self.active.as_ref()
            && process.handle.as_ref().is_none_or(|handle| handle.is_finished()) {
            return Ok(Some(self.stop()));
        }
        Ok(None)
    }
    pub fn stop(&mut self) -> Completion {
        if let Some(completion) = &self.completion { return completion.clone(); }
        let completion = match self.active.take() {
            #[cfg(target_os = "linux")]
            Some(Active::Process(mut process)) => process.settle(),
            Some(Active::Snapshot(snapshot)) => match snapshot.stop() {
                Ok(()) => Completion { exit: None, capture_complete: true, error: None },
                Err(error) => Completion::failed(None, error),
            },
            None => Completion::failed(None, "collector ownership already transferred; settlement unconfirmed"),
        };
        self.completion = Some(completion.clone());
        completion
    }
    #[cfg(test)]
    pub fn completed(exit: ExitStatus) -> Self {
        Self { active: None, completion: Some(Completion { exit: Some(exit), capture_complete: true, error: None }) }
    }
}
impl Drop for Producer {
    fn drop(&mut self) { let _ = self.stop(); }
}

#[cfg(target_os = "linux")]
struct Process {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<Completion>>,
    policy: Policy,
}
#[cfg(target_os = "linux")]
impl Process {
    fn settle(&mut self) -> Completion {
        self.stop.store(true, Ordering::Release);
        let Some(handle) = self.handle.take() else { return Completion::failed(None, "collector worker custody is unconfirmed"); };
        let deadline = Instant::now() + self.policy.graceful.unwrap_or_default() + Duration::from_secs(8);
        while !handle.is_finished() && Instant::now() < deadline { std::thread::sleep(Duration::from_millis(5)); }
        if !handle.is_finished() {
            return Completion::failed(None, "collector settlement deadline exceeded; existing worker retains guardian and raw capture custody");
        }
        handle.join().unwrap_or_else(|_| Completion::failed(None, "collector capture worker panicked; capture completeness unconfirmed"))
    }
}

#[cfg(target_os = "linux")]
fn capture(mut gate: crate::process_gate::ManagedGate, mut files: [File; 2], policy: Policy, stop: &AtomicBool) -> Completion {
    let mut exit = None;
    let outcome = (|| -> Result<(), String> {
        let mut out = gate.take_stdout().ok_or("missing collector stdout pipe")?;
        let mut err = gate.take_stderr().ok_or("missing collector stderr pipe")?;
        super::snapshot::nonblocking(&out).and_then(|()| super::snapshot::nonblocking(&err)).map_err(|error| error.to_string())?;
        gate.resume().map_err(|error| error.to_string())?;
        let mut lengths = [0, 0];
        let mut eof = [false; 2];
        let mut grace = None;
        let mut stopped = false;
        let mut drain = None;
        loop {
            if stop.load(Ordering::Acquire) && !stopped {
                stopped = true;
                if let Some(duration) = policy.graceful {
                    let deadline = Instant::now() + duration;
                    gate.interrupt_principal(deadline).map_err(|error| format!("collector principal interruption failed: {error}"))?;
                    grace = Some(deadline);
                } else {
                    gate.stop().map_err(|error| error.to_string())?;
                    return Err("collector was cancelled without a graceful flush contract; raw capture retained".into());
                }
            }
            if drain.is_none() && gate.try_wait().map_err(|error| error.to_string())?.is_some() {
                exit = gate.principal_exit().map(|principal| principal.status());
                if exit.is_none() { return Err("collector principal exit is unknown despite guardian settlement".into()); }
                drain = Some(Instant::now() + Duration::from_secs(1));
            }
            if drain.is_none() && grace.is_some_and(|deadline| Instant::now() >= deadline) {
                gate.stop().map_err(|error| error.to_string())?;
                return Err("collector graceful flush deadline exceeded; forced cancellation cannot prove complete capture".into());
            }
            for (index, pipe) in [(0, &mut out as &mut dyn Read), (1, &mut err as &mut dyn Read)] {
                pump(pipe, &mut files[index], &mut lengths[index], &mut eof[index], policy.stream_bytes, drain.or(grace))?;
            }
            if drain.is_some() && eof.iter().all(|eof| *eof) {
                for file in &mut files { file.flush().map_err(|error| format!("cannot flush collector capture: {error}"))?; }
                return Ok(());
            }
            if drain.is_some_and(|deadline| Instant::now() >= deadline) { return Err("collector pipe remained open after confirmed guardian quiescence; raw capture retained".into()); }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    match outcome {
        Ok(()) => Completion { exit, capture_complete: true, error: None },
        Err(error) => {
            let cleanup = gate.stop().err().map(|error| format!("; guardian cleanup unconfirmed: {error}"));
            Completion::failed(exit, format!("{error}{}", cleanup.unwrap_or_default()))
        }
    }
}

#[cfg(target_os = "linux")]
fn pump(reader: &mut dyn Read, writer: &mut dyn Write, length: &mut u64, eof: &mut bool, limit: u64, deadline: Option<Instant>) -> Result<(), String> {
    for _ in 0..16 {
        if *eof { break; }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) { break; }
        let mut buffer = [0; 4096];
        match reader.read(&mut buffer) {
            Ok(0) => *eof = true,
            Ok(count) => {
                *length = length.checked_add(count as u64).filter(|length| *length <= limit).ok_or("collector capture exceeds adapter byte limit")?;
                writer.write_all(&buffer[..count]).map_err(|error| format!("cannot write collector capture: {error}"))?;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(format!("cannot read collector capture: {error}")),
        }
    }
    Ok(())
}
