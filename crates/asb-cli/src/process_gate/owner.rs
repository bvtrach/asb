//! Lifetime binding for the exclusive native gate, including its stopped phase.
#![allow(unsafe_code)]

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);
static CANCELLATION_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn cancel(_: libc::c_int) {
    CANCELLED.store(true, Ordering::Relaxed);
    let value = 1_u64;
    // Nonblocking eventfd write is async-signal-safe. Wake the separate child
    // even when cancellation races with the gate entering SIGSTOP.
    unsafe {
        libc::write(
            CANCELLATION_FD.load(Ordering::Relaxed),
            (&value as *const u64).cast(),
            std::mem::size_of_val(&value),
        );
    }
}

pub(super) struct Owner {
    process: OwnedFd,
    cancellation: OwnedFd,
    watcher_done: OwnedFd,
}

impl Owner {
    pub(super) fn bind(receipt_fd: Option<i32>, control_fd: Option<i32>) -> io::Result<Self> {
        let parent = unsafe { libc::getppid() };
        let expected = match std::env::var("ASB_GATE_OWNER_PID") {
            Ok(value) => value
                .parse::<libc::pid_t>()
                .map_err(|_| io::Error::other("invalid native gate owner identity"))?,
            Err(std::env::VarError::NotPresent) => parent,
            Err(_) => return Err(io::Error::other("invalid native gate owner identity")),
        };
        // The gate executable is still single threaded at this point.
        unsafe { std::env::remove_var("ASB_GATE_OWNER_PID") };
        if expected <= 1 || parent != expected {
            return Err(io::Error::other("native gate owner already exited"));
        }
        let process = pin(expected)?;
        let owner = Self {
            process,
            cancellation: event()?,
            watcher_done: event()?,
        };
        if unsafe { libc::getppid() } != expected || owner.cancelled()? {
            return Err(io::Error::other("native gate owner exited during binding"));
        }
        // This handler belongs only to the exclusive gate executable, never
        // the shared runner or the client (exec resets caught dispositions).
        let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
        action.sa_sigaction = cancel as *const () as usize;
        CANCELLATION_FD.store(owner.cancellation.as_raw_fd(), Ordering::Relaxed);
        unsafe { libc::sigemptyset(&mut action.sa_mask) };
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        owner.start_waker(receipt_fd, control_fd)?;
        Ok(owner)
    }

    pub(super) fn cancelled(&self) -> io::Result<bool> {
        if CANCELLED.load(Ordering::Relaxed) {
            return Ok(true);
        }
        let mut descriptor = libc::pollfd {
            fd: self.process.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, 0) };
        if result < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(CANCELLED.load(Ordering::Relaxed))
            } else {
                Err(error)
            };
        }
        Ok(descriptor.revents != 0)
    }

    pub(super) fn wait(
        &self,
        child: &mut std::process::Child,
        mut control: Option<&mut super::control::ChildControl>,
    ) -> io::Result<std::process::ExitStatus> {
        let principal = pin(child.id() as libc::pid_t)?;
        let mut descriptors = [
            libc::pollfd {
                fd: self.process.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.cancellation.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: principal.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: control.as_ref().map_or(-1, |control| control.descriptor()),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            if self.cancelled()? {
                return Err(io::Error::other(
                    "native gate execution cancelled or owner exited",
                ));
            }
            if let Some(status) = child.try_wait()? {
                if let Some(control) = control.as_mut() {
                    control.exited(status)?;
                }
                return Ok(status);
            }
            if let Some(control) = control.as_mut()
                && control.take_interrupt()?
            {
                // Only the exact directly owned principal pinned before this
                // wait can receive this fixed operation. The guardian remains
                // uncancelled to permit graceful target flush and quiescence.
                let result = unsafe {
                    libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        principal.as_raw_fd(),
                        libc::SIGINT,
                        std::ptr::null::<libc::siginfo_t>(),
                        0,
                    )
                };
                if result == 0 {
                    control.acknowledge()?;
                } else if io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                    return Err(io::Error::last_os_error());
                }
            }
            // No sampling thread or busy polling is added to the measured
            // interval. Principal exit, owner exit and cancellation all wake
            // this process through kernel descriptors.
            if unsafe {
                libc::poll(
                    descriptors.as_mut_ptr(),
                    descriptors.len() as libc::nfds_t,
                    -1,
                )
            } < 0
            {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
    }

    fn start_waker(&self, receipt_fd: Option<i32>, control_fd: Option<i32>) -> io::Result<()> {
        let gate = pin(std::process::id() as libc::pid_t)?;
        // A stopped process cannot run a watchdog thread. This tiny exclusive
        // child survives SIGSTOP of the gate, pins both process identities and
        // repeats SIGCONT after owner death, closing the check-before-stop
        // race. The gate remains the sole subreaper and cleanup authority.
        // Only async-signal-safe syscalls run after fork in the child.
        let child = unsafe { libc::fork() };
        if child < 0 {
            return Err(io::Error::last_os_error());
        }
        if child == 0 {
            unsafe {
                // fork does not honor CLOEXEC. Only the gate may author proofs;
                // its wake-only child must not retain the protocol socket.
                if let Some(fd) = receipt_fd {
                    libc::close(fd);
                }
                if let Some(fd) = control_fd {
                    libc::close(fd);
                }
                for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
                    // A terminal/owner-group signal must not kill the only
                    // process able to wake a stopped gate. Normal completion
                    // explicitly releases this watcher before tree cleanup.
                    libc::signal(signal, libc::SIG_IGN);
                }
                for fd in 0..=2 {
                    libc::close(fd);
                }
                let mut descriptors = [
                    libc::pollfd {
                        fd: self.process.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: gate.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: self.cancellation.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: self.watcher_done.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                loop {
                    let result = libc::poll(descriptors.as_mut_ptr(), 4, -1);
                    if result < 0 {
                        continue;
                    }
                    if descriptors[1].revents != 0 || descriptors[3].revents != 0 {
                        libc::_exit(0);
                    }
                    if descriptors[0].revents != 0 || descriptors[2].revents != 0 {
                        break;
                    }
                }
                let mut finished = [descriptors[1], descriptors[3]];
                loop {
                    libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        gate.as_raw_fd(),
                        libc::SIGCONT,
                        std::ptr::null::<libc::siginfo_t>(),
                        0,
                    );
                    if libc::poll(finished.as_mut_ptr(), 2, 25) > 0 {
                        libc::_exit(0);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn stop_waker(&self) {
        let value = 1_u64;
        unsafe {
            libc::write(
                self.watcher_done.as_raw_fd(),
                (&value as *const u64).cast(),
                8,
            );
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.stop_waker();
        CANCELLATION_FD.store(-1, Ordering::Relaxed);
    }
}

fn event() -> io::Result<OwnedFd> {
    let descriptor = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(descriptor) })
    }
}

fn pin(pid: libc::pid_t) -> io::Result<OwnedFd> {
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(descriptor as i32) })
    }
}
