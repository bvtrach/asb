//! Bounded model-catalog subprocess transport; never stores request credentials.
use crate::Error;
use std::process::{Command, ExitStatus};

#[cfg(unix)]
const MAX_RESPONSE: usize = 16 * 1024 * 1024;

pub(super) struct Captured {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[cfg(not(unix))]
pub(super) fn capture(_: &mut Command, _: &[u8], _: &str) -> Result<Captured, Error> {
    Err(Error::Input("bounded model discovery transport currently requires Unix; no request was started. A cancellable non-Unix transport remains unsupported".into()))
}

#[cfg(unix)]
pub(super) fn capture(
    command: &mut Command,
    config: &[u8],
    provider: &str,
) -> Result<Captured, Error> {
    unix::capture(command, config, provider, unix::Budget::PRODUCTION)
}

#[cfg(unix)]
mod unix {
    use super::*;
    use crate::process_cleanup::OwnedReaper;
    use std::io::{self, Read, Write};
    use std::os::fd::AsRawFd;
    use std::process::Child;
    use std::time::{Duration, Instant};

    const STDERR_LIMIT: usize = 8192;
    const QUANTUM: usize = 65536;
    pub(super) struct Budget {
        pub io: Duration,
        pub total: Duration,
    }
    impl Budget {
        pub(super) const PRODUCTION: Self = Self {
            io: Duration::from_secs(30),
            total: Duration::from_secs(35),
        };
    }

    struct Owned {
        child: Option<Child>,
        reaper: OwnedReaper,
        deadline: Instant,
    }
    fn cleanup_poll(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
        #[cfg(all(test, target_os = "linux"))]
        if let Some(result) = super::tests::cleanup_poll_override() {
            return result;
        }
        child.try_wait()
    }
    impl Owned {
        fn cleanup(&mut self) -> Result<(), Error> {
            let Some(mut child) = self.child.take() else {
                return Ok(());
            };
            if matches!(cleanup_poll(&mut child), Ok(Some(_))) {
                return Ok(());
            }
            let _ = child.kill();
            let deadline = self.deadline.min(Instant::now() + Duration::from_secs(5));
            loop {
                let reason = match cleanup_poll(&mut child) {
                    Ok(Some(_)) => return Ok(()),
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(
                            Duration::from_millis(10)
                                .min(deadline.saturating_duration_since(Instant::now())),
                        );
                        continue;
                    }
                    Ok(None) => "child exit exceeded the cleanup deadline".to_owned(),
                    Err(error) => format!("cannot confirm child exit: {error}"),
                };
                self.reaper.adopt(child);
                return Err(Error::Io(format!(
                    "model discovery cleanup was not confirmed: {reason}; an owned reaper retains the child"
                )));
            }
        }
        fn finish<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
            match (result, self.cleanup()) {
                (result, Ok(())) => result,
                (Ok(_), Err(error)) => Err(error),
                (Err(error), Err(cleanup)) => Err(Error::Io(format!("{error}; {cleanup}"))),
            }
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.cleanup();
        }
    }

    #[allow(unsafe_code)]
    fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
        let fd = pipe.as_raw_fd();
        // SAFETY: the borrowed, owned pipe remains open throughout both calls;
        // fcntl preserves its original descriptor flags except O_NONBLOCK.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn read_ready<R: Read>(
        pipe: &mut Option<R>,
        retained: &mut Vec<u8>,
        tail: bool,
        deadline: Instant,
    ) -> io::Result<()> {
        let Some(reader) = pipe.as_mut() else {
            return Ok(());
        };
        let mut chunk = [0u8; 4096];
        let mut drained = 0;
        while drained < QUANTUM && Instant::now() < deadline {
            let limit = if tail {
                chunk.len()
            } else {
                chunk.len().min(super::MAX_RESPONSE + 1 - retained.len())
            };
            if limit == 0 {
                break;
            }
            match reader.read(&mut chunk[..limit]) {
                Ok(0) => {
                    *pipe = None;
                    break;
                }
                Ok(count) => {
                    drained += count;
                    retained.extend_from_slice(&chunk[..count]);
                    if tail && retained.len() > STDERR_LIMIT {
                        retained.drain(..retained.len() - STDERR_LIMIT);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub(super) fn capture(
        command: &mut Command,
        config: &[u8],
        provider: &str,
        budget: Budget,
    ) -> Result<Captured, Error> {
        let started = Instant::now();
        let deadline = started + budget.total;
        let io_deadline = deadline.min(started + budget.io);
        let reaper = OwnedReaper::new("model-discovery").map_err(Error::Io)?;
        let child = command.spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                Error::Io(format!("model discovery needs `curl`, but it could not be started: {error}. Install it, then rerun the command. Suggested command:\n  sudo apt-get update && sudo apt-get install -y curl"))
            } else { Error::Io(format!("cannot start curl for model discovery: {error}")) }
        })?;
        let mut owned = Owned {
            child: Some(child),
            reaper,
            deadline,
        };
        #[cfg(all(test, target_os = "linux"))]
        super::tests::admitted(owned.child.as_ref().expect("admitted child").id());
        let result = communicate(&mut owned, config, provider, io_deadline);
        owned.finish(result)
    }

    #[allow(unsafe_code)]
    fn communicate(
        owned: &mut Owned,
        config: &[u8],
        provider: &str,
        deadline: Instant,
    ) -> Result<Captured, Error> {
        let child = owned.child.as_mut().expect("owned model discovery child");
        let mut input = child.stdin.take();
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let setup =
            |error: String| Error::Io(format!("cannot configure model discovery pipes: {error}"));
        nonblocking(
            input
                .as_ref()
                .ok_or_else(|| setup("stdin missing".into()))?,
        )
        .map_err(|error| setup(error.to_string()))?;
        nonblocking(
            stdout
                .as_ref()
                .ok_or_else(|| setup("stdout missing".into()))?,
        )
        .map_err(|error| setup(error.to_string()))?;
        nonblocking(
            stderr
                .as_ref()
                .ok_or_else(|| setup("stderr missing".into()))?,
        )
        .map_err(|error| setup(error.to_string()))?;
        let mut sent = 0;
        let mut body = Vec::new();
        let mut diagnostic = Vec::new();
        let mut status = None;
        loop {
            if sent == config.len() {
                input = None;
            }
            if status.is_none() {
                status = child.try_wait().map_err(|error| {
                    Error::Io(format!("cannot finish model discovery request: {error}"))
                })?;
            }
            if input.is_none()
                && stdout.is_none()
                && stderr.is_none()
                && let Some(status) = status
            {
                return Ok(Captured {
                    status,
                    stdout: body,
                    stderr: diagnostic,
                });
            }
            if Instant::now() >= deadline {
                let phase = if input.is_some() {
                    "sending request configuration"
                } else if stdout.is_some() || stderr.is_some() {
                    "waiting for response streams to close"
                } else {
                    "waiting for curl to exit"
                };
                return Err(Error::Input(format!(
                    "model discovery for {provider} timed out while {phase}; check the provider connection and project proxy configuration"
                )));
            }
            let fd = |pipe: Option<&dyn AsRawFd>| pipe.map_or(-1, |pipe| pipe.as_raw_fd());
            let mut polls = [
                libc::pollfd {
                    fd: fd(input.as_ref().map(|pipe| pipe as &dyn AsRawFd)),
                    events: libc::POLLOUT,
                    revents: 0,
                },
                libc::pollfd {
                    fd: fd(stdout.as_ref().map(|pipe| pipe as &dyn AsRawFd)),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: fd(stderr.as_ref().map(|pipe| pipe as &dyn AsRawFd)),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout = remaining.as_millis().clamp(1, 25) as libc::c_int;
            // SAFETY: poll receives a live fixed-size array and no descriptor
            // is closed concurrently; each descriptor is owned above.
            if unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout) } < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(Error::Io(format!(
                    "cannot poll model discovery pipes: {error}"
                )));
            }
            if polls.iter().any(|poll| poll.revents & libc::POLLNVAL != 0) {
                return Err(Error::Io("model discovery pipe became invalid".into()));
            }
            if polls[0].revents != 0
                && let Some(writer) = input.as_mut()
            {
                match writer.write(&config[sent..config.len().min(sent + 16384)]) {
                    Ok(0) => {
                        return Err(Error::Io(
                            "cannot send model discovery request: pipe made no progress".into(),
                        ));
                    }
                    Ok(count) => sent += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => {
                        return Err(Error::Io(format!(
                            "cannot send model discovery request: {error}"
                        )));
                    }
                }
            }
            if polls[1].revents != 0 {
                read_ready(&mut stdout, &mut body, false, deadline).map_err(|error| {
                    Error::Io(format!("cannot read model catalog for {provider}: {error}"))
                })?;
                if body.len() > super::MAX_RESPONSE {
                    return Err(Error::Input(format!(
                        "model catalog for {provider} exceeds the 16 MiB response limit; retry later or contact the provider"
                    )));
                }
            }
            if polls[2].revents != 0 {
                read_ready(&mut stderr, &mut diagnostic, true, deadline).map_err(|error| {
                    Error::Io(format!("cannot read model discovery diagnostics: {error}"))
                })?;
            }
        }
    }
}

#[cfg(all(test, unix))]
#[path = "../../tests/unit/model_transport.rs"]
mod tests;
