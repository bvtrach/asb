//! Factual offline-test prerequisite, never a product bootstrap bypass.
use super::fixture_writer::{Keeper, OneSlotKeeper};
use std::ffi::OsStr;
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const REMEDY: &str = "offline test prerequisite: install/provision real Python 3.12 or 3.13 with the standard-library venv module and expose python3.12 or python3.13 on this command's PATH before running tests (GitHub-hosted CI uses actions/setup-python). ASB init was not invoked; no bootstrap/download was attempted";

struct Probe {
    child: Option<Child>,
    keeper: OneSlotKeeper,
}
impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            // Only this directly admitted probe is transferred. No PID scan,
            // process-group signal or unbounded caller-side wait is involved.
            self.keeper.adopt(child);
        }
    }
}

fn candidate(name: &str, search: &OsStr, cwd: &Path) -> Option<PathBuf> {
    std::env::split_paths(search)
        .map(|directory| {
            let directory = if directory.is_absolute() {
                directory
            } else {
                cwd.join(directory)
            };
            directory.join(name)
        })
        .find(|path| {
            std::fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

fn supported(program: &Path, search: &OsStr, cwd: &Path) -> io::Result<bool> {
    let keeper = OneSlotKeeper::new()?;
    let (mut reader, writer) = UnixStream::pair()?;
    reader.set_nonblocking(true)?;
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", search)
        .current_dir(cwd)
        .args([
            "-I",
            "-S",
            "-c",
            "import sys, venv; print('%d.%d' % sys.version_info[:2])",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::null());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let mut probe = Probe {
        child: Some(command.spawn()?),
        keeper,
    };
    drop(command); // Release the parent's configured stdout writer before EOF.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bytes = Vec::new();
    let mut status = None;
    let mut eof = false;
    while Instant::now() < deadline {
        let mut chunk = [0; 128];
        match reader.read(&mut chunk) {
            Ok(0) => eof = true,
            Ok(count) if bytes.len() + count <= 128 => bytes.extend_from_slice(&chunk[..count]),
            Ok(_) => {
                return Err(io::Error::other(
                    "Python prerequisite response exceeded 128 bytes",
                ));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        if status.is_none() {
            status = probe.child.as_mut().unwrap().try_wait()?;
        }
        if let Some(status) = status.filter(|_| eof) {
            probe.child.take(); // Actual direct-child exit observed/reaped.
            return Ok(status.success() && matches!(bytes.as_slice(), b"3.12\n" | b"3.13\n"));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "Python prerequisite probe exceeded five seconds",
    ))
}

pub fn require(cwd: &Path, search: &OsStr) -> Result<PathBuf, String> {
    for name in ["python3.12", "python3.13"] {
        if let Some(program) = candidate(name, search, cwd)
            && supported(&program, search, cwd)
                .map_err(|error| format!("{REMEDY}; probe failed: {error}"))?
        {
            return Ok(program);
        }
    }
    Err(REMEDY.into())
}

#[test]
fn real_supported_prerequisite_respects_relative_empty_path_and_child_cwd() {
    use std::os::unix::ffi::OsStringExt;
    let cwd = std::env::current_dir().unwrap();
    let real = require(&cwd, &std::env::var_os("PATH").unwrap_or_default()).unwrap();
    let root =
        super::fixture_namespace::allocate(&std::env::temp_dir(), "asb-python-prerequisite-")
            .unwrap();
    std::fs::create_dir(root.join("bin")).unwrap();
    std::os::unix::fs::symlink(&real, root.join("bin/python3.12")).unwrap();
    let search = std::env::join_paths([Path::new(""), Path::new("bin")]).unwrap();
    assert_eq!(
        require(&root, &search).unwrap(),
        root.join("bin/python3.12")
    );
    let raw = std::ffi::OsString::from_vec(b"bin-\xff".to_vec());
    std::fs::create_dir(root.join(&raw)).unwrap();
    std::os::unix::fs::symlink(&real, root.join(&raw).join("python3.13")).unwrap();
    assert_eq!(
        require(&root, &raw).unwrap(),
        root.join(raw).join("python3.13")
    );
    super::fixture_writer::cleanup(&root).unwrap();
}

#[test]
fn missing_and_noninterpreter_prerequisites_report_precise_no_bootstrap_remedy() {
    let root =
        super::fixture_namespace::allocate(&std::env::temp_dir(), "asb-python-prerequisite-")
            .unwrap();
    let search = root.as_os_str();
    assert_eq!(require(&root, search).unwrap_err(), REMEDY);
    std::os::unix::fs::symlink("/bin/true", root.join("python3.12")).unwrap();
    assert_eq!(require(&root, search).unwrap_err(), REMEDY);
    assert!(!root.join(".asb").exists());
    assert!(!root.join(".asb-tools").exists());
    super::fixture_writer::cleanup(&root).unwrap();
}
