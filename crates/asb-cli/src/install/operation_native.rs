//! Fixed, credential-free datagrams; EOF and installer exit are never proofs.
#![allow(unsafe_code)]
use super::Request;
use crate::{Error, install, launch::ProcessLaunch, process_gate};
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixDatagram;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};
use std::time::{Duration, Instant};

const VERSION: &str = "ASBINSTALL1";
const FD_ENV: &str = "ASB_INSTALL_PROGRESS_FD";
const FRAME: usize = 8;
const MAGIC: [u8; 4] = *b"ASI1";
const PROGRESS: u8 = 0;
const SUCCESS: u8 = 1;
const FAILURE: u8 = 2;
const MAX_NAMES: usize = 128;
const MAX_NAME: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Completion {
    Success,
    Input,
    Io,
    Run,
    Usage,
}

impl Completion {
    fn diagnostic(self) -> &'static str {
        match self {
            Self::Success => "installation finished",
            Self::Input => "installation input was rejected",
            Self::Io => "installation filesystem or command failed",
            Self::Run => "installation execution failed",
            Self::Usage => "installation request was rejected",
        }
    }
}

#[derive(Debug, PartialEq)]
enum Message {
    Progress(u8),
    Final(Completion),
}

pub(crate) struct Operation {
    gate: process_gate::ManagedGate,
    socket: UnixDatagram,
    percent: u8,
    result: Option<Completion>,
    finished: bool,
}

fn failure(message: impl std::fmt::Display) -> Error {
    Error::Io(format!("installation supervision failed: {message}"))
}

impl Operation {
    pub(crate) fn start(root: PathBuf, request: Request) -> Result<Self, Error> {
        let (mode, directory, names) = match request {
            Request::Tool { name, directory } => ("tool", directory, vec![name]),
            Request::Workload { name, directory } => ("workload", directory, vec![name]),
            Request::Dependencies {
                selected,
                directory,
            } => ("dependencies", directory, selected),
        };
        validate_names(&names).map_err(failure)?;
        if !root.is_absolute() || !directory.is_absolute() {
            return Err(failure("installation paths must be absolute"));
        }
        let executable = process_gate::executable().map_err(failure)?;
        let mut launch = ProcessLaunch::new(&executable);
        launch
            .arg("__asb-install")
            .arg(VERSION)
            .arg(mode)
            .arg(&root)
            .arg(&directory);
        for name in names {
            launch.arg(name);
        }
        let (socket, writer) = UnixDatagram::pair().map_err(failure)?;
        socket.set_nonblocking(true).map_err(failure)?;
        writer.set_nonblocking(true).map_err(failure)?;
        // Bound queued progress independently of installer duration/output.
        for (fd, option) in [
            (socket.as_raw_fd(), libc::SO_RCVBUF),
            // Linux AF_UNIX datagram queue capacity is governed by SNDBUF.
            (writer.as_raw_fd(), libc::SO_SNDBUF),
        ] {
            let size: libc::c_int = 16 * 1024;
            if unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    option,
                    (&size as *const libc::c_int).cast(),
                    std::mem::size_of_val(&size) as libc::socklen_t,
                )
            } < 0
            {
                return Err(failure(io::Error::last_os_error()));
            }
        }
        let (mut command, route) = launch.native(Some(&executable));
        command
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let descriptor = writer.as_raw_fd();
        command.env(FD_ENV, descriptor.to_string());
        // No allocation or environment access occurs in the post-fork callback.
        unsafe {
            command.pre_exec(move || set_cloexec(descriptor, false));
        }
        let admission =
            process_gate::Admission::new(route.validate_gate(&mut command).map_err(failure)?)
                .map_err(failure)?;
        let mut gate = admission.spawn().map_err(failure)?;
        drop(writer);
        if let Err(error) = gate.resume() {
            let primary = failure(error);
            return match gate.stop() {
                Ok(_) => Err(primary),
                Err(cleanup) => Err(failure(format!("{primary}; cleanup: {cleanup}"))),
            };
        }
        Ok(Self {
            gate,
            socket,
            percent: 0,
            result: None,
            finished: false,
        })
    }

    pub(crate) fn percent(&self) -> u8 {
        self.percent
    }

    fn receive(&mut self) -> Result<(), Error> {
        // A sender cannot starve input, cancellation or rendering with progress.
        for _ in 0..64 {
            let mut bytes = [0; FRAME + 1];
            match self.socket.recv(&mut bytes) {
                Ok(FRAME) => {
                    let message = decode(&bytes[..FRAME]).map_err(failure)?;
                    if self.result.is_some() {
                        return Err(failure("message after final status"));
                    }
                    match message {
                        Message::Progress(percent) => self.percent = percent,
                        Message::Final(result) => self.result = Some(result),
                    }
                }
                Ok(_) => return Err(failure("invalid status frame length")),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(failure(error)),
            }
        }
        Ok(())
    }

    pub(crate) fn poll(&mut self) -> Result<Option<Result<(), Error>>, Error> {
        self.receive()?;
        let Some(status) = self.gate.try_wait().map_err(failure)? else {
            return Ok(None);
        };
        // Gate completion synchronizes the final send. The bounded kernel queue
        // may contain more than one tick of progress; drain at most 4096 frames.
        for _ in 0..64 {
            self.receive()?;
        }
        self.finished = true;
        Ok(Some(match self.result {
            Some(Completion::Success) if status.success() => Ok(()),
            Some(result) if result != Completion::Success => Err(Error::Io(format!(
                "{}; check .asb/install.log in the selected project",
                result.diagnostic()
            ))),
            _ => Err(failure(
                "missing final status or unsuccessful gate exit; check .asb/install.log",
            )),
        }))
    }

    pub(crate) fn cancel(&mut self) -> Result<(), Error> {
        let result = self.gate.stop().map(|_| ()).map_err(failure);
        self.finished = true;
        result
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        // ManagedGate already owns the pre-established fallback; never spawn
        // or join a worker here, and never directly kill an admitted guardian.
        if !self.finished
            && let Err(error) = self.cancel()
        {
            eprintln!("{error}");
        }
    }
}

fn validate_names(names: &[String]) -> io::Result<()> {
    if names.is_empty()
        || names.len() > MAX_NAMES
        || names.iter().any(|name| {
            name.is_empty()
                || name.len() > MAX_NAME
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(io::Error::other("invalid installation identifiers"));
    }
    Ok(())
}

fn frame(kind: u8, percent: u8) -> [u8; FRAME] {
    let mut bytes = [0; FRAME];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4] = kind;
    bytes[5] = percent;
    bytes
}

fn decode(bytes: &[u8]) -> io::Result<Message> {
    if bytes.len() != FRAME
        || bytes[..4] != MAGIC
        || bytes[4] > FAILURE
        || bytes[5] > 100
        || bytes[6..] != [0, 0]
        || (bytes[4] == SUCCESS && bytes[5] != 0)
    {
        return Err(io::Error::other("invalid installation status frame"));
    }
    match (bytes[4], bytes[5]) {
        (PROGRESS, percent) => Ok(Message::Progress(percent)),
        (SUCCESS, 0) => Ok(Message::Final(Completion::Success)),
        (FAILURE, 1) => Ok(Message::Final(Completion::Input)),
        (FAILURE, 2) => Ok(Message::Final(Completion::Io)),
        (FAILURE, 3) => Ok(Message::Final(Completion::Run)),
        (FAILURE, 4) => Ok(Message::Final(Completion::Usage)),
        _ => Err(io::Error::other("invalid installation final status")),
    }
}

fn set_cloexec(fd: i32, enabled: bool) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0
        || unsafe {
            libc::fcntl(
                fd,
                libc::F_SETFD,
                if enabled {
                    flags | libc::FD_CLOEXEC
                } else {
                    flags & !libc::FD_CLOEXEC
                },
            )
        } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn entry(args: &[OsString]) -> ExitCode {
    match helper(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::from(2),
    }
}

fn helper(args: &[OsString]) -> Result<(), ()> {
    // Validate all framing before calling installers or touching project files.
    if args.len() < 5 || args.len() > MAX_NAMES + 4 || args[0] != VERSION {
        return Err(());
    }
    let mode = args[1].to_str().ok_or(())?;
    if !matches!(mode, "tool" | "workload" | "dependencies") {
        return Err(());
    }
    let root = PathBuf::from(&args[2]);
    let directory = PathBuf::from(&args[3]);
    if !root.is_absolute()
        || !directory.is_absolute()
        || std::env::current_dir().map_err(|_| ())?
            != std::fs::canonicalize(&root).map_err(|_| ())?
    {
        return Err(());
    }
    let names = args[4..]
        .iter()
        .map(|name| name.to_str().map(str::to_owned).ok_or(()))
        .collect::<Result<Vec<_>, _>>()?;
    validate_names(&names).map_err(|_| ())?;
    if mode != "dependencies" && names.len() != 1 {
        return Err(());
    }
    let descriptor = std::env::var(FD_ENV)
        .map_err(|_| ())?
        .parse::<i32>()
        .map_err(|_| ())?;
    if descriptor <= 2 {
        return Err(());
    }
    let mut kind: libc::c_int = 0;
    let mut length = std::mem::size_of_val(&kind) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            descriptor,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut kind as *mut libc::c_int).cast(),
            &mut length,
        )
    } < 0
        || kind != libc::SOCK_DGRAM
    {
        return Err(());
    }
    set_cloexec(descriptor, true).map_err(|_| ())?;
    // SAFETY: this private inherited descriptor is consumed exactly once.
    let socket = unsafe { UnixDatagram::from_raw_fd(descriptor) };
    socket.set_nonblocking(true).map_err(|_| ())?;
    let mut progress = |percent: u8, _stage: &str| {
        // Full buffers discard transient progress. No raw installer message,
        // subprocess diagnostic or credential can enter this fixed protocol.
        let _ = socket.send(&frame(PROGRESS, percent.min(100)));
    };
    let mut output = io::sink();
    let result = match mode {
        "tool" => install::tool_with_progress(
            install::ToolInstallOptions {
                name: names[0].clone(),
                source: None,
                dir: Some(directory),
                dry_run: false,
            },
            &mut output,
            &mut progress,
        ),
        "workload" => install::workload_with_progress(
            install::WorkloadInstallOptions {
                source: names[0].clone(),
                name: None,
                dir: Some(directory),
            },
            &mut output,
            &mut progress,
        ),
        _ => install::ensure_collector_dependencies(
            &names,
            &root,
            &directory,
            &mut output,
            &mut progress,
            false,
        ),
    };
    if let Err(error) = &result {
        // Keep the original private diagnostic, never its contents in frames.
        if let Ok(mut log) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join(".asb/install.log"))
        {
            let _ = writeln!(log, "ASB installation failed: {error}");
        }
    }
    let final_frame = match &result {
        Ok(()) => frame(SUCCESS, 0),
        Err(Error::Input(_)) => frame(FAILURE, 1),
        Err(Error::Io(_)) => frame(FAILURE, 2),
        Err(Error::Run(_)) => frame(FAILURE, 3),
        Err(Error::Usage(_)) => frame(FAILURE, 4),
    };
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match socket.send(&final_frame) {
            Ok(FRAME) => return result.map_err(|_| ()),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => return Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn protocol_is_fixed_and_rejects_untrusted_messages() {
        assert_eq!(decode(&frame(PROGRESS, 80)).unwrap(), Message::Progress(80));
        assert_eq!(
            decode(&frame(SUCCESS, 0)).unwrap(),
            Message::Final(Completion::Success)
        );
        assert_eq!(
            decode(&frame(FAILURE, 2)).unwrap(),
            Message::Final(Completion::Io)
        );
        for bytes in [
            vec![],
            vec![0; 8],
            frame(PROGRESS, 101).to_vec(),
            frame(SUCCESS, 1).to_vec(),
        ] {
            assert!(decode(&bytes).is_err());
        }
        assert!(validate_names(&["secret\nvalue".into()]).is_err());
        assert!(validate_names(&vec!["x".into(); MAX_NAMES + 1]).is_err());
    }
    #[test]
    fn helper_rejects_bad_framing_without_installing() {
        assert!(helper(&[]).is_err());
        assert!(helper(&[OsString::from("old")]).is_err());
    }

    #[test]
    fn nonblocking_progress_pressure_never_waits_for_a_reader() {
        let (reader, writer) = UnixDatagram::pair().unwrap();
        writer.set_nonblocking(true).unwrap();
        let started = Instant::now();
        let mut sent = 0;
        loop {
            match writer.send(&frame(PROGRESS, 50)) {
                Ok(FRAME) => sent += 1,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                other => panic!("unexpected send: {other:?}"),
            }
            assert!(sent < 10000, "progress queue is not bounded");
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        reader.set_nonblocking(true).unwrap();
        let mut received = 0;
        while reader.recv(&mut [0; FRAME]).is_ok() {
            received += 1;
        }
        assert_eq!(received, sent);
    }

    #[test]
    fn unwinding_an_operation_stops_its_actual_installer() {
        const CASE: &str = "ASB_INSTALL_UNWIND_ROOT";
        if let Some(root) = std::env::var_os(CASE) {
            let root = PathBuf::from(root);
            let pidfile = root.join("principal");
            let unwind = std::panic::catch_unwind(|| {
                let _operation = Operation::start(
                    root.clone(),
                    Request::Tool {
                        name: "pi".into(),
                        directory: root.join("tools"),
                    },
                )
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !pidfile.exists() {
                    assert!(Instant::now() < deadline, "installer never started");
                    std::thread::sleep(Duration::from_millis(5));
                }
                panic!("fixture unwind after installer admission");
            });
            assert!(unwind.is_err());
            let pid = std::fs::read_to_string(pidfile).unwrap();
            assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
            return;
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/install-operation-fixtures")
            .join(crate::workloads::TaskIdentity::local("install", None).attempt_id);
        for directory in [".asb", "bin", "home"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        std::fs::write(
            root.join(".asb/project.toml"),
            "tools_dir='tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n",
        )
        .unwrap();
        for (name, script) in [
            ("node", "#!/bin/sh\nprintf 'v22.0.0\\n'\n"),
            (
                "npm",
                "#!/bin/sh\nprintf '%s' $$ > principal\nexec /usr/bin/sleep 60\n",
            ),
        ] {
            let path = root.join("bin").join(name);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let current = std::env::current_exe().unwrap();
        let binary = current.parent().unwrap().parent().unwrap().join("asb");
        let name = format!(
            "{}::unwinding_an_operation_stops_its_actual_installer",
            module_path!().split_once("::").unwrap().1
        );
        let mut command = std::process::Command::new(current);
        command
            .args(["--exact", &name, "--nocapture"])
            .env_clear()
            .env(CASE, &root)
            .env("ASB_RUNTIME_EXECUTABLE", binary)
            .env("HOME", root.join("home"))
            .env("PATH", root.join("bin"))
            .current_dir(&root);
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeper_failure_starts_no_installation() {
        let scope = crate::process_cleanup::test_support::Scope::new();
        crate::process_cleanup::test_support::set_creation_failure(true);
        let root = PathBuf::from("/unused-install-fixture");
        let result = Operation::start(
            root.clone(),
            Request::Tool {
                name: "pi".into(),
                directory: root,
            },
        );
        crate::process_cleanup::test_support::set_creation_failure(false);
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("fixture resource exhaustion")
        );
        assert_eq!(scope.count(), 0);
    }
}
