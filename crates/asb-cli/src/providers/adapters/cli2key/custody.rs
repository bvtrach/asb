//! Direct-child custody and bounded, continuously drained startup output.
use crate::process_cleanup::OwnedReaper;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(10);
const MAX_LINE: usize = 4096;
const MAX_BYTES: usize = 64 * 1024;
const MAX_LINES: usize = 64;

struct Reader {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // The nonblocking worker retains its stream until it observes stop.
        // Never join a potentially unscheduled thread from a destructor.
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let _ = self.worker.take().unwrap().join();
        }
    }
}

/// The same owner survives startup and the ready provider session.
pub(super) struct Local {
    children: Vec<Child>,
    readers: Vec<Reader>,
    reaper: OwnedReaper,
}

impl Local {
    pub(super) fn new() -> Result<Self, String> {
        Ok(Self {
            children: Vec::with_capacity(2),
            readers: Vec::with_capacity(2),
            reaper: OwnedReaper::new("cli2key")?,
        })
    }

    pub(super) fn spawn(&mut self, command: &mut Command, key: bool) -> Result<String, String> {
        let label = if key {
            "cli2key bridge"
        } else {
            "cli2key compatibility proxy"
        };
        // Allocate the transport and fallible reader BEFORE creating a child.
        let (mut input, output) = UnixStream::pair()
            .map_err(|error| format!("cannot establish cli2key output transport: {error}"))?;
        input
            .set_nonblocking(true)
            .map_err(|error| format!("cannot bound cli2key output transport: {error}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let (sender, receiver) = mpsc::sync_channel(1);
        #[cfg(all(test, target_os = "linux"))]
        tests::reader_creation()?;
        #[cfg(all(test, target_os = "linux"))]
        let receipt = tests::reader_receipt();
        let worker = std::thread::Builder::new()
            .name("asb-cli2key-output".into())
            .spawn(move || {
                drain_output(&mut input, &stopped, &sender, key);
                #[cfg(all(test, target_os = "linux"))]
                receipt.store(true, Ordering::Release);
            })
            .map_err(|error| format!("cannot establish cli2key output reader: {error}"))?;
        self.readers.push(Reader {
            stop,
            worker: Some(worker),
        });
        command.stdout(Stdio::from(OwnedFd::from(output)));
        match command.spawn() {
            Ok(child) => self.children.push(child), // Immediate custody.
            Err(error) => {
                command.stdout(Stdio::null());
                return Err(format!(
                    "could not start {label}: {error}. {}",
                    if key {
                        "Check directory access and reinstall with `asb tool install cli2key`."
                    } else {
                        "Check Node.js with `node --version` and verify executable permissions."
                    }
                ));
            }
        }
        // Command retains its configured Stdio across spawn. Close that parent
        // writer so child-side stdout closure really reaches the reader as EOF.
        command.stdout(Stdio::null());
        #[cfg(all(test, target_os = "linux"))]
        tests::after_spawn(self.children.last().unwrap().id())?;
        let deadline = Instant::now() + super::STARTUP_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("{label} did not become ready within 5 seconds"));
            }
            match receiver.recv_timeout(remaining.min(POLL)) {
                Ok(result) => {
                    let value = result?;
                    if let Some(status) = self
                        .children
                        .last_mut()
                        .unwrap()
                        .try_wait()
                        .map_err(|error| format!("cannot inspect cli2key readiness: {error}"))?
                    {
                        return Err(format!("{label} exited at readiness with {status}"));
                    }
                    return Ok(value);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(format!(
                        "{label} closed its startup output; {label} did not become ready"
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(status) = self
                .children
                .last_mut()
                .unwrap()
                .try_wait()
                .map_err(|error| format!("cannot inspect cli2key startup: {error}"))?
            {
                return Err(format!(
                    "{label} closed its startup output with {status}; {label} did not become ready"
                ));
            }
        }
    }

    pub(super) fn failure(mut self, reason: String) -> String {
        match self.cleanup() {
            Ok(()) => reason,
            Err(cleanup) => format!("{reason}; {cleanup}"),
        }
    }

    pub(super) fn finish(mut self) -> Result<(), String> {
        self.cleanup()
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let timeout = Duration::from_secs(5);
        #[cfg(all(test, target_os = "linux"))]
        let timeout = tests::cleanup_timeout().unwrap_or(timeout);
        let deadline = Instant::now() + timeout;
        for child in &mut self.children {
            let _ = child.kill();
        }
        let mut reasons = Vec::new();
        for mut child in self.children.drain(..).rev() {
            loop {
                match poll(&mut child) {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
                    result => {
                        reasons.push(match result {
                            Err(error) => format!("cannot confirm child exit: {error}"),
                            _ => "child did not exit before the shared cleanup deadline".into(),
                        });
                        self.reaper.adopt(child);
                        break;
                    }
                }
            }
        }
        for reader in &self.readers {
            reader.stop.store(true, Ordering::Release);
        }
        for reader in &mut self.readers {
            while reader
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
                && Instant::now() < deadline
            {
                std::thread::sleep(POLL);
            }
            if reader.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                let _ = reader.worker.take().unwrap().join();
            } else {
                reasons
                    .push("output reader did not finish before the shared cleanup deadline".into());
            }
        }
        self.readers.clear();
        if !reasons.is_empty() {
            Err(format!(
                "cli2key cleanup was not confirmed: {}; an owned reaper retains unconfirmed children and stopped readers retain their transports",
                reasons.join("; ")
            ))
        } else {
            Ok(())
        }
    }
}

struct Readiness {
    line: Vec<u8>,
    bytes: usize,
    lines: usize,
}

impl Readiness {
    fn new() -> Self {
        Self {
            line: Vec::with_capacity(MAX_LINE),
            bytes: 0,
            lines: 0,
        }
    }

    fn accept(&mut self, chunk: &[u8], key: bool) -> Option<Result<String, String>> {
        for &byte in chunk {
            self.bytes += 1;
            if self.bytes > MAX_BYTES || (byte != b'\n' && self.line.len() >= MAX_LINE) {
                return Some(Err(
                    "cli2key startup output exceeded its byte or line limit".into(),
                ));
            }
            if byte != b'\n' {
                self.line.push(byte);
                continue;
            }
            self.lines += 1;
            let text = std::str::from_utf8(&self.line)
                .ok()
                .map(|text| text.trim_end_matches('\r'));
            let ready = if key {
                text.and_then(|text| text.strip_prefix("Codex Bridge API key: "))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            } else {
                text.filter(|text| *text == "ASB cli2key proxy ready")
                    .map(|_| String::new())
            };
            self.line.clear();
            if let Some(value) = ready {
                return Some(Ok(value));
            }
            if self.lines >= MAX_LINES {
                return Some(Err("cli2key startup output exceeded its message limit; bridge did not provide an API key or proxy readiness".into()));
            }
        }
        None
    }
}

fn drain_output(
    input: &mut UnixStream,
    stopped: &AtomicBool,
    sender: &mpsc::SyncSender<Result<String, String>>,
    key: bool,
) {
    let mut startup = Some(Readiness::new());
    let mut chunk = [0; 2048];
    while !stopped.load(Ordering::Acquire) {
        match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if let Some(parser) = &mut startup
                    && let Some(result) = parser.accept(&chunk[..count], key)
                {
                    let _ = sender.try_send(result);
                    // Drain/discard subsequent bytes without parsing or retaining them.
                    startup = None;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL)
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn poll(child: &mut Child) -> std::io::Result<Option<std::process::ExitStatus>> {
    #[cfg(all(test, target_os = "linux"))]
    if let Some(result) = tests::poll_override() {
        return result;
    }
    child.try_wait()
}

impl Drop for Local {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../tests/unit/cli2key_custody.rs"]
mod tests;
