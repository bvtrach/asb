//! Private owned-listener handoff and bounded compatibility-only ready facts.
#![allow(unsafe_code)] // Child-only fcntl on the exact owned listener descriptor.
use std::fs::File;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::{Arc, Mutex};

const PREFIX: &[u8] = b"ASB cli2key proxy ready";
const MAX_LINE: usize = 4096;
const MAX_BYTES: usize = 64 * 1024;
const MAX_LINES: usize = 64;

pub(super) struct Reservation {
    listener: Arc<TcpListener>,
    address: SocketAddrV4,
    image: Option<Arc<File>>,
    helper: std::path::PathBuf,
}
impl Reservation {
    pub(super) fn new() -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let SocketAddr::V4(address) = listener.local_addr()? else {
            return Err(io::Error::other("compatibility listener is not IPv4"));
        };
        let (image, helper) = exec_image()?;
        Ok(Self {
            listener: Arc::new(listener),
            address,
            image,
            helper,
        })
    }
    pub(super) fn address(&self) -> SocketAddrV4 {
        self.address
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn socket_identity(&self) -> (u64, u64) {
        use std::os::unix::fs::MetadataExt;
        let metadata =
            std::fs::metadata(format!("/proc/self/fd/{}", self.listener.as_raw_fd())).unwrap();
        (metadata.dev(), metadata.ino())
    }
    pub(super) fn configure(&self, command: &mut Command) {
        let listener = Arc::clone(&self.listener);
        command.args(["--listen-fd", &listener.as_raw_fd().to_string()]);
        command.arg("--exec-helper").arg(&self.helper);
        let image = self.image.clone();
        if let Some(image) = &image {
            command.args(["--owned-image-fd", &image.as_raw_fd().to_string()]);
        }
        // SAFETY: the Arc owns this exact FD through spawn. Only async-signal-
        // safe fcntl/errno retrieval run after fork. Parent flags never change.
        unsafe {
            command.pre_exec(move || {
                let fd = listener.as_raw_fd();
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if let Some(image) = &image {
                    let fd = image.as_raw_fd();
                    let flags = libc::fcntl(fd, libc::F_GETFD);
                    if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
}

fn exec_image() -> io::Result<(Option<Arc<File>>, std::path::PathBuf)> {
    if std::env::var_os("ASB_RUNTIME_EXECUTABLE").is_some() {
        // Existing bundled loader contract: its explicit launcher must run,
        // not a raw /proc image that might be the dynamic loader itself.
        return Ok((None, crate::process_gate::executable()?));
    }
    #[cfg(target_os = "linux")]
    {
        #[cfg(test)]
        let path = std::env::current_exe()?
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| io::Error::other("missing actual ASB fixture directory"))?
            .join("asb");
        #[cfg(not(test))]
        let path = std::path::PathBuf::from("/proc/self/exe");
        let image = Arc::new(File::open(path)?);
        let helper = std::path::PathBuf::from(format!("/proc/self/fd/{}", image.as_raw_fd()));
        Ok((Some(image), helper))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok((None, crate::process_gate::executable()?))
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    schema_version: u32,
    host: String,
    port: u16,
}

#[derive(Default)]
struct Observation {
    ready: Option<SocketAddrV4>,
    fault: Option<&'static str>,
    ended: bool,
}
#[derive(Clone)]
pub(super) struct Watch {
    expected: SocketAddrV4,
    state: Arc<Mutex<Observation>>,
}
impl Watch {
    pub(super) fn new(expected: SocketAddrV4) -> Self {
        Self {
            expected,
            state: Arc::new(Mutex::new(Observation::default())),
        }
    }
    pub(super) fn check(&self) -> Result<Option<SocketAddrV4>, String> {
        let state = self.state.lock().unwrap();
        if let Some(fault) = state.fault {
            Err(format!("cli2key compatibility proxy {fault}"))
        } else {
            Ok(state.ready)
        }
    }
    pub(super) fn ended(&self) -> bool {
        self.state.lock().unwrap().ended
    }
    fn fail(&self, reason: &'static str) {
        self.state.lock().unwrap().fault.get_or_insert(reason);
    }
}

pub(super) struct Parser {
    watch: Watch,
    line: Vec<u8>,
    overlong: bool,
    ready: bool,
    bytes: usize,
    lines: usize,
    reserved_bytes: usize,
    reserved_lines: usize,
}
impl Parser {
    pub(super) fn new(watch: Watch) -> Self {
        Self {
            watch,
            line: Vec::with_capacity(MAX_LINE),
            overlong: false,
            ready: false,
            bytes: 0,
            lines: 0,
            reserved_bytes: 0,
            reserved_lines: 0,
        }
    }
    pub(super) fn accept(&mut self, chunk: &[u8]) {
        for &byte in chunk {
            if !self.ready {
                self.bytes = self.bytes.saturating_add(1);
                if self.bytes > MAX_BYTES {
                    self.watch
                        .fail("startup output exceeded its byte or line limit");
                }
            }
            if byte != b'\n' {
                if self.line.len() < MAX_LINE {
                    self.line.push(byte);
                } else {
                    self.overlong = true;
                    if !self.ready || self.line.starts_with(PREFIX) {
                        self.watch
                            .fail("startup output exceeded its byte or line limit");
                    }
                }
                continue;
            }
            self.lines = self.lines.saturating_add(1);
            if self.line.starts_with(PREFIX) {
                self.reserved_bytes = self.reserved_bytes.saturating_add(self.line.len() + 1);
                self.reserved_lines = self.reserved_lines.saturating_add(1);
                if self.overlong
                    || self.reserved_bytes > MAX_BYTES
                    || self.reserved_lines > MAX_LINES
                {
                    self.watch.fail("ready output exceeded its protocol limit");
                } else {
                    self.ready_line();
                }
            }
            self.line.clear();
            self.overlong = false;
            if !self.ready && self.lines >= MAX_LINES {
                self.watch.fail(
                    "startup output exceeded its message limit; proxy did not provide readiness",
                );
            }
        }
    }
    fn ready_line(&mut self) {
        let parsed = self
            .line
            .strip_prefix(b"ASB cli2key proxy ready ")
            .and_then(|json| serde_json::from_slice::<Ready>(json).ok());
        let Some(ready) = parsed else {
            self.watch.fail("provided malformed ready data");
            return;
        };
        if ready.schema_version != 1 || ready.host != "127.0.0.1" || ready.port == 0 {
            self.watch.fail("provided invalid ready address");
            return;
        }
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, ready.port);
        if address != self.watch.expected {
            self.watch
                .fail("ready address disagrees with its owned listener");
            return;
        }
        let mut state = self.watch.state.lock().unwrap();
        if state.ready.is_some() {
            state.fault.get_or_insert("provided duplicate ready data");
        } else {
            state.ready = Some(address);
            self.ready = true;
        }
    }
    pub(super) fn end(&mut self) {
        if self.line.starts_with(PREFIX) {
            self.watch.fail("closed with incomplete ready data");
        }
        self.watch.state.lock().unwrap().ended = true;
    }
    pub(super) fn read_failed(&self) {
        self.watch.fail("ready output could not be read completely");
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../tests/unit/cli2key_proxy_ready.rs"]
mod tests;
