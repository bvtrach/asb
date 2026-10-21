//! One bounded duplex exchange; Linux ownership includes helper descendants.
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const RESPONSE: usize = 4 * 1024 * 1024;
const QUANTUM: usize = 64 * 1024;
#[cfg(not(target_os = "linux"))]
const CLEANUP: Duration = Duration::from_secs(6);

enum Process {
    #[cfg(target_os = "linux")]
    Gate(crate::process_gate::ManagedGate),
    #[cfg(not(target_os = "linux"))]
    Direct {
        child: Option<std::process::Child>,
        keeper: crate::process_cleanup::OwnedReaper,
    },
}
impl Process {
    fn spawn(python: &Path, source: &str) -> Result<Self, &'static str> {
        let mut launch = crate::launch::ProcessLaunch::new(python);
        for arg in ["-I", "-S", "-B", "-c", source] {
            launch.arg(arg);
        }
        #[cfg(target_os = "linux")]
        {
            let binary = runtime().map_err(|_| "runtime supervisor unavailable")?;
            let (mut command, route) = launch.native(Some(&binary));
            pipes(&mut command);
            let validated = route
                .validate_gate(&mut command)
                .map_err(|_| "invalid runtime launch")?;
            let admission = crate::process_gate::Admission::new(validated)
                .map_err(|_| "runtime cleanup custody unavailable before spawn")?;
            let mut gate = admission
                .spawn()
                .map_err(|_| "runtime helper could not start")?;
            if gate.resume().is_err() {
                return Err(if gate.stop().is_ok() {
                    "runtime supervisor did not admit the helper"
                } else {
                    "runtime admission failed and cleanup remains unconfirmed; pre-established custody retained"
                });
            }
            Ok(Self::Gate(gate))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let keeper = crate::process_cleanup::OwnedReaper::new("runtime-discovery")
                .map_err(|_| "runtime cleanup custody unavailable before spawn")?;
            let mut command = launch.direct();
            pipes(&mut command);
            let child = command
                .spawn()
                .map_err(|_| "runtime helper could not start")?;
            Ok(Self::Direct {
                child: Some(child),
                keeper,
            })
        }
    }
    fn input(&mut self) -> Option<ChildStdin> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Gate(gate) => gate.take_stdin(),
            #[cfg(not(target_os = "linux"))]
            Self::Direct { child, .. } => child.as_mut()?.stdin.take(),
        }
    }
    fn output(&mut self) -> Option<ChildStdout> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Gate(gate) => gate.take_stdout(),
            #[cfg(not(target_os = "linux"))]
            Self::Direct { child, .. } => child.as_mut()?.stdout.take(),
        }
    }
    fn poll(&mut self) -> io::Result<Option<ExitStatus>> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Gate(gate) => gate.try_wait(),
            #[cfg(not(target_os = "linux"))]
            Self::Direct { child, .. } => child
                .as_mut()
                .ok_or_else(|| io::Error::other("child retained in cleanup custody"))?
                .try_wait(),
        }
    }
    fn stop(&mut self) -> io::Result<()> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Gate(gate) => gate.stop().map(|_| ()),
            #[cfg(not(target_os = "linux"))]
            Self::Direct { child, keeper } => {
                let Some(mut owned) = child.take() else {
                    return Ok(());
                };
                if matches!(owned.try_wait(), Ok(Some(_))) {
                    return Ok(());
                }
                let _ = owned.kill();
                let deadline = Instant::now() + CLEANUP;
                loop {
                    match owned.try_wait() {
                        Ok(Some(_)) => return Ok(()),
                        Ok(None) if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        _ => {
                            keeper.adopt(owned);
                            return Err(io::Error::other(
                                "runtime child retained in cleanup custody",
                            ));
                        }
                    }
                }
            }
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
#[cfg(target_os = "linux")]
fn runtime() -> io::Result<std::path::PathBuf> {
    #[cfg(test)]
    if std::env::var_os("ASB_RUNTIME_EXECUTABLE").is_none() {
        return Ok(std::env::current_exe()?
            .parent()
            .ok_or_else(|| io::Error::other("test executable parent missing"))?
            .parent()
            .ok_or_else(|| io::Error::other("test target directory missing"))?
            .join("asb"));
    }
    crate::process_gate::executable()
}
fn pipes(command: &mut Command) {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
}
#[allow(unsafe_code)]
fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    // SAFETY: this owner exclusively holds the newly created pipe endpoint.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(super) fn run(
    python: &Path,
    source: &str,
    request: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("runtime exchange deadline overflows")?;
    let mut process = Process::spawn(python, source)?;
    #[cfg(test)]
    faults::after_spawn(&mut process);
    let result = communicate(&mut process, request, deadline);
    match (result, process.stop()) {
        (result, Ok(())) => result.map_err(str::to_owned),
        (result, Err(_)) => Err(format!(
            "{}; cleanup remains unconfirmed; pre-established custody retained",
            result.err().unwrap_or("runtime exchange completed")
        )),
    }
}
fn communicate(
    process: &mut Process,
    request: &[u8],
    deadline: Instant,
) -> Result<Vec<u8>, &'static str> {
    let mut input = Some(process.input().ok_or("runtime stdin unavailable")?);
    let mut output = Some(process.output().ok_or("runtime stdout unavailable")?);
    nonblocking(input.as_ref().unwrap()).map_err(|_| "runtime stdin configuration failed")?;
    nonblocking(output.as_ref().unwrap()).map_err(|_| "runtime stdout configuration failed")?;
    let mut sent = 0;
    let mut bytes = Vec::new();
    let mut status = None;
    loop {
        if Instant::now() >= deadline {
            return Err("runtime exchange deadline exceeded");
        }
        if let Some(writer) = input.as_mut() {
            let end = request.len().min(sent + QUANTUM);
            match writer.write(&request[sent..end]) {
                Ok(0) if sent < request.len() => {
                    return Err("runtime request write made no progress");
                }
                Ok(count) => sent += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err("runtime request delivery failed"),
            }
            if sent == request.len() {
                input = None;
            }
        }
        if let Some(reader) = output.as_mut() {
            let mut buffer = [0; 16 * 1024];
            let mut consumed = 0;
            while consumed < QUANTUM && Instant::now() < deadline {
                #[cfg(test)]
                if faults::read_failure() {
                    return Err("runtime response read failed");
                }
                let remaining = (RESPONSE + 1 - bytes.len()).min(buffer.len());
                match reader.read(&mut buffer[..remaining]) {
                    Ok(0) => {
                        output = None;
                        break;
                    }
                    Ok(count) => {
                        if count > RESPONSE - bytes.len() {
                            return Err("runtime response exceeds its byte bound");
                        }
                        bytes.reserve_exact(count);
                        bytes.extend_from_slice(&buffer[..count]);
                        consumed += count;
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => return Err("runtime response read failed"),
                }
            }
        }
        if status.is_none() {
            status = process
                .poll()
                .map_err(|_| "runtime exit or quiescence was not confirmed")?;
        }
        if let Some(status) = status {
            if !status.success() {
                return Err("runtime helper failed");
            }
            if sent != request.len() {
                return Err("runtime helper exited before request delivery completed");
            }
            if output.is_none() {
                return Ok(bytes);
            }
        }
        std::thread::sleep(
            Duration::from_millis(2).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(test)]
pub(super) mod faults {
    use super::*;
    std::thread_local! { static FAULT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
    pub(in crate::runtime_inputs) fn set(value: u8) {
        FAULT.set(value);
    }
    pub(super) fn read_failure() -> bool {
        FAULT.get() == 1
    }
    pub(super) fn after_spawn(process: &mut Process) {
        #[cfg(target_os = "linux")]
        if FAULT.get() == 3 {
            let Process::Gate(gate) = process;
            gate.lose_receipt_for_test();
        }
        if FAULT.get() == 2 {
            panic!("private runtime unwind fixture");
        }
    }
}
