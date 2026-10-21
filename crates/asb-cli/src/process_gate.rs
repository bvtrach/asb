#![allow(unsafe_code)]

//! Internal process start gate used to close the collector attachment race.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "linux")]
mod descendants;
#[cfg(target_os = "linux")]
mod owner;
#[cfg(target_os = "linux")]
pub(crate) use descendants::DescendantScope;

/// A loader-bundled runtime uses its launcher for recursive ASB execution.
pub(crate) fn executable() -> io::Result<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("ASB_RUNTIME_EXECUTABLE") {
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute() || !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid ASB runtime launcher",
            ));
        }
        return Ok(path);
    }
    // The pre-exec child still runs this exact ASB image. Its kernel handle
    // remains valid if a simultaneous installation replaces the original
    // pathname; current_exe() can otherwise return an unusable '(deleted)' path.
    #[cfg(target_os = "linux")]
    return Ok(std::path::PathBuf::from("/proc/self/exe"));
    #[cfg(not(target_os = "linux"))]
    std::env::current_exe()
}

#[cfg(unix)]
pub(crate) fn install(
    command: &mut Command,
    gate_executable: &Path,
    target: &str,
    args: &[String],
) {
    use std::os::unix::process::CommandExt;
    let gate_executable = gate_executable.to_owned();
    let target = target.to_owned();
    let args = args.to_owned();
    #[cfg(target_os = "linux")]
    command.env("ASB_GATE_OWNER_PID", std::process::id().to_string());
    let environment_changes = command
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.map(ToOwned::to_owned)))
        .collect::<Vec<_>>();
    unsafe {
        command.pre_exec(move || {
            let mut values = vec![
                gate_executable.as_os_str().to_owned(),
                OsString::from("__asb-gate"),
                OsString::from(&target),
            ];
            values.extend(args.iter().map(OsString::from));
            let c_values = values
                .iter()
                .map(|value| {
                    std::ffi::CString::new(value.as_os_str().as_bytes()).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "gate argument contains NUL")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut pointers = c_values
                .iter()
                .map(|value| value.as_ptr())
                .collect::<Vec<_>>();
            pointers.push(std::ptr::null());
            let mut environment = std::env::vars_os().collect::<Vec<_>>();
            for (key, value) in &environment_changes {
                environment.retain(|(existing, _)| existing != key);
                if let Some(value) = value {
                    environment.push((key.clone(), value.clone()));
                }
            }
            let c_environment = environment
                .iter()
                .map(|(key, value)| {
                    std::ffi::CString::new(
                        [
                            key.as_os_str().as_bytes(),
                            b"=",
                            value.as_os_str().as_bytes(),
                        ]
                        .concat(),
                    )
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "gate environment contains NUL")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut environment_pointers = c_environment
                .iter()
                .map(|value| value.as_ptr())
                .collect::<Vec<_>>();
            environment_pointers.push(std::ptr::null());
            libc::execve(
                c_values[0].as_ptr(),
                pointers.as_ptr(),
                environment_pointers.as_ptr(),
            );
            Err(io::Error::last_os_error())
        });
    }
}

#[cfg(unix)]
pub(crate) fn resume(child: &mut std::process::Child) -> io::Result<()> {
    wait_stopped(child)?;
    if unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGCONT) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub(crate) fn wait_stopped(child: &mut std::process::Child) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let pid = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if child.try_wait()?.is_some() {
                return Err(io::Error::other(
                    "native process gate exited before readiness",
                ));
            }
            let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
            if status.lines().any(|line| {
                line.starts_with("State:") && line.split_whitespace().nth(1) == Some("T")
            }) {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(io::Error::other(
                    "native process gate readiness deadline exceeded",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = child;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn pause_current() -> io::Result<()> {
    if unsafe { libc::raise(libc::SIGSTOP) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Cooperatively cancel an owned gate, allowing its subreaper to finish.
/// Repeated CONT closes cancellation racing with the gate's initial STOP.
#[cfg(target_os = "linux")]
pub(crate) fn cancel_gate(child: &mut std::process::Child) -> io::Result<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other(
                "native gate cleanup deadline exceeded; descendant cleanup is unconfirmed",
            ));
        }
        // This unreaped Child belongs exclusively to this caller; its PID
        // cannot be reused until the caller waits for it.
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            libc::kill(child.id() as libc::pid_t, libc::SIGCONT);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[cfg(unix)]
pub(crate) fn run_gate(args: &[OsString]) -> std::process::ExitCode {
    let Some((program, target_args)) = args.split_first() else {
        return std::process::ExitCode::from(2);
    };
    #[cfg(target_os = "linux")]
    let mut descendants = match DescendantScope::new() {
        Ok(scope) => scope,
        Err(error) => {
            eprintln!("asb process gate could not retain orphaned descendants: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    #[cfg(target_os = "linux")]
    let owner = match owner::Owner::bind() {
        Ok(owner) => owner,
        Err(error) => {
            eprintln!("asb process gate could not bind its owner: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    if pause_current().is_err() {
        return std::process::ExitCode::from(2);
    }
    #[cfg(target_os = "linux")]
    let status = (|| {
        if owner.cancelled()? {
            return Err(io::Error::other(
                "native gate owner exited before client execution",
            ));
        }
        let mut child = Command::new(program).args(target_args).spawn()?;
        owner.wait(&mut child)
    })();
    #[cfg(not(target_os = "linux"))]
    let status = Command::new(program).args(target_args).status();
    #[cfg(target_os = "linux")]
    owner.stop_waker();
    #[cfg(target_os = "linux")]
    if let Err(error) = descendants.quiesce() {
        eprintln!("asb process gate could not stop owned agent descendants: {error}");
        return std::process::ExitCode::from(2);
    }
    match status {
        Ok(status) => std::process::ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(error) => {
            eprintln!("asb process gate: {error}");
            std::process::ExitCode::from(127)
        }
    }
}

/// Keep descendants that daemonize or outlive an intermediate helper attached
/// to the measured root. Without a subreaper, Linux reparents those helpers
/// to PID 1 and process-tree collectors can no longer observe them. This is
/// set only in the short-lived gate child, never in the coordinator process.
#[cfg(target_os = "linux")]
fn become_child_subreaper() -> io::Result<()> {
    // SAFETY: prctl does not dereference pointers for these operations and is
    // scoped to the gate process.
    let result = unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "process_gate/coverage_tests.rs"]
mod coverage_tests;
