//! Owned, proof-aware gate custody. A process exit alone is not tree quiescence.
use super::receipt::{self, Receiver};
use crate::process_cleanup::OwnedReaper;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus};
use std::time::{Duration, Instant};

const STARTUP: Duration = Duration::from_secs(5);
const CLEANUP: Duration = Duration::from_secs(6);

/// Create fallback custody and the private channel before admitting a child.
pub(crate) struct Admission {
    receiver: Receiver,
    writer: UnixStream,
    keeper: OwnedReaper,
}

impl Admission {
    pub(crate) fn new(
        command: &mut Command,
        executable: &Path,
        target: &str,
        args: &[String],
    ) -> io::Result<Self> {
        let keeper = OwnedReaper::new("asb-gate-custody").map_err(io::Error::other)?;
        let (receiver, writer) = Receiver::pair()?;
        let fd = writer.as_raw_fd();
        command.env(receipt::ENV_FD, fd.to_string());
        super::install_with_receipt(command, executable, target, args, Some(fd));
        Ok(Self {
            receiver,
            writer,
            keeper,
        })
    }

    pub(crate) fn spawn(self, command: &mut Command) -> io::Result<ManagedGate> {
        let child = command.spawn()?;
        let Self {
            receiver,
            writer,
            keeper,
        } = self;
        drop(writer);
        Ok(ManagedGate {
            child: Some(child),
            receiver,
            keeper,
            admitted: false,
            complete: None,
            observed: None,
        })
    }
}

pub(crate) struct ManagedGate {
    child: Option<Child>,
    receiver: Receiver,
    keeper: OwnedReaper,
    admitted: bool,
    complete: Option<ExitStatus>,
    observed: Option<ExitStatus>,
}

impl ManagedGate {
    pub(crate) fn id(&self) -> u32 {
        self.child.as_ref().expect("owned gate").id()
    }

    pub(crate) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }

    pub(crate) fn resume(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + STARTUP;
        loop {
            self.receiver.poll(self.id()).map_err(incompatible)?;
            if let Some(status) = self.child.as_mut().expect("owned gate").try_wait()? {
                self.observed = Some(status);
                return Err(incompatible("gate exited before admission"));
            }
            if self.receiver.hello {
                let status = std::fs::read_to_string(format!("/proc/{}/status", self.id()))?;
                if status.lines().any(|line| {
                    line.starts_with("State:") && line.split_whitespace().nth(1) == Some("T")
                }) {
                    // From here this current-ABI gate owns all cleanup. Never
                    // send KILL to it, even if CONT or a later receipt fails.
                    self.admitted = true;
                    if unsafe { libc::kill(self.id() as i32, libc::SIGCONT) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(incompatible(
                    "no valid Hello and stopped readiness before deadline",
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.complete {
            return Ok(Some(status));
        }
        if self.child.is_none() {
            return Err(io::Error::other(
                "native gate retained in wait-only custody; cleanup is unconfirmed",
            ));
        }
        self.receiver.poll(self.id())?;
        let status = match self.observed {
            Some(status) => Some(status),
            None => self.child.as_mut().expect("owned gate").try_wait()?,
        };
        self.observed = status;
        if let Some(status) = status {
            // Child exit synchronizes its final write, but a prior nonblocking
            // read may have raced it. Poll once more; never wait for EOF, which
            // may be held by a duplicated descriptor in a protocol fixture.
            self.receiver.poll(self.id())?;
            let code = self.receiver.quiescent.ok_or_else(|| io::Error::other(
                "native gate exited without a quiescence receipt; descendant cleanup is unconfirmed"
            ))?;
            if status.code() != Some(i32::from(code)) {
                return Err(io::Error::other(
                    "native gate exit disagrees with quiescence receipt",
                ));
            }
            self.complete = Some(status);
        }
        Ok(status)
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn stop(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.complete {
            return Ok(status);
        }
        if self.child.is_none() {
            return Err(io::Error::other(
                "native gate retained in wait-only custody; cleanup is unconfirmed",
            ));
        }
        let deadline = Instant::now() + CLEANUP;
        if !self.admitted {
            // Never TERM or CONT an unknown/legacy stopped launcher. Its
            // watcher may translate TERM into CONT and thereby launch work.
            let child = self.child.as_mut().expect("owned gate");
            if let Some(status) = self.observed {
                self.complete = Some(status);
                return Ok(status);
            }
            let _ = child.kill();
            loop {
                if let Some(status) = child.try_wait()? {
                    self.complete = Some(status);
                    return Ok(status);
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        } else {
            loop {
                match self.try_wait() {
                    Ok(Some(status)) => return Ok(status),
                    Ok(None) => {}
                    Err(error) => {
                        if self.observed.is_none() {
                            unsafe {
                                libc::kill(self.id() as i32, libc::SIGTERM);
                                libc::kill(self.id() as i32, libc::SIGCONT);
                            }
                        }
                        self.handoff();
                        return Err(io::Error::other(format!(
                            "{error}; gate retained in wait-only custody"
                        )));
                    }
                }
                unsafe {
                    libc::kill(self.id() as i32, libc::SIGTERM);
                    libc::kill(self.id() as i32, libc::SIGCONT);
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        self.handoff();
        Err(io::Error::other(
            "native gate cleanup deadline exceeded; descendant cleanup is unconfirmed; gate retained in wait-only custody",
        ))
    }

    fn handoff(&mut self) {
        if let Some(child) = self.child.take() {
            self.keeper.adopt_wait_only(child);
        }
    }
}

impl Drop for ManagedGate {
    fn drop(&mut self) {
        if self.child.is_some() && self.complete.is_none() {
            if let Err(error) = self.stop() {
                eprintln!("asb process gate cleanup: {error}");
            }
        }
    }
}

fn incompatible(reason: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!(
        "ASB process gate ABI is incompatible: {reason}; rebuild ASB and use the current ASB runtime"
    ))
}

#[cfg(test)]
mod tests;
