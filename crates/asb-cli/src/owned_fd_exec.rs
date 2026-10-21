//! Private exact-owned-descriptor sanitation before same-PID Unix exec.
#![allow(unsafe_code)] // fstat/fcntl borrow only explicitly identified child FDs.
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    fd: i32,
    device: String,
    inode: String,
    kind: Kind,
}
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Socket,
    File,
}
impl Kind {
    fn mode(self) -> libc::mode_t {
        match self {
            Self::Socket => libc::S_IFSOCK,
            Self::File => libc::S_IFREG,
        }
    }
}
fn decimal(value: &str) -> Result<u64, ()> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    value.parse().map_err(|_| ())
}
fn claims(value: &std::ffi::OsStr) -> Result<Vec<(i32, u64, u64, Kind)>, ()> {
    let text = value.to_str().ok_or(())?;
    if text.len() > 8192 {
        return Err(());
    }
    let claims: Vec<Claim> = serde_json::from_str(text).map_err(|_| ())?;
    if claims.is_empty() || claims.len() > 4 {
        return Err(());
    }
    let mut seen = std::collections::BTreeSet::new();
    claims
        .into_iter()
        .map(|claim| {
            if claim.fd < 3 || !seen.insert(claim.fd) {
                return Err(());
            }
            Ok((
                claim.fd,
                decimal(&claim.device)?,
                decimal(&claim.inode)?,
                claim.kind,
            ))
        })
        .collect()
}

fn sanitize(claims: &[(i32, u64, u64, Kind)]) -> Result<(), ()> {
    for &(fd, device, inode, kind) in claims {
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: fstat borrows the supplied descriptor and initializes stat on
        // success. This single-threaded exec entry never guesses other FDs.
        if unsafe { libc::fstat(fd, metadata.as_mut_ptr()) } != 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::EBADF) {
                continue;
            }
            return Err(());
        }
        let metadata = unsafe { metadata.assume_init() };
        // Unix aliases differ by platform; retain the explicit u64 result
        // without assuming either native field already has that Rust type.
        let observed_device: u64 = metadata.st_dev as _;
        let observed_inode: u64 = metadata.st_ino as _;
        if observed_device != device
            || observed_inode != inode
            || metadata.st_mode & libc::S_IFMT != kind.mode()
        {
            // A prior exec may have closed this number, or it may now denote
            // an unrelated object. Neither grants authority over that object.
            continue;
        }
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            return Err(());
        }
    }
    Ok(())
}

pub(crate) fn entry(args: &[OsString]) -> ExitCode {
    let result = (|| {
        if args.len() < 3 || args[1] != "--" || args[2].is_empty() {
            return Err(());
        }
        let claims = claims(&args[0])?;
        sanitize(&claims)?;
        // Same PID, original raw argv/environment/cwd/stdio. No shell, spawn,
        // extra child/reaper, provider decision, or command reconstruction from
        // a previously configured Rust Command is involved.
        let _error = Command::new(&args[2]).args(&args[3..]).exec();
        Err(())
    })();
    let _: Result<(), ()> = result;
    // Fixed diagnostic: never echo raw control data, command args or secrets.
    let _ = io::stderr().write_all(b"asb: owned descriptor exec handoff failed\n");
    ExitCode::from(2)
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/unit/owned_fd_exec.rs"]
mod tests;
